//! Dokument wielowarstwowy i kompozycja warstw do bufora.

use crate::blend::{
    apply_opacity_u8, blend_darken, blend_lighten, blend_multiply, blend_normal, blend_overlay,
    blend_screen, BlendMode,
};
use crate::layer::Layer;
use crate::pixel::Rgba8;
use crate::tile::TILE_SIZE;

/// Prostokątny obszar o podanych współrzędnych i wymiarach w pikselach.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    /// Współrzędna X lewego górnego rogu.
    pub x: u32,
    /// Współrzędna Y lewego górnego rogu.
    pub y: u32,
    /// Szerokość prostokąta w pikselach.
    pub width: u32,
    /// Wysokość prostokąta w pikselach.
    pub height: u32,
}

impl Rect {
    /// Tworzy nowy prostokąt o podanych współrzędnych i wymiarach.
    #[inline]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Tworzy prostokąt o zadanym rozmiarze zaczynający się w punkcie (0, 0).
    #[inline]
    pub const fn from_size(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// Zwraca pole powierzchni prostokąta (liczbę pikseli).
    #[inline]
    pub const fn area(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }
}

/// Błędy mogące wystąpić podczas kompozycji dokumentu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionError {
    /// Przekazany bufor wyjściowy ma mniejszy rozmiar niż wymagana liczba pikseli dla prostokąta.
    BufferTooSmall { expected: usize, actual: usize },
}

impl std::fmt::Display for CompositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooSmall { expected, actual } => {
                write!(
                    f,
                    "bufor wyjściowy jest za mały: oczekiwano co najmniej {} pikseli, otrzymano {}",
                    expected, actual
                )
            }
        }
    }
}

impl std::error::Error for CompositionError {}

/// Dokument graficzny składający się z warstw ułożonych w stos.
///
/// Warstwa o indeksie 0 znajduje się na samym dole stosu (tło),
/// a warstwy o kolejnych indeksach są nakładane na nią (od dołu do góry).
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    /// Szerokość dokumentu w pikselach.
    pub width: u32,
    /// Wysokość dokumentu w pikselach.
    pub height: u32,
    /// Lista warstw w kolejności od najniższej do najwyższej.
    pub layers: Vec<Layer>,
}

impl Document {
    /// Tworzy nowy pusty dokument o zadanych wymiarach bez warstw.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            layers: Vec::new(),
        }
    }

    /// Tworzy nowy dokument z jedną początkową warstwą o podanej nazwie.
    pub fn with_default_layer(width: u32, height: u32, name: impl Into<String>) -> Self {
        let mut doc = Self::new(width, height);
        doc.add_layer(Layer::new(name, width, height));
        doc
    }

    /// Dodaje warstwę na samą górę stosu warstw.
    pub fn add_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    /// Wstawia warstwę pod wskazany indeks na stosie.
    /// Zwraca błąd, jeśli indeks przekracza bieżącą liczbę warstw.
    pub fn insert_layer(&mut self, index: usize, layer: Layer) -> Result<(), usize> {
        if index <= self.layers.len() {
            self.layers.insert(index, layer);
            Ok(())
        } else {
            Err(index)
        }
    }

    /// Usuwa warstwę o wskazanym indeksie i zwraca ją.
    pub fn remove_layer(&mut self, index: usize) -> Option<Layer> {
        if index < self.layers.len() {
            Some(self.layers.remove(index))
        } else {
            None
        }
    }

    /// Zwraca referencję do warstwy o podanym indeksie.
    pub fn layer(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }

    /// Zwraca mutowalną referencję do warstwy o podanym indeksie.
    pub fn layer_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }

    /// Zwraca liczbę warstw w dokumencie.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Przycina dokument (wszystkie warstwy) do prostokąta `rect` przyciętego do granic dokumentu.
    pub fn crop(&mut self, rect: Rect) {
        let x0 = rect.x.min(self.width);
        let y0 = rect.y.min(self.height);
        let x1 = rect.x.saturating_add(rect.width).min(self.width);
        let y1 = rect.y.saturating_add(rect.height).min(self.height);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (nw, nh) = (x1 - x0, y1 - y0);
        for layer in &mut self.layers {
            let mut np = crate::layer::TiledLayer::new(nw, nh);
            for y in 0..nh {
                for x in 0..nw {
                    if let Some(px) = layer.pixels.get_pixel(x0 + x, y0 + y) {
                        if px.a() > 0 {
                            let _ = np.set_pixel(x, y, px);
                        }
                    }
                }
            }
            layer.pixels = np;
        }
        self.width = nw;
        self.height = nh;
    }

    /// Zmienia rozmiar płótna na (w, h), przesuwając zawartość o (off_x, off_y); piksele poza płótnem są tracone.
    pub fn resize_canvas(&mut self, w: u32, h: u32, off_x: i32, off_y: i32) {
        if w == 0 || h == 0 {
            return;
        }
        for layer in &mut self.layers {
            let mut np = crate::layer::TiledLayer::new(w, h);
            for y in 0..layer.pixels.height {
                for x in 0..layer.pixels.width {
                    let Some(px) = layer.pixels.get_pixel(x, y) else {
                        continue;
                    };
                    if px.a() == 0 {
                        continue;
                    }
                    let nx = x as i64 + off_x as i64;
                    let ny = y as i64 + off_y as i64;
                    if nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 {
                        let _ = np.set_pixel(nx as u32, ny as u32, px);
                    }
                }
            }
            layer.pixels = np;
        }
        self.width = w;
        self.height = h;
    }

    /// Scala warstwę `index` z warstwą pod nią (index - 1) zgodnie z jej kryciem, widocznością i trybem mieszania;
    /// wynik zastępuje dolną warstwę, górna jest usuwana. Zwraca false, gdy scalenie jest niemożliwe (index == 0 lub poza zakresem).
    pub fn merge_down(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.layers.len() {
            return false;
        }
        let (w, h) = (self.width, self.height);
        let mut tmp = Document::new(w, h);
        let mut lower = self.layers[index - 1].clone();
        let lower_opacity = lower.opacity;
        let lower_visible = lower.visible;
        lower.opacity = 1.0;
        lower.visible = true;
        lower.blend = BlendMode::Normal;
        tmp.layers.push(lower);
        tmp.layers.push(self.layers[index].clone());
        let mut buf = vec![Rgba8::TRANSPARENT; (w as usize) * (h as usize)];
        let _ = tmp.composite_rect(0, 0, w, h, &mut buf);
        let mut merged = self.layers[index - 1].clone();
        merged.pixels = crate::layer::TiledLayer::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let px = buf[(y * w + x) as usize];
                if px.a() > 0 {
                    let _ = merged.pixels.set_pixel(x, y, px);
                }
            }
        }
        merged.opacity = lower_opacity;
        merged.visible = lower_visible;
        self.layers[index - 1] = merged;
        self.layers.remove(index);
        true
    }

    /// Maska (w*h) z kanału alfa warstwy `index` — do polecenia „Alfa do zaznaczenia”. Pusta gdy indeks poza zakresem.
    pub fn alpha_mask(&self, index: usize) -> Vec<u8> {
        let Some(layer) = self.layers.get(index) else {
            return Vec::new();
        };
        let (w, h) = (self.width, self.height);
        let mut mask = vec![0u8; (w as usize) * (h as usize)];
        for y in 0..h {
            for x in 0..w {
                if let Some(px) = layer.pixels.get_pixel(x, y) {
                    mask[(y * w + x) as usize] = px.a();
                }
            }
        }
        mask
    }

    /// Skaluje cały dokument (wszystkie warstwy) do wymiarów (new_w, new_h) z interpolacją dwuliniową.
    pub fn scale(&mut self, new_w: u32, new_h: u32) {
        if new_w == 0 || new_h == 0 || self.width == 0 || self.height == 0 {
            return;
        }
        for layer in &mut self.layers {
            let buf = layer.pixels.to_vec();
            let resampled = crate::transform::resample_bilinear(
                &buf,
                self.width as usize,
                self.height as usize,
                new_w as usize,
                new_h as usize,
            );
            layer.pixels = crate::layer::TiledLayer::from_buffer(&resampled, new_w, new_h);
        }
        self.width = new_w;
        self.height = new_h;
    }

    /// Obraca cały dokument (wszystkie warstwy) o 90 stopni zgodnie z ruchem wskazówek zegara.
    pub fn rotate90_cw(&mut self) {
        for layer in &mut self.layers {
            layer.pixels = crate::transform::rotate90_cw(&layer.pixels);
        }
        std::mem::swap(&mut self.width, &mut self.height);
    }

    /// Obraca cały dokument (wszystkie warstwy) o 90 stopni przeciwnie do ruchu wskazówek zegara.
    pub fn rotate90_ccw(&mut self) {
        for layer in &mut self.layers {
            layer.pixels = crate::transform::rotate90_ccw(&layer.pixels);
        }
        std::mem::swap(&mut self.width, &mut self.height);
    }

    /// Obraca cały dokument (wszystkie warstwy) o 180 stopni.
    pub fn rotate180(&mut self) {
        for layer in &mut self.layers {
            layer.pixels = crate::transform::rotate180(&layer.pixels);
        }
    }

    /// Odbija cały dokument (wszystkie warstwy) w poziomie.
    pub fn flip_horizontal(&mut self) {
        for layer in &mut self.layers {
            layer.pixels = crate::transform::flip_horizontal(&layer.pixels);
        }
    }

    /// Odbija cały dokument (wszystkie warstwy) w pionie.
    pub fn flip_vertical(&mut self) {
        for layer in &mut self.layers {
            layer.pixels = crate::transform::flip_vertical(&layer.pixels);
        }
    }

    /// Sprawdza, czy dokument nie posiada żadnych warstw.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Zwraca wynik kompozycji wszystkich widocznych warstw dla pojedynczego piksela (x, y).
    pub fn compose_pixel(&self, x: u32, y: u32) -> Rgba8 {
        let mut buf = [Rgba8::TRANSPARENT; 1];
        let _ = self.composite_rect(x, y, 1, 1, &mut buf);
        buf[0]
    }

    /// Składa widoczne warstwy dla zadanego prostokąta `rect` do bufora `out_buffer`.
    #[inline]
    pub fn compose_rect(
        &self,
        rect: Rect,
        out_buffer: &mut [Rgba8],
    ) -> Result<(), CompositionError> {
        self.composite_rect(rect.x, rect.y, rect.width, rect.height, out_buffer)
    }

    /// Składa widoczne warstwy dla zadanego prostokąta `(x, y, w, h)` do bufora `out`.
    ///
    /// - Brak alokacji w pętli.
    /// - Iteracja tylko po kaflach 64×64 przecinających prostokąt.
    /// - Puste kafle (`None`) są natychmiast pomijane.
    /// - Warstwy są nakładane w kolejności od dołu do góry.
    /// - Pomijane są warstwy z `visible == false` oraz `opacity <= 0.0`.
    /// - Mieszanie Porter-Duff 'over' dla premultiplied alpha z zaokrąglaniem bez `f32` w pętli pikseli.
    pub fn composite_rect(
        &self,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        out: &mut [Rgba8],
    ) -> Result<(), CompositionError> {
        let expected_len = (w as usize) * (h as usize);
        if out.len() < expected_len {
            return Err(CompositionError::BufferTooSmall {
                expected: expected_len,
                actual: out.len(),
            });
        }

        if w == 0 || h == 0 {
            return Ok(());
        }

        // Czyścimy żądany obszar bufora do przezroczystości
        out[..expected_len].fill(Rgba8::TRANSPARENT);

        if self.width == 0 || self.height == 0 || x >= self.width || y >= self.height {
            return Ok(());
        }

        let tile_size = TILE_SIZE as u32;
        let max_x = (x + w - 1).min(self.width - 1);
        let max_y = (y + h - 1).min(self.height - 1);

        let min_tx = x / tile_size;
        let max_tx = max_x / tile_size;
        let min_ty = y / tile_size;
        let max_ty = max_y / tile_size;

        // Przetwarzamy kafel po kaflu (L1 cache) bez alokacji pamięci na stercie
        for ty in min_ty..=max_ty {
            let tile_y0 = ty * tile_size;
            let inter_y0 = tile_y0.max(y);
            let inter_y1 = (tile_y0 + tile_size).min(y + h).min(self.height);
            if inter_y0 >= inter_y1 {
                continue;
            }

            for tx in min_tx..=max_tx {
                let tile_x0 = tx * tile_size;
                let inter_x0 = tile_x0.max(x);
                let inter_x1 = (tile_x0 + tile_size).min(x + w).min(self.width);
                if inter_x0 >= inter_x1 {
                    continue;
                }

                // Warstwy od dołu (indeks 0) do góry
                for layer in &self.layers {
                    if !layer.visible || layer.opacity <= 0.0 {
                        continue;
                    }

                    let opacity_u8 = (layer.opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
                    if opacity_u8 == 0 {
                        continue;
                    }

                    // Puste kafle (None) są pomijane bez przetwarzania pikseli
                    let tile = match layer.pixels.tile_at(tx, ty) {
                        Some(t) => t,
                        None => continue,
                    };

                    for py in inter_y0..inter_y1 {
                        let ly = (py - tile_y0) as usize;
                        let buf_y = (py - y) as usize;
                        let row_buf_start = buf_y * (w as usize);
                        let row_tile_start = ly * TILE_SIZE;

                        for px in inter_x0..inter_x1 {
                            let lx = (px - tile_x0) as usize;
                            let buf_x = (px - x) as usize;
                            let buf_idx = row_buf_start + buf_x;
                            let tile_pixel_idx = row_tile_start + lx;

                            let src_raw = tile[tile_pixel_idx];
                            if src_raw.is_transparent() {
                                continue;
                            }

                            let src = if opacity_u8 >= 255 {
                                src_raw
                            } else {
                                apply_opacity_u8(src_raw, opacity_u8)
                            };

                            if src.is_transparent() {
                                continue;
                            }

                            out[buf_idx] = match layer.blend {
                                BlendMode::Normal => blend_normal(out[buf_idx], src),
                                BlendMode::Multiply => blend_multiply(out[buf_idx], src),
                                BlendMode::Screen => blend_screen(out[buf_idx], src),
                                BlendMode::Darken => blend_darken(out[buf_idx], src),
                                BlendMode::Lighten => blend_lighten(out[buf_idx], src),
                                BlendMode::Overlay => blend_overlay(out[buf_idx], src),
                            };
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_methods() {
        let r = Rect::new(10, 20, 30, 40);
        assert_eq!(r.x, 10);
        assert_eq!(r.y, 20);
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 40);
        assert_eq!(r.area(), 1200);

        let r_size = Rect::from_size(50, 60);
        assert_eq!(r_size.x, 0);
        assert_eq!(r_size.y, 0);
        assert_eq!(r_size.width, 50);
        assert_eq!(r_size.height, 60);
        assert_eq!(r_size.area(), 3000);
    }

    #[test]
    fn test_composition_error_display() {
        let err = CompositionError::BufferTooSmall {
            expected: 100,
            actual: 50,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("100"));
        assert!(msg.contains("50"));
    }

    #[test]
    fn test_document_new_and_layers() {
        let mut doc = Document::new(800, 600);
        assert_eq!(doc.width, 800);
        assert_eq!(doc.height, 600);
        assert_eq!(doc.layer_count(), 0);
        assert!(doc.is_empty());

        let layer1 = Layer::new("Warstwa 1", 800, 600);
        doc.add_layer(layer1);
        assert_eq!(doc.layer_count(), 1);
        assert!(!doc.is_empty());
        assert_eq!(doc.layer(0).map(|l| l.name.as_str()), Some("Warstwa 1"));

        let layer0 = Layer::new("Tło", 800, 600);
        assert!(doc.insert_layer(0, layer0).is_ok());
        assert_eq!(doc.layer_count(), 2);
        assert_eq!(doc.layer(0).map(|l| l.name.as_str()), Some("Tło"));
        assert_eq!(doc.layer(1).map(|l| l.name.as_str()), Some("Warstwa 1"));

        assert!(doc.insert_layer(10, Layer::new("Zły", 800, 600)).is_err());

        if let Some(l) = doc.layer_mut(1) {
            l.name = "Warstwa Zmieniona".to_string();
        }
        assert_eq!(
            doc.layer(1).map(|l| l.name.as_str()),
            Some("Warstwa Zmieniona")
        );

        let removed = doc.remove_layer(0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, "Tło");
        assert_eq!(doc.layer_count(), 1);

        assert!(doc.remove_layer(5).is_none());
    }

    #[test]
    fn test_document_with_default_layer() {
        let doc = Document::with_default_layer(640, 480, "Tło");
        assert_eq!(doc.width, 640);
        assert_eq!(doc.height, 480);
        assert_eq!(doc.layer_count(), 1);
        assert_eq!(doc.layer(0).map(|l| l.name.as_str()), Some("Tło"));
    }

    #[test]
    fn test_composite_rect_buffer_too_small() {
        let doc = Document::new(100, 100);
        let mut buf = [Rgba8::TRANSPARENT; 10];
        let result = doc.composite_rect(0, 0, 10, 10, &mut buf);
        assert_eq!(
            result,
            Err(CompositionError::BufferTooSmall {
                expected: 100,
                actual: 10
            })
        );
    }

    // (5.1) Test: pusta warstwa
    #[test]
    fn test_composite_rect_empty_layer() {
        let doc = Document::with_default_layer(100, 100, "Pusta");
        assert_eq!(doc.layer(0).unwrap().pixels.count_allocated_tiles(), 0);

        let mut buf = [Rgba8::WHITE; 16];
        assert!(doc.composite_rect(0, 0, 4, 4, &mut buf).is_ok());
        // Wszystkie piksele powinny pozostać przezroczyste
        assert!(buf.iter().all(|px| px.is_transparent()));
    }

    // (5.2) Test: dwie nieprzezroczyste warstwy
    #[test]
    fn test_composite_rect_two_opaque_layers() {
        let mut doc = Document::new(100, 100);

        let mut bottom = Layer::new("Dół (Niebieski)", 100, 100);
        let blue = Rgba8::new(0, 0, 255, 255);
        bottom
            .set_pixel(10, 10, blue)
            .expect("poprawne współrzędne");
        doc.add_layer(bottom);

        let mut top = Layer::new("Góra (Czerwony)", 100, 100);
        let red = Rgba8::new(255, 0, 0, 255);
        top.set_pixel(10, 10, red).expect("poprawne współrzędne");
        doc.add_layer(top);

        let mut buf = [Rgba8::TRANSPARENT; 1];
        assert!(doc.composite_rect(10, 10, 1, 1, &mut buf).is_ok());

        // Górna nieprzezroczysta warstwa całkowicie przykrywa dolną
        assert_eq!(buf[0], red);
    }

    // (5.3) Test: półprzezroczysta warstwa z opacity
    #[test]
    fn test_composite_rect_semi_transparent_with_opacity() {
        let mut doc = Document::new(100, 100);

        // Dolna warstwa: biała nieprzezroczysta [255, 255, 255, 255]
        let mut bottom = Layer::new("Białe Tło", 100, 100);
        bottom
            .set_pixel(5, 5, Rgba8::WHITE)
            .expect("poprawne współrzędne");
        doc.add_layer(bottom);

        // Górna warstwa: czarna [0, 0, 0, 255] z kryciem 50%
        let mut top = Layer::new("Czarny 50%", 100, 100);
        top.opacity = 0.5;
        top.set_pixel(5, 5, Rgba8::BLACK)
            .expect("poprawne współrzędne");
        doc.add_layer(top);

        let mut buf = [Rgba8::TRANSPARENT; 1];
        assert!(doc.composite_rect(5, 5, 1, 1, &mut buf).is_ok());

        // Wynik: szary [127, 127, 127, 255] (50.2% czerni + 49.8% bieli)
        let px = buf[0];
        assert_eq!(px.a(), 255);
        assert_eq!(px.r(), 127);
        assert_eq!(px.g(), 127);
        assert_eq!(px.b(), 127);
    }

    // (5.4) Test: niewidoczna warstwa
    #[test]
    fn test_composite_rect_invisible_layer() {
        let mut doc = Document::new(100, 100);

        let mut bottom = Layer::new("Dół", 100, 100);
        let red = Rgba8::new(255, 0, 0, 255);
        bottom.set_pixel(10, 10, red).expect("poprawne współrzędne");
        doc.add_layer(bottom);

        let mut top = Layer::new("Niewidoczna Góra", 100, 100);
        let green = Rgba8::new(0, 255, 0, 255);
        top.visible = false;
        top.set_pixel(10, 10, green).expect("poprawne współrzędne");
        doc.add_layer(top);

        let mut buf = [Rgba8::TRANSPARENT; 1];
        assert!(doc.composite_rect(10, 10, 1, 1, &mut buf).is_ok());

        // Niewidoczna warstwa jest ignorowana — pozostaje czerwony
        assert_eq!(buf[0], red);
    }

    // (5.5) Test: prostokąt na granicy kafli (np. x=60, w=10)
    #[test]
    fn test_composite_rect_across_tile_boundary() {
        let mut doc = Document::with_default_layer(128, 64, "Warstwa");
        let red = Rgba8::new(255, 0, 0, 255);
        let blue = Rgba8::new(0, 0, 255, 255);

        // Kafel 0 kończy się na x=63, Kafel 1 zaczyna się od x=64
        // Ustawiamy piksel po lewej stronie granicy (w kaflu 0)
        doc.layer_mut(0)
            .unwrap()
            .set_pixel(62, 5, red)
            .expect("poprawne");
        // Ustawiamy piksel po prawej stronie granicy (w kaflu 1)
        doc.layer_mut(0)
            .unwrap()
            .set_pixel(66, 5, blue)
            .expect("poprawne");

        // Prostokąt przecinający granicę kafli: x=60..70 (w=10), y=5..6 (h=1)
        let mut buf = [Rgba8::TRANSPARENT; 10];
        assert!(doc.composite_rect(60, 5, 10, 1, &mut buf).is_ok());

        // x=62 to przesunięcie 2 od początku prostokąta x=60
        assert_eq!(buf[2], red);
        // x=66 to przesunięcie 6 od początku prostokąta x=60
        assert_eq!(buf[6], blue);
        // Pozostałe piksele powinny być przezroczyste
        assert_eq!(buf[0], Rgba8::TRANSPARENT);
        assert_eq!(buf[1], Rgba8::TRANSPARENT);
        assert_eq!(buf[3], Rgba8::TRANSPARENT);
        assert_eq!(buf[4], Rgba8::TRANSPARENT);
        assert_eq!(buf[5], Rgba8::TRANSPARENT);
        assert_eq!(buf[7], Rgba8::TRANSPARENT);
        assert_eq!(buf[8], Rgba8::TRANSPARENT);
        assert_eq!(buf[9], Rgba8::TRANSPARENT);
    }

    #[test]
    fn test_compose_pixel_and_compose_rect_helper() {
        let mut doc = Document::with_default_layer(100, 100, "Warstwa");
        let col = Rgba8::new(40, 80, 120, 200);
        doc.layer_mut(0)
            .unwrap()
            .set_pixel(15, 25, col)
            .expect("poprawne");

        let px = doc.compose_pixel(15, 25);
        assert_eq!(px, col);

        let mut buf = [Rgba8::TRANSPARENT; 1];
        doc.compose_rect(Rect::new(15, 25, 1, 1), &mut buf).unwrap();
        assert_eq!(buf[0], col);

        assert_eq!(doc.compose_pixel(100, 100), Rgba8::TRANSPARENT);
    }

    #[test]
    fn test_composite_rect_outside_bounds_and_zero_size() {
        let doc = Document::new(100, 100);
        let mut buf = [Rgba8::WHITE; 16];
        assert!(doc.composite_rect(200, 200, 4, 4, &mut buf).is_ok());
        assert!(buf.iter().all(|px| px.is_transparent()));

        let mut zero_buf = [];
        assert!(doc.composite_rect(0, 0, 0, 0, &mut zero_buf).is_ok());
    }

    #[test]
    fn test_crop_and_resize_canvas() {
        let mut doc = Document::with_default_layer(100, 80, "t");
        let red = Rgba8::new(255, 0, 0, 255);
        doc.layers[0].set_pixel(60, 50, red).unwrap();
        doc.crop(Rect::new(50, 40, 20, 20));
        assert_eq!((doc.width, doc.height), (20, 20));
        assert_eq!(doc.layers[0].get_pixel(10, 10), Some(red));
        doc.crop(Rect::new(500, 500, 10, 10));
        assert_eq!((doc.width, doc.height), (20, 20));
        doc.resize_canvas(40, 30, 5, 3);
        assert_eq!((doc.width, doc.height), (40, 30));
        assert_eq!(doc.layers[0].get_pixel(15, 13), Some(red));
    }

    #[test]
    fn test_document_transformations() {
        let mut doc = Document::with_default_layer(10, 20, "L1");
        doc.add_layer(Layer::new("L2", 10, 20));
        let red = Rgba8::new(255, 0, 0, 255);
        let blue = Rgba8::new(0, 0, 255, 255);
        doc.layers[0].set_pixel(2, 4, red).unwrap();
        doc.layers[1].set_pixel(2, 4, blue).unwrap();

        // flip_horizontal
        doc.flip_horizontal();
        assert_eq!(doc.width, 10);
        assert_eq!(doc.height, 20);
        assert_eq!(doc.layers[0].get_pixel(7, 4), Some(red));
        assert_eq!(doc.layers[1].get_pixel(7, 4), Some(blue));

        // flip_vertical
        doc.flip_vertical();
        assert_eq!(doc.layers[0].get_pixel(7, 15), Some(red));
        assert_eq!(doc.layers[1].get_pixel(7, 15), Some(blue));

        // rotate180
        doc.rotate180();
        assert_eq!(doc.layers[0].get_pixel(2, 4), Some(red));
        assert_eq!(doc.layers[1].get_pixel(2, 4), Some(blue));

        // rotate90_cw: 10x20 -> 20x10. (2, 4) -> (20 - 1 - 4, 2) = (15, 2)
        doc.rotate90_cw();
        assert_eq!(doc.width, 20);
        assert_eq!(doc.height, 10);
        assert_eq!(doc.layers[0].get_pixel(15, 2), Some(red));
        assert_eq!(doc.layers[1].get_pixel(15, 2), Some(blue));

        // rotate90_ccw: 20x10 -> 10x20. (15, 2) -> (2, 4)
        doc.rotate90_ccw();
        assert_eq!(doc.width, 10);
        assert_eq!(doc.height, 20);
        assert_eq!(doc.layers[0].get_pixel(2, 4), Some(red));
        assert_eq!(doc.layers[1].get_pixel(2, 4), Some(blue));

        // scale: 10x20 -> 20x40
        doc.scale(20, 40);
        assert_eq!(doc.width, 20);
        assert_eq!(doc.height, 40);
        // Piksel powinien być przeskalowany w okolice (4..5, 8..9)
        let px0 = doc.layers[0].get_pixel(4, 8).unwrap();
        assert!(px0.a() > 0);
    }

    #[test]
    fn test_merge_down_and_alpha_mask() {
        let mut doc = Document::with_default_layer(4, 4, "d");
        doc.add_layer(Layer::new("g", 4, 4));
        let red = Rgba8::new(255, 0, 0, 255);
        doc.layers[1].set_pixel(1, 1, red).unwrap();

        // Test merge_down
        assert_eq!(doc.layer_count(), 2);
        assert!(doc.merge_down(1));
        assert_eq!(doc.layer_count(), 1);
        assert_eq!(doc.layers[0].get_pixel(1, 1), Some(red));

        // Test merge_down on invalid index
        assert!(!doc.merge_down(0));

        // Test alpha_mask
        let mask = doc.alpha_mask(0);
        assert_eq!(mask[4 + 1], 255);
    }
}
