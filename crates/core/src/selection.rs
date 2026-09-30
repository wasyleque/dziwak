//! Maska zaznaczenia (Selection) oparta na siatce kafli 64×64 bajtów.
//!
//! Każdy kafel maski przechowuje wartości krycia 0..=255 w buforze `Arc<[u8; 4096]>`.
//! Puste kafle (`None`) oznaczają brak zaznaczenia (wartość 0), co eliminuje alokację pamięci.
//! Gdy flaga `has_selection` wynosi `false`, oznacza to brak aktywnego zaznaczenia,
//! co w programach graficznych odpowiada zaznaczeniu całego obrazu (coverage = 255).

use std::sync::Arc;

use crate::document::Rect;
use crate::tile::{TILE_PIXELS, TILE_SIZE};

/// Sposób łączenia nowego zaznaczenia z istniejącym (jak w GIMP: Shift = dodaj, Ctrl = odejmij, Shift+Ctrl = przetnij).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectMode {
    #[default]
    Replace,
    Add,
    Subtract,
    Intersect,
}

/// Kafel maski zaznaczenia o rozmiarze 64×64 bajtów (4 KiB) współdzielony wskaźnikiem Arc.
pub type SelectionTile = Arc<[u8; TILE_PIXELS]>;

/// Tworzy kafel maski zaznaczenia wypełniony zadaną wartością.
pub fn create_filled_selection_tile(val: u8) -> SelectionTile {
    Arc::new([val; TILE_PIXELS])
}

/// Sprawdza, czy wszystkie piksele w kaflu maski wynoszą 0.
pub fn is_selection_tile_empty(tile: &[u8; TILE_PIXELS]) -> bool {
    tile.iter().all(|&v| v == 0)
}

/// Maska zaznaczenia dla dokumentu o wymiarach `width` × `height` pikseli.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    /// Szerokość płótna w pikselach.
    pub width: u32,
    /// Wysokość płótna w pikselach.
    pub height: u32,
    /// Siatka kafli maski (0..=255). Pusty kafel to `None`.
    pub tiles: Vec<Option<SelectionTile>>,
    /// Flaga informująca, czy istnieje aktywne zaznaczenie.
    /// `false` oznacza brak zaznaczenia (cały obraz jest edytowalny, coverage = 255).
    pub has_selection: bool,
}

impl Selection {
    /// Tworzy nowe zaznaczenie dla dokumentu o zadanych wymiarach.
    /// Początkowo brak zaznaczenia (`has_selection = false`), co oznacza cały obraz.
    pub fn new(width: u32, height: u32) -> Self {
        let tiles_x = if width == 0 {
            0
        } else {
            (width - 1) / (TILE_SIZE as u32) + 1
        };
        let tiles_y = if height == 0 {
            0
        } else {
            (height - 1) / (TILE_SIZE as u32) + 1
        };
        let total_tiles = (tiles_x as usize) * (tiles_y as usize);
        Self {
            width,
            height,
            tiles: vec![None; total_tiles],
            has_selection: false,
        }
    }

    /// Zwraca liczbę kolumn kafli w masce.
    pub fn tiles_across(&self) -> u32 {
        if self.width == 0 {
            0
        } else {
            (self.width - 1) / (TILE_SIZE as u32) + 1
        }
    }

    /// Zwraca liczbę wierszy kafli w masce.
    pub fn tiles_down(&self) -> u32 {
        if self.height == 0 {
            0
        } else {
            (self.height - 1) / (TILE_SIZE as u32) + 1
        }
    }

    /// Zwraca indeks w wektorze `tiles` dla współrzędnych kafla (tx, ty).
    pub fn tile_index(&self, tx: u32, ty: u32) -> Option<usize> {
        let across = self.tiles_across();
        let down = self.tiles_down();
        if tx < across && ty < down {
            Some((ty * across + tx) as usize)
        } else {
            None
        }
    }

    /// Zwraca stopień pokrycia zaznaczenia dla piksela (x, y) w zakresie 0..=255.
    ///
    /// Jeśli nie ma aktywnego zaznaczenia (`!has_selection`), zwraca 255 dla pikseli
    /// leżących wewnątrz granic płótna (zasada "brak zaznaczenia = cały obraz").
    /// Poza granicami płótna zawsze zwraca 0.
    pub fn coverage(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        if !self.has_selection {
            return 255;
        }

        let tile_size = TILE_SIZE as u32;
        let tx = x / tile_size;
        let ty = y / tile_size;
        let lx = (x % tile_size) as usize;
        let ly = (y % tile_size) as usize;

        let across = self.tiles_across();
        let tile_idx = (ty * across + tx) as usize;
        match &self.tiles[tile_idx] {
            Some(tile) => tile[ly * TILE_SIZE + lx],
            None => 0,
        }
    }

    /// Czyści zaznaczenie, przywracając stan "brak zaznaczenia = cały obraz".
    pub fn clear(&mut self) {
        self.has_selection = false;
        self.tiles.fill(None);
    }

    /// Tworzy zaznaczenie prostokątne o lewym górnym rogu w `(x, y)` i wymiarach `width × height`.
    pub fn select_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.has_selection = true;
        self.tiles.fill(None);

        let x0 = x.min(self.width);
        let y0 = y.min(self.height);
        let x1 = (x.saturating_add(width)).min(self.width);
        let y1 = (y.saturating_add(height)).min(self.height);

        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let tile_size = TILE_SIZE as u32;
        let min_tx = x0 / tile_size;
        let max_tx = (x1 - 1) / tile_size;
        let min_ty = y0 / tile_size;
        let max_ty = (y1 - 1) / tile_size;
        let across = self.tiles_across();

        let all_255_tile = create_filled_selection_tile(255);

        for ty in min_ty..=max_ty {
            let ty0 = ty * tile_size;
            let ty1 = ty0 + tile_size;
            for tx in min_tx..=max_tx {
                let tx0 = tx * tile_size;
                let tx1 = tx0 + tile_size;
                let tile_idx = (ty * across + tx) as usize;

                if tx0 >= x0 && tx1 <= x1 && ty0 >= y0 && ty1 <= y1 {
                    // Cały kafel mieści się wewnątrz prostokąta zaznaczenia
                    self.tiles[tile_idx] = Some(Arc::clone(&all_255_tile));
                } else {
                    let mut data = [0u8; TILE_PIXELS];
                    let clip_y0 = ty0.max(y0);
                    let clip_y1 = ty1.min(y1);
                    let clip_x0 = tx0.max(x0);
                    let clip_x1 = tx1.min(x1);

                    for y_px in clip_y0..clip_y1 {
                        let ly = (y_px - ty0) as usize;
                        let lx0 = (clip_x0 - tx0) as usize;
                        let lx1 = (clip_x1 - tx0) as usize;
                        data[ly * TILE_SIZE + lx0..ly * TILE_SIZE + lx1].fill(255);
                    }

                    self.tiles[tile_idx] = Some(Arc::new(data));
                }
            }
        }
    }

    /// Tworzy zaznaczenie eliptyczne wpisane w prostokąt `(x, y, width, height)` z antyaliasingiem krawędzi.
    pub fn select_ellipse(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.has_selection = true;
        self.tiles.fill(None);

        if width == 0 || height == 0 {
            return;
        }

        let x0 = x.min(self.width);
        let y0 = y.min(self.height);
        let x1 = (x.saturating_add(width)).min(self.width);
        let y1 = (y.saturating_add(height)).min(self.height);

        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let rx = width as f32 * 0.5;
        let ry = height as f32 * 0.5;
        let cx = x as f32 + rx;
        let cy = y as f32 + ry;

        let tile_size = TILE_SIZE as u32;
        let min_tx = x0 / tile_size;
        let max_tx = (x1 - 1) / tile_size;
        let min_ty = y0 / tile_size;
        let max_ty = (y1 - 1) / tile_size;
        let across = self.tiles_across();

        for ty in min_ty..=max_ty {
            let ty0 = ty * tile_size;
            let ty1 = (ty0 + tile_size).min(self.height);
            for tx in min_tx..=max_tx {
                let tx0 = tx * tile_size;
                let tx1 = (tx0 + tile_size).min(self.width);
                let tile_idx = (ty * across + tx) as usize;

                let clip_y0 = ty0.max(y0);
                let clip_y1 = ty1.min(y1);
                let clip_x0 = tx0.max(x0);
                let clip_x1 = tx1.min(x1);

                let mut data = [0u8; TILE_PIXELS];
                let mut any_non_zero = false;

                for py_u in clip_y0..clip_y1 {
                    let py = py_u as f32 + 0.5;
                    let dy = py - cy;
                    let ly = (py_u - ty0) as usize;
                    let row_offset = ly * TILE_SIZE;

                    for px_u in clip_x0..clip_x1 {
                        let px = px_u as f32 + 0.5;
                        let dx = px - cx;
                        let lx = (px_u - tx0) as usize;

                        let nx = dx / rx;
                        let ny = dy / ry;
                        let d2 = nx * nx + ny * ny;

                        let cov = if d2 <= 0.0 {
                            255
                        } else {
                            let val = d2 - 1.0;
                            let gx = 2.0 * dx / (rx * rx);
                            let gy = 2.0 * dy / (ry * ry);
                            let glen = gx.hypot(gy);
                            let dist = if glen > 1e-6 { -val / glen } else { 0.0 };
                            ((dist + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8
                        };

                        if cov > 0 {
                            data[row_offset + lx] = cov;
                            any_non_zero = true;
                        }
                    }
                }

                if any_non_zero {
                    self.tiles[tile_idx] = Some(Arc::new(data));
                }
            }
        }
    }

    /// Odwraca zaznaczenie (krycie staje się 255 - krycie).
    pub fn invert(&mut self) {
        if !self.has_selection {
            self.has_selection = true;
            self.tiles.fill(None);
            return;
        }

        let across = self.tiles_across();
        let down = self.tiles_down();
        let tile_size = TILE_SIZE as u32;

        let all_255_tile = create_filled_selection_tile(255);

        for ty in 0..down {
            for tx in 0..across {
                let tile_idx = (ty * across + tx) as usize;
                let x0 = tx * tile_size;
                let y0 = ty * tile_size;
                let x1 = (x0 + tile_size).min(self.width);
                let y1 = (y0 + tile_size).min(self.height);

                let is_full_tile = (x1 - x0 == tile_size) && (y1 - y0 == tile_size);

                match &mut self.tiles[tile_idx] {
                    None => {
                        if is_full_tile {
                            self.tiles[tile_idx] = Some(Arc::clone(&all_255_tile));
                        } else {
                            let mut data = [0u8; TILE_PIXELS];
                            for y in y0..y1 {
                                let ly = (y - y0) as usize;
                                let start = ly * TILE_SIZE;
                                let end = start + (x1 - x0) as usize;
                                data[start..end].fill(255);
                            }
                            self.tiles[tile_idx] = Some(Arc::new(data));
                        }
                    }
                    Some(tile) => {
                        let tile_data = Arc::make_mut(tile);
                        let mut any_non_zero = false;

                        for y in 0..TILE_SIZE as u32 {
                            let py = y0 + y;
                            let ly = y as usize;
                            let row_offset = ly * TILE_SIZE;

                            for x in 0..TILE_SIZE as u32 {
                                let px = x0 + x;
                                let lx = x as usize;
                                let idx = row_offset + lx;

                                if px < self.width && py < self.height {
                                    tile_data[idx] = 255 - tile_data[idx];
                                    if tile_data[idx] > 0 {
                                        any_non_zero = true;
                                    }
                                } else {
                                    tile_data[idx] = 0;
                                }
                            }
                        }

                        if !any_non_zero {
                            self.tiles[tile_idx] = None;
                        }
                    }
                }
            }
        }
    }

    /// Zwraca prostokąt otaczający (bounding box) zaznaczenia lub `None`, jeśli nic nie jest zaznaczone
    /// lub nie ma aktywnego zaznaczenia (`!has_selection`).
    pub fn bounds(&self) -> Option<Rect> {
        if !self.has_selection {
            return None;
        }

        let mut min_x = u32::MAX;
        let mut max_x = 0;
        let mut min_y = u32::MAX;
        let mut max_y = 0;
        let mut any = false;

        let across = self.tiles_across();
        let down = self.tiles_down();
        let tile_size = TILE_SIZE as u32;

        for ty in 0..down {
            for tx in 0..across {
                let tile_idx = (ty * across + tx) as usize;
                if let Some(tile) = &self.tiles[tile_idx] {
                    let x0 = tx * tile_size;
                    let y0 = ty * tile_size;
                    let x1 = (x0 + tile_size).min(self.width);
                    let y1 = (y0 + tile_size).min(self.height);

                    for y in y0..y1 {
                        let ly = (y - y0) as usize;
                        let row_offset = ly * TILE_SIZE;
                        for x in x0..x1 {
                            let lx = (x - x0) as usize;
                            if tile[row_offset + lx] > 0 {
                                min_x = min_x.min(x);
                                max_x = max_x.max(x);
                                min_y = min_y.min(y);
                                max_y = max_y.max(y);
                                any = true;
                            }
                        }
                    }
                }
            }
        }

        if any {
            Some(Rect::new(
                min_x,
                min_y,
                max_x - min_x + 1,
                max_y - min_y + 1,
            ))
        } else {
            None
        }
    }

    /// Łączy maskę `mask` (długość width*height, wartości 0..=255) z bieżącym zaznaczeniem.
    pub fn combine_mask(&mut self, mask: &[u8], mode: SelectMode) {
        if mask.len() != (self.width * self.height) as usize {
            return;
        }

        let tiles_across = self.tiles_across();
        let tiles_down = self.tiles_down();

        // Najpierw policz wszystkie kafle do nowego Vec
        let mut new_tiles = vec![None; self.tiles.len()];

        for ty in 0..tiles_down {
            for tx in 0..tiles_across {
                let mut buf = [0u8; TILE_PIXELS];
                let mut any = false;

                let x0 = tx * TILE_SIZE as u32;
                let y0 = ty * TILE_SIZE as u32;

                for ly in 0..TILE_SIZE as u32 {
                    let y = y0 + ly;
                    if y >= self.height {
                        break;
                    }

                    for lx in 0..TILE_SIZE as u32 {
                        let x = x0 + lx;
                        if x >= self.width {
                            break;
                        }

                        let idx = (y * self.width + x) as usize;
                        let m = mask[idx] as u32;
                        let old = if self.has_selection {
                            self.coverage(x, y) as u32
                        } else if mode == SelectMode::Subtract || mode == SelectMode::Intersect {
                            255
                        } else {
                            0
                        };

                        let new = match mode {
                            SelectMode::Replace => m,
                            SelectMode::Add => old.max(m),
                            SelectMode::Subtract => old * (255 - m) / 255,
                            SelectMode::Intersect => old.min(m),
                        };

                        buf[ly as usize * TILE_SIZE + lx as usize] = new as u8;
                        if new > 0 {
                            any = true;
                        }
                    }
                }

                let idx = (ty * tiles_across + tx) as usize;
                new_tiles[idx] = if any { Some(Arc::new(buf)) } else { None };
            }
        }

        self.tiles = new_tiles;
        self.has_selection = self.tiles.iter().any(|t| t.is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_default_state_and_clear() {
        let mut sel = Selection::new(100, 100);
        // Początkowo brak zaznaczenia = cały obraz (coverage 255)
        assert!(!sel.has_selection);
        assert_eq!(sel.coverage(50, 50), 255);
        assert_eq!(sel.coverage(0, 0), 255);
        assert_eq!(sel.bounds(), None);

        // Poza granicami zawsze 0
        assert_eq!(sel.coverage(100, 100), 0);

        // Zaznaczamy cokolwiek
        sel.select_rect(10, 10, 20, 20);
        assert!(sel.has_selection);
        assert_eq!(sel.coverage(15, 15), 255);
        assert_eq!(sel.coverage(0, 0), 0);
        assert_eq!(sel.bounds(), Some(Rect::new(10, 10, 20, 20)));

        // Czyszczenie przywraca stan domyślny
        sel.clear();
        assert!(!sel.has_selection);
        assert_eq!(sel.coverage(15, 15), 255);
        assert_eq!(sel.coverage(0, 0), 255);
        assert_eq!(sel.bounds(), None);
    }

    #[test]
    fn test_select_rect_on_tile_boundary() {
        // Płótno 128×128 (4 kafle po 64×64: granica w x=64, y=64)
        let mut sel = Selection::new(128, 128);
        // Zaznaczenie 60..70 x 60..70 przecinające 4 kafle
        sel.select_rect(60, 60, 10, 10);
        assert!(sel.has_selection);

        // Granice zaznaczenia
        assert_eq!(sel.coverage(59, 59), 0);
        assert_eq!(sel.coverage(60, 60), 255);
        assert_eq!(sel.coverage(69, 69), 255);
        assert_eq!(sel.coverage(70, 70), 0);

        // Przecięcie na rogach 4 kafli:
        assert_eq!(sel.coverage(63, 63), 255); // kafel (0, 0)
        assert_eq!(sel.coverage(64, 63), 255); // kafel (1, 0)
        assert_eq!(sel.coverage(63, 64), 255); // kafel (0, 1)
        assert_eq!(sel.coverage(64, 64), 255); // kafel (1, 1)

        // Bounding box
        assert_eq!(sel.bounds(), Some(Rect::new(60, 60, 10, 10)));

        // Wszystkie 4 kafle powinny być zaalokowane
        assert!(sel.tiles[0].is_some());
        assert!(sel.tiles[1].is_some());
        assert!(sel.tiles[2].is_some());
        assert!(sel.tiles[3].is_some());
    }

    #[test]
    fn test_select_ellipse() {
        let mut sel = Selection::new(100, 100);
        // Elipsa w bounding boxie (10, 10, 40, 40)
        sel.select_ellipse(10, 10, 40, 40);
        assert!(sel.has_selection);

        // Środek elipsy (30, 30): pełne krycie 255
        assert_eq!(sel.coverage(30, 30), 255);

        // Róg bounding boxa (10, 10): krycie 0
        assert_eq!(sel.coverage(10, 10), 0);
        assert_eq!(sel.coverage(49, 10), 0);
        assert_eq!(sel.coverage(10, 49), 0);
        assert_eq!(sel.coverage(49, 49), 0);

        // Punkty na krawędzi elipsy posiadają antyaliasing (płynne przejście 0..=255)
        let edge_val = sel.coverage(30, 10);
        assert!(
            edge_val > 0,
            "punkt na szczycie elipsy powinien mieć pokrycie > 0"
        );

        // Bounding box powinien obejmować obszar elipsy
        let bounds = sel.bounds().expect("elipsa powinna mieć bounds");
        assert_eq!(bounds.x, 10);
        assert_eq!(bounds.y, 10);
        assert_eq!(bounds.width, 40);
        assert_eq!(bounds.height, 40);
    }

    #[test]
    fn test_selection_invert() {
        let mut sel = Selection::new(100, 100);
        sel.select_rect(20, 20, 10, 10);

        assert_eq!(sel.coverage(25, 25), 255);
        assert_eq!(sel.coverage(0, 0), 0);
        assert_eq!(sel.bounds(), Some(Rect::new(20, 20, 10, 10)));

        // Pierwsze odwrócenie: obszar (20..30, 20..30) ma 0, reszta 255
        sel.invert();
        assert!(sel.has_selection);
        assert_eq!(sel.coverage(25, 25), 0);
        assert_eq!(sel.coverage(0, 0), 255);
        assert_eq!(sel.coverage(90, 90), 255);
        assert_eq!(sel.bounds(), Some(Rect::new(0, 0, 100, 100)));

        // Drugie odwrócenie: powrót do pierwotnego zaznaczenia
        sel.invert();
        assert_eq!(sel.coverage(25, 25), 255);
        assert_eq!(sel.coverage(0, 0), 0);
        assert_eq!(sel.bounds(), Some(Rect::new(20, 20, 10, 10)));
    }

    #[test]
    fn test_combine_mask() {
        let mut sel = Selection::new(100, 100);

        // Test Replace z maską prostokąta 3x3
        let mut mask = vec![0u8; 100 * 100];
        for y in 48..51 {
            for x in 48..51 {
                mask[y * 100 + x] = 255;
            }
        }
        sel.combine_mask(&mask, SelectMode::Replace);
        assert!(sel.has_selection);
        assert_eq!(sel.coverage(50, 50), 255);
        assert_eq!(sel.coverage(47, 47), 0);

        // Test Add dwóch rozłącznych masek
        let mut mask1 = vec![0u8; 100 * 100];
        mask1[20 * 100 + 20] = 255;
        sel.combine_mask(&mask1, SelectMode::Add);
        assert_eq!(sel.coverage(20, 20), 255);

        let mut mask2 = vec![0u8; 100 * 100];
        mask2[30 * 100 + 30] = 255;
        sel.combine_mask(&mask2, SelectMode::Add);
        assert_eq!(sel.coverage(20, 20), 255);
        assert_eq!(sel.coverage(30, 30), 255);

        // Test Subtract maski z zaznaczenia wszystkiego
        let mut sel2 = Selection::new(100, 100);
        sel2.select_rect(0, 0, 100, 100);
        let mut mask3 = vec![0u8; 100 * 100];
        mask3[50 * 100 + 50] = 255;
        sel2.combine_mask(&mask3, SelectMode::Subtract);
        assert_eq!(sel2.coverage(50, 50), 0);
        assert_eq!(sel2.coverage(0, 0), 255);

        // Test Intersect
        let mut sel3 = Selection::new(100, 100);
        sel3.select_rect(20, 20, 40, 40);
        let mut mask4 = vec![0u8; 100 * 100];
        for y in 30..50 {
            for x in 30..50 {
                mask4[y * 100 + x] = 255;
            }
        }
        sel3.combine_mask(&mask4, SelectMode::Intersect);
        assert_eq!(sel3.coverage(30, 30), 255);
        assert_eq!(sel3.coverage(20, 20), 0);

        // Test maska samych zer z Replace -> has_selection false
        let mut sel4 = Selection::new(100, 100);
        let mask5 = vec![0u8; 100 * 100];
        sel4.combine_mask(&mask5, SelectMode::Replace);
        assert!(!sel4.has_selection);

        // Test zła długość maski -> bez zmian
        let mut sel5 = Selection::new(100, 100);
        let mask6 = vec![0u8; 50 * 50]; // Zła długość
        sel5.combine_mask(&mask6, SelectMode::Replace);
        assert!(!sel5.has_selection); // Powinno pozostać bez zmian
    }
}
