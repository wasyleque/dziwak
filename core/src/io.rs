//! Zapis i odczyt obrazów rastrowych (PNG, JPEG, WebP).

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use image::ImageEncoder;
use thiserror::Error;

use crate::document::Document;
use crate::layer::Layer;
use crate::pixel::Rgba8;

/// Błędy operacji wejścia/wyjścia na plikach graficznych.
#[derive(Error, Debug)]
pub enum ImageError {
    /// Błąd systemowy wejścia/wyjścia (I/O).
    #[error("Błąd wejścia/wyjścia: {0}")]
    Io(#[from] std::io::Error),

    /// Błąd dekodowania lub kodowania biblioteki `image`.
    #[error("Błąd operacji na obrazie: {0}")]
    Image(#[from] image::ImageError),

    /// Podany format pliku nie jest obsługiwany przez Dziwaka (obsługiwane: PNG, JPEG, WebP).
    #[error("Nieobsługiwany format pliku: {0}")]
    UnsupportedFormat(String),

    /// Ścieżka pliku nie zawiera rozszerzenia.
    #[error("Ścieżka pliku nie posiada rozszerzenia")]
    MissingExtension,

    /// Błąd kompozycji warstw dokumentu do bufora.
    #[error("Błąd kompozycji warstw: {0}")]
    Composition(#[from] crate::document::CompositionError),

    /// Błąd formatu pliku Dziwaka (.dziwak).
    #[error("Błąd pliku Dziwaka: {0}")]
    Dziwak(#[from] crate::format::FormatError),
}

/// Zwraca format obrazu na podstawie rozszerzenia ścieżki pliku.
pub fn format_from_path(path: &Path) -> Result<image::ImageFormat, ImageError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or(ImageError::MissingExtension)?
        .to_ascii_lowercase();

    match ext.as_str() {
        "png" => Ok(image::ImageFormat::Png),
        "jpg" | "jpeg" => Ok(image::ImageFormat::Jpeg),
        "webp" => Ok(image::ImageFormat::WebP),
        other => Err(ImageError::UnsupportedFormat(other.to_string())),
    }
}

/// Sprawdza czy ścieżka wskazuje na plik z rozszerzeniem .dziwak
fn is_dziwak(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dziwak"))
}

/// Wczytuje obraz z pliku (PNG, JPEG lub WebP) i tworzy nowy [`Document`] z pojedynczą warstwą.
///
/// Piksele są automatycznie konwertowane z formatu prostego (straight RGBA)
/// na format z premultiplied alpha używany wewnętrznie w Dziwaku.
pub fn load_image(path: impl AsRef<Path>) -> Result<Document, ImageError> {
    let path = path.as_ref();
    if is_dziwak(path) {
        return Ok(crate::format::load_dziwak(path)?);
    }
    let format = format_from_path(path)?;

    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut img_reader = image::ImageReader::new(reader);
    img_reader.set_format(format);
    let dynamic_img = img_reader.decode()?;

    let rgba_img = dynamic_img.to_rgba8();
    let (width, height) = (rgba_img.width(), rgba_img.height());

    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Wczytany obraz");

    let mut doc = Document::new(width, height);
    let mut layer = Layer::new(file_stem, width, height);

    for (x, y, pixel) in rgba_img.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let pm_pixel = Rgba8::from_straight(r, g, b, a);
        let _ = layer.set_pixel(x, y, pm_pixel);
    }

    doc.add_layer(layer);
    Ok(doc)
}

/// Spłaszcza i zapisuje dokument do pliku o zadanym formacie (PNG, JPEG lub WebP).
///
/// Format jest wykrywany na podstawie rozszerzenia pliku w ścieżce `path`.
/// Piksele są spłaszczane przez [`Document::composite_rect`] i konwertowane
/// z premultiplied alpha na format prosty (straight).
pub fn save_image(doc: &Document, path: impl AsRef<Path>) -> Result<(), ImageError> {
    let path = path.as_ref();
    if is_dziwak(path) {
        return Ok(crate::format::save_dziwak(doc, path)?);
    }
    let format = format_from_path(path)?;

    let width = doc.width;
    let height = doc.height;
    let total_pixels = (width as usize) * (height as usize);

    let mut buffer = vec![Rgba8::TRANSPARENT; total_pixels];
    doc.composite_rect(0, 0, width, height, &mut buffer)?;

    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    match format {
        image::ImageFormat::Jpeg => {
            // JPEG nie obsługuje kanału alfa — zapisujemy 3 kanały RGB
            let mut rgb_bytes = Vec::with_capacity(total_pixels * 3);
            for px in &buffer {
                let straight = px.to_straight();
                rgb_bytes.push(straight[0]);
                rgb_bytes.push(straight[1]);
                rgb_bytes.push(straight[2]);
            }

            let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut writer);
            encoder.encode(&rgb_bytes, width, height, image::ExtendedColorType::Rgb8)?;
        }
        image::ImageFormat::Png => {
            let mut rgba_bytes = Vec::with_capacity(total_pixels * 4);
            for px in &buffer {
                let straight = px.to_straight();
                rgba_bytes.extend_from_slice(&straight);
            }

            let encoder = image::codecs::png::PngEncoder::new(&mut writer);
            encoder.write_image(&rgba_bytes, width, height, image::ExtendedColorType::Rgba8)?;
        }
        image::ImageFormat::WebP => {
            let mut rgba_bytes = Vec::with_capacity(total_pixels * 4);
            for px in &buffer {
                let straight = px.to_straight();
                rgba_bytes.extend_from_slice(&straight);
            }

            let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut writer);
            encoder.encode(&rgba_bytes, width, height, image::ExtendedColorType::Rgba8)?;
        }
        _ => return Err(ImageError::UnsupportedFormat(format!("{:?}", format))),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::layer::Layer;
    use crate::pixel::Rgba8;

    #[test]
    fn test_format_from_path() {
        assert_eq!(
            format_from_path(Path::new("test.png")).unwrap(),
            image::ImageFormat::Png
        );
        assert_eq!(
            format_from_path(Path::new("folder/photo.JPG")).unwrap(),
            image::ImageFormat::Jpeg
        );
        assert_eq!(
            format_from_path(Path::new("image.jpeg")).unwrap(),
            image::ImageFormat::Jpeg
        );
        assert_eq!(
            format_from_path(Path::new("grafika.webp")).unwrap(),
            image::ImageFormat::WebP
        );

        assert!(matches!(
            format_from_path(Path::new("plik.bmp")),
            Err(ImageError::UnsupportedFormat(_))
        ));
        assert!(matches!(
            format_from_path(Path::new("plik")),
            Err(ImageError::MissingExtension)
        ));
    }

    #[test]
    fn test_round_trip_png() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("dziwak_test_roundtrip.png");

        let width = 64;
        let height = 64;
        let mut original_doc = Document::new(width, height);
        let mut layer = Layer::new("Warstwa Testowa", width, height);

        // Ustawiamy kilka pikseli o zróżnicowanych barwach i przezroczystościach
        let red = Rgba8::from_straight(255, 0, 0, 255);
        let green = Rgba8::from_straight(0, 255, 0, 255);
        let semi_blue = Rgba8::from_straight(0, 0, 255, 128);

        layer.set_pixel(10, 10, red).unwrap();
        layer.set_pixel(20, 20, green).unwrap();
        layer.set_pixel(30, 30, semi_blue).unwrap();
        original_doc.add_layer(layer);

        // Zapis do PNG
        save_image(&original_doc, &file_path).expect("zapis PNG powinien się powieść");

        // Odczyt z PNG
        let loaded_doc = load_image(&file_path).expect("odczyt PNG powinien się powieść");
        assert_eq!(loaded_doc.width, width);
        assert_eq!(loaded_doc.height, height);
        assert_eq!(loaded_doc.layer_count(), 1);

        let loaded_layer = loaded_doc.layer(0).unwrap();
        assert_eq!(loaded_layer.get_pixel(10, 10), Some(red));
        assert_eq!(loaded_layer.get_pixel(20, 20), Some(green));
        // Przy round-trip semi-transparent różnica po zaokrągleniu nie przekracza 1
        let read_semi = loaded_layer.get_pixel(30, 30).unwrap();
        assert_eq!(read_semi.a(), semi_blue.a());
        assert!((read_semi.b() as i16 - semi_blue.b() as i16).abs() <= 1);

        // Sprawdzamy przezroczyste tło
        assert_eq!(loaded_layer.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));

        // Sprzątanie pliku tymczasowego
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_round_trip_webp() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("dziwak_test_roundtrip.webp");

        let width = 32;
        let height = 32;
        let mut original_doc = Document::new(width, height);
        let mut layer = Layer::new("Warstwa WebP", width, height);

        let yellow = Rgba8::from_straight(255, 255, 0, 255);
        layer.set_pixel(5, 5, yellow).unwrap();
        original_doc.add_layer(layer);

        save_image(&original_doc, &file_path).expect("zapis WebP powinien się powieść");
        let loaded_doc = load_image(&file_path).expect("odczyt WebP powinien się powieść");

        assert_eq!(loaded_doc.width, width);
        assert_eq!(loaded_doc.height, height);
        assert_eq!(loaded_doc.layer(0).unwrap().get_pixel(5, 5), Some(yellow));

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_save_and_load_jpeg() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("dziwak_test_roundtrip.jpg");

        let width = 16;
        let height = 16;
        let mut original_doc = Document::new(width, height);
        let mut layer = Layer::new("Warstwa JPEG", width, height);

        let color = Rgba8::from_straight(200, 100, 50, 255);
        for y in 0..height {
            for x in 0..width {
                layer.set_pixel(x, y, color).unwrap();
            }
        }
        original_doc.add_layer(layer);

        save_image(&original_doc, &file_path).expect("zapis JPEG powinien się powieść");
        let loaded_doc = load_image(&file_path).expect("odczyt JPEG powinien się powieść");

        assert_eq!(loaded_doc.width, width);
        assert_eq!(loaded_doc.height, height);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_dziwak_round_trip_via_io() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("dziwak_io_{}.dziwak", std::process::id()));

        let mut doc = Document::with_default_layer(20, 20, "t");
        doc.layers[0]
            .set_pixel(3, 4, Rgba8::new(10, 20, 30, 255))
            .unwrap();

        save_image(&doc, &file_path).expect("zapis Dziwaka powinien się powieść");
        let loaded_doc = load_image(&file_path).expect("odczyt Dziwaka powinien się powieść");

        assert_eq!(loaded_doc.width, 20);
        assert_eq!(loaded_doc.height, 20);
        assert_eq!(loaded_doc.layer_count(), 1);

        let pixel = loaded_doc.layer(0).unwrap().get_pixel(3, 4).unwrap();
        assert_eq!(pixel.r(), 10);
        assert_eq!(pixel.g(), 20);
        assert_eq!(pixel.b(), 30);
        assert_eq!(pixel.a(), 255);

        let _ = std::fs::remove_file(&file_path);
    }
}
