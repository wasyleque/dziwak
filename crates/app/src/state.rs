use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use dziwak_core::{
    apply_dab, create_filled_tile, BrushMode, Document, History, Layer, Rect, Rgba8, Selection,
    TiledLayer, TILE_PIXELS, TILE_SIZE,
};
use eframe::egui;

/// Wynik operacji systemowego okna dialogowego (wykonywanego asynchronicznie w osobnym wątku).
pub enum DialogResult {
    /// Żądanie otwarcia pliku ze wskazanej ścieżki (w nowej zakładce).
    Open(PathBuf),
    /// Żądanie zapisu pliku pod wskazaną ścieżkę dla zakładki o podanym `tab_id`.
    Save { tab_id: u64, path: PathBuf },
    /// Anulowanie wyboru pliku do zapisu dla zakładki o podanym `tab_id`.
    SaveCancelled { tab_id: u64 },
}

/// Decyzja przy próbie zamknięcia okna aplikacji.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDecision {
    /// Pozwól na natychmiastowe zamknięcie okna (brak niezapisanych zmian lub zamknięcie potwierdzone).
    Allow,
    /// Wstrzymaj zamknięcie i zapytaj użytkownika o zapisanie zmian.
    AskUser,
}

/// Transformacja widoku płótna: przesunięcie i skala (zoom).
#[derive(Clone, Copy, Debug)]
pub struct ViewTransform {
    /// Pozycja punktu (0, 0) płótna na ekranie (w pikselach okna).
    pub pan: egui::Vec2,
    /// Współczynnik powiększenia (1.0 = 100%).
    pub zoom: f32,
}

/// Alias typu dla transformacji widoku (dla wstecznej kompatybilności).
#[allow(dead_code)]
pub type CanvasTransform = ViewTransform;

impl ViewTransform {
    pub fn new(pan: egui::Vec2, zoom: f32) -> Self {
        Self { pan, zoom }
    }

    /// Konwertuje punkt z przestrzeni płótna na współrzędne ekranowe.
    pub fn canvas_to_screen(&self, canvas_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            self.pan.x + canvas_pos.x * self.zoom,
            self.pan.y + canvas_pos.y * self.zoom,
        )
    }

    /// Konwertuje współrzędne ekranowe na punkt w przestrzeni płótna.
    pub fn screen_to_canvas(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (screen_pos.x - self.pan.x) / self.zoom,
            (screen_pos.y - self.pan.y) / self.zoom,
        )
    }

    /// Zmienia powiększenie wokół zadanego punktu ekranowego (np. pod kursorem myszy).
    pub fn zoom_around(
        &mut self,
        cursor_pos: egui::Pos2,
        factor: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let new_zoom = (self.zoom * factor).clamp(min_zoom, max_zoom);
        let actual_factor = new_zoom / self.zoom;
        let cursor_vec = cursor_pos.to_vec2();
        self.pan = cursor_vec - (cursor_vec - self.pan) * actual_factor;
        self.zoom = new_zoom;
    }
}

/// Zarządza teksturami GPU dla siatki kafli dokumentu i kompozycją zmienionych (brudnych) kafli.
pub struct TileRenderer {
    /// Tekstury kafli na GPU (jeden slot na kafel, indeksowany `ty * tiles_across + tx`).
    pub textures: Vec<Option<egui::TextureHandle>>,
    /// Zbiór indeksów kafli wymagających rekompozycji i przesłania na GPU.
    pub dirty_tiles: BTreeSet<usize>,
    /// Bufor wielokrotnego użytku dla pojedynczego kafla (eliminacja alokacji w pętli renderowania).
    pub tile_buffer: [Rgba8; TILE_PIXELS],
    /// Bufor pikseli egui wielokrotnego użytku.
    pub color_buffer: Vec<egui::Color32>,
    /// Liczba kolumn kafli w siatce dokumentu.
    pub tiles_across: u32,
    /// Liczba wierszy kafli w siatce dokumentu.
    pub tiles_down: u32,
    /// Opcje tekstury (LINEAR przy zoom < 1.0, NEAREST w przeciwnym przypadku).
    pub texture_options: egui::TextureOptions,
}

impl TileRenderer {
    /// Tworzy nowy renderer dla dokumentu o zadanych wymiarach w pikselach.
    pub fn new(doc_width: u32, doc_height: u32) -> Self {
        let tile_size = TILE_SIZE as u32;
        let tiles_across = if doc_width == 0 {
            0
        } else {
            (doc_width - 1) / tile_size + 1
        };
        let tiles_down = if doc_height == 0 {
            0
        } else {
            (doc_height - 1) / tile_size + 1
        };
        let total = (tiles_across * tiles_down) as usize;
        let mut textures = Vec::with_capacity(total);
        textures.resize_with(total, || None);

        let dirty_tiles = (0..total).collect();

        Self {
            textures,
            dirty_tiles,
            tile_buffer: [Rgba8::TRANSPARENT; TILE_PIXELS],
            color_buffer: Vec::with_capacity(TILE_PIXELS),
            tiles_across,
            tiles_down,
            texture_options: egui::TextureOptions::NEAREST,
        }
    }

    /// Oznacza kafel (tx, ty) jako wymagający rekompozycji i ponownego wysłania na GPU.
    pub fn mark_tile_dirty(&mut self, tx: u32, ty: u32) {
        if tx < self.tiles_across && ty < self.tiles_down {
            let idx = (ty * self.tiles_across + tx) as usize;
            self.dirty_tiles.insert(idx);
        }
    }

    /// Dobiera filtrowanie tekstur do powiększenia: LINEAR przy zoom < 1.0, NEAREST w pozostałych przypadkach.
    pub fn set_zoom(&mut self, zoom: f32) {
        let wanted = if zoom < 1.0 {
            egui::TextureOptions::LINEAR
        } else {
            egui::TextureOptions::NEAREST
        };
        if wanted != self.texture_options {
            self.texture_options = wanted;
            self.mark_all_dirty();
        }
    }

    /// Oznacza wszystkie kafle jako brudne (np. po wczytaniu nowego dokumentu).
    pub fn mark_all_dirty(&mut self) {
        let total = (self.tiles_across * self.tiles_down) as usize;
        self.dirty_tiles = (0..total).collect();
    }

    /// Zwalnia wszystkie uchwyty tekstur GPU kafli i oznacza wszystkie kafle jako brudne
    /// (używane przy przejściu zakładki w tło w celu oszczędzania pamięci GPU/RAM).
    pub fn release_textures(&mut self) {
        for slot in &mut self.textures {
            *slot = None;
        }
        self.mark_all_dirty();
    }

    /// Wyznacza zakres indeksów kafli `(min_tx, max_tx, min_ty, max_ty)` przecinających `viewport_rect`.
    /// Zwraca `None`, jeśli dokument jest pusty lub całkowicie poza widocznym obszarem.
    pub fn visible_tile_range(
        &self,
        transform: &ViewTransform,
        doc_width: u32,
        doc_height: u32,
        viewport_rect: egui::Rect,
    ) -> Option<(u32, u32, u32, u32)> {
        if self.tiles_across == 0
            || self.tiles_down == 0
            || doc_width == 0
            || doc_height == 0
            || viewport_rect.width() <= 0.0
            || viewport_rect.height() <= 0.0
        {
            return None;
        }

        let tile_size = TILE_SIZE as f32;
        let vis_canvas_min = transform.screen_to_canvas(viewport_rect.min);
        let vis_canvas_max = transform.screen_to_canvas(viewport_rect.max);

        let canvas_w = doc_width as f32;
        let canvas_h = doc_height as f32;

        let raw_min_x = vis_canvas_min.x.min(vis_canvas_max.x);
        let raw_max_x = vis_canvas_min.x.max(vis_canvas_max.x);
        let raw_min_y = vis_canvas_min.y.min(vis_canvas_max.y);
        let raw_max_y = vis_canvas_min.y.max(vis_canvas_max.y);

        if raw_max_x <= 0.0 || raw_min_x >= canvas_w || raw_max_y <= 0.0 || raw_min_y >= canvas_h {
            return None;
        }

        let min_x = raw_min_x.clamp(0.0, canvas_w);
        let max_x = raw_max_x.clamp(0.0, canvas_w);
        let min_y = raw_min_y.clamp(0.0, canvas_h);
        let max_y = raw_max_y.clamp(0.0, canvas_h);

        let min_tx = ((min_x / tile_size).floor() as u32).min(self.tiles_across.saturating_sub(1));
        let max_tx = ((max_x / tile_size).ceil() as u32)
            .saturating_sub(1)
            .max(min_tx)
            .min(self.tiles_across.saturating_sub(1));
        let min_ty = ((min_y / tile_size).floor() as u32).min(self.tiles_down.saturating_sub(1));
        let max_ty = ((max_y / tile_size).ceil() as u32)
            .saturating_sub(1)
            .max(min_ty)
            .min(self.tiles_down.saturating_sub(1));

        Some((min_tx, max_tx, min_ty, max_ty))
    }

    /// Aktualizuje pojedynczy kafel o indeksie `tile_idx` na GPU.
    fn upload_single_tile(&mut self, ctx: &egui::Context, doc: &Document, tile_idx: usize) {
        if tile_idx >= self.textures.len() || self.tiles_across == 0 {
            return;
        }

        let tile_size = TILE_SIZE as u32;
        let tx = (tile_idx as u32) % self.tiles_across;
        let ty = (tile_idx as u32) / self.tiles_across;

        let x = tx * tile_size;
        let y = ty * tile_size;

        // Składamy warstwy dla pojedynczego kafla do prealokowanego bufora
        let _ = doc.composite_rect(x, y, tile_size, tile_size, &mut self.tile_buffer);

        // Sprawdzamy czy kafel zawiera jakiekolwiek piksele nieprzezroczyste
        let is_empty = self.tile_buffer.iter().all(|px| px.is_transparent());

        if is_empty {
            // Pusty kafel nie zajmuje pamięci tekstury GPU
            self.textures[tile_idx] = None;
        } else {
            self.color_buffer.clear();
            self.color_buffer.extend(
                self.tile_buffer.iter().map(|px| {
                    egui::Color32::from_rgba_premultiplied(px.r(), px.g(), px.b(), px.a())
                }),
            );

            let color_image = egui::ColorImage {
                size: [TILE_SIZE, TILE_SIZE],
                source_size: egui::vec2(TILE_SIZE as f32, TILE_SIZE as f32),
                pixels: self.color_buffer.clone(),
            };

            match &mut self.textures[tile_idx] {
                Some(handle) => {
                    handle.set(color_image, self.texture_options);
                }
                None => {
                    let name = format!("tile_{}_{}", tx, ty);
                    let handle = ctx.load_texture(name, color_image, self.texture_options);
                    self.textures[tile_idx] = Some(handle);
                }
            }
        }
    }

    /// Aktualizuje tekstury GPU dla wszystkich brudnych kafli.
    #[allow(dead_code)]
    pub fn update_dirty_tiles(&mut self, ctx: &egui::Context, doc: &Document) {
        if self.dirty_tiles.is_empty() {
            return;
        }

        let dirty: Vec<usize> = self.dirty_tiles.iter().copied().collect();
        self.dirty_tiles.clear();
        for tile_idx in dirty {
            self.upload_single_tile(ctx, doc, tile_idx);
        }
    }

    /// Aktualizuje tekstury GPU wyłącznie dla tych kafli, które są jednocześnie brudne i widoczne w `viewport_rect`.
    /// Kafle brudne znajdujące się poza widokiem pozostają w zbiorze `dirty_tiles` do momentu przewinięcia widoku.
    pub fn update_visible_dirty_tiles(
        &mut self,
        ctx: &egui::Context,
        doc: &Document,
        transform: &ViewTransform,
        viewport_rect: egui::Rect,
    ) {
        if self.dirty_tiles.is_empty() {
            return;
        }
        let Some((min_tx, max_tx, min_ty, max_ty)) =
            self.visible_tile_range(transform, doc.width, doc.height, viewport_rect)
        else {
            return;
        };

        for ty in min_ty..=max_ty {
            for tx in min_tx..=max_tx {
                let tile_idx = (ty * self.tiles_across + tx) as usize;
                if self.dirty_tiles.remove(&tile_idx) {
                    self.upload_single_tile(ctx, doc, tile_idx);
                }
            }
        }
    }

    /// Rysuje na płótnie wyłącznie te kafle, które przecinają bieżący prostokąt widoku.
    pub fn draw_visible_tiles(
        &self,
        painter: &egui::Painter,
        transform: &ViewTransform,
        doc_width: u32,
        doc_height: u32,
        viewport_rect: egui::Rect,
        show_tile_grid: bool,
    ) {
        let Some((min_tx, max_tx, min_ty, max_ty)) =
            self.visible_tile_range(transform, doc_width, doc_height, viewport_rect)
        else {
            return;
        };

        let tile_size = TILE_SIZE as f32;
        let canvas_w = doc_width as f32;
        let canvas_h = doc_height as f32;

        for ty in min_ty..=max_ty {
            let tile_y_px = ty as f32 * tile_size;
            let tile_h = (canvas_h - tile_y_px).clamp(0.0, tile_size);
            if tile_h <= 0.0 {
                continue;
            }

            for tx in min_tx..=max_tx {
                let tile_x_px = tx as f32 * tile_size;
                let tile_w = (canvas_w - tile_x_px).clamp(0.0, tile_size);
                if tile_w <= 0.0 {
                    continue;
                }

                let idx = (ty * self.tiles_across + tx) as usize;
                if idx >= self.textures.len() {
                    continue;
                }

                let canvas_rect = egui::Rect::from_min_size(
                    egui::pos2(tile_x_px, tile_y_px),
                    egui::vec2(tile_w, tile_h),
                );

                let screen_min = transform.canvas_to_screen(canvas_rect.min);
                let screen_max = transform.canvas_to_screen(canvas_rect.max);
                let screen_rect = egui::Rect::from_min_max(screen_min, screen_max);

                // Rysujemy teksturę kafla na GPU, jeśli kafel nie jest pusty
                if let Some(texture) = &self.textures[idx] {
                    let uv_rect = egui::Rect::from_min_max(
                        egui::pos2(0.0, 0.0),
                        egui::pos2(tile_w / tile_size, tile_h / tile_size),
                    );
                    painter.image(texture.id(), screen_rect, uv_rect, egui::Color32::WHITE);
                }

                // Opcjonalna ramka siatki kafli (dla celów diagnostycznych / wizualizacji)
                if show_tile_grid {
                    painter.rect_stroke(
                        screen_rect,
                        0.0_f32,
                        egui::Stroke::new(
                            1.0_f32,
                            egui::Color32::from_rgba_unmultiplied(80, 160, 255, 60),
                        ),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
    }
}

/// Tworzy przykładowy wielowarstwowy dokument testowy prezentujący działanie kafli i warstw.
pub fn create_demo_document(width: u32, height: u32) -> Document {
    let mut doc = Document::new(width, height);

    // Warstwa 0: Tło — współdzielony kafel wypełniony bielą
    let mut bg = Layer::new("Tło", width, height);
    let white_tile = create_filled_tile(Rgba8::WHITE);
    for tile_opt in &mut bg.pixels.tiles {
        *tile_opt = Some(Arc::clone(&white_tile));
    }
    doc.add_layer(bg);

    // Warstwa 1: Kolorowe figury geometryczne przecinające granice kafli
    let mut shapes = Layer::new("Figury", width, height);
    // Czerwony prostokąt
    for y in 80..220.min(height) {
        for x in 80..260.min(width) {
            let _ = shapes.set_pixel(x, y, Rgba8::new(230, 50, 50, 255));
        }
    }
    // Niebieski prostokąt przecinający granice kafli
    for y in 160..340.min(height) {
        for x in 180..420.min(width) {
            let _ = shapes.set_pixel(x, y, Rgba8::new(45, 120, 240, 255));
        }
    }
    // Złoty prostokąt
    for y in 240..420.min(height) {
        for x in 320..540.min(width) {
            let _ = shapes.set_pixel(x, y, Rgba8::new(245, 185, 30, 255));
        }
    }
    doc.add_layer(shapes);

    // Warstwa 2: Półprzezroczysty fioletowy akcent (krycie 65%)
    let mut overlay = Layer::new("Półprzezroczysta nakładka", width, height);
    overlay.opacity = 0.65;
    for y in 120..380.min(height) {
        for x in 140..480.min(width) {
            let _ = overlay.set_pixel(x, y, Rgba8::new(170, 50, 220, 255));
        }
    }
    doc.add_layer(overlay);

    doc
}

/// Aktywne narzędzie pracy na płótnie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActiveTool {
    /// Przesuwanie widoku płótna.
    Pan,
    /// Przesuwanie pikseli aktywnej warstwy lub zaznaczenia (M).
    Move,
    /// Malowanie pędzlem okrągłym (P).
    #[default]
    Brush,
    /// Malowanie ołówkiem z twardymi krawędziami (N).
    Pencil,
    /// Malowanie aerografem nakładanym w czasie (A).
    Airbrush,
    /// Wymazywanie gumką okrągłą (Shift+E).
    Eraser,
    /// Wypełnianie spójnego obszaru (kubełek, Shift+B).
    Bucket,
    /// Gradient liniowy (G).
    Gradient,
    /// Próbkowanie koloru (pipeta, O).
    Eyedropper,
    /// Zaznaczenie prostokątne (R).
    SelectRect,
    /// Zaznaczenie eliptyczne (E).
    SelectEllipse,
    /// Różdżka: zaznaczenie obszaru o podobnym kolorze (U).
    MagicWand,
    /// Kadrowanie dokumentu (Shift+C).
    Crop,
    /// Zaznaczenie odręczne (lasso, F).
    SelectFree,
    /// Zaznaczenie wg koloru (Shift+O).
    SelectColor,
    /// Obrót aktywnej warstwy (Shift+R).
    Rotate,
    /// Skalowanie aktywnej warstwy (Shift+T).
    Scale,
    /// Odbicie aktywnej warstwy (Shift+F).
    Flip,
    /// Klonowanie ze źródła (C).
    Clone,
    /// Rozmazywanie pędzlem (Shift+S).
    Smudge,
    /// Rozjaśnianie / ściemnianie (Shift+D).
    DodgeBurn,
    /// Rozmywanie / wyostrzanie pędzlem (Shift+U).
    BlurSharpen,
    /// Wstawianie tekstu czcionką (T).
    Text,
    /// Lupa: powiększanie i pomniejszanie widoku (Z).
    Zoom,
    /// Miarka: mierzenie odległości i kąta na płótnie (Shift+M).
    Measure,
}

/// Kształt bieżącego zaznaczenia do rysowania maszerujących mrówek.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectionShape {
    #[default]
    Rect,
    Ellipse,
}

/// Tryb narzędzia Rozjaśnianie / Ściemnianie (T9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DodgeBurnType {
    #[default]
    Dodge,
    Burn,
}

/// Tryb narzędzia Rozmywanie / Wyostrzanie (T9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlurSharpenType {
    #[default]
    Blur,
    Sharpen,
}

/// Rodzaj i parametry aktywnego filtra obrazu (E10).
#[derive(Clone, Debug, PartialEq)]
pub enum FilterKind {
    /// Korekcja jasności (-255..=255) i kontrastu (0.0..=3.0).
    BrightnessContrast { brightness: i16, contrast: f32 },
    /// Korekcja odcienia (-180°..=180°), nasycenia (0.0..=3.0) i jasności (-1.0..=1.0).
    HueSaturation {
        hue_shift_deg: f32,
        sat_mul: f32,
        light_delta: f32,
    },
    /// Rozmycie Gaussa o promieniu (1..=30 px).
    GaussianBlur { radius: usize },
    /// Maska wyostrzająca (Unsharp Mask): promień (1..=20 px) i siła (0.1..=5.0).
    UnsharpMask { radius: usize, amount: f32 },
}

impl FilterKind {
    /// Zwraca tytuł okna dialogowego dla danego filtra.
    pub fn title(&self) -> &'static str {
        match self {
            Self::BrightnessContrast { .. } => "Jasność i kontrast",
            Self::HueSaturation { .. } => "Odcień i nasycenie",
            Self::GaussianBlur { .. } => "Rozmycie Gaussa",
            Self::UnsharpMask { .. } => "Wyostrzanie",
        }
    }

    /// Stosuje filtr do podanej warstwy z uwzględnieniem opcjonalnego zaznaczenia.
    pub fn apply(&self, layer: &mut TiledLayer, sel: Option<&Selection>) -> Option<Rect> {
        match *self {
            Self::BrightnessContrast {
                brightness,
                contrast,
            } => dziwak_core::brightness_contrast(layer, sel, brightness, contrast),
            Self::HueSaturation {
                hue_shift_deg,
                sat_mul,
                light_delta,
            } => dziwak_core::hue_saturation(layer, sel, hue_shift_deg, sat_mul, light_delta),
            Self::GaussianBlur { radius } => dziwak_core::gaussian_blur(layer, sel, radius),
            Self::UnsharpMask { radius, amount } => {
                dziwak_core::unsharp_mask(layer, sel, radius, amount)
            }
        }
    }
}

/// Stan okna dialogowego filtra z podglądem na żywo i debouncem (~100 ms).
pub struct FilterDialog {
    /// Aktywny filtr i jego bieżące parametry.
    pub kind: FilterKind,
    /// Czy włączony jest podgląd na żywo na płótnie.
    pub preview: bool,
    /// Klon warstwy sprzed modyfikacji filtrem (do przywracania i historii).
    pub original_layer: Layer,
    /// Indeks modyfikowanej warstwy.
    pub target_layer_idx: usize,
    /// Znacznik czasu ostatniego zaaplikowania podglądu (debounce ~100 ms).
    pub last_apply_time: Option<Instant>,
    /// Czy użytkownik zmienił suwak w trakcie okresu wyciszenia (wymaga aktualizacji).
    pub pending_apply: bool,
    /// Poprzedni prostokąt zmian wygenerowany przez filtr (do oznaczania brudnych kafli).
    pub last_changed_rect: Option<Rect>,
}

/// Rodzaj i parametry aktywnego przekształcenia warstwy (T6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayerTransformKind {
    /// Obrót o kąt w stopniach (-180°..=180°).
    Rotate { angle_deg: f32 },
    /// Skalowanie do zadanego rozmiaru (szerokość x wysokość) z zachowaniem proporcji.
    Scale {
        width: u32,
        height: u32,
        orig_width: u32,
        orig_height: u32,
        keep_aspect: bool,
    },
    /// Odbicie w osi poziomej lub pionowej.
    Flip { horizontal: bool },
}

impl LayerTransformKind {
    /// Zwraca tytuł okna dialogowego dla danego przekształcenia.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Rotate { .. } => "Obrót warstwy",
            Self::Scale { .. } => "Skalowanie warstwy",
            Self::Flip { .. } => "Odbicie warstwy",
        }
    }

    /// Wykonuje przekształcenie na warstwie kafelkowej.
    pub fn apply(&self, src: &TiledLayer) -> TiledLayer {
        match *self {
            Self::Rotate { angle_deg } => {
                let rad = angle_deg.to_radians();
                dziwak_core::rotate_layer_centered(src, rad)
            }
            Self::Scale { width, height, .. } => {
                dziwak_core::scale_layer_centered(src, width, height)
            }
            Self::Flip { horizontal } => {
                if horizontal {
                    dziwak_core::flip_horizontal(src)
                } else {
                    dziwak_core::flip_vertical(src)
                }
            }
        }
    }
}

/// Stan okna dialogowego przekształcenia warstwy z podglądem na żywo i debouncem (~100 ms).
pub struct TransformDialog {
    /// Rodzaj transformacji i jej parametry.
    pub kind: LayerTransformKind,
    /// Czy podgląd na żywo na płótnie jest aktywny.
    pub preview: bool,
    /// Klon warstwy sprzed przekształcenia (do przywracania i historii).
    pub original_layer: Layer,
    /// Indeks przekształcanej warstwy.
    pub target_layer_idx: usize,
    /// Znacznik czasu ostatniego zaaplikowania podglądu (debounce).
    pub last_apply_time: Option<Instant>,
    /// Czy parametry uległy zmianie w trakcie debounce.
    pub pending_apply: bool,
}

/// Stan okna dialogowego "Skaluj obraz..." (T7).
#[derive(Clone, Debug)]
pub struct ScaleImageDialog {
    /// Nowa szerokość dokumentu.
    pub width: u32,
    /// Nowa wysokość dokumentu.
    pub height: u32,
    /// Początkowa szerokość dokumentu przed skalowaniem.
    pub orig_width: u32,
    /// Początkowa wysokość dokumentu przed skalowaniem.
    pub orig_height: u32,
    /// Czy zachowywać proporcje boków przy zmianie wymiarów.
    pub keep_aspect: bool,
}

/// Stan okna dialogowego "Rozmiar płótna..." (T7).
#[derive(Clone, Debug)]
pub struct CanvasSizeDialog {
    /// Nowa szerokość płótna.
    pub width: u32,
    /// Nowa wysokość płótna.
    pub height: u32,
    /// Początkowa szerokość dokumentu przed zmianą rozmiaru płótna.
    pub orig_width: u32,
    /// Początkowa wysokość dokumentu przed zmianą rozmiaru płótna.
    pub orig_height: u32,
    /// Przesunięcie zawartości w osi X.
    pub offset_x: i32,
    /// Przesunięcie zawartości w osi Y.
    pub offset_y: i32,
}

/// Stan aktywnej ramki tekstowej edytowanej bezpośrednio na płótnie (w stylu GIMP, T10).
pub struct TextEditorState {
    /// Wprowadzony tekst (wieloliniowy, rozdzielony `\n`).
    pub text: String,
    /// Pozycja kursora jako indeks znaku (`0..=text.chars().count()`).
    pub cursor: usize,
    /// Pozycja lewego górnego rogu ramki na płótnie (x, y) w pikselach.
    pub pos: (i32, i32),
    /// Minimalny rozmiar ramki (szerokość, wysokość) z przeciągnięcia myszą (`(0.0, 0.0)` przy pojedynczym kliknięciu).
    pub min_size: (f32, f32),
    /// Indeks warstwy tekstowej utworzonej w dokumencie dla tej ramki.
    pub layer_index: usize,
    /// Indeks warstwy aktywnej przed utworzeniem ramki tekstowej (przywracany po Esc lub pustym zatwierdzeniu).
    pub prev_active_layer_index: usize,
    /// Oczekiwana liczba warstw w dokumencie podczas edycji tej ramki.
    pub expected_layer_count: usize,
    /// Prostokąt ostatnio odciśniętej maski na warstwie tekstowej (do oznaczania brudnych kafli).
    pub last_stamped_rect: Option<Rect>,
    /// Wygenerowana maska tekstu do naniesienia na warstwę.
    pub preview_mask: Option<dziwak_core::text::TextMask>,
    /// Kolor, dla którego ostatnio narysowano zawartość warstwy tekstowej.
    pub preview_color: egui::Color32,
    /// Czy maska i pozycje kursora wymagają przeliczenia.
    pub preview_dirty: bool,
    /// Pozycje poziome (x) kursora dla każdej linii z `dziwak_core::text::caret_positions`.
    pub caret_x_positions: Vec<Vec<f32>>,
    /// Bazowa wysokość pojedynczej linii czcionki w pikselach.
    pub base_line_height: f32,
    /// Komunikat błędu, jeśli nie udało się wczytać czcionki.
    pub error: Option<String>,
}

impl TextEditorState {
    /// Zwraca (indeks_linii, indeks_kolumny_w_znakach) dla bieżącej pozycji kursora.
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let mut line = 0usize;
        let mut col = 0usize;
        for (i, c) in self.text.chars().enumerate() {
            if i >= self.cursor {
                break;
            }
            if c == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Przelicza parę (linia, kolumna w znakach) na indeks znaku w całym buforze tekstu.
    pub fn line_col_to_cursor(text: &str, target_line: usize, target_col: usize) -> usize {
        let mut idx = 0usize;
        for (li, line) in text.split('\n').enumerate() {
            let line_len = line.chars().count();
            if li == target_line {
                return idx + target_col.min(line_len);
            }
            idx += line_len + 1;
        }
        text.chars().count()
    }
}

/// Zwraca posortowaną, unikalną listę rodzin czcionek z `fc-list : family` (liczoną raz i cache'owaną).
pub fn system_font_families() -> &'static [String] {
    static FAMILIES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FAMILIES.get_or_init(|| {
        let Ok(output) = std::process::Command::new("fc-list")
            .arg(":")
            .arg("family")
            .output()
        else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        let Ok(stdout) = String::from_utf8(output.stdout) else {
            return Vec::new();
        };
        let mut set = BTreeSet::new();
        for line in stdout.lines() {
            for part in line.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    set.insert(trimmed.to_string());
                }
            }
        }
        set.into_iter().collect()
    })
}

/// Wyszukuje ścieżkę do pliku czcionki przez `fc-match -f %{file} <rodzina>` z awaryjnymi ścieżkami systemowymi (T10).
pub fn find_font_path(family: &str) -> Option<PathBuf> {
    if let Ok(output) = std::process::Command::new("fc-match")
        .arg("-f")
        .arg("%{file}")
        .arg(family)
        .output()
    {
        if output.status.success() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                let p = PathBuf::from(s.trim());
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    // Windows: C:\\Windows\\Fonts (nazwa rodziny bez spacji, potem Segoe UI / Arial)
    let windir = std::env::var_os("WINDIR").map(PathBuf::from);
    if let Some(fonts) = windir.map(|w| w.join("Fonts")) {
        let base = family.replace(' ', "").to_lowercase();
        for name in [
            format!("{base}.ttf"),
            "segoeui.ttf".into(),
            "arial.ttf".into(),
        ] {
            let p = fonts.join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    for fallback in [
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        let p = PathBuf::from(fallback);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Maksymalny wspólny budżet pamięci historii dla wszystkich otwartych zakładek (256 MiB).
pub const HISTORY_BUDGET_BYTES: usize = 256 * 1024 * 1024;

/// Wiadomość z wątku AI: `(tab_id, wynik maski w*h albo komunikat błędu)`.
pub type AiResultMessage = (u64, Result<Vec<u8>, String>);

/// Stan pojedynczej zakładki dokumentu (obrazu).
pub struct DocumentTab {
    /// Stabilny identyfikator zakładki (do kierowania wyników operacji asynchronicznych).
    pub id: u64,
    /// Aktywny dokument graficzny.
    pub document: Document,
    /// Historia operacji (undo/redo).
    pub history: History,
    /// Renderer kafli (zarządza teksturami GPU i brudnymi kafelkami).
    pub tile_renderer: TileRenderer,
    /// Transformacja widoku (przesunięcie i powiększenie).
    pub transform: ViewTransform,
    /// Czy płótno zostało już wycentrowane przy pierwszym renderze.
    pub initialized: bool,
    /// Dopasowuj obraz przy każdej zmianie rozmiaru okna, dopóki użytkownik sam nie przybliży/przesunie.
    pub auto_fit: bool,
    /// Rozmiar obszaru płótna z poprzedniej klatki (wykrywanie zmiany rozmiaru okna).
    pub last_viewport_size: egui::Vec2,
    /// Żądanie wyśrodkowania obrazu bez zmiany powiększenia (Widok → Wyśrodkuj obraz).
    pub center_requested: bool,
    /// Maska aktywnego zaznaczenia.
    pub selection: Selection,
    /// Pamięć podręczna odcinków obrysu zaznaczenia (maszerujące mrówki).
    pub selection_outline: Vec<(u32, u32, u32, u32)>,
    /// Numer rewizji zaznaczenia, dla którego wyliczono `selection_outline`.
    pub selection_outline_rev: u64,
    /// Kształt ostatnio wybranego zaznaczenia (prostokąt lub elipsa).
    pub selection_shape: SelectionShape,
    /// Punkt początkowy przeciągania zaznaczenia w współrzędnych płótna (x, y).
    pub selection_drag_start: Option<egui::Pos2>,
    /// Bieżący punkt przeciągania zaznaczenia w współrzędnych płótna (x, y).
    pub selection_drag_current: Option<egui::Pos2>,
    /// Czy trwa aktywne pociągnięcie narzędziem malarskim.
    pub is_painting: bool,
    /// Ostatnia pozycja pociągnięcia na płótnie (x, y).
    pub last_stroke_pos: Option<(f32, f32)>,
    /// Czas ostatniego naniesienia śladu aerografu (A).
    pub last_airbrush_dab: Option<Instant>,
    /// Akumulowany dystans od ostatniego punktu pociągnięcia.
    pub stroke_carry: f32,
    /// Indeks aktywnej warstwy w dokumencie.
    pub active_layer_index: usize,
    /// Czy użytkownik aktualnie przeciąga suwak krycia warstwy.
    pub is_dragging_opacity: bool,
    /// Indeks warstwy, której nazwa jest aktualnie edytowana (dwuklik).
    pub editing_layer_index: Option<usize>,
    /// Bieżący tekst wprowadzany podczas edycji nazwy warstwy.
    pub editing_layer_name: String,
    /// Flaga żądania ustawienia fokusu w polu tekstowym edycji nazwy.
    pub request_focus_rename: bool,
    /// Aktywne okno dialogowe filtra obrazu (jeśli otwarte).
    pub filter_dialog: Option<FilterDialog>,
    /// Aktywne okno dialogowe przekształcenia warstwy (T6).
    pub transform_dialog: Option<TransformDialog>,
    /// Aktywne okno dialogowe skalowania całego obrazu (T7).
    pub scale_image_dialog: Option<ScaleImageDialog>,
    /// Aktywne okno dialogowe zmiany rozmiaru płótna (T7).
    pub canvas_size_dialog: Option<CanvasSizeDialog>,
    /// Aktywna ramka tekstowa edytowana bezpośrednio na płótnie (T10).
    pub text_editor: Option<TextEditorState>,
    /// Punkt początkowy przeciągania nowej ramki tekstu na płótnie.
    pub text_drag_start: Option<egui::Pos2>,
    /// Bieżący punkt przeciągania nowej ramki tekstu na płótnie.
    pub text_drag_current: Option<egui::Pos2>,
    /// Czy trwa przesuwanie aktywnej ramki tekstu za jej krawędź.
    pub is_moving_text_box: bool,
    /// Przesunięcie kursora względem rogu ramki tekstu podczas jej przesuwania.
    pub text_move_offset: egui::Vec2,
    /// Punkt początkowy przeciągania gradientu w współrzędnych płótna.
    pub gradient_drag_start: Option<egui::Pos2>,
    /// Bieżący punkt przeciągania gradientu w współrzędnych płótna.
    pub gradient_drag_current: Option<egui::Pos2>,
    /// Punkt początkowy przeciągania narzędziem Przesuwanie (M) w współrzędnych płótna.
    pub move_drag_start: Option<egui::Pos2>,
    /// Kopia zapasowa warstwy sprzed rozpoczęcia bieżącego przesuwania.
    pub move_initial_layer: Option<Layer>,
    /// Kopia zapasowa zaznaczenia sprzed rozpoczęcia bieżącego przesuwania.
    pub move_initial_selection: Option<Selection>,
    /// Czy trwa aktywne przesuwanie myszą (narzędzie Move).
    pub is_moving: bool,
    /// Bieżący prostokąt kadrowania zatwierdzany klawiszem Enter (T2).
    pub crop_rect: Option<dziwak_core::Rect>,
    /// Początek przeciągania ramki kadrowania na płótnie.
    pub crop_drag_start: Option<egui::Pos2>,
    /// Bieżąca pozycja kursora podczas przeciągania ramki kadrowania.
    pub crop_drag_current: Option<egui::Pos2>,
    /// Punkty wielokąta dla narzędzia Zaznaczenie odręczne (F, T3).
    pub free_select_points: Vec<(f32, f32)>,
    /// Tryb łączenia maski dla zaznaczenia odręcznego (Replace / Add / Subtract / Intersect).
    pub free_select_mode: dziwak_core::selection::SelectMode,
    /// Czy trwa aktywne tworzenie zaznaczenia odręcznego.
    pub is_free_selecting: bool,
    /// Ścieżka bieżącego pliku (None dla nowego dokumentu).
    pub current_file_path: Option<PathBuf>,
    /// Czy dokument posiada niezapisane modyfikacje (E13).
    pub is_modified: bool,
    /// Punkt źródłowy klonowania (x, y) w przestrzeni płótna (C, T8).
    pub clone_source: Option<(f32, f32)>,
    /// Stałe przesunięcie klonowania (dx, dy) dla trybu wyrównanego (T8).
    pub clone_offset: Option<(i32, i32)>,
    /// Kopia warstwy pikseli pobrana na początku pociągnięcia klonowania (T8).
    pub clone_source_layer: Option<dziwak_core::TiledLayer>,
    /// Bufor farby dla aktywnego pociągnięcia narzędzia Rozmazywanie (Shift+S, M47).
    pub smudge_buffer: Option<dziwak_core::brush::SmudgeBuffer>,
    /// Kopia warstwy pikseli pobrana na początku pociągnięcia rozmywania/wyostrzania (T9).
    pub blur_sharpen_source: Option<dziwak_core::TiledLayer>,
    /// Ścieżka oczekująca na potwierdzenie spłaszczenia warstw przy zapisie do formatu jednowarstwowego (E13).
    pub pending_flatten_save: Option<PathBuf>,
    /// Numery wersji poszczególnych warstw (zwiększane przy modyfikacjach pikseli).
    pub layer_versions: Vec<u64>,
    /// Pamięć podręczna miniatur warstw (GPU TextureHandle + wersja).
    pub layer_thumbnails: Vec<Option<LayerThumbnailEntry>>,
    /// Odbiornik wyniku usuwania tła AI z wątku roboczego: `(tab_id, wynik maski w*h albo komunikat błędu)`.
    pub ai_rx: Option<Receiver<AiResultMessage>>,
    /// Czy wynik AI ma zostać zaznaczeniem (true) czy usunąć tło z warstwy (false).
    pub ai_as_selection: bool,
    /// Początek przeciągania obszaru lupy na ekranie (T11).
    pub zoom_drag_start: Option<egui::Pos2>,
    /// Bieżąca pozycja przeciągania obszaru lupy na ekranie (T11).
    pub zoom_drag_current: Option<egui::Pos2>,
    /// Początek linii pomiarowej miarki na płótnie (T11).
    pub measure_start: Option<egui::Pos2>,
    /// Koniec linii pomiarowej miarki na płótnie (T11).
    pub measure_end: Option<egui::Pos2>,
    /// Czy użytkownik aktualnie przeciąga miarkę (T11).
    pub is_measuring: bool,
}

impl DocumentTab {
    /// Tworzy nową zakładkę z dokumentem demonstracyjnym 800×600 px i zadanym limitem historii.
    pub fn new(id: u64, history_max_bytes: usize) -> Self {
        let width = 800;
        let height = 600;
        let document = create_demo_document(width, height);
        Self::from_document(id, document, None, history_max_bytes)
    }

    /// Tworzy zakładkę z gotowego dokumentu i opcjonalnej ścieżki pliku.
    pub fn from_document(
        id: u64,
        document: Document,
        current_file_path: Option<PathBuf>,
        history_max_bytes: usize,
    ) -> Self {
        let width = document.width;
        let height = document.height;
        let history = History::new(history_max_bytes);
        let tile_renderer = TileRenderer::new(width, height);
        let active_layer_index = if document.layer_count() > 1 { 1 } else { 0 };
        let selection = Selection::new(width, height);

        Self {
            id,
            document,
            history,
            tile_renderer,
            transform: ViewTransform::new(egui::Vec2::ZERO, 1.0),
            initialized: false,
            auto_fit: true,
            last_viewport_size: egui::Vec2::ZERO,
            center_requested: false,
            selection,
            selection_outline: Vec::new(),
            selection_outline_rev: 0,
            selection_shape: SelectionShape::Rect,
            selection_drag_start: None,
            selection_drag_current: None,
            is_painting: false,
            last_stroke_pos: None,
            last_airbrush_dab: None,
            stroke_carry: 0.0,
            active_layer_index,
            is_dragging_opacity: false,
            editing_layer_index: None,
            editing_layer_name: String::new(),
            request_focus_rename: false,
            filter_dialog: None,
            transform_dialog: None,
            scale_image_dialog: None,
            canvas_size_dialog: None,
            text_editor: None,
            text_drag_start: None,
            text_drag_current: None,
            is_moving_text_box: false,
            text_move_offset: egui::Vec2::ZERO,
            gradient_drag_start: None,
            gradient_drag_current: None,
            move_drag_start: None,
            move_initial_layer: None,
            move_initial_selection: None,
            is_moving: false,
            crop_rect: None,
            crop_drag_start: None,
            crop_drag_current: None,
            free_select_points: Vec::new(),
            free_select_mode: dziwak_core::selection::SelectMode::Replace,
            is_free_selecting: false,
            current_file_path,
            is_modified: false,
            clone_source: None,
            clone_offset: None,
            clone_source_layer: None,
            smudge_buffer: None,
            blur_sharpen_source: None,
            pending_flatten_save: None,
            layer_versions: Vec::new(),
            layer_thumbnails: Vec::new(),
            ai_rx: None,
            ai_as_selection: false,
            zoom_drag_start: None,
            zoom_drag_current: None,
            measure_start: None,
            measure_end: None,
            is_measuring: false,
        }
    }

    /// Zwraca wyświetlaną nazwę zakładki (nazwa pliku lub przetłumaczone 'bez nazwy').
    pub fn document_name(&self) -> String {
        match &self.current_file_path {
            Some(path) => match path.file_name().and_then(|n| n.to_str()) {
                Some(s) if !s.is_empty() => s.to_string(),
                _ => crate::i18n::tr("bez nazwy").to_string(),
            },
            None => crate::i18n::tr("bez nazwy").to_string(),
        }
    }

    /// Zwalnia tekstury GPU tej zakładki (kafle płótna + miniatury warstw) przed przejściem w tło.
    pub fn release_gpu_resources(&mut self) {
        self.tile_renderer.release_textures();
        self.layer_thumbnails.clear();
    }
}

/// Główny stan aplikacji Dziwak.
pub struct DziwakApp {
    /// Aktywna zakładka obrazu.
    pub doc: DocumentTab,
    /// Nieaktywne zakładki obrazów w tle.
    pub background_tabs: Vec<DocumentTab>,
    /// Kolejność wyświetlania zakładek na pasku zakładek (identyfikatory `DocumentTab::id`).
    pub tab_order: Vec<u64>,
    /// Licznik kolejnego wolnego identyfikatora zakładki.
    pub next_tab_id: u64,
    /// Czy trwa przesuwanie płótna (nawigacja).
    pub is_panning: bool,
    /// Czy bieżące wciśnięcie przycisku myszy zaczęło się na płótnie (nie w oknie/menu).
    pub canvas_owns_pointer: bool,
    /// Opcja wyświetlania siatki kafli 64×64 px.
    pub show_tile_grid: bool,
    /// Nadawca kanału dla asynchronicznych okien dialogowych.
    pub dialog_tx: Sender<DialogResult>,
    /// Odbiorca kanału dla asynchronicznych okien dialogowych.
    pub dialog_rx: Receiver<DialogResult>,
    /// Opcjonalny komunikat błędu wyświetlany w oknie dialogowym egui.
    pub error_message: Option<String>,
    /// Aktywne narzędzie (Przesuwanie, Pędzel, Gumka, Zaznaczenie).
    pub active_tool: ActiveTool,
    /// Średnica pędzla/gumki w pikselach.
    pub brush_size: f32,
    /// Twardość pędzla/gumki (0.0..=1.0).
    pub brush_hardness: f32,
    /// Krycie pędzla/gumki (0.0..=1.0).
    pub brush_opacity: f32,
    /// Kolor pędzla.
    pub brush_color: egui::Color32,
    /// Rodzina czcionki dla narzędzia Tekst.
    pub text_font_family: String,
    /// Rozmiar czcionki w pikselach dla narzędzia Tekst.
    pub text_font_size: f32,
    /// Wyrównanie poziome tekstu (lewo/środek/prawo).
    pub text_align: dziwak_core::text::TextAlign,
    /// Dodatkowy odstęp między liniami tekstu w pikselach.
    pub text_line_spacing: f32,
    /// Pamięć podręczna wczytanych bajtów czcionki `(rodzina, bajty)`.
    pub cached_font_bytes: Option<(String, Vec<u8>)>,
    /// Kolor tła (używany m.in. w gradiencie).
    pub bg_color: egui::Color32,
    /// Tolerancja wypełniania kubełkiem (0..=255).
    pub fill_tolerance: u8,
    /// Czy próbkowanie kubełka obejmuje wszystkie warstwy (czy tylko aktywną).
    pub fill_sample_all_layers: bool,
    /// Bufor wielokrotnego użytku dla całego złożonego dokumentu / warstwy (kubełek).
    pub composite_buffer: Vec<Rgba8>,
    /// Promień próbkowania pipety: 0 = 1×1 (0 px), 1 = 3×3 (1 px), 2 = 5×5 (2 px).
    pub pipette_radius: u32,
    /// Siła rozmazywania pędzlem (Shift+S, T9).
    pub smudge_rate: f32,
    /// Aktywny tryb rozjaśniania/ściemniania (Shift+D, T9).
    pub dodge_burn_type: DodgeBurnType,
    /// Ekspozycja dla rozjaśniania/ściemniania (T9).
    pub dodge_burn_exposure: f32,
    /// Aktywny tryb rozmywania/wyostrzania (Shift+U, T9).
    pub blur_sharpen_type: BlurSharpenType,
    /// Siła rozmywania/wyostrzania (T9).
    pub blur_sharpen_rate: f32,
    /// Doki schowane klawiszem Tab (M28).
    pub docks_hidden: bool,
    /// Ostatnio ustawiony tytuł okna (do unikania nadmiarowych wywołań ViewportCommand).
    pub last_window_title: String,
    /// Pozycja kursora w przestrzeni płótna (do paska stanu i linijek).
    pub cursor_canvas_pos: Option<egui::Pos2>,
    /// Aktywna zakładka w prawym panelu dokującym (Pędzle / Historia cofania).
    pub right_tab: RightTab,
    /// Czy wyświetlane jest okno dialogowe 'O programie Dziwak'.
    pub show_about_dialog: bool,
    /// Aktywny cel próbnika kolorów (kolor pierwszoplanowy lub tła).
    pub color_picker_target: Option<ColorPickerTarget>,
    /// Bufor kresek dla poziomej linijki (używany wielokrotnie, zero alokacji per klatka).
    pub ruler_ticks_h: Vec<dziwak_core::ruler::Tick>,
    /// Bufor kresek dla pionowej linijki (używany wielokrotnie, zero alokacji per klatka).
    pub ruler_ticks_v: Vec<dziwak_core::ruler::Tick>,
    /// Uchwyt do tekstury szachownicy przezroczystości (rysowanej raz na GPU w trybie Repeat).
    pub checkerboard_texture: Option<egui::TextureHandle>,
    /// Bufor pikseli Rgba8 dla miniatury (wielokrotnego użytku, 0 alokacji per klatka).
    pub thumbnail_rgba_buf: Vec<Rgba8>,
    /// Czy wyświetlane jest modalne okno pytania o zapis przed zamknięciem.
    pub show_close_confirm_dialog: bool,
    /// Identyfikator zakładki, której dotyczy aktualne pytanie o zapis przy zamykaniu.
    pub close_confirm_tab_id: Option<u64>,
    /// Czy aktualny przepływ pytania o zapis dotyczy zamknięcia całego programu (Super+W / Ctrl+Q).
    pub closing_entire_app: bool,
    /// Zbiór identyfikatorów zakładek, dla których użytkownik wybrał 'Nie zapisuj' podczas zamykania programu.
    pub close_discarded_tabs: BTreeSet<u64>,
    /// Czy zamknięcie programu zostało już potwierdzone (np. 'Nie zapisuj' lub po udanym zapisie).
    pub close_confirmed: bool,
    /// Czy zamknąć zakładkę / kontynuować zamykanie programu po udanym zapisie zainicjowanym z okna zamykania.
    pub close_after_save: bool,
}

/// Pojedynczy wpis w pamięci podręcznej miniatur warstw.
pub struct LayerThumbnailEntry {
    /// Uchwyt tekstury miniatury na GPU.
    pub texture: egui::TextureHandle,
    /// Numer wersji warstwy, dla którego wygenerowano miniaturę.
    pub version: u64,
    /// Czas ostatniego przeliczenia miniatury.
    pub last_update: Instant,
}

/// Aktywna zakładka w prawym panelu (dok).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RightTab {
    #[default]
    Brushes,
    History,
}

/// Cel wyboru koloru w popupie próbnika kolorów.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorPickerTarget {
    Foreground,
    Background,
}

/// Predefiniowane ustawienie parametrów pędzla.
#[derive(Clone, Copy, Debug)]
pub struct BrushPreset {
    pub name: &'static str,
    pub size: f32,
    pub hardness: f32,
}

pub const BRUSH_PRESETS: [BrushPreset; 6] = [
    BrushPreset {
        name: "Precyzyjny (2 px)",
        size: 2.0,
        hardness: 1.0,
    },
    BrushPreset {
        name: "Średni twardy (8 px)",
        size: 8.0,
        hardness: 1.0,
    },
    BrushPreset {
        name: "Duży twardy (20 px)",
        size: 20.0,
        hardness: 0.9,
    },
    BrushPreset {
        name: "Mały miękki (15 px)",
        size: 15.0,
        hardness: 0.2,
    },
    BrushPreset {
        name: "Średni miękki (40 px)",
        size: 40.0,
        hardness: 0.1,
    },
    BrushPreset {
        name: "Duży miękki (80 px)",
        size: 80.0,
        hardness: 0.0,
    },
];

impl DziwakApp {
    pub fn new() -> Self {
        let (dialog_tx, dialog_rx) = channel();
        let initial_tab_id = 1;
        let doc = DocumentTab::new(initial_tab_id, HISTORY_BUDGET_BYTES);

        Self {
            doc,
            background_tabs: Vec::new(),
            tab_order: vec![initial_tab_id],
            next_tab_id: initial_tab_id + 1,
            canvas_owns_pointer: false,
            is_panning: false,
            show_tile_grid: false,
            dialog_tx,
            dialog_rx,
            error_message: None,
            active_tool: ActiveTool::Brush,
            brush_size: 20.0,
            brush_hardness: 0.8,
            brush_opacity: 1.0,
            brush_color: egui::Color32::from_rgb(30, 30, 30),
            text_font_family: "Noto Sans".to_string(),
            text_font_size: 40.0,
            text_align: dziwak_core::text::TextAlign::Left,
            text_line_spacing: 0.0,
            cached_font_bytes: None,
            bg_color: egui::Color32::WHITE,
            fill_tolerance: 32,
            fill_sample_all_layers: true,
            composite_buffer: Vec::new(),
            pipette_radius: 0,
            smudge_rate: 0.5,
            dodge_burn_type: DodgeBurnType::Dodge,
            dodge_burn_exposure: 0.5,
            blur_sharpen_type: BlurSharpenType::Blur,
            blur_sharpen_rate: 0.5,
            docks_hidden: false,
            last_window_title: String::new(),
            cursor_canvas_pos: None,
            right_tab: RightTab::Brushes,
            show_about_dialog: false,
            color_picker_target: None,
            ruler_ticks_h: Vec::with_capacity(64),
            ruler_ticks_v: Vec::with_capacity(64),
            checkerboard_texture: None,
            thumbnail_rgba_buf: Vec::with_capacity(32 * 32),
            show_close_confirm_dialog: false,
            close_confirm_tab_id: None,
            closing_entire_app: false,
            close_discarded_tabs: BTreeSet::new(),
            close_confirmed: false,
            close_after_save: false,
        }
    }

    /// Przydziela nowy, unikalny identyfikator zakładki.
    fn alloc_tab_id(&mut self) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.wrapping_add(1).max(1);
        id
    }

    /// Zwraca łączną liczbę otwartych zakładek (aktywna + w tle).
    pub fn tab_count(&self) -> usize {
        1 + self.background_tabs.len()
    }

    /// Sprawdza, czy zakładka o podanym `tab_id` jest otwarta.
    pub fn has_tab(&self, tab_id: u64) -> bool {
        self.doc.id == tab_id || self.background_tabs.iter().any(|t| t.id == tab_id)
    }

    /// Zwraca referencję do zakładki o podanym identyfikatorze (aktywnej lub w tle).
    pub fn tab_by_id(&self, tab_id: u64) -> Option<&DocumentTab> {
        if self.doc.id == tab_id {
            Some(&self.doc)
        } else {
            self.background_tabs.iter().find(|t| t.id == tab_id)
        }
    }

    /// Zwraca mutowalną referencję do zakładki o podanym identyfikatorze (aktywnej lub w tle).
    #[allow(dead_code)]
    pub fn tab_by_id_mut(&mut self, tab_id: u64) -> Option<&mut DocumentTab> {
        if self.doc.id == tab_id {
            Some(&mut self.doc)
        } else {
            self.background_tabs.iter_mut().find(|t| t.id == tab_id)
        }
    }

    /// Dzieli wspólny budżet pamięci historii (`HISTORY_BUDGET_BYTES`) równo między wszystkie otwarte zakładki.
    pub fn rebalance_history_budget(&mut self) {
        let count = self.tab_count().max(1);
        let per_tab_max = (HISTORY_BUDGET_BYTES / count).max(1);
        self.doc
            .history
            .set_max_bytes(per_tab_max, Some(&self.doc.document));
        for tab in &mut self.background_tabs {
            tab.history.set_max_bytes(per_tab_max, Some(&tab.document));
        }
    }

    /// Przełącza aktywną zakładkę na zakładkę o identyfikatorze `target_id` przez `std::mem::swap` (O(1)).
    /// Przed przejściem obecnej zakładki w tło zwalnia jej tekstury GPU (kafle oraz miniatury warstw).
    /// Zwraca `true`, jeśli przełączenie nastąpiło (lub zakładka była już aktywna).
    pub fn switch_to_tab(&mut self, target_id: u64) -> bool {
        if self.doc.id == target_id {
            return true;
        }
        let Some(bg_idx) = self.background_tabs.iter().position(|t| t.id == target_id) else {
            return false;
        };

        // Zwalniamy tekstury GPU zakładki przechodzącej w tło
        self.doc.release_gpu_resources();

        // O(1) podmiana całej struktury DocumentTab
        std::mem::swap(&mut self.doc, &mut self.background_tabs[bg_idx]);

        self.is_panning = false;
        self.canvas_owns_pointer = false;
        self.ensure_layer_caches();

        // Jeśli w tle zakończył się wątek AI dla tej zakładki, odbierz i zastosuj jego wynik teraz
        let _ = self.poll_ai_background();
        true
    }

    /// Przełącza na następną zakładkę zgodnie z kolejnością na pasku (`Ctrl+Tab`).
    pub fn next_tab(&mut self) {
        if self.tab_order.len() <= 1 {
            return;
        }
        let cur_pos = self
            .tab_order
            .iter()
            .position(|&id| id == self.doc.id)
            .unwrap_or(0);
        let next_id = self.tab_order[(cur_pos + 1) % self.tab_order.len()];
        let _ = self.switch_to_tab(next_id);
    }

    /// Przełącza na poprzednią zakładkę zgodnie z kolejnością na pasku (`Ctrl+Shift+Tab`).
    pub fn prev_tab(&mut self) {
        if self.tab_order.len() <= 1 {
            return;
        }
        let cur_pos = self
            .tab_order
            .iter()
            .position(|&id| id == self.doc.id)
            .unwrap_or(0);
        let prev_pos = if cur_pos == 0 {
            self.tab_order.len() - 1
        } else {
            cur_pos - 1
        };
        let prev_id = self.tab_order[prev_pos];
        let _ = self.switch_to_tab(prev_id);
    }

    /// Tworzy nową zakładkę ('bez nazwy') i czyni ją aktywną (`Plik → Nowy`, `Ctrl+N`).
    pub fn new_tab(&mut self) -> u64 {
        let id = self.alloc_tab_id();
        self.doc.release_gpu_resources();
        let new_tab = DocumentTab::new(id, HISTORY_BUDGET_BYTES);
        let old_tab = std::mem::replace(&mut self.doc, new_tab);
        self.background_tabs.push(old_tab);
        self.tab_order.push(id);
        self.is_panning = false;
        self.canvas_owns_pointer = false;
        self.close_confirmed = false;
        self.ensure_layer_caches();
        self.rebalance_history_budget();
        id
    }

    /// Otwiera plik z podanej ścieżki zawsze w NOWEJ zakładce (`Plik → Otwórz`).
    /// Zwraca identyfikator nowej zakładki w razie powodzenia.
    pub fn open_file_in_new_tab(&mut self, path: PathBuf) -> Option<u64> {
        match dziwak_core::load_image(&path) {
            Ok(loaded_doc) => {
                let id = self.alloc_tab_id();
                self.doc.release_gpu_resources();
                let new_tab =
                    DocumentTab::from_document(id, loaded_doc, Some(path), HISTORY_BUDGET_BYTES);
                let old_tab = std::mem::replace(&mut self.doc, new_tab);
                self.background_tabs.push(old_tab);
                self.tab_order.push(id);
                self.is_panning = false;
                self.canvas_owns_pointer = false;
                self.close_confirmed = false;
                self.ensure_layer_caches();
                self.mark_document_dirty();
                self.rebalance_history_budget();
                self.error_message = None;
                Some(id)
            }
            Err(err) => {
                self.error_message = Some(format!(
                    "{}:\n{err}",
                    crate::i18n::tr("Błąd wczytywania pliku")
                ));
                None
            }
        }
    }

    /// Sprawdza, czy aplikacja ma tylko jedną, nienaruszoną startową zakładkę 'bez nazwy'.
    fn is_single_untouched_default_tab(&self) -> bool {
        self.background_tabs.is_empty()
            && self.doc.current_file_path.is_none()
            && !self.doc.is_modified
            && !self.doc.history.can_undo()
            && !self.doc.history.can_redo()
    }

    /// Otwiera listę plików przekazanych z linii poleceń (`dziwak a.png b.png`) jako osobne zakładki.
    pub fn open_files_from_cli(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            self.open_file_from_path(path);
        }
    }

    /// Zamyka zakładkę o podanym `tab_id` natychmiast (bez pytania o zapis).
    /// Jeśli zamykana jest ostatnia zakładka, zastępuje ją świeżą pustą zakładką 'bez nazwy' (jak po starcie programu).
    pub fn close_tab_immediately(&mut self, tab_id: u64) {
        if !self.has_tab(tab_id) {
            return;
        }
        self.close_discarded_tabs.remove(&tab_id);
        if self.close_confirm_tab_id == Some(tab_id) {
            self.close_confirm_tab_id = None;
            self.show_close_confirm_dialog = false;
        }

        if self.tab_count() <= 1 {
            let new_id = self.alloc_tab_id();
            self.doc = DocumentTab::new(new_id, HISTORY_BUDGET_BYTES);
            self.background_tabs.clear();
            self.tab_order.clear();
            self.tab_order.push(new_id);
            self.is_panning = false;
            self.canvas_owns_pointer = false;
            self.ensure_layer_caches();
            self.rebalance_history_budget();
            return;
        }

        if self.doc.id == tab_id {
            let cur_pos = self
                .tab_order
                .iter()
                .position(|&id| id == tab_id)
                .unwrap_or(0);
            let next_active_id = if cur_pos + 1 < self.tab_order.len() {
                self.tab_order[cur_pos + 1]
            } else {
                self.tab_order[cur_pos - 1]
            };
            let _ = self.switch_to_tab(next_active_id);
        }

        self.background_tabs.retain(|t| t.id != tab_id);
        self.tab_order.retain(|&id| id != tab_id);
        self.rebalance_history_budget();
    }

    /// Żąda zamknięcia pojedynczej zakładki (krzyżyk, środkowy przycisk myszy, `Ctrl+W`).
    /// Jeśli zakładka jest zmodyfikowana, aktywuje ją i wyświetla okno pytania o zapis.
    pub fn request_close_tab(&mut self, tab_id: u64) {
        let Some(is_modified) = self.tab_by_id(tab_id).map(|t| t.is_modified) else {
            return;
        };
        if !is_modified {
            self.close_tab_immediately(tab_id);
        } else {
            let _ = self.switch_to_tab(tab_id);
            self.close_confirm_tab_id = Some(tab_id);
            self.closing_entire_app = false;
            self.close_after_save = false;
            self.show_close_confirm_dialog = true;
        }
    }

    /// Zwraca identyfikator pierwszej zmodyfikowanej zakładki (wg `tab_order`), której jeszcze nie odrzucono.
    fn next_unhandled_modified_tab_id(&self) -> Option<u64> {
        for &tid in &self.tab_order {
            if self.close_discarded_tabs.contains(&tid) {
                continue;
            }
            if self.tab_by_id(tid).is_some_and(|t| t.is_modified) {
                return Some(tid);
            }
        }
        None
    }

    /// Przechodzi do kolejnej zmodyfikowanej zakładki podczas zamykania programu albo kończy zamykanie.
    /// Zwraca `true`, jeśli wszystkie zmodyfikowane zakładki zostały obsłużone i można zamknąć okno.
    fn advance_app_close_or_finish(&mut self) -> bool {
        if let Some(next_tid) = self.next_unhandled_modified_tab_id() {
            let _ = self.switch_to_tab(next_tid);
            self.close_confirm_tab_id = Some(next_tid);
            self.closing_entire_app = true;
            self.close_after_save = false;
            self.show_close_confirm_dialog = true;
            false
        } else {
            self.show_close_confirm_dialog = false;
            self.close_after_save = false;
            self.close_confirm_tab_id = None;
            self.close_confirmed = true;
            true
        }
    }

    /// Zapisuje bieżący stan dokumentu do historii i oznacza dokument jako zmodyfikowany (E13).
    pub fn push_history(&mut self) {
        self.doc.history.push(&self.doc.document);
        self.doc.is_modified = true;
        self.close_confirmed = false;
    }

    /// Zwiększa licznik wersji wskazanej warstwy, unieważniając jej miniaturę.
    pub fn bump_layer_version(&mut self, layer_idx: usize) {
        self.ensure_layer_caches();
        if layer_idx < self.doc.layer_versions.len() {
            self.doc.layer_versions[layer_idx] = self.doc.layer_versions[layer_idx].wrapping_add(1);
        }
    }

    /// Przerysowuje cały dokument i unieważnia miniatury wszystkich warstw
    /// (po zmianach struktury stosu miniatury indeksowane pozycją byłyby nieaktualne).
    pub fn mark_document_dirty(&mut self) {
        self.doc.tile_renderer.mark_all_dirty();
        self.ensure_layer_caches();
        for v in &mut self.doc.layer_versions {
            *v = v.wrapping_add(1);
        }
    }

    /// Upewnia się, że bufory wersji i miniatur mają właściwy rozmiar odpowiadający liczbie warstw.
    pub fn ensure_layer_caches(&mut self) {
        let count = self.doc.document.layer_count();
        if self.doc.layer_versions.len() != count {
            self.doc.layer_versions.resize(count, 1);
        }
        if self.doc.layer_thumbnails.len() != count {
            self.doc.layer_thumbnails.resize_with(count, || None);
        }
    }

    /// Zwraca teksturę miniatury warstwy (max 32 px), przeliczając ją tylko po zmianie wersji warstwy
    /// i nie częściej niż co 250 ms.
    pub fn layer_thumbnail_texture(
        &mut self,
        ctx: &egui::Context,
        layer_idx: usize,
    ) -> Option<(egui::TextureId, egui::Vec2)> {
        self.ensure_layer_caches();
        let layer = self.doc.document.layers.get(layer_idx)?;
        let version = self.doc.layer_versions[layer_idx];
        let now = Instant::now();
        let needs_update = match &self.doc.layer_thumbnails[layer_idx] {
            None => true,
            Some(entry) => {
                entry.version != version && now.duration_since(entry.last_update).as_millis() >= 250
            }
        };
        if needs_update {
            let (tw, th) = dziwak_core::thumbnail::layer_thumbnail(
                &layer.pixels,
                32,
                &mut self.thumbnail_rgba_buf,
            );
            if tw == 0 || th == 0 {
                return None;
            }
            let pixels = self
                .thumbnail_rgba_buf
                .iter()
                .map(|px| egui::Color32::from_rgba_premultiplied(px.r(), px.g(), px.b(), px.a()))
                .collect();
            let image = egui::ColorImage {
                size: [tw as usize, th as usize],
                source_size: egui::vec2(tw as f32, th as f32),
                pixels,
            };
            match &mut self.doc.layer_thumbnails[layer_idx] {
                Some(entry) => {
                    entry.texture.set(image, egui::TextureOptions::LINEAR);
                    entry.version = version;
                    entry.last_update = now;
                }
                slot => {
                    let texture = ctx.load_texture(
                        format!("layer_thumb_{layer_idx}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    *slot = Some(LayerThumbnailEntry {
                        texture,
                        version,
                        last_update: now,
                    });
                }
            }
        } else if self.doc.layer_thumbnails[layer_idx]
            .as_ref()
            .is_some_and(|e| e.version != version)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        self.doc.layer_thumbnails[layer_idx]
            .as_ref()
            .map(|e| (e.texture.id(), e.texture.size_vec2()))
    }

    /// Zwraca wyświetlaną nazwę bieżącego dokumentu (nazwa pliku lub przetłumaczone 'bez nazwy').
    pub fn document_name(&self) -> String {
        self.doc.document_name()
    }

    /// Zwraca aktualny tytuł okna: 'Dziwak — nazwa_pliku' + gwiazdka gdy zmodyfikowany (E13).
    pub fn window_title(&self) -> String {
        let name = self.document_name();
        let star = if self.doc.is_modified { "*" } else { "" };
        format!("Dziwak — {name}{star}")
    }

    /// Obsługuje żądanie zamknięcia okna aplikacji: dla każdej zmienionej zakładki kolejno
    /// aktywuje ją, otwiera okno pytania o zapis i zwraca `CloseDecision::AskUser`.
    pub fn on_close_requested(&mut self) -> CloseDecision {
        if self.close_confirmed {
            return CloseDecision::Allow;
        }
        if let Some(tid) = self.next_unhandled_modified_tab_id() {
            let _ = self.switch_to_tab(tid);
            self.close_confirm_tab_id = Some(tid);
            self.closing_entire_app = true;
            self.show_close_confirm_dialog = true;
            CloseDecision::AskUser
        } else {
            self.show_close_confirm_dialog = false;
            CloseDecision::Allow
        }
    }

    /// Potwierdza odrzucenie zmian dla bieżącej zakładki ('Nie zapisuj' w oknie pytania o zapis).
    /// Zwraca `true`, jeśli zamykany jest cały program i wszystkie zmodyfikowane zakładki zostały obsłużone
    /// (wtedy UI powinno wysłać `ViewportCommand::Close`).
    pub fn confirm_discard_and_close(&mut self) -> bool {
        let target_id = self.close_confirm_tab_id.unwrap_or(self.doc.id);
        if self.closing_entire_app {
            self.close_discarded_tabs.insert(target_id);
            self.advance_app_close_or_finish()
        } else {
            self.show_close_confirm_dialog = false;
            self.close_after_save = false;
            self.close_confirm_tab_id = None;
            self.close_tab_immediately(target_id);
            false
        }
    }

    /// Odrzuca zmiany we wszystkich zakładkach i pozwala na natychmiastowe zamknięcie programu ('Nie zapisuj żadnego').
    pub fn confirm_discard_all_and_close(&mut self) {
        self.show_close_confirm_dialog = false;
        self.close_after_save = false;
        self.close_confirm_tab_id = None;
        self.closing_entire_app = false;
        self.close_discarded_tabs.clear();
        self.close_confirmed = true;
    }

    /// Kończy operację zamykania (zakładki lub programu) po pomyślnym zapisie pliku.
    /// Zwraca `true`, jeśli zamykany jest cały program i wszystkie zmodyfikowane zakładki zostały już zapisane/odrzucone.
    pub fn complete_close_after_save(&mut self) -> bool {
        self.close_after_save = false;
        let target_id = self.close_confirm_tab_id.unwrap_or(self.doc.id);
        if self.closing_entire_app {
            self.advance_app_close_or_finish()
        } else {
            self.show_close_confirm_dialog = false;
            self.close_confirm_tab_id = None;
            self.close_tab_immediately(target_id);
            false
        }
    }

    /// Anuluje zamykanie zakładki lub programu ('Anuluj' w oknie pytania o zapis).
    pub fn cancel_close_request(&mut self) {
        self.show_close_confirm_dialog = false;
        self.close_after_save = false;
        self.close_confirm_tab_id = None;
        self.closing_entire_app = false;
        self.close_discarded_tabs.clear();
        self.close_confirmed = false;
    }

    /// Otwiera plik z podanej ścieżki: jeśli istnieje tylko początkowa nienaruszona zakładka 'bez nazwy',
    /// wczytuje obraz do niej; w przeciwnym razie otwiera plik w nowej zakładce.
    pub fn open_file_from_path(&mut self, path: PathBuf) {
        if !self.is_single_untouched_default_tab() {
            let _ = self.open_file_in_new_tab(path);
            return;
        }
        match dziwak_core::load_image(&path) {
            Ok(loaded_doc) => {
                self.doc.document = loaded_doc;
                self.doc.current_file_path = Some(path);
                self.doc.is_modified = false;
                self.close_confirmed = false;
                self.close_after_save = false;
                self.show_close_confirm_dialog = false;
                self.doc.initialized = false;
                self.on_document_changed();
                self.rebalance_history_budget();
                self.error_message = None;
            }
            Err(err) => {
                self.error_message = Some(format!(
                    "{}:\n{err}",
                    crate::i18n::tr("Błąd wczytywania pliku")
                ));
            }
        }
    }

    /// Zapisuje bieżący dokument pod wskazaną ścieżkę z obsługą ostrzeżenia o spłaszczaniu warstw.
    /// Zwraca `true`, jeśli zapis został wykonany natychmiast i zakończył się sukcesem.
    pub fn request_save_to_path(&mut self, path: PathBuf) -> bool {
        let is_dziwak = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("dziwak"));

        if !is_dziwak && self.doc.document.layer_count() > 1 {
            self.doc.pending_flatten_save = Some(path);
            false
        } else {
            self.execute_save(path)
        }
    }

    /// Zapisuje bieżący dokument pod wskazaną ścieżkę i zeruje flagę zmodyfikowania (E13).
    /// Zwraca `true`, jeśli zapis się powiódł.
    pub fn execute_save(&mut self, path: PathBuf) -> bool {
        match dziwak_core::save_image(&self.doc.document, &path) {
            Ok(()) => {
                self.doc.current_file_path = Some(path);
                self.doc.is_modified = false;
                self.error_message = None;
                true
            }
            Err(err) => {
                self.error_message =
                    Some(format!("{}:\n{err}", crate::i18n::tr("Błąd zapisu pliku")));
                self.close_after_save = false;
                self.closing_entire_app = false;
                self.close_confirm_tab_id = None;
                self.close_discarded_tabs.clear();
                false
            }
        }
    }

    /// Cofa ostatnią operację w historii. Zwraca true, jeśli wymiary dokumentu uległy zmianie.
    pub fn handle_undo(&mut self) -> bool {
        let old_size = (self.doc.document.width, self.doc.document.height);
        if self.doc.history.undo(&mut self.doc.document) {
            let new_size = (self.doc.document.width, self.doc.document.height);
            self.on_document_changed();
            old_size != new_size
        } else {
            false
        }
    }

    /// Ponawia wcześniej cofniętą operację w historii. Zwraca true, jeśli wymiary dokumentu uległy zmianie.
    pub fn handle_redo(&mut self) -> bool {
        let old_size = (self.doc.document.width, self.doc.document.height);
        if self.doc.history.redo(&mut self.doc.document) {
            let new_size = (self.doc.document.width, self.doc.document.height);
            self.on_document_changed();
            old_size != new_size
        } else {
            false
        }
    }

    /// Nanosi ślad aktywnego narzędzia na bieżącą warstwę i oznacza dotknięte kafle jako brudne.
    pub fn apply_tool_dab(&mut self, cx: f32, cy: f32) {
        if self.doc.document.layers.is_empty() {
            return;
        }

        let (mode, hardness) = match self.active_tool {
            ActiveTool::Brush => (BrushMode::Paint, self.brush_hardness),
            ActiveTool::Pencil => (BrushMode::Paint, 1.0),
            ActiveTool::Airbrush => (BrushMode::Paint, self.brush_hardness),
            ActiveTool::Eraser => (BrushMode::Erase, self.brush_hardness),
            _ => return,
        };

        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let layer = &mut self.doc.document.layers[layer_idx];

        let radius = (self.brush_size * 0.5).max(0.5);
        let opacity = self.brush_opacity;
        let color = Rgba8::from_straight(
            self.brush_color.r(),
            self.brush_color.g(),
            self.brush_color.b(),
            self.brush_color.a(),
        );

        let sel_arg = if self.doc.selection.has_selection {
            Some(&self.doc.selection)
        } else {
            None
        };

        if let Some(rect) = apply_dab(
            layer, cx, cy, radius, hardness, opacity, color, mode, sel_arg,
        ) {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Ustawia punkt źródłowy dla narzędzia Klonowanie (T8).
    pub fn set_clone_source(&mut self, src: (f32, f32)) {
        self.doc.clone_source = Some(src);
        self.doc.clone_offset = None;
    }

    /// Rozpoczyna pociągnięcie klonowania w zadanym punkcie płótna (T8).
    pub fn start_clone_stroke(&mut self, p: (f32, f32)) {
        if self.doc.document.layers.is_empty() || self.doc.clone_source.is_none() {
            return;
        }
        self.push_history();
        self.doc.is_painting = true;

        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        self.doc.clone_source_layer = Some(self.doc.document.layers[layer_idx].pixels.clone());

        if self.doc.clone_offset.is_none() {
            if let Some(src) = self.doc.clone_source {
                self.doc.clone_offset =
                    Some(((src.0 - p.0).round() as i32, (src.1 - p.1).round() as i32));
            }
        }

        self.apply_clone_dab(p.0, p.1);
        self.doc.last_stroke_pos = Some(p);
    }

    /// Nanosi pojedynczy ślad klonowania na aktywną warstwę (T8).
    pub fn apply_clone_dab(&mut self, cx: f32, cy: f32) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let Some(offset) = self.doc.clone_offset else {
            return;
        };
        let Some(source_layer) = &self.doc.clone_source_layer else {
            return;
        };
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let layer = &mut self.doc.document.layers[layer_idx];

        let radius = (self.brush_size * 0.5).max(0.5);
        let hardness = self.brush_hardness;
        let opacity = self.brush_opacity;
        let sel_arg = if self.doc.selection.has_selection {
            Some(&self.doc.selection)
        } else {
            None
        };

        if let Some(rect) = dziwak_core::brush::clone_dab(
            layer,
            source_layer,
            cx,
            cy,
            offset.0,
            offset.1,
            radius,
            hardness,
            opacity,
            sel_arg,
        ) {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Kończy pociągnięcie klonowania i zwalnia sklonowaną warstwę źródłową (T8).
    pub fn finish_clone_stroke(&mut self) {
        self.doc.is_painting = false;
        self.doc.last_stroke_pos = None;
        self.doc.clone_source_layer = None;
    }

    /// Nanosi pojedynczy ślad rozmazywania (Shift+S, T9/M47).
    pub fn apply_smudge_dab(&mut self, cx: f32, cy: f32) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let radius = (self.brush_size * 0.5).max(0.5);
        let hardness = self.brush_hardness;
        let rate = self.smudge_rate;
        if self.doc.smudge_buffer.is_none() {
            self.doc.smudge_buffer = Some(dziwak_core::brush::smudge_begin(
                &self.doc.document.layers[layer_idx],
                cx,
                cy,
                radius,
            ));
        }
        let Some(buf) = &mut self.doc.smudge_buffer else {
            return;
        };
        let sel_arg = if self.doc.selection.has_selection {
            Some(&self.doc.selection)
        } else {
            None
        };

        let layer = &mut self.doc.document.layers[layer_idx];
        if let Some(rect) =
            dziwak_core::brush::smudge_dab(layer, buf, cx, cy, radius, hardness, rate, sel_arg)
        {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Nanosi ślad rozjaśniania lub ściemniania (Shift+D, T9). Ctrl odwraca tryb.
    pub fn apply_dodge_burn_dab(&mut self, cx: f32, cy: f32, invert_mode: bool) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let base_amount = match self.dodge_burn_type {
            DodgeBurnType::Dodge => self.dodge_burn_exposure,
            DodgeBurnType::Burn => -self.dodge_burn_exposure,
        };
        let amount = if invert_mode {
            -base_amount
        } else {
            base_amount
        };

        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let radius = (self.brush_size * 0.5).max(0.5);
        let hardness = self.brush_hardness;
        let sel_arg = if self.doc.selection.has_selection {
            Some(&self.doc.selection)
        } else {
            None
        };

        let layer = &mut self.doc.document.layers[layer_idx];
        if let Some(rect) =
            dziwak_core::dodge_burn_dab(layer, cx, cy, radius, hardness, amount, sel_arg)
        {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Nanosi ślad rozmywania lub wyostrzania (Shift+U, T9). Ctrl odwraca tryb.
    pub fn apply_blur_sharpen_dab(&mut self, cx: f32, cy: f32, invert_mode: bool) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let base_amount = match self.blur_sharpen_type {
            BlurSharpenType::Blur => self.blur_sharpen_rate,
            BlurSharpenType::Sharpen => -self.blur_sharpen_rate,
        };
        let amount = if invert_mode {
            -base_amount
        } else {
            base_amount
        };

        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let fallback_source;
        let source = match &self.doc.blur_sharpen_source {
            Some(src) => src,
            None => {
                fallback_source = self.doc.document.layers[layer_idx].pixels.clone();
                &fallback_source
            }
        };
        let radius = (self.brush_size * 0.5).max(0.5);
        let hardness = self.brush_hardness;
        let sel_arg = if self.doc.selection.has_selection {
            Some(&self.doc.selection)
        } else {
            None
        };

        let layer = &mut self.doc.document.layers[layer_idx];
        if let Some(rect) =
            dziwak_core::blur_sharpen_dab(layer, source, cx, cy, radius, hardness, amount, sel_arg)
        {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Oznacza kafle przecinające podany prostokąt jako brudne (do rekompozycji).
    pub fn mark_rect_dirty(&mut self, rect: Rect) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let tile_size = TILE_SIZE as u32;
        let min_tx = rect.x / tile_size;
        let max_tx = rect.x.saturating_add(rect.width.saturating_sub(1)) / tile_size;
        let min_ty = rect.y / tile_size;
        let max_ty = rect.y.saturating_add(rect.height.saturating_sub(1)) / tile_size;

        for ty in min_ty..=max_ty {
            for tx in min_tx..=max_tx {
                self.doc.tile_renderer.mark_tile_dirty(tx, ty);
            }
        }
    }

    /// Kopiuje piksele podanej warstwy do bufora ciągłego RGBA8.
    pub(crate) fn sample_layer_to_buffer(
        layer: &dziwak_core::Layer,
        w: usize,
        h: usize,
        out: &mut [Rgba8],
    ) {
        out.fill(Rgba8::TRANSPARENT);
        let tile_size = dziwak_core::TILE_SIZE;
        let across = layer.pixels.tiles_across() as usize;
        let down = layer.pixels.tiles_down() as usize;

        for ty in 0..down {
            for tx in 0..across {
                let tile_idx = ty * across + tx;
                if let Some(tile) = &layer.pixels.tiles[tile_idx] {
                    let start_x = tx * tile_size;
                    let start_y = ty * tile_size;
                    let end_x = (start_x + tile_size).min(w);
                    let end_y = (start_y + tile_size).min(h);
                    for y in start_y..end_y {
                        let local_y = y - start_y;
                        let row_offset = y * w;
                        let tile_row_offset = local_y * tile_size;
                        for x in start_x..end_x {
                            let local_x = x - start_x;
                            out[row_offset + x] = tile[tile_row_offset + local_x];
                        }
                    }
                }
            }
        }
    }

    /// Scala aktywną warstwę z warstwą pod nią (menu kontekstowe warstwy).
    pub fn merge_active_layer_down(&mut self) {
        let idx = self.doc.active_layer_index;
        if idx == 0 || idx >= self.doc.document.layer_count() {
            return;
        }
        self.push_history();
        if self.doc.document.merge_down(idx) {
            self.doc.active_layer_index = idx - 1;
            self.on_document_changed();
            self.mark_document_dirty();
        }
    }

    /// Tworzy zaznaczenie z kanału alfa aktywnej warstwy (Alfa do zaznaczenia).
    pub fn alpha_to_selection(&mut self) {
        let mask = self.doc.document.alpha_mask(self.doc.active_layer_index);
        if !mask.is_empty() {
            self.doc
                .selection
                .combine_mask(&mask, dziwak_core::selection::SelectMode::Replace);
        }
    }

    /// Zamienia kolor pierwszoplanowy na przezroczystość na aktywnej warstwie (tolerancja jak kubełka).
    pub fn color_to_alpha_active(&mut self) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let (w, h) = (
            self.doc.document.width as usize,
            self.doc.document.height as usize,
        );
        if w == 0 || h == 0 {
            return;
        }
        if self.composite_buffer.len() != w * h {
            self.composite_buffer.resize(w * h, Rgba8::TRANSPARENT);
        }
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        Self::sample_layer_to_buffer(
            &self.doc.document.layers[layer_idx],
            w,
            h,
            &mut self.composite_buffer,
        );
        let color = Rgba8::from_straight(
            self.brush_color.r(),
            self.brush_color.g(),
            self.brush_color.b(),
            255,
        );
        let keep = dziwak_core::fill::color_to_alpha_mask(
            &self.composite_buffer,
            w,
            h,
            color,
            self.fill_tolerance,
        );
        self.push_history();
        if let Some(rect) =
            dziwak_core::fill::mask_alpha(&mut self.doc.document.layers[layer_idx], &keep)
        {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Usuwa jednolite tło aktywnej warstwy: piksele połączone z brzegami obrazu (tolerancja jak kubełka) stają się przezroczyste.
    pub fn remove_uniform_background(&mut self) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let w = self.doc.document.width as usize;
        let h = self.doc.document.height as usize;
        if w == 0 || h == 0 {
            return;
        }
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        if self.composite_buffer.len() != w * h {
            self.composite_buffer.resize(w * h, Rgba8::TRANSPARENT);
        }
        Self::sample_layer_to_buffer(
            &self.doc.document.layers[layer_idx],
            w,
            h,
            &mut self.composite_buffer,
        );
        let bg = dziwak_core::fill::border_background_mask(
            &self.composite_buffer,
            w,
            h,
            self.fill_tolerance,
        );
        let keep: Vec<u8> = bg.iter().map(|&m| 255 - m).collect();
        self.push_history();
        if let Some(rect) =
            dziwak_core::fill::mask_alpha(&mut self.doc.document.layers[layer_idx], &keep)
        {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Różdżka: zaznacza obszar podobnego koloru wokół (cx, cy) z tolerancją `fill_tolerance`,
    /// łącząc z bieżącym zaznaczeniem wg `mode` (Shift = dodaj, Ctrl = odejmij, Shift+Ctrl = przetnij).
    pub fn apply_magic_wand(&mut self, cx: f32, cy: f32, mode: dziwak_core::selection::SelectMode) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        if cx < 0.0 || cy < 0.0 {
            return;
        }
        let px = cx.floor() as u32;
        let py = cy.floor() as u32;
        if px >= self.doc.document.width || py >= self.doc.document.height {
            return;
        }

        let w = self.doc.document.width as usize;
        let h = self.doc.document.height as usize;
        let len = w * h;
        if len == 0 {
            return;
        }

        if self.composite_buffer.len() != len {
            self.composite_buffer.resize(len, Rgba8::TRANSPARENT);
        }

        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);

        if self.fill_sample_all_layers {
            let _ = self.doc.document.compose_rect(
                Rect::new(0, 0, self.doc.document.width, self.doc.document.height),
                &mut self.composite_buffer,
            );
        } else {
            Self::sample_layer_to_buffer(
                &self.doc.document.layers[layer_idx],
                w,
                h,
                &mut self.composite_buffer,
            );
        }

        let mask = dziwak_core::fill::flood_fill_mask(
            &self.composite_buffer,
            w,
            h,
            px as usize,
            py as usize,
            self.fill_tolerance,
        );

        self.doc.selection.combine_mask(&mask, mode);
    }

    /// Zaznacza wszystkie piksele w całym obrazie o kolorze zbliżonym do wskazanego (T4).
    pub fn apply_select_by_color(
        &mut self,
        cx: f32,
        cy: f32,
        mode: dziwak_core::selection::SelectMode,
    ) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        if cx < 0.0 || cy < 0.0 {
            return;
        }
        let px = cx.floor() as u32;
        let py = cy.floor() as u32;
        if px >= self.doc.document.width || py >= self.doc.document.height {
            return;
        }

        let w = self.doc.document.width as usize;
        let h = self.doc.document.height as usize;
        let len = w * h;
        if len == 0 {
            return;
        }

        if self.composite_buffer.len() != len {
            self.composite_buffer.resize(len, Rgba8::TRANSPARENT);
        }

        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);

        if self.fill_sample_all_layers {
            let _ = self.doc.document.compose_rect(
                Rect::new(0, 0, self.doc.document.width, self.doc.document.height),
                &mut self.composite_buffer,
            );
        } else {
            Self::sample_layer_to_buffer(
                &self.doc.document.layers[layer_idx],
                w,
                h,
                &mut self.composite_buffer,
            );
        }

        let target_color = self.composite_buffer[py as usize * w + px as usize];
        let mask = dziwak_core::fill::color_select_mask(
            &self.composite_buffer,
            w,
            h,
            target_color,
            self.fill_tolerance,
        );

        self.doc.selection.combine_mask(&mask, mode);
    }

    /// Wypełnia spójny obszar kolorem pędzla (narzędzie Kubełek).
    pub fn apply_bucket_fill(&mut self, cx: f32, cy: f32) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        if cx < 0.0 || cy < 0.0 {
            return;
        }
        let px = cx.floor() as u32;
        let py = cy.floor() as u32;
        if px >= self.doc.document.width || py >= self.doc.document.height {
            return;
        }

        let w = self.doc.document.width as usize;
        let h = self.doc.document.height as usize;
        let len = w * h;
        if len == 0 {
            return;
        }

        if self.composite_buffer.len() != len {
            self.composite_buffer.resize(len, Rgba8::TRANSPARENT);
        }

        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);

        if self.fill_sample_all_layers {
            let _ = self.doc.document.compose_rect(
                Rect::new(0, 0, self.doc.document.width, self.doc.document.height),
                &mut self.composite_buffer,
            );
        } else {
            Self::sample_layer_to_buffer(
                &self.doc.document.layers[layer_idx],
                w,
                h,
                &mut self.composite_buffer,
            );
        }

        let mask = dziwak_core::fill::flood_fill_mask(
            &self.composite_buffer,
            w,
            h,
            px as usize,
            py as usize,
            self.fill_tolerance,
        );

        let color = Rgba8::from_straight(
            self.brush_color.r(),
            self.brush_color.g(),
            self.brush_color.b(),
            self.brush_color.a(),
        );

        // Zapisujemy stan w historii przed modyfikacją
        self.push_history();

        let sel_arg = if self.doc.selection.has_selection {
            Some(&self.doc.selection)
        } else {
            None
        };

        let layer = &mut self.doc.document.layers[layer_idx];
        if let Some(rect) = dziwak_core::fill::apply_fill(layer, &mask, color, sel_arg) {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Próbkuje kolor ze złożonego obrazu i ustawia go jako bieżący kolor pędzla (narzędzie Pipeta).
    pub fn sample_color_at(&mut self, canvas_pos: egui::Pos2) {
        if self.doc.document.width == 0 || self.doc.document.height == 0 {
            return;
        }
        if canvas_pos.x < 0.0 || canvas_pos.y < 0.0 {
            return;
        }
        let x = canvas_pos.x.floor() as u32;
        let y = canvas_pos.y.floor() as u32;
        if x >= self.doc.document.width || y >= self.doc.document.height {
            return;
        }

        let sampled =
            dziwak_core::eyedropper::sample_color(&self.doc.document, x, y, self.pipette_radius);
        let [r, g, b, a] = sampled.to_straight();
        self.brush_color = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
    }

    /// Rysuje gradient liniowy między dwoma punktami na aktywnej warstwie (narzędzie Gradient).
    pub fn apply_linear_gradient_between(&mut self, a: egui::Pos2, b: egui::Pos2) {
        if self.doc.document.layers.is_empty() {
            return;
        }

        let c0 = Rgba8::from_straight(
            self.brush_color.r(),
            self.brush_color.g(),
            self.brush_color.b(),
            self.brush_color.a(),
        );
        let c1 = Rgba8::from_straight(
            self.bg_color.r(),
            self.bg_color.g(),
            self.bg_color.b(),
            self.bg_color.a(),
        );

        // Zapisujemy stan w historii przed modyfikacją
        self.push_history();

        let sel_arg = if self.doc.selection.has_selection {
            Some(&self.doc.selection)
        } else {
            None
        };

        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let layer = &mut self.doc.document.layers[layer_idx];

        if let Some(rect) = dziwak_core::gradient::apply_linear_gradient(
            layer,
            (a.x, a.y),
            (b.x, b.y),
            c0,
            c1,
            sel_arg,
        ) {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Czyści aktywną warstwę w obrębie zaznaczenia (lub całą warstwę, gdy brak zaznaczenia): Edycja → Wyczyść (Delete).
    pub fn clear_selection_or_layer(&mut self) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let w = self.doc.document.width as usize;
        let h = self.doc.document.height as usize;
        if w == 0 || h == 0 {
            return;
        }
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let mut keep = vec![0u8; w * h];
        if self.doc.selection.has_selection {
            for y in 0..h {
                for x in 0..w {
                    keep[y * w + x] = 255 - self.doc.selection.coverage(x as u32, y as u32);
                }
            }
        }
        self.push_history();
        if let Some(rect) =
            dziwak_core::fill::mask_alpha(&mut self.doc.document.layers[layer_idx], &keep)
        {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Wypełnia zaznaczenie (lub całą aktywną warstwę, gdy brak zaznaczenia) wskazanym kolorem.
    pub fn fill_selection_with_color(&mut self, c: egui::Color32) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let w = self.doc.document.width as usize;
        let h = self.doc.document.height as usize;
        if w == 0 || h == 0 {
            return;
        }
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let mut mask = vec![255u8; w * h];
        if self.doc.selection.has_selection {
            for y in 0..h {
                for x in 0..w {
                    mask[y * w + x] = self.doc.selection.coverage(x as u32, y as u32);
                }
            }
        }
        let color = Rgba8::from_straight(c.r(), c.g(), c.b(), c.a());
        self.push_history();
        if let Some(rect) = dziwak_core::fill::apply_fill(
            &mut self.doc.document.layers[layer_idx],
            &mask,
            color,
            None,
        ) {
            self.mark_rect_dirty(rect);
            self.bump_layer_version(layer_idx);
        }
    }

    /// Wypełnia zaznaczenie (lub całą aktywną warstwę) kolorem pierwszoplanowym: Edycja → Wypełnij kolorem pierwszoplanowym (Ctrl+,).
    pub fn fill_with_fg_color(&mut self) {
        self.fill_selection_with_color(self.brush_color);
    }

    /// Wypełnia zaznaczenie (lub całą aktywną warstwę) kolorem tła: Edycja → Wypełnij kolorem tła (Ctrl+.).
    pub fn fill_with_bg_color(&mut self) {
        self.fill_selection_with_color(self.bg_color);
    }

    /// Rozpoczyna operację przesuwania aktywnej warstwy lub zawartości zaznaczenia (narzędzie Move, M).
    pub fn start_move(&mut self, canvas_pos: egui::Pos2) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        self.push_history();
        self.doc.is_moving = true;
        self.doc.move_drag_start = Some(canvas_pos);
        self.doc.move_initial_layer = Some(self.doc.document.layers[layer_idx].clone());
        if self.doc.selection.has_selection && self.doc.selection.bounds().is_some() {
            self.doc.move_initial_selection = Some(self.doc.selection.clone());
        } else {
            self.doc.move_initial_selection = None;
        }
    }

    /// Przemieszcza aktywną warstwę (lub zawartość zaznaczenia) o zadane przesunięcie w pikselach (dx, dy).
    pub fn apply_move_offset(&mut self, dx: i32, dy: i32) {
        let Some(init_layer) = self.doc.move_initial_layer.as_ref() else {
            return;
        };
        if self.doc.document.layers.is_empty() {
            return;
        }
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let w = self.doc.document.width;
        let h = self.doc.document.height;

        let mut new_layer_pixels = TiledLayer::new(w, h);
        let across = init_layer.pixels.tiles_across();

        if let Some(init_sel) = self.doc.move_initial_selection.as_ref() {
            // 1. Kopiujemy piksele nieobjęte zaznaczeniem (pozostają w pierwotnym miejscu)
            for (tile_idx, tile_opt) in init_layer.pixels.tiles.iter().enumerate() {
                if let Some(tile) = tile_opt {
                    let tx = (tile_idx as u32) % across;
                    let ty = (tile_idx as u32) / across;
                    let base_x = tx * (TILE_SIZE as u32);
                    let base_y = ty * (TILE_SIZE as u32);
                    for ly in 0..TILE_SIZE {
                        for lx in 0..TILE_SIZE {
                            let px = tile[ly * TILE_SIZE + lx];
                            if !px.is_transparent() {
                                let sx = base_x + lx as u32;
                                let sy = base_y + ly as u32;
                                if init_sel.coverage(sx, sy) == 0 {
                                    let _ = new_layer_pixels.set_pixel(sx, sy, px);
                                }
                            }
                        }
                    }
                }
            }

            // 2. Kopiujemy piksele objęte zaznaczeniem z przesunięciem (dx, dy)
            for (tile_idx, tile_opt) in init_layer.pixels.tiles.iter().enumerate() {
                if let Some(tile) = tile_opt {
                    let tx = (tile_idx as u32) % across;
                    let ty = (tile_idx as u32) / across;
                    let base_x = tx * (TILE_SIZE as u32);
                    let base_y = ty * (TILE_SIZE as u32);
                    for ly in 0..TILE_SIZE {
                        for lx in 0..TILE_SIZE {
                            let px = tile[ly * TILE_SIZE + lx];
                            if !px.is_transparent() {
                                let sx = base_x + lx as u32;
                                let sy = base_y + ly as u32;
                                if init_sel.coverage(sx, sy) > 0 {
                                    let nx = sx as i32 + dx;
                                    let ny = sy as i32 + dy;
                                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                        let _ =
                                            new_layer_pixels.set_pixel(nx as u32, ny as u32, px);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 3. Przesuwamy maskę zaznaczenia o (dx, dy)
            if let Some(bounds) = init_sel.bounds() {
                let mask_w = w as usize;
                let mask_h = h as usize;
                let mut mask = vec![0u8; mask_w * mask_h];
                for y in bounds.y..(bounds.y + bounds.height) {
                    for x in bounds.x..(bounds.x + bounds.width) {
                        let cov = init_sel.coverage(x, y);
                        if cov > 0 {
                            let nx = x as i32 + dx;
                            let ny = y as i32 + dy;
                            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                mask[ny as usize * mask_w + nx as usize] = cov;
                            }
                        }
                    }
                }
                let mut new_sel = Selection::new(w, h);
                new_sel.combine_mask(&mask, dziwak_core::selection::SelectMode::Replace);
                self.doc.selection = new_sel;
            }
        } else {
            // Przemieszczanie całej aktywnej warstwy
            for (tile_idx, tile_opt) in init_layer.pixels.tiles.iter().enumerate() {
                if let Some(tile) = tile_opt {
                    let tx = (tile_idx as u32) % across;
                    let ty = (tile_idx as u32) / across;
                    let base_x = tx * (TILE_SIZE as u32);
                    let base_y = ty * (TILE_SIZE as u32);
                    for ly in 0..TILE_SIZE {
                        for lx in 0..TILE_SIZE {
                            let px = tile[ly * TILE_SIZE + lx];
                            if !px.is_transparent() {
                                let sx = base_x + lx as u32;
                                let sy = base_y + ly as u32;
                                let nx = sx as i32 + dx;
                                let ny = sy as i32 + dy;
                                if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                                    let _ = new_layer_pixels.set_pixel(nx as u32, ny as u32, px);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.doc.document.layers[layer_idx].pixels = new_layer_pixels;
        self.mark_document_dirty();
        self.bump_layer_version(layer_idx);
    }

    /// Kończy operację przesuwania aktywnej warstwy lub zaznaczenia.
    pub fn finish_move(&mut self) {
        if self.doc.is_moving {
            self.doc.is_moving = false;
            self.doc.move_drag_start = None;
            self.doc.move_initial_layer = None;
            self.doc.move_initial_selection = None;
            self.doc.is_modified = true;
        }
    }

    /// Przycina dokument (wszystkie warstwy) do podanego prostokąta `rect` i rejestruje operację w historii (T2).
    pub fn crop_document(&mut self, rect: Rect) {
        if rect.width == 0 || rect.height == 0 {
            self.cancel_crop();
            return;
        }
        if rect.x == 0
            && rect.y == 0
            && rect.width == self.doc.document.width
            && rect.height == self.doc.document.height
        {
            self.cancel_crop();
            return;
        }
        self.push_history();
        self.doc.document.crop(rect);
        self.cancel_crop();
        self.on_document_changed();
    }

    /// Zatwierdza bieżącą ramkę kadrowania (z `crop_rect` lub aktywnego przeciągania).
    pub fn commit_crop(&mut self) -> bool {
        let rect = if let (Some(start), Some(curr)) =
            (self.doc.crop_drag_start, self.doc.crop_drag_current)
        {
            let min_x = start.x.min(curr.x).max(0.0);
            let min_y = start.y.min(curr.y).max(0.0);
            let max_x = start.x.max(curr.x).min(self.doc.document.width as f32);
            let max_y = start.y.max(curr.y).min(self.doc.document.height as f32);
            let w = (max_x - min_x).max(0.0);
            let h = (max_y - min_y).max(0.0);
            if w >= 1.0 && h >= 1.0 {
                Some(Rect::new(
                    min_x.floor() as u32,
                    min_y.floor() as u32,
                    w.ceil() as u32,
                    h.ceil() as u32,
                ))
            } else {
                None
            }
        } else {
            self.doc.crop_rect
        };

        if let Some(r) = rect {
            if r.width > 0 && r.height > 0 {
                self.crop_document(r);
                return true;
            }
        }
        false
    }

    /// Anuluje aktywną ramkę kadrowania.
    pub fn cancel_crop(&mut self) {
        self.doc.crop_rect = None;
        self.doc.crop_drag_start = None;
        self.doc.crop_drag_current = None;
    }

    /// Przycina dokument do ramki otaczającej aktywnego zaznaczenia (Obraz -> Przytnij do zaznaczenia).
    pub fn crop_to_selection(&mut self) -> bool {
        if !self.doc.selection.has_selection {
            return false;
        }
        if let Some(bounds) = self.doc.selection.bounds() {
            if bounds.width > 0 && bounds.height > 0 {
                self.crop_document(bounds);
                return true;
            }
        }
        false
    }

    /// Dodaje punkt do aktywnego zaznaczenia odręcznego (T3).
    pub fn add_free_select_point(
        &mut self,
        pt: (f32, f32),
        mode: dziwak_core::selection::SelectMode,
    ) {
        if self.doc.free_select_points.is_empty() {
            self.doc.free_select_mode = mode;
            self.doc.is_free_selecting = true;
        }
        self.doc.free_select_points.push(pt);
    }

    /// Kończy i zatwierdza zaznaczenie odręczne, generując wygładzoną maskę wielokąta (T3).
    pub fn finish_free_select(&mut self) -> bool {
        if self.doc.free_select_points.len() < 3 {
            self.cancel_free_select();
            return false;
        }

        let w = self.doc.document.width as usize;
        let h = self.doc.document.height as usize;
        if w == 0 || h == 0 {
            self.cancel_free_select();
            return false;
        }

        let mask = dziwak_core::polygon::polygon_mask(&self.doc.free_select_points, w, h);
        self.doc
            .selection
            .combine_mask(&mask, self.doc.free_select_mode);
        self.cancel_free_select();
        true
    }

    /// Anuluje bieżące zaznaczenie odręczne.
    pub fn cancel_free_select(&mut self) {
        self.doc.free_select_points.clear();
        self.doc.is_free_selecting = false;
    }

    /// Otwiera okno dialogowe filtra i inicjuje podgląd na aktywnej warstwie.
    pub fn open_filter_dialog(&mut self, kind: FilterKind) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        self.cancel_filter_dialog();

        let target_layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let original_layer = self.doc.document.layers[target_layer_idx].clone();

        self.doc.filter_dialog = Some(FilterDialog {
            kind,
            preview: true,
            original_layer,
            target_layer_idx,
            last_apply_time: None,
            pending_apply: false,
            last_changed_rect: None,
        });

        self.apply_filter_preview();
        if let Some(dialog) = &mut self.doc.filter_dialog {
            dialog.last_apply_time = Some(Instant::now());
        }
    }

    /// Stosuje podgląd filtra na docelowej warstwie (przywracając najpierw czysty klon).
    pub fn apply_filter_preview(&mut self) {
        let Some(mut dialog) = self.doc.filter_dialog.take() else {
            return;
        };
        if dialog.target_layer_idx >= self.doc.document.layers.len() {
            self.doc.filter_dialog = Some(dialog);
            return;
        }

        // 1. Oznaczamy poprzedni zmieniony prostokąt jako brudny, aby przywrócone kafle się odświeżyły
        if let Some(old_rect) = dialog.last_changed_rect.take() {
            self.mark_rect_dirty(old_rect);
        }

        // 2. Przywracamy czysty klon warstwy sprzed nałożenia filtra
        self.doc.document.layers[dialog.target_layer_idx] = dialog.original_layer.clone();

        // 3. Jeśli podgląd jest włączony, aplikujemy filtr na warstwie
        if dialog.preview {
            let sel_arg = if self.doc.selection.has_selection {
                Some(&self.doc.selection)
            } else {
                None
            };
            let rect = dialog.kind.apply(
                &mut self.doc.document.layers[dialog.target_layer_idx].pixels,
                sel_arg,
            );
            if let Some(new_rect) = rect {
                self.mark_rect_dirty(new_rect);
                dialog.last_changed_rect = Some(new_rect);
            }
        }

        self.doc.filter_dialog = Some(dialog);
    }

    /// Anuluje działanie filtra: przywraca stan warstwy sprzed otwarcia okna i zamyka dialog.
    pub fn cancel_filter_dialog(&mut self) {
        if let Some(dialog) = self.doc.filter_dialog.take() {
            if dialog.target_layer_idx < self.doc.document.layers.len() {
                self.doc.document.layers[dialog.target_layer_idx] = dialog.original_layer;
                if let Some(old_rect) = dialog.last_changed_rect {
                    self.mark_rect_dirty(old_rect);
                }
            }
        }
    }

    /// Zatwierdza działanie filtra: zapisuje w historii stan SPRZED filtra, zachowuje wynik i zamyka okno.
    pub fn apply_filter_dialog(&mut self) {
        let Some(dialog) = self.doc.filter_dialog.take() else {
            return;
        };
        if dialog.target_layer_idx >= self.doc.document.layers.len() {
            return;
        }

        // Jeśli podgląd był wyłączony lub były oczekujące zmiany suwaka (debounce),
        // aplikujemy ostateczne wartości filtra:
        if dialog.pending_apply || !dialog.preview {
            self.doc.document.layers[dialog.target_layer_idx] = dialog.original_layer.clone();
            let sel_arg = if self.doc.selection.has_selection {
                Some(&self.doc.selection)
            } else {
                None
            };
            let rect = dialog.kind.apply(
                &mut self.doc.document.layers[dialog.target_layer_idx].pixels,
                sel_arg,
            );
            if let Some(old_rect) = dialog.last_changed_rect {
                self.mark_rect_dirty(old_rect);
            }
            if let Some(new_rect) = rect {
                self.mark_rect_dirty(new_rect);
            }
        }

        // W dokumencie znajduje się teraz warstwa z nałożonym filtrem.
        // Zgodnie z wytycznymi: stan SPRZED filtra (z klona) zapisujemy na stosie historii.
        let filtered_layer = std::mem::replace(
            &mut self.doc.document.layers[dialog.target_layer_idx],
            dialog.original_layer,
        );

        self.push_history();

        // Przywracamy warstwę przefiltrowaną
        self.doc.document.layers[dialog.target_layer_idx] = filtered_layer;
    }

    /// Przełącza aktywne narzędzie i w razie potrzeby otwiera odpowiednie okno dialogowe (np. transformacji)
    /// lub zatwierdza aktywną ramkę tekstową.
    pub fn set_active_tool(&mut self, target_tool: ActiveTool) {
        if self.active_tool == ActiveTool::Text && target_tool != ActiveTool::Text {
            self.commit_text();
        }
        self.active_tool = target_tool;
        match target_tool {
            ActiveTool::Rotate => {
                self.open_transform_dialog(LayerTransformKind::Rotate { angle_deg: 0.0 });
            }
            ActiveTool::Scale => {
                let (w, h) = (self.doc.document.width, self.doc.document.height);
                self.open_transform_dialog(LayerTransformKind::Scale {
                    width: w,
                    height: h,
                    orig_width: w,
                    orig_height: h,
                    keep_aspect: true,
                });
            }
            ActiveTool::Flip => {
                self.open_transform_dialog(LayerTransformKind::Flip { horizontal: true });
            }
            _ => {}
        }
    }

    /// Zwraca `true`, jeśli na płótnie aktywna jest ramka tekstowa narzędzia Tekst.
    pub fn is_editing_text(&self) -> bool {
        self.active_tool == ActiveTool::Text && self.doc.text_editor.is_some()
    }

    /// Sprawdza, czy warstwa tekstowa przypisana do aktywnej ramki nadal istnieje i jest aktywna.
    pub fn is_text_layer_valid(&self) -> bool {
        let Some(editor) = &self.doc.text_editor else {
            return false;
        };
        editor.layer_index < self.doc.document.layer_count()
            && self.doc.active_layer_index == editor.layer_index
            && self.doc.document.layer_count() == editor.expected_layer_count
            && self
                .doc
                .document
                .layers
                .get(editor.layer_index)
                .is_some_and(|l| l.name == "Tekst" || l.name.starts_with("Tekst: "))
    }

    /// Kończy edycję tekstu bez modyfikowania stosu warstw (np. po usunięciu warstwy lub zmianie aktywnej warstwy).
    pub fn abort_text_editing(&mut self) {
        self.doc.text_editor = None;
        self.doc.is_moving_text_box = false;
        self.doc.text_drag_start = None;
        self.doc.text_drag_current = None;
    }

    /// Weryfikuje poprawność warstwy tekstowej; jeśli użytkownik zmienił aktywną warstwę
    /// lub usunął warstwę tekstową, kończy edycję bez awarii i zwraca `false`.
    pub fn ensure_text_editor_valid(&mut self) -> bool {
        if self.doc.text_editor.is_none() {
            return false;
        }
        if !self.is_text_layer_valid() {
            self.abort_text_editing();
            return false;
        }
        true
    }

    /// Tworzy nową ramkę tekstową na płótnie w punkcie `(x, y)` o minimalnym rozmiarze `(min_w, min_h)`.
    /// Jeśli wcześniejsza ramka była aktywna, najpierw ją zatwierdza.
    /// Zapisuje migawkę w historii i od razu tworzy nową pustą warstwę 'Tekst' nad aktywną warstwą.
    pub fn start_text_box(&mut self, x: i32, y: i32, min_w: f32, min_h: f32) {
        if self.doc.text_editor.is_some() {
            self.commit_text();
        }
        self.push_history();
        let prev_active = self
            .doc
            .active_layer_index
            .min(self.doc.document.layer_count().saturating_sub(1));
        let insert_idx = if self.doc.document.layers.is_empty() {
            0
        } else {
            (prev_active + 1).min(self.doc.document.layer_count())
        };
        let new_layer = Layer::new("Tekst", self.doc.document.width, self.doc.document.height);
        let _ = self.doc.document.insert_layer(insert_idx, new_layer);
        self.doc.active_layer_index = insert_idx;
        let expected_layer_count = self.doc.document.layer_count();

        self.doc.text_editor = Some(TextEditorState {
            text: String::new(),
            cursor: 0,
            pos: (x, y),
            min_size: (min_w.max(0.0), min_h.max(0.0)),
            layer_index: insert_idx,
            prev_active_layer_index: prev_active,
            expected_layer_count,
            last_stamped_rect: None,
            preview_mask: None,
            preview_color: self.brush_color,
            preview_dirty: true,
            caret_x_positions: vec![vec![0.0]],
            base_line_height: self.text_font_size,
            error: None,
        });
        self.update_text_preview_data();
    }

    /// Przesuwa aktywną ramkę tekstową do pozycji `(x, y)` i od razu przerysowuje zawartość warstwy tekstowej.
    pub fn set_text_box_pos(&mut self, x: i32, y: i32) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        let changed = if let Some(editor) = &mut self.doc.text_editor {
            if editor.pos != (x, y) {
                editor.pos = (x, y);
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed {
            self.redraw_text_layer();
        }
    }

    /// Przerysowuje zawartość i nazwę warstwy tekstowej na podstawie bieżącego stanu `text_editor`.
    pub fn redraw_text_layer(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        let Some(editor) = &mut self.doc.text_editor else {
            return;
        };
        let layer_idx = editor.layer_index;
        let first_20: String = editor
            .text
            .chars()
            .map(|c| if c == '\n' { ' ' } else { c })
            .take(20)
            .collect();
        let layer_name = if first_20.trim().is_empty() {
            "Tekst".to_string()
        } else {
            format!("Tekst: {first_20}")
        };
        let pos = editor.pos;
        let mask_clone = editor.preview_mask.clone();
        let old_rect = editor.last_stamped_rect.take();
        editor.preview_color = self.brush_color;

        if let Some(old_r) = old_rect {
            self.mark_rect_dirty(old_r);
        }

        let color = Rgba8::from_straight(
            self.brush_color.r(),
            self.brush_color.g(),
            self.brush_color.b(),
            self.brush_color.a(),
        );

        let new_rect = if let Some(layer) = self.doc.document.layers.get_mut(layer_idx) {
            layer.name = layer_name;
            layer.pixels.clear();
            if let Some(mask) = &mask_clone {
                dziwak_core::text::stamp_mask(layer, mask, pos.0, pos.1, color)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(editor) = &mut self.doc.text_editor {
            editor.last_stamped_rect = new_rect;
        }
        if let Some(new_r) = new_rect {
            self.mark_rect_dirty(new_r);
        }
        self.mark_document_dirty();
        self.bump_layer_version(layer_idx);
    }

    /// Oznacza podgląd aktywnej ramki tekstowej jako wymagający przeliczenia (po zmianie opcji).
    pub fn mark_text_preview_dirty(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        if let Some(editor) = &mut self.doc.text_editor {
            editor.preview_dirty = true;
        }
        self.update_text_preview_data();
    }

    /// Wczytuje (z pamięcią podręczną) bajty wybranej rodziny czcionki, przelicza maskę oraz pozycje kursora
    /// i od razu przerysowuje warstwę tekstową.
    pub fn update_text_preview_data(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        let family = self.text_font_family.clone();
        let need_reload = self
            .cached_font_bytes
            .as_ref()
            .is_none_or(|(cached_fam, _)| cached_fam != &family);
        if need_reload {
            if let Some(path) = find_font_path(&family) {
                if let Ok(bytes) = std::fs::read(&path) {
                    self.cached_font_bytes = Some((family.clone(), bytes));
                } else {
                    self.cached_font_bytes = None;
                }
            } else {
                self.cached_font_bytes = None;
            }
        }

        if let Some(editor) = &mut self.doc.text_editor {
            editor.preview_dirty = false;

            if let Some((_, font_data)) = &self.cached_font_bytes {
                let size_px = self.text_font_size.max(4.0);
                let lh = dziwak_core::text::font_line_height(font_data, size_px);
                editor.base_line_height = if lh > 0.0 { lh } else { size_px };
                editor.caret_x_positions =
                    dziwak_core::text::caret_positions(font_data, &editor.text, size_px);
                if editor.caret_x_positions.is_empty() {
                    editor.caret_x_positions = vec![vec![0.0]];
                }

                if editor.text.is_empty() {
                    editor.preview_mask = None;
                    editor.error = None;
                } else {
                    let mask = if self.text_align == dziwak_core::text::TextAlign::Left
                        && self.text_line_spacing.abs() < 1e-4
                    {
                        dziwak_core::text::rasterize_text(font_data, &editor.text, size_px)
                    } else {
                        dziwak_core::text::rasterize_text_with_options(
                            font_data,
                            &editor.text,
                            size_px,
                            self.text_align,
                            self.text_line_spacing,
                            editor.min_size.0,
                        )
                    };
                    editor.preview_mask = mask;
                    editor.error = None;
                }
            } else {
                editor.preview_mask = None;
                editor.caret_x_positions = vec![vec![0.0]];
                editor.base_line_height = self.text_font_size.max(8.0);
                editor.error = Some(format!("Nie znaleziono czcionki: {family}"));
            }
        }

        self.redraw_text_layer();
    }

    /// Zwraca prostokąt aktywnej ramki tekstowej w przestrzeni płótna.
    pub fn text_box_canvas_rect(&self) -> Option<egui::Rect> {
        let editor = self.doc.text_editor.as_ref()?;
        let base_lh = editor.base_line_height.max(self.text_font_size).max(8.0);
        let step_lh = (editor.base_line_height.max(8.0) + self.text_line_spacing).max(1.0);
        let num_lines = editor.text.split('\n').count().max(1);
        let lines_h = if num_lines <= 1 {
            base_lh
        } else {
            (num_lines - 1) as f32 * step_lh + base_lh
        };
        let max_caret_x = editor
            .caret_x_positions
            .iter()
            .filter_map(|v| v.last().copied())
            .fold(0.0_f32, f32::max);
        let (mask_w, mask_h) = editor
            .preview_mask
            .as_ref()
            .map_or((0.0, 0.0), |m| (m.width as f32, m.height as f32));

        let w = editor
            .min_size
            .0
            .max(mask_w)
            .max(max_caret_x + 4.0)
            .max((self.text_font_size * 0.6).max(16.0));
        let h = editor.min_size.1.max(mask_h).max(lines_h).max(16.0);
        Some(egui::Rect::from_min_size(
            egui::pos2(editor.pos.0 as f32, editor.pos.1 as f32),
            egui::vec2(w, h),
        ))
    }

    /// Zwraca odcinek pionowy kursora tekstowego `(x, y_top, y_bottom)` w przestrzeni płótna.
    pub fn text_caret_canvas_segment(&self) -> Option<(f32, f32, f32)> {
        let editor = self.doc.text_editor.as_ref()?;
        let (line_idx, col_idx) = editor.cursor_line_col();
        let line_xs = editor.caret_x_positions.get(line_idx);
        let local_x = line_xs
            .and_then(|xs| xs.get(col_idx).or_else(|| xs.last()))
            .copied()
            .unwrap_or(0.0);
        let line_w = line_xs.and_then(|xs| xs.last()).copied().unwrap_or(0.0);
        let max_caret_x = editor
            .caret_x_positions
            .iter()
            .filter_map(|v| v.last().copied())
            .fold(0.0_f32, f32::max);
        let align_box_w = max_caret_x.max(editor.min_size.0);
        let offset_x = match self.text_align {
            dziwak_core::text::TextAlign::Left => 0.0,
            dziwak_core::text::TextAlign::Center => ((align_box_w - line_w) * 0.5).max(0.0),
            dziwak_core::text::TextAlign::Right => (align_box_w - line_w).max(0.0),
        };
        let base_lh = editor.base_line_height.max(8.0);
        let step_lh = (base_lh + self.text_line_spacing).max(1.0);
        let x = editor.pos.0 as f32 + offset_x + local_x;
        let y0 = editor.pos.1 as f32 + (line_idx as f32) * step_lh;
        let y1 = y0 + base_lh;
        Some((x, y0, y1))
    }

    /// Ustawia kursor tekstowy w miejscu kliknięcia wewnątrz ramki (współrzędne płótna).
    pub fn set_text_cursor_from_canvas_pos(&mut self, canvas_pos: egui::Pos2) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        let Some(editor) = &mut self.doc.text_editor else {
            return;
        };
        let base_lh = editor.base_line_height.max(8.0);
        let step_lh = (base_lh + self.text_line_spacing).max(1.0);
        let rel_y = (canvas_pos.y - editor.pos.1 as f32).max(0.0);
        let num_lines = editor.caret_x_positions.len().max(1);
        let line_idx = ((rel_y / step_lh).floor() as usize).min(num_lines - 1);

        let max_caret_x = editor
            .caret_x_positions
            .iter()
            .filter_map(|v| v.last().copied())
            .fold(0.0_f32, f32::max);
        let align_box_w = max_caret_x.max(editor.min_size.0);
        let line_xs = editor.caret_x_positions.get(line_idx);
        let line_w = line_xs.and_then(|xs| xs.last()).copied().unwrap_or(0.0);
        let offset_x = match self.text_align {
            dziwak_core::text::TextAlign::Left => 0.0,
            dziwak_core::text::TextAlign::Center => ((align_box_w - line_w) * 0.5).max(0.0),
            dziwak_core::text::TextAlign::Right => (align_box_w - line_w).max(0.0),
        };
        let rel_x = canvas_pos.x - editor.pos.0 as f32 - offset_x;
        let col_idx = if let Some(xs) = line_xs {
            xs.iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    ((*a - rel_x).abs())
                        .partial_cmp(&((*b - rel_x).abs()))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        } else {
            0
        };
        editor.cursor = TextEditorState::line_col_to_cursor(&editor.text, line_idx, col_idx);
    }

    fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
        s.char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(s.len())
    }

    /// Wstawia ciąg znaków w miejscu kursora aktywnej ramki tekstowej.
    pub fn text_insert_str(&mut self, s: &str) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        let filtered: String = s
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .filter(|&c| c == '\n' || !c.is_control())
            .collect();
        if filtered.is_empty() {
            return;
        }
        let Some(editor) = &mut self.doc.text_editor else {
            return;
        };
        let total_chars = editor.text.chars().count();
        editor.cursor = editor.cursor.min(total_chars);
        let byte_idx = Self::char_to_byte_idx(&editor.text, editor.cursor);
        editor.text.insert_str(byte_idx, &filtered);
        editor.cursor += filtered.chars().count();
        self.update_text_preview_data();
    }

    /// Wstawia znak nowej linii (`\n`) w miejscu kursora aktywnej ramki tekstowej.
    pub fn text_insert_newline(&mut self) {
        self.text_insert_str("\n");
    }

    /// Usuwa znak przed kursorem (Backspace); na początku bufora nic nie robi.
    pub fn text_backspace(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        let Some(editor) = &mut self.doc.text_editor else {
            return;
        };
        let total_chars = editor.text.chars().count();
        editor.cursor = editor.cursor.min(total_chars);
        if editor.cursor == 0 {
            return;
        }
        editor.cursor -= 1;
        let byte_idx = Self::char_to_byte_idx(&editor.text, editor.cursor);
        editor.text.remove(byte_idx);
        self.update_text_preview_data();
    }

    /// Usuwa znak za kursorem (Delete); na końcu bufora nic nie robi.
    pub fn text_delete(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        let Some(editor) = &mut self.doc.text_editor else {
            return;
        };
        let total_chars = editor.text.chars().count();
        editor.cursor = editor.cursor.min(total_chars);
        if editor.cursor >= total_chars {
            return;
        }
        let byte_idx = Self::char_to_byte_idx(&editor.text, editor.cursor);
        editor.text.remove(byte_idx);
        self.update_text_preview_data();
    }

    /// Przesuwa kursor o jeden znak w lewo.
    pub fn text_move_left(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        if let Some(editor) = &mut self.doc.text_editor {
            let total_chars = editor.text.chars().count();
            editor.cursor = editor.cursor.min(total_chars).saturating_sub(1);
        }
    }

    /// Przesuwa kursor o jeden znak w prawo.
    pub fn text_move_right(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        if let Some(editor) = &mut self.doc.text_editor {
            let total_chars = editor.text.chars().count();
            if editor.cursor < total_chars {
                editor.cursor += 1;
            }
        }
    }

    /// Przesuwa kursor do poprzedniej linii (strzałka w górę).
    pub fn text_move_up(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        if let Some(editor) = &mut self.doc.text_editor {
            let (line, col) = editor.cursor_line_col();
            if line > 0 {
                editor.cursor = TextEditorState::line_col_to_cursor(&editor.text, line - 1, col);
            } else {
                editor.cursor = 0;
            }
        }
    }

    /// Przesuwa kursor do następnej linii (strzałka w dół).
    pub fn text_move_down(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        if let Some(editor) = &mut self.doc.text_editor {
            let (line, col) = editor.cursor_line_col();
            let num_lines = editor.text.split('\n').count();
            if line + 1 < num_lines {
                editor.cursor = TextEditorState::line_col_to_cursor(&editor.text, line + 1, col);
            } else {
                editor.cursor = editor.text.chars().count();
            }
        }
    }

    /// Przesuwa kursor na początek bieżącej linii (Home).
    pub fn text_move_home(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        if let Some(editor) = &mut self.doc.text_editor {
            let (line, _) = editor.cursor_line_col();
            editor.cursor = TextEditorState::line_col_to_cursor(&editor.text, line, 0);
        }
    }

    /// Przesuwa kursor na koniec bieżącej linii (End).
    pub fn text_move_end(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        if let Some(editor) = &mut self.doc.text_editor {
            let (line, _) = editor.cursor_line_col();
            let line_len = editor
                .text
                .split('\n')
                .nth(line)
                .map_or(0, |l| l.chars().count());
            editor.cursor = TextEditorState::line_col_to_cursor(&editor.text, line, line_len);
        }
    }

    /// Kończy edycję aktywnej ramki tekstowej (warstwa tekstowa już istnieje w dokumencie).
    /// Jeśli tekst jest pusty — usuwa warstwę tekstową i przywraca poprzednią aktywną warstwę.
    pub fn commit_text(&mut self) -> bool {
        if !self.ensure_text_editor_valid() {
            return false;
        }
        if self
            .doc
            .text_editor
            .as_ref()
            .is_some_and(|e| e.preview_dirty || e.preview_color != self.brush_color)
        {
            self.update_text_preview_data();
        }
        let Some(editor) = self.doc.text_editor.take() else {
            return false;
        };
        self.doc.is_moving_text_box = false;
        self.doc.text_drag_start = None;
        self.doc.text_drag_current = None;

        if editor.text.trim().is_empty() {
            if editor.layer_index < self.doc.document.layer_count() {
                let _ = self.doc.document.remove_layer(editor.layer_index);
                self.doc.active_layer_index = editor
                    .prev_active_layer_index
                    .min(self.doc.document.layer_count().saturating_sub(1));
                self.ensure_layer_caches();
                self.mark_document_dirty();
            }
            return false;
        }

        self.bump_layer_version(editor.layer_index);
        self.mark_document_dirty();
        true
    }

    /// Anuluje edycję tekstu na płótnie (Esc): usuwa warstwę tekstową i przywraca poprzednią aktywną warstwę.
    pub fn cancel_text(&mut self) {
        if !self.ensure_text_editor_valid() {
            return;
        }
        let Some(editor) = self.doc.text_editor.take() else {
            return;
        };
        self.doc.is_moving_text_box = false;
        self.doc.text_drag_start = None;
        self.doc.text_drag_current = None;

        if editor.layer_index < self.doc.document.layer_count() {
            let _ = self.doc.document.remove_layer(editor.layer_index);
            self.doc.active_layer_index = editor
                .prev_active_layer_index
                .min(self.doc.document.layer_count().saturating_sub(1));
            self.ensure_layer_caches();
            self.mark_document_dirty();
        }
    }

    /// Otwiera okno dialogowe przekształcenia aktywnej warstwy (T6) i inicjuje podgląd.
    pub fn open_transform_dialog(&mut self, kind: LayerTransformKind) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        self.cancel_filter_dialog();
        self.cancel_transform_dialog();

        let target_layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        let original_layer = self.doc.document.layers[target_layer_idx].clone();

        self.doc.transform_dialog = Some(TransformDialog {
            kind,
            preview: true,
            original_layer,
            target_layer_idx,
            last_apply_time: None,
            pending_apply: false,
        });

        self.apply_transform_preview();
        if let Some(dialog) = &mut self.doc.transform_dialog {
            dialog.last_apply_time = Some(Instant::now());
        }
    }

    /// Stosuje podgląd przekształcenia na docelowej warstwie.
    pub fn apply_transform_preview(&mut self) {
        let Some(dialog) = self.doc.transform_dialog.as_ref() else {
            return;
        };
        if dialog.target_layer_idx >= self.doc.document.layers.len() {
            return;
        }

        if dialog.preview {
            let new_pixels = dialog.kind.apply(&dialog.original_layer.pixels);
            self.doc.document.layers[dialog.target_layer_idx].pixels = new_pixels;
        } else {
            self.doc.document.layers[dialog.target_layer_idx].pixels =
                dialog.original_layer.pixels.clone();
        }

        let target = dialog.target_layer_idx;
        self.mark_document_dirty();
        self.bump_layer_version(target);
    }

    /// Anuluje działanie okna transformacji: przywraca stan warstwy i zamyka dialog.
    pub fn cancel_transform_dialog(&mut self) {
        if let Some(dialog) = self.doc.transform_dialog.take() {
            if dialog.target_layer_idx < self.doc.document.layers.len() {
                self.doc.document.layers[dialog.target_layer_idx] = dialog.original_layer;
                self.mark_document_dirty();
                self.bump_layer_version(dialog.target_layer_idx);
            }
        }
    }

    /// Zatwierdza przekształcenie warstwy: zapisuje stan w historii i zamyka okno dialogowe.
    pub fn apply_transform_dialog(&mut self) {
        let Some(dialog) = self.doc.transform_dialog.take() else {
            return;
        };
        if dialog.target_layer_idx >= self.doc.document.layers.len() {
            return;
        }

        let new_pixels = dialog.kind.apply(&dialog.original_layer.pixels);
        self.doc.document.layers[dialog.target_layer_idx] = dialog.original_layer;
        self.push_history();
        self.doc.document.layers[dialog.target_layer_idx].pixels = new_pixels;
        self.mark_document_dirty();
        self.bump_layer_version(dialog.target_layer_idx);
    }

    /// Aktualizuje renderer kafli po zmianie dokumentu i oznacza wszystkie kafle jako brudne.
    pub fn on_document_changed(&mut self) {
        self.cancel_filter_dialog();
        self.cancel_transform_dialog();
        self.cancel_scale_image_dialog();
        self.cancel_canvas_size_dialog();
        self.abort_text_editing();
        if self.doc.document.layer_count() > 0 {
            self.doc.active_layer_index = self
                .doc
                .active_layer_index
                .min(self.doc.document.layer_count() - 1);
        } else {
            self.doc.active_layer_index = 0;
        }
        if let Some(idx) = self.doc.editing_layer_index {
            if idx >= self.doc.document.layer_count() {
                self.doc.editing_layer_index = None;
                self.doc.editing_layer_name.clear();
            }
        }
        let w = self.doc.document.width;
        let h = self.doc.document.height;
        self.doc.selection = Selection::new(w, h);
        self.doc.selection_drag_start = None;
        self.doc.selection_drag_current = None;
        self.doc.gradient_drag_start = None;
        self.doc.gradient_drag_current = None;
        self.doc.crop_rect = None;
        self.doc.crop_drag_start = None;
        self.doc.crop_drag_current = None;
        self.doc.free_select_points.clear();
        self.doc.is_free_selecting = false;
        self.doc.layer_thumbnails.clear();
        self.doc.layer_versions.clear();
        self.ensure_layer_caches();
        self.composite_buffer.clear();
        let expected_across = if w == 0 {
            0
        } else {
            (w - 1) / (TILE_SIZE as u32) + 1
        };
        let expected_down = if h == 0 {
            0
        } else {
            (h - 1) / (TILE_SIZE as u32) + 1
        };
        if self.doc.tile_renderer.tiles_across != expected_across
            || self.doc.tile_renderer.tiles_down != expected_down
        {
            self.doc.tile_renderer = TileRenderer::new(w, h);
        }
        self.mark_document_dirty();
    }

    /// Otwiera okno dialogowe "Skaluj obraz..." (T7).
    pub fn open_scale_image_dialog(&mut self) {
        let (w, h) = (self.doc.document.width, self.doc.document.height);
        self.doc.scale_image_dialog = Some(ScaleImageDialog {
            width: w,
            height: h,
            orig_width: w,
            orig_height: h,
            keep_aspect: true,
        });
    }

    /// Anuluje / zamyka okno dialogowe "Skaluj obraz...".
    pub fn cancel_scale_image_dialog(&mut self) {
        self.doc.scale_image_dialog = None;
    }

    /// Otwiera okno dialogowe "Rozmiar płótna..." (T7).
    pub fn open_canvas_size_dialog(&mut self) {
        let (w, h) = (self.doc.document.width, self.doc.document.height);
        self.doc.canvas_size_dialog = Some(CanvasSizeDialog {
            width: w,
            height: h,
            orig_width: w,
            orig_height: h,
            offset_x: 0,
            offset_y: 0,
        });
    }

    /// Anuluje / zamyka okno dialogowe "Rozmiar płótna...".
    pub fn cancel_canvas_size_dialog(&mut self) {
        self.doc.canvas_size_dialog = None;
    }

    /// Skaluje cały dokument (wszystkie warstwy) do wymiarów (new_w, new_h) (T7).
    pub fn scale_image(&mut self, new_w: u32, new_h: u32) {
        if new_w == 0 || new_h == 0 {
            return;
        }
        self.push_history();
        self.doc.document.scale(new_w, new_h);
        self.on_document_changed();
    }

    /// Zmienia rozmiar płótna dla wszystkich warstw z przesunięciem (off_x, off_y) (T7).
    pub fn resize_canvas(&mut self, new_w: u32, new_h: u32, off_x: i32, off_y: i32) {
        if new_w == 0 || new_h == 0 {
            return;
        }
        self.push_history();
        self.doc.document.resize_canvas(new_w, new_h, off_x, off_y);
        self.on_document_changed();
    }

    /// Obraca cały dokument o 90 stopni zgodnie z ruchem wskazówek zegara (T7).
    pub fn rotate_image_90_cw(&mut self) {
        self.push_history();
        self.doc.document.rotate90_cw();
        self.on_document_changed();
    }

    /// Obraca cały dokument o 90 stopni przeciwnie do ruchu wskazówek zegara (T7).
    pub fn rotate_image_90_ccw(&mut self) {
        self.push_history();
        self.doc.document.rotate90_ccw();
        self.on_document_changed();
    }

    /// Obraca cały dokument o 180 stopni (T7).
    pub fn rotate_image_180(&mut self) {
        self.push_history();
        self.doc.document.rotate180();
        self.on_document_changed();
    }

    /// Odbija cały dokument w poziomie (T7).
    pub fn flip_image_horizontal(&mut self) {
        self.push_history();
        self.doc.document.flip_horizontal();
        self.on_document_changed();
    }

    /// Odbija cały dokument w pionie (T7).
    pub fn flip_image_vertical(&mut self) {
        self.push_history();
        self.doc.document.flip_vertical();
        self.on_document_changed();
    }

    /// Dodaje nową pustą warstwę powyżej aktywnej i zaznacza ją.
    pub fn add_new_layer(&mut self) {
        self.abort_text_editing();
        self.push_history();
        let new_name = format!("Warstwa {}", self.doc.document.layer_count() + 1);
        let new_layer = Layer::new(new_name, self.doc.document.width, self.doc.document.height);
        let insert_idx = if self.doc.document.layers.is_empty() {
            0
        } else {
            (self.doc.active_layer_index + 1).min(self.doc.document.layer_count())
        };
        let _ = self.doc.document.insert_layer(insert_idx, new_layer);
        self.doc.active_layer_index = insert_idx;
        self.mark_document_dirty();
    }

    /// Duplikuje aktywną warstwę (współdzieląc kafle CoW) i wstawia ją powyżej.
    pub fn duplicate_active_layer(&mut self) {
        if self.doc.document.layers.is_empty() {
            return;
        }
        self.abort_text_editing();
        self.push_history();
        let active_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layer_count() - 1);
        let dup_layer = self.doc.document.layers[active_idx].duplicate(format!(
            "{} (kopia)",
            self.doc.document.layers[active_idx].name
        ));
        let insert_idx = active_idx + 1;
        let _ = self.doc.document.insert_layer(insert_idx, dup_layer);
        self.doc.active_layer_index = insert_idx;
        self.mark_document_dirty();
    }

    /// Usuwa aktywną warstwę (jeśli w dokumencie znajduje się więcej niż jedna warstwa).
    pub fn remove_active_layer(&mut self) {
        if self.doc.document.layer_count() <= 1 {
            return;
        }
        self.abort_text_editing();
        self.push_history();
        let remove_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layer_count() - 1);
        self.doc.document.remove_layer(remove_idx);
        self.doc.active_layer_index =
            remove_idx.min(self.doc.document.layer_count().saturating_sub(1));
        self.mark_document_dirty();
    }

    /// Spłaszcza wszystkie warstwy dokumentu do pojedynczej warstwy 'Spłaszczony' (etap G1).
    pub fn flatten_image(&mut self) {
        if self.doc.document.layer_count() <= 1 {
            return;
        }
        self.abort_text_editing();
        self.push_history();
        let w = self.doc.document.width;
        let h = self.doc.document.height;
        let mut buf = vec![Rgba8::TRANSPARENT; (w * h) as usize];
        let _ = self
            .doc
            .document
            .compose_rect(Rect::new(0, 0, w, h), &mut buf);
        let mut new_layer = Layer::new("Spłaszczony", w, h);
        for y in 0..h {
            for x in 0..w {
                let px = buf[(y * w + x) as usize];
                let _ = new_layer.set_pixel(x, y, px);
            }
        }
        self.doc.document.layers = vec![new_layer];
        self.doc.active_layer_index = 0;
        self.on_document_changed();
        self.mark_document_dirty();
    }

    /// Przesuwa aktywną warstwę w górę stosu (w stronę wyższego indeksu / wierzchu).
    pub fn move_active_layer_up(&mut self) {
        if self.doc.active_layer_index + 1 < self.doc.document.layer_count() {
            self.abort_text_editing();
            self.push_history();
            self.doc
                .document
                .layers
                .swap(self.doc.active_layer_index, self.doc.active_layer_index + 1);
            self.doc.active_layer_index += 1;
            self.mark_document_dirty();
        }
    }

    /// Przesuwa aktywną warstwę w dół stosu (w stronę niższego indeksu / spodu).
    pub fn move_active_layer_down(&mut self) {
        if self.doc.active_layer_index > 0
            && self.doc.active_layer_index < self.doc.document.layer_count()
        {
            self.abort_text_editing();
            self.push_history();
            self.doc
                .document
                .layers
                .swap(self.doc.active_layer_index, self.doc.active_layer_index - 1);
            self.doc.active_layer_index -= 1;
            self.mark_document_dirty();
        }
    }

    /// Przełącza widoczność warstwy o podanym indeksie.
    pub fn toggle_layer_visibility(&mut self, idx: usize) {
        if idx < self.doc.document.layers.len() {
            self.push_history();
            self.doc.document.layers[idx].visible = !self.doc.document.layers[idx].visible;
            self.mark_document_dirty();
        }
    }

    /// Zmienia nazwę warstwy o podanym indeksie.
    pub fn rename_layer(&mut self, idx: usize, new_name: String) {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Some(layer) = self.doc.document.layers.get(idx) {
            if layer.name == trimmed {
                return;
            }
        } else {
            return;
        }
        self.push_history();
        if let Some(layer) = self.doc.document.layers.get_mut(idx) {
            layer.name = trimmed.to_string();
            self.mark_document_dirty();
        }
    }

    /// Centruje płótno w zadanym prostokącie widoku roboczego.
    pub fn center_canvas(&mut self, viewport_rect: egui::Rect) {
        let canvas_size = egui::vec2(
            self.doc.document.width as f32,
            self.doc.document.height as f32,
        );
        let scaled_size = canvas_size * self.doc.transform.zoom;
        self.doc.transform.pan =
            viewport_rect.min.to_vec2() + (viewport_rect.size() - scaled_size) * 0.5;
    }

    /// Dopasowuje powiększenie (zoom) i wyśrodkowuje dokument w obszarze widoku okna.
    pub fn fit_to_viewport(&mut self, viewport_rect: egui::Rect) {
        if self.doc.document.width == 0
            || self.doc.document.height == 0
            || viewport_rect.width() <= 0.0
            || viewport_rect.height() <= 0.0
        {
            return;
        }

        let margin = 40.0_f32;
        let avail_w = (viewport_rect.width() - margin).max(10.0_f32);
        let avail_h = (viewport_rect.height() - margin).max(10.0_f32);

        let scale_x = avail_w / self.doc.document.width as f32;
        let scale_y = avail_h / self.doc.document.height as f32;
        let zoom = scale_x.min(scale_y).clamp(0.05_f32, 64.0_f32);

        self.doc.transform.zoom = zoom;
        self.center_canvas(viewport_rect);
    }

    /// Wykonuje powiększenie do wskazanego prostokąta płótna (T11).
    pub fn zoom_to_canvas_rect(&mut self, rect: egui::Rect, viewport_rect: egui::Rect) {
        if rect.width() <= 0.0
            || rect.height() <= 0.0
            || viewport_rect.width() <= 0.0
            || viewport_rect.height() <= 0.0
        {
            return;
        }
        let scale_x = viewport_rect.width() / rect.width();
        let scale_y = viewport_rect.height() / rect.height();
        let new_zoom = scale_x.min(scale_y).clamp(0.05_f32, 64.0_f32);
        let c_center = rect.center();
        let vp_center = viewport_rect.center();
        self.doc.transform.pan = vp_center.to_vec2() - c_center.to_vec2() * new_zoom;
        self.doc.transform.zoom = new_zoom;
        self.doc.tile_renderer.set_zoom(new_zoom);
        self.doc.auto_fit = false;
    }

    /// Oblicza parametry pomiaru (odległość, kąt w stopniach, dx, dy) jeśli miarka jest zdefiniowana (T11).
    pub fn measure_stats(&self) -> Option<(f32, f32, f32, f32)> {
        let start = self.doc.measure_start?;
        let end = self.doc.measure_end?;
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let len = dx.hypot(dy);
        let angle = (-dy).atan2(dx).to_degrees();
        Some((len, angle, dx, dy))
    }

    /// Uruchamia asynchroniczny dialog wyboru pliku (lub wielu plików) do otwarcia w osobnym wątku (nie blokuje GUI).
    pub fn open_file_dialog(&self) {
        let tx = self.dialog_tx.clone();
        std::thread::spawn(move || {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter(
                    "Wszystkie obrazy (*.png, *.jpg, *.jpeg, *.webp, *.dziwak)",
                    &["png", "jpg", "jpeg", "webp", "dziwak"],
                )
                .add_filter("Dziwak (.dziwak)", &["dziwak"])
                .add_filter("Obraz PNG (*.png)", &["png"])
                .add_filter("Obraz JPEG (*.jpg, *.jpeg)", &["jpg", "jpeg"])
                .add_filter("Obraz WebP (*.webp)", &["webp"])
                .pick_files()
            {
                for path in paths {
                    let _ = tx.send(DialogResult::Open(path));
                }
            }
        });
    }

    /// Uruchamia asynchroniczny dialog zapisu pliku w osobnym wątku dla aktywnej zakładki (nie blokuje GUI).
    pub fn save_file_dialog(&self) {
        let tx = self.dialog_tx.clone();
        let tab_id = self.doc.id;
        std::thread::spawn(move || {
            match rfd::FileDialog::new()
                .set_file_name("obraz.dziwak")
                .add_filter("Dziwak (.dziwak)", &["dziwak"])
                .add_filter("Obraz PNG (*.png)", &["png"])
                .add_filter("Obraz JPEG (*.jpg, *.jpeg)", &["jpg", "jpeg"])
                .add_filter("Obraz WebP (*.webp)", &["webp"])
                .save_file()
            {
                Some(path) => {
                    let _ = tx.send(DialogResult::Save { tab_id, path });
                }
                None => {
                    let _ = tx.send(DialogResult::SaveCancelled { tab_id });
                }
            }
        });
    }

    /// Uruchamia usuwanie tła modelem AI w osobnym wątku (nie blokuje interfejsu).
    #[cfg(feature = "ai")]
    pub fn start_ai_background(&mut self, as_selection: bool) {
        if self.doc.ai_rx.is_some() || self.doc.document.layers.is_empty() {
            return;
        }
        let w = self.doc.document.width as usize;
        let h = self.doc.document.height as usize;
        if w == 0 || h == 0 {
            return;
        }
        let tab_id = self.doc.id;
        let layer_idx = self
            .doc
            .active_layer_index
            .min(self.doc.document.layers.len() - 1);
        if self.composite_buffer.len() != w * h {
            self.composite_buffer.resize(w * h, Rgba8::TRANSPARENT);
        }
        Self::sample_layer_to_buffer(
            &self.doc.document.layers[layer_idx],
            w,
            h,
            &mut self.composite_buffer,
        );
        let rgba: Vec<[u8; 4]> = self
            .composite_buffer
            .iter()
            .map(|px| px.to_straight())
            .collect();
        let (tx, rx) = std::sync::mpsc::channel();
        self.doc.ai_rx = Some(rx);
        self.doc.ai_as_selection = as_selection;
        std::thread::spawn(move || {
            let result = (|| {
                let path = dziwak_ai::default_model_path()
                    .filter(|p| p.exists())
                    .ok_or_else(|| "Brak modelu AI. Uruchom scripts/fetch-models.sh".to_string())?;
                let remover = dziwak_ai::BgRemover::load(&path).map_err(|e| e.to_string())?;
                remover
                    .foreground_mask(&rgba, w, h)
                    .map_err(|e| e.to_string())
            })();
            let _ = tx.send((tab_id, result));
        });
    }

    /// Sprawdza, czy wątek AI skończył dla aktywnej zakładki; stosuje wynik. Zwraca true, gdy AI nadal liczy.
    pub fn poll_ai_background(&mut self) -> bool {
        let Some(rx) = &self.doc.ai_rx else {
            return false;
        };
        let active_id = self.doc.id;
        match rx.try_recv() {
            Ok((tab_id, Ok(mask))) => {
                self.doc.ai_rx = None;
                if tab_id != active_id {
                    return false;
                }
                if self.doc.ai_as_selection {
                    self.doc
                        .selection
                        .combine_mask(&mask, dziwak_core::selection::SelectMode::Replace);
                } else if !self.doc.document.layers.is_empty() {
                    let layer_idx = self
                        .doc
                        .active_layer_index
                        .min(self.doc.document.layers.len() - 1);
                    self.push_history();
                    if let Some(rect) = dziwak_core::fill::mask_alpha(
                        &mut self.doc.document.layers[layer_idx],
                        &mask,
                    ) {
                        self.mark_rect_dirty(rect);
                        self.bump_layer_version(layer_idx);
                    }
                }
                false
            }
            Ok((tab_id, Err(msg))) => {
                self.doc.ai_rx = None;
                if tab_id == active_id {
                    self.error_message = Some(msg);
                }
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.doc.ai_rx = None;
                self.error_message = Some("Wątek AI zakończył się błędem".to_string());
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_around_preserves_cursor_canvas_pos() {
        let mut transform = CanvasTransform::new(egui::vec2(100.0, 50.0), 1.0);
        let cursor = egui::pos2(300.0, 250.0);
        let canvas_pt_before = transform.screen_to_canvas(cursor);

        transform.zoom_around(cursor, 2.0, 0.05, 64.0);

        let canvas_pt_after = transform.screen_to_canvas(cursor);
        assert!((canvas_pt_before.x - canvas_pt_after.x).abs() < 1e-4);
        assert!((canvas_pt_before.y - canvas_pt_after.y).abs() < 1e-4);
        assert!((transform.zoom - 2.0).abs() < 1e-4);
    }

    #[test]
    fn test_canvas_to_screen_and_back() {
        let transform = CanvasTransform::new(egui::vec2(50.0, 75.0), 2.5);
        let original_canvas = egui::pos2(123.0, 456.0);
        let screen = transform.canvas_to_screen(original_canvas);
        let back = transform.screen_to_canvas(screen);
        assert!((original_canvas.x - back.x).abs() < 1e-4);
        assert!((original_canvas.y - back.y).abs() < 1e-4);
    }

    #[test]
    fn test_zoom_clamping() {
        let mut transform = CanvasTransform::new(egui::Vec2::ZERO, 1.0);
        let cursor = egui::pos2(100.0, 100.0);

        // Zbyt mały zoom
        transform.zoom_around(cursor, 0.001, 0.05, 64.0);
        assert!((transform.zoom - 0.05).abs() < 1e-4);

        // Zbyt duży zoom
        transform.zoom_around(cursor, 10000.0, 0.05, 64.0);
        assert!((transform.zoom - 64.0).abs() < 1e-4);
    }

    #[test]
    fn test_tile_renderer_new_and_dimensions() {
        let renderer = TileRenderer::new(128, 65);
        assert_eq!(renderer.tiles_across, 2);
        assert_eq!(renderer.tiles_down, 2);
        assert_eq!(renderer.textures.len(), 4);
        assert_eq!(renderer.dirty_tiles.len(), 4);

        let empty = TileRenderer::new(0, 0);
        assert_eq!(empty.tiles_across, 0);
        assert_eq!(empty.tiles_down, 0);
        assert_eq!(empty.textures.len(), 0);
        assert_eq!(empty.dirty_tiles.len(), 0);
    }

    #[test]
    fn test_tile_renderer_mark_dirty() {
        let mut renderer = TileRenderer::new(128, 128);
        renderer.dirty_tiles.clear();
        assert!(renderer.dirty_tiles.is_empty());

        renderer.mark_tile_dirty(1, 0);
        assert_eq!(renderer.dirty_tiles.len(), 1);
        assert!(renderer.dirty_tiles.contains(&1));

        renderer.mark_all_dirty();
        assert_eq!(renderer.dirty_tiles.len(), 4);

        // Nieprawidłowy indeks nie psuje stanu
        renderer.mark_tile_dirty(10, 10);
        assert_eq!(renderer.dirty_tiles.len(), 4);
    }

    #[test]
    fn test_create_demo_document() {
        let doc = create_demo_document(800, 600);
        assert_eq!(doc.width, 800);
        assert_eq!(doc.height, 600);
        assert_eq!(doc.layer_count(), 3);
        assert_eq!(doc.layer(0).map(|l| l.name.as_str()), Some("Tło"));
        assert_eq!(doc.layer(1).map(|l| l.name.as_str()), Some("Figury"));
        assert_eq!(
            doc.layer(2).map(|l| l.name.as_str()),
            Some("Półprzezroczysta nakładka")
        );
    }

    #[test]
    fn test_fit_to_viewport() {
        let mut app = DziwakApp::new();
        let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 800.0));
        app.fit_to_viewport(viewport);
        assert!(app.doc.transform.zoom > 0.0);
        assert!(app.doc.transform.zoom <= 64.0);
    }

    #[test]
    fn test_tile_renderer_set_zoom_threshold() {
        let mut renderer = TileRenderer::new(128, 128);
        renderer.dirty_tiles.clear();

        // Test zoom < 1.0
        renderer.set_zoom(0.5);
        assert_eq!(renderer.texture_options, egui::TextureOptions::LINEAR);
        assert!(!renderer.dirty_tiles.is_empty());

        // Clear dirty tiles
        renderer.dirty_tiles.clear();

        // Test zoom >= 1.0 (should not mark dirty)
        renderer.set_zoom(0.7);
        assert_eq!(renderer.texture_options, egui::TextureOptions::LINEAR);
        assert!(renderer.dirty_tiles.is_empty());

        // Change zoom to > 1.0
        renderer.set_zoom(2.0);
        assert_eq!(renderer.texture_options, egui::TextureOptions::NEAREST);
    }

    #[test]
    fn test_history_undo_redo_in_app() {
        let mut app = DziwakApp::new();
        assert!(!app.doc.history.can_undo());
        assert!(!app.doc.history.can_redo());

        // Zapisujemy stan przed zmianą
        app.doc.history.push(&app.doc.document);
        assert!(app.doc.history.can_undo());

        // Wprowadzamy zmianę w dokumencie
        app.doc.document.layers[0]
            .set_pixel(0, 0, Rgba8::new(123, 45, 67, 255))
            .unwrap();

        // Wykonujemy Undo
        let _ = app.handle_undo();
        assert!(!app.doc.history.can_undo());
        assert!(app.doc.history.can_redo());
        assert!(!app.doc.tile_renderer.dirty_tiles.is_empty());

        // Wykonujemy Redo
        let _ = app.handle_redo();
        assert!(app.doc.history.can_undo());
        assert!(!app.doc.history.can_redo());
        assert_eq!(
            app.doc.document.layers[0].get_pixel(0, 0),
            Some(Rgba8::new(123, 45, 67, 255))
        );
    }

    #[test]
    fn test_brush_tool_dab_and_tile_dirty() {
        let mut app = DziwakApp::new();
        app.active_tool = ActiveTool::Brush;
        app.brush_size = 16.0;
        app.brush_hardness = 1.0;
        app.brush_opacity = 1.0;
        app.brush_color = egui::Color32::RED;

        app.doc.tile_renderer.dirty_tiles.clear();
        assert!(app.doc.tile_renderer.dirty_tiles.is_empty());

        // Dab w punkcie (32.0, 32.0)
        app.apply_tool_dab(32.0, 32.0);

        // Kafel 0 powinien być oznaczony jako brudny
        assert!(!app.doc.tile_renderer.dirty_tiles.is_empty());
        assert!(app.doc.tile_renderer.dirty_tiles.contains(&0));

        let active_layer = app.doc.active_layer_index;
        let p = app.doc.document.layers[active_layer]
            .get_pixel(32, 32)
            .unwrap();
        assert_eq!(p.r(), 255);
        assert_eq!(p.g(), 0);
        assert_eq!(p.b(), 0);
    }

    #[test]
    fn test_eraser_tool_dab() {
        let mut app = DziwakApp::new();
        app.active_tool = ActiveTool::Eraser;
        app.brush_size = 20.0;
        app.brush_hardness = 1.0;
        app.brush_opacity = 1.0;

        let active_layer = app.doc.active_layer_index;
        app.doc.document.layers[active_layer]
            .set_pixel(10, 10, Rgba8::new(255, 255, 0, 255))
            .unwrap();

        app.apply_tool_dab(10.0, 10.0);
        let p = app.doc.document.layers[active_layer]
            .get_pixel(10, 10)
            .unwrap();
        assert_eq!(p, Rgba8::TRANSPARENT);
    }

    #[test]
    fn test_pencil_tool_dab_hard_edge() {
        let mut app = DziwakApp::new();
        app.active_tool = ActiveTool::Pencil;
        app.brush_size = 20.0; // radius = 10.0
        app.brush_hardness = 0.0; // Pencil ignores hardness and uses 1.0 (hard edge)
        app.brush_opacity = 1.0;
        app.brush_color = egui::Color32::BLUE;

        // Ślad w (32, 32)
        app.apply_tool_dab(32.0, 32.0);

        let active_layer = app.doc.active_layer_index;
        // Piksel w odległości 8 px od środka (wewnątrz radius 10) musi mieć pełne krycie 255
        let p = app.doc.document.layers[active_layer]
            .get_pixel(40, 32)
            .unwrap();
        assert_eq!(p.b(), 255);
        assert_eq!(p.a(), 255);
    }

    #[test]
    fn test_airbrush_tool_dab() {
        let mut app = DziwakApp::new();
        app.active_tool = ActiveTool::Airbrush;
        app.brush_size = 16.0;
        app.brush_hardness = 0.5;
        app.brush_opacity = 1.0;
        app.brush_color = egui::Color32::GREEN;

        app.doc.tile_renderer.dirty_tiles.clear();
        app.apply_tool_dab(32.0, 32.0);

        assert!(!app.doc.tile_renderer.dirty_tiles.is_empty());
        let active_layer = app.doc.active_layer_index;
        let p = app.doc.document.layers[active_layer]
            .get_pixel(32, 32)
            .unwrap();
        assert_eq!(p.g(), 255);
    }

    #[test]
    fn test_layer_panel_add_and_undo() {
        let mut app = DziwakApp::new();
        let initial_count = app.doc.document.layer_count();
        app.doc.active_layer_index = 1;
        app.doc.tile_renderer.dirty_tiles.clear();

        app.add_new_layer();
        assert_eq!(app.doc.document.layer_count(), initial_count + 1);
        assert_eq!(app.doc.active_layer_index, 2);
        assert_eq!(
            app.doc.document.layers[2].name,
            format!("Warstwa {}", initial_count + 1)
        );
        assert!(!app.doc.tile_renderer.dirty_tiles.is_empty());

        // Undo przywraca poprzednią liczbę warstw
        let _ = app.handle_undo();
        assert_eq!(app.doc.document.layer_count(), initial_count);
        assert!(app.doc.active_layer_index < app.doc.document.layer_count());
    }

    #[test]
    fn test_layer_panel_duplicate_and_undo() {
        let mut app = DziwakApp::new();
        let initial_count = app.doc.document.layer_count();
        app.doc.active_layer_index = 1;
        let orig_name = app.doc.document.layers[1].name.clone();

        app.duplicate_active_layer();
        assert_eq!(app.doc.document.layer_count(), initial_count + 1);
        assert_eq!(app.doc.active_layer_index, 2);
        assert_eq!(
            app.doc.document.layers[2].name,
            format!("{orig_name} (kopia)")
        );

        // Kafle powinny być współdzielone przez Arc
        if let (Some(orig_tile), Some(dup_tile)) = (
            &app.doc.document.layers[1].pixels.tiles[0],
            &app.doc.document.layers[2].pixels.tiles[0],
        ) {
            assert!(Arc::ptr_eq(orig_tile, dup_tile));
        }

        let _ = app.handle_undo();
        assert_eq!(app.doc.document.layer_count(), initial_count);
    }

    #[test]
    fn test_layer_panel_remove_preserves_last_layer() {
        let mut app = DziwakApp::new();
        assert_eq!(app.doc.document.layer_count(), 3);

        app.doc.active_layer_index = 2;
        app.remove_active_layer();
        assert_eq!(app.doc.document.layer_count(), 2);
        assert_eq!(app.doc.active_layer_index, 1);

        app.remove_active_layer();
        assert_eq!(app.doc.document.layer_count(), 1);
        assert_eq!(app.doc.active_layer_index, 0);

        // Nie można usunąć ostatniej warstwy
        app.remove_active_layer();
        assert_eq!(app.doc.document.layer_count(), 1);
        assert_eq!(app.doc.active_layer_index, 0);

        // Undo przywraca warstwy
        let _ = app.handle_undo();
        assert_eq!(app.doc.document.layer_count(), 2);
    }

    #[test]
    fn test_layer_panel_move_up_down() {
        let mut app = DziwakApp::new();
        assert_eq!(app.doc.document.layer_count(), 3);
        // Warstwy: 0: "Tło", 1: "Figury", 2: "Półprzezroczysta nakładka"
        app.doc.active_layer_index = 1;

        // W górę: przesuwa warstwę 1 na pozycję 2
        app.move_active_layer_up();
        assert_eq!(app.doc.active_layer_index, 2);
        assert_eq!(app.doc.document.layers[2].name, "Figury");
        assert_eq!(app.doc.document.layers[1].name, "Półprzezroczysta nakładka");

        // Ponowne w górę nie przekracza zakresu
        app.move_active_layer_up();
        assert_eq!(app.doc.active_layer_index, 2);

        // W dół: wraca na pozycję 1
        app.move_active_layer_down();
        assert_eq!(app.doc.active_layer_index, 1);
        assert_eq!(app.doc.document.layers[1].name, "Figury");

        // W dół na pozycję 0
        app.move_active_layer_down();
        assert_eq!(app.doc.active_layer_index, 0);
        assert_eq!(app.doc.document.layers[0].name, "Figury");

        // Ponowne w dół nie przekracza zakresu
        app.move_active_layer_down();
        assert_eq!(app.doc.active_layer_index, 0);
    }

    #[test]
    fn test_layer_panel_toggle_visibility() {
        let mut app = DziwakApp::new();
        assert!(app.doc.document.layers[1].visible);

        app.toggle_layer_visibility(1);
        assert!(!app.doc.document.layers[1].visible);
        assert!(!app.doc.tile_renderer.dirty_tiles.is_empty());

        let _ = app.handle_undo();
        assert!(app.doc.document.layers[1].visible);
    }

    #[test]
    fn test_layer_panel_rename() {
        let mut app = DziwakApp::new();
        assert_eq!(app.doc.document.layers[1].name, "Figury");

        app.rename_layer(1, "Szkice".to_string());
        assert_eq!(app.doc.document.layers[1].name, "Szkice");

        // Pusta nazwa nie powinna zmienić nazwy
        app.rename_layer(1, "   ".to_string());
        assert_eq!(app.doc.document.layers[1].name, "Szkice");

        let _ = app.handle_undo();
        assert_eq!(app.doc.document.layers[1].name, "Figury");
    }

    #[test]
    fn test_selection_default_state_and_shortcuts() {
        let mut app = DziwakApp::new();
        // Początkowo brak aktywnego zaznaczenia
        assert!(!app.doc.selection.has_selection);
        assert_eq!(app.doc.selection.bounds(), None);

        // Zaznacz wszystko (symulacja Ctrl+A)
        app.doc
            .selection
            .select_rect(0, 0, app.doc.document.width, app.doc.document.height);
        app.doc.selection_shape = SelectionShape::Rect;
        assert!(app.doc.selection.has_selection);
        assert_eq!(
            app.doc.selection.bounds(),
            Some(dziwak_core::Rect::new(
                0,
                0,
                app.doc.document.width,
                app.doc.document.height
            ))
        );

        // Odznacz (symulacja Ctrl+Shift+A)
        app.doc.selection.clear();
        assert!(!app.doc.selection.has_selection);
        assert_eq!(app.doc.selection.bounds(), None);

        // Zaznaczenie nie modyfikuje historii dokumentu
        assert!(!app.doc.history.can_undo());
    }

    #[test]
    fn test_selection_invert_in_app() {
        let mut app = DziwakApp::new();
        app.doc.selection.select_rect(10, 10, 20, 20);
        assert!(app.doc.selection.has_selection);

        app.doc.selection.invert();
        assert!(app.doc.selection.has_selection);
        // Po odwróceniu piksel (15, 15) ma pokrycie 0, a (50, 50) ma pokrycie 255
        assert_eq!(app.doc.selection.coverage(15, 15), 0);
        assert_eq!(app.doc.selection.coverage(50, 50), 255);
    }

    #[test]
    fn test_brush_respects_selection_in_app() {
        let mut app = DziwakApp::new();
        app.active_tool = ActiveTool::Brush;
        app.brush_size = 8.0;
        app.brush_hardness = 1.0;
        app.brush_opacity = 1.0;
        app.brush_color = egui::Color32::from_rgb(0, 255, 0);

        // Zaznaczamy tylko obszar [40..60, 40..60]
        app.doc.selection.select_rect(40, 40, 20, 20);

        let active_layer = app.doc.active_layer_index;
        // Czyścimy piksele, aby sprawdzić efekt pędzla
        app.doc.document.layers[active_layer]
            .set_pixel(10, 10, Rgba8::TRANSPARENT)
            .unwrap();
        app.doc.document.layers[active_layer]
            .set_pixel(50, 50, Rgba8::TRANSPARENT)
            .unwrap();

        // Dab poza zaznaczeniem w (10, 10)
        app.apply_tool_dab(10.0, 10.0);
        assert_eq!(
            app.doc.document.layers[active_layer].get_pixel(10, 10),
            Some(Rgba8::TRANSPARENT),
            "Piksel poza zaznaczeniem nie powinien ulec zmianie"
        );

        // Dab wewnątrz zaznaczenia w (50, 50)
        app.apply_tool_dab(50.0, 50.0);
        let p50 = app.doc.document.layers[active_layer]
            .get_pixel(50, 50)
            .unwrap();
        assert_eq!(
            p50.g(),
            255,
            "Piksel wewnątrz zaznaczenia powinien otrzymać kolor pędzla"
        );
    }

    #[test]
    fn test_selection_tools_switch() {
        let mut app = DziwakApp::new();
        assert_eq!(app.active_tool, ActiveTool::Brush);

        app.active_tool = ActiveTool::SelectRect;
        assert_eq!(app.active_tool, ActiveTool::SelectRect);

        app.active_tool = ActiveTool::SelectEllipse;
        assert_eq!(app.active_tool, ActiveTool::SelectEllipse);
    }

    #[test]
    fn test_filter_dialog_open_and_cancel_restores_layer() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;

        // Ustawiamy znany piksel w warstwie
        app.doc.document.layers[target_layer]
            .set_pixel(100, 100, Rgba8::new(100, 100, 100, 255))
            .unwrap();
        let orig_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();

        // Otwieramy filtr jasności/kontrastu
        app.open_filter_dialog(FilterKind::BrightnessContrast {
            brightness: 50,
            contrast: 1.0_f32,
        });

        // Weryfikujemy, że podgląd zmodyfikował piksel
        let preview_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();
        assert_ne!(preview_pixel, orig_pixel);
        assert_eq!(preview_pixel.r(), 150);

        // Anulujemy filtr
        app.cancel_filter_dialog();
        assert!(app.doc.filter_dialog.is_none());

        // Piksel powinien powrócić do stanu oryginalnego
        let restored_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();
        assert_eq!(restored_pixel, orig_pixel);

        // Historia nie powinna zawierać żadnego nowego wpisu
        assert!(!app.doc.history.can_undo());
    }

    #[test]
    fn test_filter_dialog_apply_pushes_history_and_applies_filter() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;

        app.doc.document.layers[target_layer]
            .set_pixel(100, 100, Rgba8::new(100, 100, 100, 255))
            .unwrap();
        let orig_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();

        app.open_filter_dialog(FilterKind::BrightnessContrast {
            brightness: 50,
            contrast: 1.0_f32,
        });

        // Zastosowanie filtra
        app.apply_filter_dialog();
        assert!(app.doc.filter_dialog.is_none());

        // Piksel jest zmieniony
        let applied_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();
        assert_eq!(applied_pixel.r(), 150);

        // Na stosie historii pojawił się stan do cofnięcia
        assert!(app.doc.history.can_undo());

        // Cofamy (Undo)
        let _ = app.handle_undo();
        assert!(!app.doc.history.can_undo());
        assert!(app.doc.history.can_redo());
        let undone_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();
        assert_eq!(undone_pixel, orig_pixel);

        // Ponawiamy (Redo)
        let _ = app.handle_redo();
        assert!(app.doc.history.can_undo());
        assert!(!app.doc.history.can_redo());
        let redone_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();
        assert_eq!(redone_pixel, applied_pixel);
    }

    #[test]
    fn test_filter_dialog_without_preview() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;

        app.doc.document.layers[target_layer]
            .set_pixel(100, 100, Rgba8::new(100, 100, 100, 255))
            .unwrap();
        let orig_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();

        app.open_filter_dialog(FilterKind::BrightnessContrast {
            brightness: 50,
            contrast: 1.0_f32,
        });

        // Wyłączamy podgląd
        if let Some(d) = &mut app.doc.filter_dialog {
            d.preview = false;
        }
        app.apply_filter_preview();

        // Piksel powinien być nienaruszony, bo preview = false
        let mid_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();
        assert_eq!(mid_pixel, orig_pixel);

        // Po kliknięciu Zastosuj filtr powinien zostać nałożony
        app.apply_filter_dialog();
        let final_pixel = app.doc.document.layers[target_layer]
            .get_pixel(100, 100)
            .unwrap();
        assert_eq!(final_pixel.r(), 150);
        assert!(app.doc.history.can_undo());
    }

    #[test]
    fn test_filter_respects_selection_in_app() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;

        app.doc.document.layers[target_layer]
            .set_pixel(10, 10, Rgba8::new(100, 100, 100, 255))
            .unwrap();
        app.doc.document.layers[target_layer]
            .set_pixel(50, 50, Rgba8::new(100, 100, 100, 255))
            .unwrap();

        // Zaznaczamy tylko obszar wokół (50, 50)
        app.doc.selection.select_rect(40, 40, 20, 20);

        app.open_filter_dialog(FilterKind::BrightnessContrast {
            brightness: 50,
            contrast: 1.0_f32,
        });
        app.apply_filter_dialog();

        // Piksel poza zaznaczeniem (10, 10) nie zmienił się
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(10, 10),
            Some(Rgba8::new(100, 100, 100, 255))
        );

        // Piksel wewnątrz zaznaczenia (50, 50) został rozjaśniony
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(50, 50),
            Some(Rgba8::new(150, 150, 150, 255))
        );
    }

    #[test]
    fn test_all_filter_kinds_apply() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer]
            .set_pixel(100, 100, Rgba8::new(200, 50, 50, 255))
            .unwrap();

        // Test HueSaturation
        app.open_filter_dialog(FilterKind::HueSaturation {
            hue_shift_deg: 180.0_f32,
            sat_mul: 1.0_f32,
            light_delta: 0.0_f32,
        });
        app.apply_filter_dialog();
        assert!(app.doc.history.can_undo());

        // Test GaussianBlur
        app.open_filter_dialog(FilterKind::GaussianBlur { radius: 2 });
        app.apply_filter_dialog();

        // Test UnsharpMask
        app.open_filter_dialog(FilterKind::UnsharpMask {
            radius: 2,
            amount: 1.5_f32,
        });
        app.apply_filter_dialog();
    }

    #[test]
    fn test_bucket_fill_single_layer_and_undo() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        app.brush_color = egui::Color32::from_rgb(120, 10, 200);
        app.fill_sample_all_layers = false;
        app.fill_tolerance = 10;

        app.apply_bucket_fill(15.0, 15.0);

        let px = app.doc.document.layers[target_layer]
            .get_pixel(15, 15)
            .unwrap();
        assert_eq!(px, Rgba8::new(120, 10, 200, 255));
        assert!(app.doc.history.can_undo());

        // Undo przywraca czystą warstwę
        let _ = app.handle_undo();
        let px_after_undo = app.doc.document.layers[target_layer]
            .get_pixel(15, 15)
            .unwrap();
        assert_eq!(px_after_undo, Rgba8::TRANSPARENT);
    }

    #[test]
    fn test_bucket_fill_respects_selection() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        // Ograniczamy wypełnianie tylko do prostokąta (20..50, 20..50)
        app.doc.selection.select_rect(20, 20, 30, 30);
        app.brush_color = egui::Color32::from_rgb(255, 255, 0);

        app.apply_bucket_fill(25.0, 25.0);

        // Wewnątrz zaznaczenia piksel został wypełniony
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(25, 25),
            Some(Rgba8::new(255, 255, 0, 255))
        );

        // Poza zaznaczeniem piksel pozostał przezroczysty
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(5, 5),
            Some(Rgba8::TRANSPARENT)
        );
    }

    #[test]
    fn test_bucket_fill_sample_all_layers() {
        let mut doc = Document::new(32, 32);
        // Warstwa 0 ma czerwony kwadrat w (0..16, 0..16)
        let mut layer0 = Layer::new("Tło", 32, 32);
        for y in 0..16 {
            for x in 0..16 {
                layer0.set_pixel(x, y, Rgba8::new(255, 0, 0, 255)).unwrap();
            }
        }
        // Warstwa 1 jest pusta
        let layer1 = Layer::new("Góra", 32, 32);
        doc.add_layer(layer0);
        doc.add_layer(layer1);

        let mut app = DziwakApp::new();
        app.doc.document = doc;
        app.on_document_changed();
        app.doc.active_layer_index = 1;
        app.fill_sample_all_layers = true;
        app.fill_tolerance = 0;
        app.brush_color = egui::Color32::from_rgb(0, 255, 0);

        // Klikamy na górnej warstwie w punkcie (5, 5) - kompozycja ma tam czerwony piksel z warstwy 0
        app.apply_bucket_fill(5.0, 5.0);

        // Warstwa 1 w obszarze (0..16, 0..16) ma zielony piksel
        assert_eq!(
            app.doc.document.layers[1].get_pixel(5, 5),
            Some(Rgba8::new(0, 255, 0, 255))
        );
        // Poza kwadratem (20, 20) warstwa 1 pozostała przezroczysta
        assert_eq!(
            app.doc.document.layers[1].get_pixel(20, 20),
            Some(Rgba8::TRANSPARENT)
        );
    }

    #[test]
    fn test_eyedropper_sample_color_and_radius() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer]
            .set_pixel(20, 20, Rgba8::new(42, 84, 168, 255))
            .unwrap();

        app.brush_color = egui::Color32::BLACK;
        app.pipette_radius = 0;

        app.sample_color_at(egui::pos2(20.0, 20.0));
        assert_eq!(
            app.brush_color,
            egui::Color32::from_rgba_unmultiplied(42, 84, 168, 255)
        );

        // Promień 1 (3x3)
        app.pipette_radius = 1;
        app.sample_color_at(egui::pos2(20.0, 20.0));
        // Kolor został pobrany ze średniej (jako że w tle jest też kompozycja)
        assert_ne!(app.brush_color, egui::Color32::BLACK);
    }

    #[test]
    fn test_linear_gradient_and_undo() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        app.brush_color = egui::Color32::from_rgb(255, 0, 0);
        app.bg_color = egui::Color32::from_rgb(0, 0, 255);

        app.apply_linear_gradient_between(egui::pos2(10.0, 0.0), egui::pos2(100.0, 0.0));

        let p_start = app.doc.document.layers[target_layer]
            .get_pixel(0, 0)
            .unwrap();
        let p_end = app.doc.document.layers[target_layer]
            .get_pixel(120, 0)
            .unwrap();

        assert_eq!(p_start, Rgba8::new(255, 0, 0, 255));
        assert_eq!(p_end, Rgba8::new(0, 0, 255, 255));
        assert!(app.doc.history.can_undo());

        // Undo przywraca stan sprzed gradientu
        let _ = app.handle_undo();
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(0, 0),
            Some(Rgba8::TRANSPARENT)
        );
    }

    #[test]
    fn test_swap_colors() {
        let mut app = DziwakApp::new();
        app.brush_color = egui::Color32::RED;
        app.bg_color = egui::Color32::BLUE;

        std::mem::swap(&mut app.brush_color, &mut app.bg_color);

        assert_eq!(app.brush_color, egui::Color32::BLUE);
        assert_eq!(app.bg_color, egui::Color32::RED);
    }

    #[test]
    fn test_window_title_unmodified_and_modified() {
        let mut app = DziwakApp::new();
        assert_eq!(app.window_title(), "Dziwak — bez nazwy");

        app.push_history();
        assert_eq!(app.window_title(), "Dziwak — bez nazwy*");

        let tmp_dir = std::env::temp_dir();
        let path = tmp_dir.join("dziwak_test_sample.dziwak");
        app.execute_save(path.clone());

        assert_eq!(app.window_title(), "Dziwak — dziwak_test_sample.dziwak");
        assert!(!app.doc.is_modified);

        // Usuwamy plik tymczasowy
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_open_file_from_path() {
        let tmp_dir = std::env::temp_dir();
        let path = tmp_dir.join("dziwak_open_test.png");

        let doc = create_demo_document(64, 64);
        dziwak_core::save_image(&doc, &path).unwrap();

        let mut app = DziwakApp::new();
        app.open_file_from_path(path.clone());

        assert_eq!(app.doc.current_file_path, Some(path.clone()));
        assert!(!app.doc.is_modified);
        assert_eq!(app.doc.document.width, 64);
        assert_eq!(app.doc.document.height, 64);
        assert_eq!(app.window_title(), "Dziwak — dziwak_open_test.png");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_flatten_warning_multilayers_png_vs_dziwak() {
        let mut app = DziwakApp::new();
        assert!(app.doc.document.layer_count() > 1);

        // Symulacja wyboru pliku PNG
        let png_path = PathBuf::from("test.png");
        let is_dziwak = png_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("dziwak"));
        if !is_dziwak && app.doc.document.layer_count() > 1 {
            app.doc.pending_flatten_save = Some(png_path.clone());
        }
        assert_eq!(app.doc.pending_flatten_save, Some(png_path));

        // Dla .dziwak brak ostrzeżenia o spłaszczeniu
        let dziwak_path = PathBuf::from("test.dziwak");
        let is_dziwak = dziwak_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("dziwak"));
        assert!(is_dziwak);
    }

    #[test]
    fn test_magic_wand_selects_region() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        // Ustawiamy piksel w (1, 1) na czerwony
        app.doc.document.layers[target_layer]
            .set_pixel(1, 1, Rgba8::new(255, 0, 0, 255))
            .unwrap();

        // Wykonujemy magic_wand z trybem Replace
        app.apply_magic_wand(1.0, 1.0, dziwak_core::selection::SelectMode::Replace);

        assert!(app.doc.selection.has_selection);
        assert_eq!(app.doc.selection.coverage(1, 1), 255);
    }

    #[test]
    fn test_remove_uniform_background() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        // Ustawiamy piksel w (0, 0) na czerwony
        app.doc.document.layers[target_layer]
            .set_pixel(0, 0, Rgba8::new(255, 0, 0, 255))
            .unwrap();

        // Wywołujemy usuwanie jednolitego tła
        app.remove_uniform_background();

        // Piksel w rogu (0, 0) powinien być przezroczysty
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(0, 0),
            Some(Rgba8::TRANSPARENT)
        );
    }

    #[cfg(feature = "ai")]
    #[test]
    #[ignore = "wymaga modelu: scripts/fetch-models.sh"]
    fn test_ai_background_end_to_end() {
        let mut app = DziwakApp::new();
        app.start_ai_background(false);
        assert!(app.doc.ai_rx.is_some());
        let start = std::time::Instant::now();
        while app.poll_ai_background() {
            assert!(start.elapsed().as_secs() < 60, "AI liczy zbyt długo");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(app.error_message, None);
        assert!(app.doc.ai_rx.is_none());
        println!("AI end-to-end: {:?}", start.elapsed());
    }

    #[test]
    fn test_move_tool_whole_layer() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        let red = Rgba8::new(255, 0, 0, 255);
        app.doc.document.layers[target_layer]
            .set_pixel(10, 10, red)
            .unwrap();

        app.active_tool = ActiveTool::Move;
        app.start_move(egui::pos2(10.0, 10.0));
        assert!(app.doc.is_moving);

        // Przesunięcie o (5, 7)
        app.apply_move_offset(5, 7);
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(10, 10),
            Some(Rgba8::TRANSPARENT)
        );
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(15, 17),
            Some(red)
        );

        app.finish_move();
        assert!(!app.doc.is_moving);
        assert!(app.doc.is_modified);

        // Test cofania (Undo) - powrót do pierwotnej pozycji
        assert!(app.doc.history.can_undo());
        let _ = app.handle_undo();
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(10, 10),
            Some(red)
        );
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(15, 17),
            Some(Rgba8::TRANSPARENT)
        );

        // Test ponawiania (Redo)
        assert!(app.doc.history.can_redo());
        let _ = app.handle_redo();
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(15, 17),
            Some(red)
        );
    }

    #[test]
    fn test_move_tool_selection_content() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        let red = Rgba8::new(255, 0, 0, 255);
        let green = Rgba8::new(0, 255, 0, 255);
        app.doc.document.layers[target_layer]
            .set_pixel(10, 10, red)
            .unwrap();
        app.doc.document.layers[target_layer]
            .set_pixel(20, 20, green)
            .unwrap();

        // Zaznaczamy prostokąt obejmujący piksel (10, 10), ale nie (20, 20)
        app.doc.selection.select_rect(8, 8, 5, 5);
        assert!(app.doc.selection.has_selection);
        assert_eq!(app.doc.selection.coverage(10, 10), 255);
        assert_eq!(app.doc.selection.coverage(20, 20), 0);

        app.active_tool = ActiveTool::Move;
        app.start_move(egui::pos2(10.0, 10.0));
        app.apply_move_offset(5, 5);

        // Niezaznaczony zielony piksel pozostał na miejscu
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(20, 20),
            Some(green)
        );
        // Zaznaczony czerwony piksel przesunął się z (10, 10) na (15, 15)
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(10, 10),
            Some(Rgba8::TRANSPARENT)
        );
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(15, 15),
            Some(red)
        );

        // Maska zaznaczenia również przesunęła się o (5, 5)
        assert_eq!(app.doc.selection.coverage(15, 15), 255);
        assert_eq!(app.doc.selection.coverage(10, 10), 0);

        app.finish_move();

        // Undo cofa zawartość warstwy do stanu sprzed przesuwania
        assert!(app.doc.history.can_undo());
        let _ = app.handle_undo();
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(10, 10),
            Some(red)
        );
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(15, 15),
            Some(Rgba8::TRANSPARENT)
        );
    }

    #[test]
    fn test_crop_document_multilayers_and_undo() {
        let mut app = DziwakApp::new();
        assert_eq!(app.doc.document.width, 800);
        assert_eq!(app.doc.document.height, 600);

        let red = Rgba8::new(255, 0, 0, 255);
        let green = Rgba8::new(0, 255, 0, 255);
        app.doc.document.layers[0].set_pixel(100, 100, red).unwrap();
        if app.doc.document.layer_count() > 1 {
            app.doc.document.layers[1]
                .set_pixel(150, 120, green)
                .unwrap();
        }

        // Przycinamy dokument do prostokąta (50, 50, 200, 150)
        let crop_rect = Rect::new(50, 50, 200, 150);
        app.crop_document(crop_rect);

        assert_eq!(app.doc.document.width, 200);
        assert_eq!(app.doc.document.height, 150);
        assert_eq!(app.doc.tile_renderer.tiles_across, (200 - 1) / 64 + 1);
        assert_eq!(app.doc.tile_renderer.tiles_down, (150 - 1) / 64 + 1);

        // Piksel (100, 100) jest teraz na (50, 50)
        assert_eq!(app.doc.document.layers[0].get_pixel(50, 50), Some(red));
        if app.doc.document.layer_count() > 1 {
            // Piksel (150, 120) jest teraz na (100, 70)
            assert_eq!(app.doc.document.layers[1].get_pixel(100, 70), Some(green));
        }

        // Cofnięcie (Undo) przywraca wymiary 800x600 i pierwotne pozycje pikseli
        assert!(app.doc.history.can_undo());
        let resized = app.handle_undo();
        assert!(resized);
        assert_eq!(app.doc.document.width, 800);
        assert_eq!(app.doc.document.height, 600);
        assert_eq!(app.doc.document.layers[0].get_pixel(100, 100), Some(red));

        // Ponowienie (Redo) ponownie przycina
        assert!(app.doc.history.can_redo());
        let resized_redo = app.handle_redo();
        assert!(resized_redo);
        assert_eq!(app.doc.document.width, 200);
        assert_eq!(app.doc.document.height, 150);
        assert_eq!(app.doc.document.layers[0].get_pixel(50, 50), Some(red));
    }

    #[test]
    fn test_crop_to_selection_and_commit_crop() {
        let mut app = DziwakApp::new();
        let blue = Rgba8::new(0, 0, 255, 255);
        app.doc.document.layers[0].set_pixel(30, 40, blue).unwrap();

        // 1. Test commit_crop z przeciągania
        app.active_tool = ActiveTool::Crop;
        app.doc.crop_drag_start = Some(egui::pos2(20.0, 30.0));
        app.doc.crop_drag_current = Some(egui::pos2(120.0, 130.0));
        let ok = app.commit_crop();
        assert!(ok);
        assert_eq!(app.doc.document.width, 100);
        assert_eq!(app.doc.document.height, 100);
        assert_eq!(app.doc.document.layers[0].get_pixel(10, 10), Some(blue));

        // 2. Test crop_to_selection
        app.doc.selection.select_rect(5, 5, 25, 35);
        let ok2 = app.crop_to_selection();
        assert!(ok2);
        assert_eq!(app.doc.document.width, 25);
        assert_eq!(app.doc.document.height, 35);
        assert_eq!(app.doc.document.layers[0].get_pixel(5, 5), Some(blue));
    }

    #[test]
    fn test_free_select_tool_polygon_and_modes() {
        let mut app = DziwakApp::new();
        assert!(!app.doc.selection.has_selection);

        app.active_tool = ActiveTool::SelectFree;

        // Trójkąt o wierzchołkach (10, 10), (50, 10), (10, 50)
        app.add_free_select_point((10.0, 10.0), dziwak_core::selection::SelectMode::Replace);
        app.add_free_select_point((50.0, 10.0), dziwak_core::selection::SelectMode::Replace);
        app.add_free_select_point((10.0, 50.0), dziwak_core::selection::SelectMode::Replace);
        assert_eq!(app.doc.free_select_points.len(), 3);
        assert!(app.doc.is_free_selecting);

        // Zatwierdzenie zaznaczenia odręcznego
        let ok = app.finish_free_select();
        assert!(ok);
        assert!(app.doc.selection.has_selection);
        assert!(!app.doc.is_free_selecting);
        assert!(app.doc.free_select_points.is_empty());

        // Punkt wewnątrz trójkąta (15, 15) powinien mieć coverage > 0
        assert!(app.doc.selection.coverage(15, 15) > 0);
        // Punkt z dala na zewnątrz (100, 100) ma coverage == 0
        assert_eq!(app.doc.selection.coverage(100, 100), 0);

        // Dodawanie kolejnego obszaru (SelectMode::Add)
        app.add_free_select_point((100.0, 100.0), dziwak_core::selection::SelectMode::Add);
        app.add_free_select_point((150.0, 100.0), dziwak_core::selection::SelectMode::Add);
        app.add_free_select_point((100.0, 150.0), dziwak_core::selection::SelectMode::Add);
        app.finish_free_select();

        // Oba punkty są teraz zaznaczone
        assert!(app.doc.selection.coverage(15, 15) > 0);
        assert!(app.doc.selection.coverage(105, 105) > 0);

        // Test anulowania
        app.add_free_select_point((200.0, 200.0), dziwak_core::selection::SelectMode::Replace);
        app.cancel_free_select();
        assert!(app.doc.free_select_points.is_empty());
        assert!(!app.doc.is_free_selecting);
    }

    #[test]
    fn test_select_by_color_tool() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        let yellow = Rgba8::new(255, 255, 0, 255);
        let dark_yellow = Rgba8::new(250, 250, 5, 255);
        let blue = Rgba8::new(0, 0, 255, 255);

        // Rozłączne piksele o kolorze żółtym
        app.doc.document.layers[target_layer]
            .set_pixel(10, 10, yellow)
            .unwrap();
        app.doc.document.layers[target_layer]
            .set_pixel(80, 80, dark_yellow)
            .unwrap();
        // Inny kolor (niebieski) pomiędzy nimi
        app.doc.document.layers[target_layer]
            .set_pixel(40, 40, blue)
            .unwrap();

        app.active_tool = ActiveTool::SelectColor;
        app.fill_tolerance = 15;
        app.fill_sample_all_layers = false;

        // Klikamy na żółty piksel (10, 10)
        app.apply_select_by_color(10.0, 10.0, dziwak_core::selection::SelectMode::Replace);

        assert!(app.doc.selection.has_selection);
        // Oba rozłączne żółte piksele są zaznaczone w całym dokumencie
        assert_eq!(app.doc.selection.coverage(10, 10), 255);
        assert_eq!(app.doc.selection.coverage(80, 80), 255);
        // Niebieski piksel oraz tło nie są zaznaczone
        assert_eq!(app.doc.selection.coverage(40, 40), 0);
        assert_eq!(app.doc.selection.coverage(0, 0), 0);

        // Dodanie niebieskiego piksela przez SelectMode::Add
        app.apply_select_by_color(40.0, 40.0, dziwak_core::selection::SelectMode::Add);
        assert_eq!(app.doc.selection.coverage(10, 10), 255);
        assert_eq!(app.doc.selection.coverage(40, 40), 255);

        // Odjęcie żółtego piksela przez SelectMode::Subtract
        app.apply_select_by_color(10.0, 10.0, dziwak_core::selection::SelectMode::Subtract);
        assert_eq!(app.doc.selection.coverage(10, 10), 0);
        assert_eq!(app.doc.selection.coverage(80, 80), 0);
        assert_eq!(app.doc.selection.coverage(40, 40), 255);
    }

    #[test]
    fn test_transform_dialog_rotate_apply_and_undo() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        let red = Rgba8::new(255, 0, 0, 255);
        app.doc.document.layers[target_layer]
            .set_pixel(50, 50, red)
            .unwrap();

        // Otwieramy dialog obrotu o 180 stopni
        app.open_transform_dialog(LayerTransformKind::Rotate { angle_deg: 180.0 });
        assert!(app.doc.transform_dialog.is_some());

        // Podgląd jest na żywo: piksel (50, 50) obrócony o 180° wokół środka (400, 300) trafia na (749, 549)
        let p_rotated = app.doc.document.layers[target_layer]
            .get_pixel(749, 549)
            .unwrap();
        assert!(p_rotated.a() > 0);

        // Zatwierdzamy
        app.apply_transform_dialog();
        assert!(app.doc.transform_dialog.is_none());

        // Cofnięcie przywraca pierwotny stan
        let _ = app.handle_undo();
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(50, 50),
            Some(red)
        );
    }

    #[test]
    fn test_transform_dialog_scale_and_cancel() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        let blue = Rgba8::new(0, 0, 255, 255);
        app.doc.document.layers[target_layer]
            .set_pixel(10, 10, blue)
            .unwrap();

        app.open_transform_dialog(LayerTransformKind::Scale {
            width: 200,
            height: 200,
            orig_width: 100,
            orig_height: 100,
            keep_aspect: true,
        });

        // Anulujemy
        app.cancel_transform_dialog();
        assert!(app.doc.transform_dialog.is_none());
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(10, 10),
            Some(blue)
        );
    }

    #[test]
    fn test_transform_dialog_flip() {
        let mut app = DziwakApp::new();
        let target_layer = app.doc.active_layer_index;
        app.doc.document.layers[target_layer].pixels.clear();

        let green = Rgba8::new(0, 255, 0, 255);
        let w = app.doc.document.width;
        app.doc.document.layers[target_layer]
            .set_pixel(0, 0, green)
            .unwrap();

        app.open_transform_dialog(LayerTransformKind::Flip { horizontal: true });
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(w - 1, 0),
            Some(green)
        );

        app.apply_transform_dialog();
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(w - 1, 0),
            Some(green)
        );

        let _ = app.handle_undo();
        assert_eq!(
            app.doc.document.layers[target_layer].get_pixel(0, 0),
            Some(green)
        );
    }

    #[test]
    fn test_set_active_tool_opens_transform_dialog() {
        let mut app = DziwakApp::new();
        assert!(app.doc.transform_dialog.is_none());

        app.set_active_tool(ActiveTool::Rotate);
        assert_eq!(app.active_tool, ActiveTool::Rotate);
        assert!(matches!(
            app.doc.transform_dialog.as_ref().map(|d| d.kind),
            Some(LayerTransformKind::Rotate { .. })
        ));

        app.set_active_tool(ActiveTool::Scale);
        assert_eq!(app.active_tool, ActiveTool::Scale);
        assert!(matches!(
            app.doc.transform_dialog.as_ref().map(|d| d.kind),
            Some(LayerTransformKind::Scale { .. })
        ));

        app.set_active_tool(ActiveTool::Flip);
        assert_eq!(app.active_tool, ActiveTool::Flip);
        assert!(matches!(
            app.doc.transform_dialog.as_ref().map(|d| d.kind),
            Some(LayerTransformKind::Flip { .. })
        ));
    }

    #[test]
    fn test_scale_image_and_undo() {
        let mut app = DziwakApp::new();
        let (orig_w, orig_h) = (app.doc.document.width, app.doc.document.height);
        let red = Rgba8::new(255, 0, 0, 255);
        app.doc.document.layers[0].set_pixel(10, 10, red).unwrap();

        app.scale_image(orig_w * 2, orig_h * 2);
        assert_eq!(app.doc.document.width, orig_w * 2);
        assert_eq!(app.doc.document.height, orig_h * 2);
        assert!(app.doc.document.layers[0].get_pixel(20, 20).unwrap().a() > 0);

        let changed_dims = app.handle_undo();
        assert!(changed_dims);
        assert_eq!(app.doc.document.width, orig_w);
        assert_eq!(app.doc.document.height, orig_h);
        assert_eq!(app.doc.document.layers[0].get_pixel(10, 10), Some(red));

        let changed_dims_redo = app.handle_redo();
        assert!(changed_dims_redo);
        assert_eq!(app.doc.document.width, orig_w * 2);
        assert_eq!(app.doc.document.height, orig_h * 2);
    }

    #[test]
    fn test_resize_canvas_and_undo() {
        let mut app = DziwakApp::new();
        let (orig_w, orig_h) = (app.doc.document.width, app.doc.document.height);
        let red = Rgba8::new(255, 0, 0, 255);
        app.doc.document.layers[0].set_pixel(10, 10, red).unwrap();

        app.resize_canvas(orig_w + 100, orig_h + 100, 20, 30);
        assert_eq!(app.doc.document.width, orig_w + 100);
        assert_eq!(app.doc.document.height, orig_h + 100);
        assert_eq!(app.doc.document.layers[0].get_pixel(30, 40), Some(red));

        let changed_dims = app.handle_undo();
        assert!(changed_dims);
        assert_eq!(app.doc.document.width, orig_w);
        assert_eq!(app.doc.document.height, orig_h);
        assert_eq!(app.doc.document.layers[0].get_pixel(10, 10), Some(red));
    }

    #[test]
    fn test_rotate_image_and_undo() {
        let mut app = DziwakApp::new();
        let (orig_w, orig_h) = (app.doc.document.width, app.doc.document.height);
        let red = Rgba8::new(255, 0, 0, 255);
        app.doc.document.layers[0].set_pixel(0, 0, red).unwrap();

        // 90 CW: swaps width and height
        app.rotate_image_90_cw();
        assert_eq!(app.doc.document.width, orig_h);
        assert_eq!(app.doc.document.height, orig_w);
        assert_eq!(
            app.doc.document.layers[0].get_pixel(orig_h - 1, 0),
            Some(red)
        );

        app.handle_undo();
        assert_eq!(app.doc.document.width, orig_w);
        assert_eq!(app.doc.document.height, orig_h);
        assert_eq!(app.doc.document.layers[0].get_pixel(0, 0), Some(red));

        // 90 CCW: swaps width and height
        app.rotate_image_90_ccw();
        assert_eq!(app.doc.document.width, orig_h);
        assert_eq!(app.doc.document.height, orig_w);

        app.handle_undo();
        assert_eq!(app.doc.document.width, orig_w);
        assert_eq!(app.doc.document.height, orig_h);

        // 180: keeps width and height
        app.rotate_image_180();
        assert_eq!(app.doc.document.width, orig_w);
        assert_eq!(app.doc.document.height, orig_h);
        assert_eq!(
            app.doc.document.layers[0].get_pixel(orig_w - 1, orig_h - 1),
            Some(red)
        );

        app.handle_undo();
        assert_eq!(app.doc.document.layers[0].get_pixel(0, 0), Some(red));
    }

    #[test]
    fn test_flip_image_and_undo() {
        let mut app = DziwakApp::new();
        let (orig_w, orig_h) = (app.doc.document.width, app.doc.document.height);
        let red = Rgba8::new(255, 0, 0, 255);
        app.doc.document.layers[0].set_pixel(0, 0, red).unwrap();

        app.flip_image_horizontal();
        assert_eq!(
            app.doc.document.layers[0].get_pixel(orig_w - 1, 0),
            Some(red)
        );
        app.handle_undo();
        assert_eq!(app.doc.document.layers[0].get_pixel(0, 0), Some(red));

        app.flip_image_vertical();
        assert_eq!(
            app.doc.document.layers[0].get_pixel(0, orig_h - 1),
            Some(red)
        );
        app.handle_undo();
        assert_eq!(app.doc.document.layers[0].get_pixel(0, 0), Some(red));
    }

    #[test]
    fn test_scale_and_canvas_dialog_lifecycle() {
        let mut app = DziwakApp::new();
        assert!(app.doc.scale_image_dialog.is_none());
        assert!(app.doc.canvas_size_dialog.is_none());

        app.open_scale_image_dialog();
        assert!(app.doc.scale_image_dialog.is_some());
        app.cancel_scale_image_dialog();
        assert!(app.doc.scale_image_dialog.is_none());

        app.open_canvas_size_dialog();
        assert!(app.doc.canvas_size_dialog.is_some());
        app.cancel_canvas_size_dialog();
        assert!(app.doc.canvas_size_dialog.is_none());
    }

    #[test]
    fn test_clone_tool_workflow() {
        let mut app = DziwakApp::new();
        app.doc.active_layer_index = 0;
        app.active_tool = ActiveTool::Clone;
        app.brush_size = 10.0;
        app.brush_hardness = 1.0;
        app.brush_opacity = 1.0;

        // Draw red pixel at source position (50, 50)
        let red = Rgba8::new(255, 0, 0, 255);
        app.doc.document.layers[0].set_pixel(50, 50, red).unwrap();

        // Set clone source to (50, 50)
        app.set_clone_source((50.0, 50.0));
        assert_eq!(app.doc.clone_source, Some((50.0, 50.0)));
        assert_eq!(app.doc.clone_offset, None);

        // Start stroke at target position (100, 100)
        app.start_clone_stroke((100.0, 100.0));
        assert_eq!(app.doc.clone_offset, Some((-50, -50)));
        assert!(app.doc.is_painting);

        // The target pixel at (100, 100) should now be red
        assert_eq!(app.doc.document.layers[0].get_pixel(100, 100), Some(red));

        app.finish_clone_stroke();
        assert!(!app.doc.is_painting);
        assert!(app.doc.clone_source_layer.is_none());

        // Undo should restore original pixel state
        app.handle_undo();
        assert_ne!(app.doc.document.layers[0].get_pixel(100, 100), Some(red));
    }

    #[test]
    fn test_t9_tools_workflow() {
        let mut app = DziwakApp::new();
        app.doc.active_layer_index = 0;
        app.brush_size = 10.0;
        app.brush_hardness = 1.0;

        let red = Rgba8::new(255, 0, 0, 255);
        app.doc.document.layers[0].set_pixel(20, 20, red).unwrap();

        // 1. Smudge from (20, 20) to (25, 20)
        app.smudge_rate = 1.0;
        app.doc.smudge_buffer = Some(dziwak_core::brush::smudge_begin(
            &app.doc.document.layers[0],
            20.0,
            20.0,
            5.0,
        ));
        app.apply_smudge_dab(25.0, 20.0);
        app.doc.smudge_buffer = None;
        assert_eq!(app.doc.document.layers[0].get_pixel(25, 20), Some(red));

        // 2. Dodge / Burn
        let gray = Rgba8::new(100, 100, 100, 255);
        app.doc.document.layers[0].set_pixel(30, 30, gray).unwrap();
        app.dodge_burn_type = DodgeBurnType::Dodge;
        app.dodge_burn_exposure = 0.5;
        app.apply_dodge_burn_dab(30.0, 30.0, false);
        let dodged = app.doc.document.layers[0].get_pixel(30, 30).unwrap();
        assert!(dodged.r() > 100);

        // Invert Dodge with Ctrl -> Burn
        app.apply_dodge_burn_dab(30.0, 30.0, true);
        let burned = app.doc.document.layers[0].get_pixel(30, 30).unwrap();
        assert!(burned.r() < dodged.r());

        // 3. Blur / Sharpen
        let black = Rgba8::new(0, 0, 0, 255);
        app.doc.document.layers[0].set_pixel(40, 40, black).unwrap();
        app.blur_sharpen_type = BlurSharpenType::Blur;
        app.blur_sharpen_rate = 0.8;
        app.apply_blur_sharpen_dab(40.0, 40.0, false);
        let blurred = app.doc.document.layers[0].get_pixel(40, 40).unwrap();
        assert!(blurred.r() > 0);
    }

    #[test]
    fn test_smudge_stroke_drags_red_into_white() {
        let mut app = DziwakApp::new();
        app.doc.active_layer_index = 0;
        app.brush_size = 6.0;
        app.brush_hardness = 1.0;
        app.smudge_rate = 0.8;

        for y in 0..10 {
            for x in 0..30 {
                let color = if x < 8 {
                    Rgba8::new(255, 0, 0, 255)
                } else {
                    Rgba8::WHITE
                };
                app.doc.document.layers[0].set_pixel(x, y, color).unwrap();
            }
        }

        let radius = (app.brush_size * 0.5).max(0.5);
        app.doc.smudge_buffer = Some(dziwak_core::brush::smudge_begin(
            &app.doc.document.layers[0],
            5.0,
            5.0,
            radius,
        ));
        for step in 0..15 {
            let x = 6.0 + step as f32;
            app.apply_smudge_dab(x, 5.0);
        }
        app.doc.smudge_buffer = None;

        let p = app.doc.document.layers[0].get_pixel(14, 5).unwrap();
        assert!(p.g() < 200, "kolor nie został przeciągnięty: {:?}", p);
        assert_eq!(
            app.doc.document.layers[0].get_pixel(14, 0).unwrap(),
            Rgba8::WHITE
        );
    }

    #[test]
    fn test_text_tool_workflow() {
        let mut app = DziwakApp::new();
        assert!(app.doc.text_editor.is_none());

        let initial_layer_count = app.doc.document.layer_count();
        let initial_active = app.doc.active_layer_index;

        app.set_active_tool(ActiveTool::Text);
        app.start_text_box(20, 30, 0.0, 0.0);
        assert!(app.doc.text_editor.is_some());
        // Warstwa 'Tekst' powstaje od razu po utworzeniu ramki nad aktywną warstwą
        assert_eq!(app.doc.document.layer_count(), initial_layer_count + 1);
        assert_eq!(app.doc.active_layer_index, initial_active + 1);
        assert_eq!(
            app.doc.text_editor.as_ref().unwrap().layer_index,
            initial_active + 1
        );
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name,
            "Tekst"
        );
        assert_eq!(app.doc.history.undo.len(), 1);

        // Wpisywanie tekstu od razu zmienia nazwę warstwy i nie dodaje wpisów historii
        app.text_insert_str("Abc");
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name,
            "Tekst: Abc"
        );
        assert_eq!(app.doc.history.undo.len(), 1);

        app.text_backspace();
        app.text_backspace();
        app.text_backspace();
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name,
            "Tekst"
        );

        app.text_insert_str("Nowy tekst ponad dwadzieścia znaków!");
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name,
            "Tekst: Nowy tekst ponad dwa"
        );
        assert_eq!(app.doc.history.undo.len(), 1);

        // Przesunięcie ramki przerysowuje warstwę bez dodawania wpisu historii
        app.set_text_box_pos(40, 50);
        assert_eq!(app.doc.history.undo.len(), 1);

        // Zmiana narzędzia zatwierdza aktywną ramkę tekstową (warstwa już istnieje)
        app.set_active_tool(ActiveTool::Brush);
        assert!(app.doc.text_editor.is_none());
        assert_eq!(app.doc.document.layer_count(), initial_layer_count + 1);
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name,
            "Tekst: Nowy tekst ponad dwa"
        );

        // Pojedyncze Undo cofa całą edycję tekstu
        app.handle_undo();
        assert_eq!(app.doc.document.layer_count(), initial_layer_count);

        // Esc (cancel_text) usuwa warstwę tekstową i przywraca poprzednią aktywną warstwę
        let prev_active = app.doc.active_layer_index;
        app.set_active_tool(ActiveTool::Text);
        app.start_text_box(10, 10, 100.0, 50.0);
        assert_eq!(app.doc.document.layer_count(), initial_layer_count + 1);
        app.text_insert_str("Anulowany");
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name,
            "Tekst: Anulowany"
        );
        app.cancel_text();
        assert!(app.doc.text_editor.is_none());
        assert_eq!(app.doc.document.layer_count(), initial_layer_count);
        assert_eq!(app.doc.active_layer_index, prev_active);

        // Pusty tekst przy zatwierdzeniu (commit_text) usuwa warstwę tekstową
        app.start_text_box(15, 15, 0.0, 0.0);
        assert_eq!(app.doc.document.layer_count(), initial_layer_count + 1);
        assert!(!app.commit_text());
        assert!(app.doc.text_editor.is_none());
        assert_eq!(app.doc.document.layer_count(), initial_layer_count);
        assert_eq!(app.doc.active_layer_index, prev_active);

        // Zmiana aktywnej warstwy w trakcie edycji kończy edycję bez awarii
        app.start_text_box(15, 15, 0.0, 0.0);
        app.text_insert_str("Test");
        app.doc.active_layer_index = 0;
        app.text_insert_str("X");
        assert!(app.doc.text_editor.is_none());

        // Usunięcie warstwy tekstowej w trakcie edycji kończy edycję bez awarii
        let count_before_del = app.doc.document.layer_count();
        app.start_text_box(15, 15, 0.0, 0.0);
        assert_eq!(app.doc.document.layer_count(), count_before_del + 1);
        app.remove_active_layer();
        assert!(app.doc.text_editor.is_none());
        assert_eq!(app.doc.document.layer_count(), count_before_del);
        app.text_insert_str("Y");
        app.commit_text();
        app.cancel_text();
    }

    #[test]
    fn test_text_editor_buffer_and_cursor_logic() {
        let mut app = DziwakApp::new();
        app.set_active_tool(ActiveTool::Text);
        app.start_text_box(10, 10, 0.0, 0.0);

        // Backspace i Delete na pustym buforze (na początku) nie psują stanu
        app.text_backspace();
        app.text_delete();
        assert_eq!(app.doc.text_editor.as_ref().unwrap().text, "");
        assert_eq!(app.doc.text_editor.as_ref().unwrap().cursor, 0);

        // Wstawianie tekstu (w tym polskich znaków UTF-8)
        app.text_insert_str("Gżegżółka");
        assert_eq!(app.doc.text_editor.as_ref().unwrap().text, "Gżegżółka");
        assert_eq!(app.doc.text_editor.as_ref().unwrap().cursor, 9);

        // Strzałki lewo/prawo, Home/End, Backspace na początku i w środku
        app.text_move_home();
        assert_eq!(app.doc.text_editor.as_ref().unwrap().cursor, 0);
        app.text_backspace(); // na początku nic nie usuwa
        assert_eq!(app.doc.text_editor.as_ref().unwrap().text, "Gżegżółka");

        app.text_move_right();
        app.text_move_right();
        assert_eq!(app.doc.text_editor.as_ref().unwrap().cursor, 2); // po "Gż"
        app.text_backspace(); // usuwa 'ż'
        assert_eq!(app.doc.text_editor.as_ref().unwrap().text, "Gegżółka");
        assert_eq!(app.doc.text_editor.as_ref().unwrap().cursor, 1);

        app.text_delete(); // usuwa 'e'
        assert_eq!(app.doc.text_editor.as_ref().unwrap().text, "Ggżółka");
        assert_eq!(app.doc.text_editor.as_ref().unwrap().cursor, 1);

        app.text_move_end();
        assert_eq!(app.doc.text_editor.as_ref().unwrap().cursor, 7);

        // Nowa linia i nawigacja góra/dół między wierszami
        app.text_insert_newline();
        app.text_insert_str("Druga");
        assert_eq!(app.doc.text_editor.as_ref().unwrap().text, "Ggżółka\nDruga");
        assert_eq!(
            app.doc.text_editor.as_ref().unwrap().cursor_line_col(),
            (1, 5)
        );

        // Strzałka w górę przenosi do wiersza 0, kolumny 5
        app.text_move_up();
        assert_eq!(
            app.doc.text_editor.as_ref().unwrap().cursor_line_col(),
            (0, 5)
        );
        app.text_move_left();
        assert_eq!(
            app.doc.text_editor.as_ref().unwrap().cursor_line_col(),
            (0, 4)
        );

        // Strzałka w dół wraca do wiersza 1, kolumny 4
        app.text_move_down();
        assert_eq!(
            app.doc.text_editor.as_ref().unwrap().cursor_line_col(),
            (1, 4)
        );
    }

    #[test]
    fn test_zoom_and_measure_tools() {
        let mut app = DziwakApp::new();

        // 1. Narzędzie Lupa (ActiveTool::Zoom)
        app.set_active_tool(ActiveTool::Zoom);
        assert_eq!(app.active_tool, ActiveTool::Zoom);

        let initial_zoom = app.doc.transform.zoom;
        // Kliknięcie / zoom around
        app.doc
            .transform
            .zoom_around(egui::pos2(400.0, 300.0), 1.5, 0.05, 64.0);
        assert!((app.doc.transform.zoom - initial_zoom * 1.5).abs() < 1e-3);

        // Przeciągnięcie prostokąta powiększenia
        let target_rect =
            egui::Rect::from_min_max(egui::pos2(100.0, 100.0), egui::pos2(300.0, 200.0));
        let viewport_rect =
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 600.0));
        app.zoom_to_canvas_rect(target_rect, viewport_rect);
        assert!(app.doc.transform.zoom >= 3.0);

        // 2. Narzędzie Miarka (ActiveTool::Measure)
        app.set_active_tool(ActiveTool::Measure);
        assert_eq!(app.active_tool, ActiveTool::Measure);
        assert!(app.measure_stats().is_none());

        // Odcinek 30 px poziomo, 40 px pionowo -> dł. 50 px
        app.doc.measure_start = Some(egui::pos2(10.0, 10.0));
        app.doc.measure_end = Some(egui::pos2(40.0, 50.0));

        let (len, angle, dx, dy) = app.measure_stats().unwrap();
        assert!((dx - 30.0).abs() < 1e-3);
        assert!((dy - 40.0).abs() < 1e-3);
        assert!((len - 50.0).abs() < 1e-3);
        assert!((angle - (-40.0_f32).atan2(30.0).to_degrees()).abs() < 1e-3);
    }

    #[test]
    fn test_moving_layer_invalidates_thumbnails() {
        let mut app = DziwakApp::new();
        assert!(app.doc.document.layer_count() >= 2);
        app.doc.active_layer_index = 0;
        app.ensure_layer_caches();
        let before = app.doc.layer_versions.clone();
        app.move_active_layer_up();
        assert_eq!(app.doc.active_layer_index, 1);
        assert_ne!(app.doc.layer_versions[0], before[0]);
        assert_ne!(app.doc.layer_versions[1], before[1]);
        let mid = app.doc.layer_versions.clone();
        app.move_active_layer_down();
        assert_ne!(app.doc.layer_versions[0], mid[0]);
    }

    #[test]
    fn test_clear_with_rect_selection_clears_only_center() {
        let mut app = DziwakApp::new();
        app.doc.active_layer_index = 0; // Warstwa 0 jest cała biała

        app.doc.selection.select_rect(10, 10, 20, 20);
        app.clear_selection_or_layer();

        // Środek zaznaczenia jest wyczyszczony (przezroczysty)
        assert_eq!(
            app.doc.document.layers[0].get_pixel(15, 15),
            Some(Rgba8::TRANSPARENT)
        );
        // Piksele poza zaznaczeniem pozostają białe
        assert_eq!(
            app.doc.document.layers[0].get_pixel(0, 0),
            Some(Rgba8::WHITE)
        );
        assert_eq!(
            app.doc.document.layers[0].get_pixel(50, 50),
            Some(Rgba8::WHITE)
        );

        // Undo przywraca wyczyszczony środek
        let _ = app.handle_undo();
        assert_eq!(
            app.doc.document.layers[0].get_pixel(15, 15),
            Some(Rgba8::WHITE)
        );
    }

    #[test]
    fn test_clear_without_selection_clears_entire_layer() {
        let mut app = DziwakApp::new();
        app.doc.active_layer_index = 0; // Warstwa 0 jest cała biała
        app.doc.selection.clear();
        assert!(!app.doc.selection.has_selection);

        app.clear_selection_or_layer();

        assert_eq!(
            app.doc.document.layers[0].get_pixel(0, 0),
            Some(Rgba8::TRANSPARENT)
        );
        assert_eq!(
            app.doc.document.layers[0].get_pixel(400, 300),
            Some(Rgba8::TRANSPARENT)
        );
        assert_eq!(
            app.doc.document.layers[0].get_pixel(799, 599),
            Some(Rgba8::TRANSPARENT)
        );

        let _ = app.handle_undo();
        assert_eq!(
            app.doc.document.layers[0].get_pixel(0, 0),
            Some(Rgba8::WHITE)
        );
    }

    #[test]
    fn test_fill_fg_and_bg_fills_only_selection() {
        let mut app = DziwakApp::new();
        app.doc.active_layer_index = 0;
        app.brush_color = egui::Color32::from_rgb(200, 40, 60);
        app.bg_color = egui::Color32::from_rgb(10, 120, 220);

        app.doc.selection.select_rect(20, 20, 15, 15);
        app.fill_with_fg_color();

        let expected_fg = Rgba8::from_straight(200, 40, 60, 255);
        assert_eq!(
            app.doc.document.layers[0].get_pixel(25, 25),
            Some(expected_fg)
        );
        assert_eq!(
            app.doc.document.layers[0].get_pixel(5, 5),
            Some(Rgba8::WHITE)
        );
        assert_eq!(
            app.doc.document.layers[0].get_pixel(50, 50),
            Some(Rgba8::WHITE)
        );

        app.fill_with_bg_color();
        let expected_bg = Rgba8::from_straight(10, 120, 220, 255);
        assert_eq!(
            app.doc.document.layers[0].get_pixel(25, 25),
            Some(expected_bg)
        );
        assert_eq!(
            app.doc.document.layers[0].get_pixel(5, 5),
            Some(Rgba8::WHITE)
        );
    }

    #[test]
    fn test_close_decision_without_modifications_allows_close() {
        let mut app = DziwakApp::new();
        assert!(!app.doc.is_modified);
        assert_eq!(app.on_close_requested(), CloseDecision::Allow);
        assert!(!app.show_close_confirm_dialog);
    }

    #[test]
    fn test_close_decision_with_modifications_asks_user() {
        let mut app = DziwakApp::new();
        app.push_history();
        assert!(app.doc.is_modified);
        assert_eq!(app.on_close_requested(), CloseDecision::AskUser);
        assert!(app.show_close_confirm_dialog);

        // Anulowanie zamykania chowa okno, ale przy kolejnej próbie znowu pyta
        app.cancel_close_request();
        assert!(!app.show_close_confirm_dialog);
        assert_eq!(app.on_close_requested(), CloseDecision::AskUser);
        assert!(app.show_close_confirm_dialog);
    }

    #[test]
    fn test_close_decision_after_discard_allows_close() {
        let mut app = DziwakApp::new();
        app.push_history();
        assert_eq!(app.on_close_requested(), CloseDecision::AskUser);
        assert!(app.show_close_confirm_dialog);

        // Po 'Nie zapisuj' zamknięcie jest dozwolone mimo is_modified == true
        assert!(app.confirm_discard_and_close());
        assert!(!app.show_close_confirm_dialog);
        assert!(app.close_confirmed);
        assert_eq!(app.on_close_requested(), CloseDecision::Allow);
    }

    #[test]
    fn test_two_tabs_independent_state_and_switch() {
        let mut app = DziwakApp::new();
        let tab1_id = app.doc.id;

        // Modyfikujemy stan w zakładce 1: nowa warstwa, historia, zaznaczenie, zoom
        app.add_new_layer();
        app.doc.selection.select_rect(10, 20, 30, 40);
        app.doc.transform.zoom = 2.5;
        app.doc.transform.pan = egui::vec2(120.0, 80.0);
        let tab1_layers = app.doc.document.layer_count();
        assert!(app.doc.history.can_undo());
        assert!(app.doc.selection.has_selection);

        // Tworzymy zakładkę 2
        let tab2_id = app.new_tab();
        assert_ne!(tab1_id, tab2_id);
        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.doc.id, tab2_id);

        // Zakładka 2 ma własny, niezależny stan początkowy
        assert_eq!(app.doc.document.layer_count(), 3);
        assert_ne!(app.doc.document.layer_count(), tab1_layers);
        assert!(!app.doc.history.can_undo());
        assert!(!app.doc.selection.has_selection);
        assert!((app.doc.transform.zoom - 1.0).abs() < 1e-4);

        // Zmieniamy stan w zakładce 2
        app.doc.transform.zoom = 0.5;
        app.doc.selection.select_rect(1, 2, 5, 6);

        // Przełączamy z powrotem na zakładkę 1 — stan zakładki 1 nie został utracony
        assert!(app.switch_to_tab(tab1_id));
        assert_eq!(app.doc.id, tab1_id);
        assert_eq!(app.doc.document.layer_count(), tab1_layers);
        assert!(app.doc.history.can_undo());
        assert!(app.doc.selection.has_selection);
        assert_eq!(app.doc.selection.coverage(15, 25), 255);
        assert_eq!(app.doc.selection.coverage(2, 3), 0);
        assert!((app.doc.transform.zoom - 2.5).abs() < 1e-4);
        assert_eq!(app.doc.transform.pan, egui::vec2(120.0, 80.0));

        // Przełączanie next_tab / prev_tab zachowuje stan zakładki 2
        app.next_tab();
        assert_eq!(app.doc.id, tab2_id);
        assert!((app.doc.transform.zoom - 0.5).abs() < 1e-4);
        assert_eq!(app.doc.selection.coverage(2, 3), 255);

        app.prev_tab();
        assert_eq!(app.doc.id, tab1_id);
    }

    #[test]
    fn test_release_textures_frees_inactive_tab_textures_and_visible_dirty_upload() {
        let ctx = egui::Context::default();
        let mut app = DziwakApp::new();
        let tab1_id = app.doc.id;

        // Renderujemy tylko wycinek płótna (np. pierwszy kafel 64x64)
        let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0));
        app.doc.transform = ViewTransform::new(egui::Vec2::ZERO, 1.0);
        let total_tiles = app.doc.tile_renderer.textures.len();
        assert!(total_tiles > 1);

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.doc.tile_renderer.update_visible_dirty_tiles(
                ctx,
                &app.doc.document,
                &app.doc.transform,
                viewport,
            );
            let _ = app.layer_thumbnail_texture(ctx, 0);
        });

        // Tylko widoczny kafel (0, 0) powinien zostać załadowany na GPU; reszta pozostaje brudna i None
        assert!(app.doc.tile_renderer.textures[0].is_some());
        let loaded_count = app
            .doc
            .tile_renderer
            .textures
            .iter()
            .filter(|t| t.is_some())
            .count();
        assert_eq!(loaded_count, 1);
        assert_eq!(app.doc.tile_renderer.dirty_tiles.len(), total_tiles - 1);
        assert!(app.doc.layer_thumbnails.iter().any(|t| t.is_some()));

        // Otwarcie nowej zakładki przenosi zakładkę 1 w tło i zwalnia jej tekstury GPU
        let _tab2_id = app.new_tab();
        let bg_tab1 = app.tab_by_id(tab1_id).unwrap();
        assert!(bg_tab1.tile_renderer.textures.iter().all(|t| t.is_none()));
        assert_eq!(bg_tab1.tile_renderer.dirty_tiles.len(), total_tiles);
        assert!(bg_tab1.layer_thumbnails.is_empty());
    }

    #[test]
    fn test_history_budget_divides_on_open_and_restores_on_close() {
        let mut app = DziwakApp::new();
        let tab1_id = app.doc.id;
        assert_eq!(app.doc.history.max_bytes, HISTORY_BUDGET_BYTES);

        let tab2_id = app.new_tab();
        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.doc.history.max_bytes, HISTORY_BUDGET_BYTES / 2);
        assert_eq!(
            app.tab_by_id(tab1_id).unwrap().history.max_bytes,
            HISTORY_BUDGET_BYTES / 2
        );

        let tab3_id = app.new_tab();
        assert_eq!(app.tab_count(), 3);
        for tid in [tab1_id, tab2_id, tab3_id] {
            assert_eq!(
                app.tab_by_id(tid).unwrap().history.max_bytes,
                HISTORY_BUDGET_BYTES / 3
            );
        }

        // Zamknięcie zakładki 3 zwiększa budżet pozostałych dwóch zakładek do 1/2
        app.close_tab_immediately(tab3_id);
        assert_eq!(app.tab_count(), 2);
        for tid in [tab1_id, tab2_id] {
            assert_eq!(
                app.tab_by_id(tid).unwrap().history.max_bytes,
                HISTORY_BUDGET_BYTES / 2
            );
        }

        // Zamknięcie zakładki 2 przywraca pełny budżet dla zakładki 1
        app.close_tab_immediately(tab2_id);
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.doc.id, tab1_id);
        assert_eq!(app.doc.history.max_bytes, HISTORY_BUDGET_BYTES);
    }

    #[test]
    fn test_closing_modified_tab_asks_for_save_and_closing_last_tab_leaves_untitled() {
        let mut app = DziwakApp::new();
        let tab1_id = app.doc.id;
        app.push_history();
        assert!(app.doc.is_modified);

        let tab2_id = app.new_tab();
        assert!(!app.doc.is_modified);

        // Zamknięcie niezmodyfikowanej zakładki 2 zamyka ją od razu bez pytania
        app.request_close_tab(tab2_id);
        assert!(!app.show_close_confirm_dialog);
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.doc.id, tab1_id);

        // Zamknięcie zmodyfikowanej zakładki 1 pyta o zapis
        app.request_close_tab(tab1_id);
        assert!(app.show_close_confirm_dialog);
        assert!(!app.closing_entire_app);
        assert_eq!(app.close_confirm_tab_id, Some(tab1_id));

        // Odrzucenie zmian ('Nie zapisuj') zamyka ostatnią zakładkę i zostawia świeżą 'bez nazwy'
        assert!(!app.confirm_discard_and_close());
        assert!(!app.show_close_confirm_dialog);
        assert_eq!(app.tab_count(), 1);
        assert_ne!(app.doc.id, tab1_id);
        assert!(!app.doc.is_modified);
        assert!(app.doc.current_file_path.is_none());
    }

    #[test]
    fn test_closing_app_with_two_modified_tabs_asks_twice() {
        let mut app = DziwakApp::new();
        let tab1_id = app.doc.id;
        app.push_history();

        let tab2_id = app.new_tab();
        app.push_history();

        // Obie zakładki są zmodyfikowane; aktywna jest zakładka 2.
        // Żądanie zamknięcia programu pyta najpierw o pierwszą zmodyfikowaną zakładkę (tab1_id) i aktywuje ją.
        assert_eq!(app.on_close_requested(), CloseDecision::AskUser);
        assert!(app.show_close_confirm_dialog);
        assert!(app.closing_entire_app);
        assert_eq!(app.doc.id, tab1_id);
        assert_eq!(app.close_confirm_tab_id, Some(tab1_id));

        // Pierwsze 'Nie zapisuj' przechodzi do drugiej zmodyfikowanej zakładki (tab2_id) i nadal nie zamyka okna
        assert!(!app.confirm_discard_and_close());
        assert!(app.show_close_confirm_dialog);
        assert!(app.closing_entire_app);
        assert_eq!(app.doc.id, tab2_id);
        assert_eq!(app.close_confirm_tab_id, Some(tab2_id));
        assert!(!app.close_confirmed);

        // Drugie 'Nie zapisuj' kończy proces i pozwala zamknąć program
        assert!(app.confirm_discard_and_close());
        assert!(!app.show_close_confirm_dialog);
        assert!(app.close_confirmed);
        assert_eq!(app.on_close_requested(), CloseDecision::Allow);
    }

    #[test]
    fn test_cli_multiple_files_open_as_separate_tabs() {
        let tmp_dir = std::env::temp_dir();
        let path_a = tmp_dir.join("dziwak_cli_tab_a.png");
        let path_b = tmp_dir.join("dziwak_cli_tab_b.png");

        let doc_a = create_demo_document(32, 32);
        let doc_b = create_demo_document(48, 48);
        dziwak_core::save_image(&doc_a, &path_a).unwrap();
        dziwak_core::save_image(&doc_b, &path_b).unwrap();

        let mut app = DziwakApp::new();
        app.open_files_from_cli(vec![path_a.clone(), path_b.clone()]);

        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.tab_order.len(), 2);
        let tab_a = app.tab_by_id(app.tab_order[0]).unwrap();
        let tab_b = app.tab_by_id(app.tab_order[1]).unwrap();
        assert_eq!(tab_a.current_file_path, Some(path_a.clone()));
        assert_eq!(tab_a.document.width, 32);
        assert_eq!(tab_b.current_file_path, Some(path_b.clone()));
        assert_eq!(tab_b.document.width, 48);

        let _ = std::fs::remove_file(path_a);
        let _ = std::fs::remove_file(path_b);
    }
}
