//! Pipeta.

use crate::document::Document;
use crate::pixel::Rgba8;

/// Średni kolor złożonego obrazu (premultiplied) w kwadracie o boku 2*radius+1 wokół (x, y), przyciętym do granic dokumentu.
/// Poza dokumentem zwraca Rgba8::TRANSPARENT.
pub fn sample_color(doc: &Document, x: u32, y: u32, radius: u32) -> Rgba8 {
    // Sprawdź warunki brzegowe
    if doc.width == 0 || doc.height == 0 || x >= doc.width || y >= doc.height {
        return Rgba8::TRANSPARENT;
    }

    // Oblicz granice kwadratu
    let x0 = x.saturating_sub(radius);
    let x1 = (x + radius).min(doc.width - 1);
    let y0 = y.saturating_sub(radius);
    let y1 = (y + radius).min(doc.height - 1);

    // Oblicz liczbę pikseli
    let width = x1 - x0 + 1;
    let height = y1 - y0 + 1;
    let n = width * height;

    // Jeśli nie ma pikseli, zwróć przezroczysty kolor
    if n == 0 {
        return Rgba8::TRANSPARENT;
    }

    // Sumuj kanały
    let mut sum_r: u64 = 0;
    let mut sum_g: u64 = 0;
    let mut sum_b: u64 = 0;
    let mut sum_a: u64 = 0;

    for py in y0..=y1 {
        for px in x0..=x1 {
            let pixel = doc.compose_pixel(px, py);
            sum_r += pixel.r() as u64;
            sum_g += pixel.g() as u64;
            sum_b += pixel.b() as u64;
            sum_a += pixel.a() as u64;
        }
    }

    let n = n as u64;
    // Oblicz średnią i zaokrąglij
    let avg_r = ((sum_r + n / 2) / n) as u8;
    let avg_g = ((sum_g + n / 2) / n) as u8;
    let avg_b = ((sum_b + n / 2) / n) as u8;
    let avg_a = ((sum_a + n / 2) / n) as u8;

    Rgba8::new(avg_r, avg_g, avg_b, avg_a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::pixel::Rgba8;

    #[test]
    fn test_sample_color_radius_0() {
        let mut doc = Document::with_default_layer(4, 4, "t");
        let color = Rgba8::new(255, 0, 0, 255); // Czerwony
        doc.layers[0].set_pixel(2, 2, color).unwrap();

        let sampled = sample_color(&doc, 2, 2, 0);
        assert_eq!(sampled, color);
    }

    #[test]
    fn test_sample_color_2x2_pixels() {
        let mut doc = Document::with_default_layer(4, 4, "t");
        let red = Rgba8::new(255, 0, 0, 255);
        let blue = Rgba8::new(0, 0, 255, 255);
        doc.layers[0].set_pixel(0, 0, red).unwrap();
        doc.layers[0].set_pixel(1, 0, red).unwrap();
        doc.layers[0].set_pixel(0, 1, blue).unwrap();
        doc.layers[0].set_pixel(1, 1, blue).unwrap();
        // radius 1 w rogu (0,0) obejmuje tylko 2x2 piksele
        assert_eq!(sample_color(&doc, 0, 0, 1), Rgba8::new(128, 0, 128, 255));
    }

    #[test]
    fn test_sample_color_outside_document() {
        let doc = Document::with_default_layer(4, 4, "t");

        // Spróbuj pobrać kolor poza dokumentem
        let sampled = sample_color(&doc, 10, 10, 1);
        assert_eq!(sampled, Rgba8::TRANSPARENT);
    }
}
