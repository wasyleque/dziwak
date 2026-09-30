//! Warstwa rastrowa oparta na siatce kafli 64×64 piksele.

use std::sync::Arc;

use crate::blend::BlendMode;
use crate::pixel::Rgba8;
use crate::tile::{create_empty_tile, Tile, TILE_SIZE};

/// Błąd przekroczenia granic warstwy przy dostępie do pikseli.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfBoundsError {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Display for OutOfBoundsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "współrzędne ({}, {}) poza granicami warstwy (szerokość: {}, wysokość: {})",
            self.x, self.y, self.width, self.height
        )
    }
}

impl std::error::Error for OutOfBoundsError {}

/// Warstwa rastrowa podzielona na kafle 64×64 px.
/// Puste kafle są reprezentowane jako `None` (nie zajmują pamięci).
#[derive(Clone, Debug, PartialEq)]
pub struct TiledLayer {
    /// Szerokość warstwy w pikselach.
    pub width: u32,
    /// Wysokość warstwy w pikselach.
    pub height: u32,
    /// Siatka kafli w porządku wierszowym (row-major).
    pub tiles: Vec<Option<Tile>>,
}

impl TiledLayer {
    /// Tworzy nową warstwę o zadanym rozmiarze, początkowo bez zaalokowanych kafli.
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
        }
    }

    /// Zwraca liczbę kolumn kafli w warstwie.
    pub fn tiles_across(&self) -> u32 {
        if self.width == 0 {
            0
        } else {
            (self.width - 1) / (TILE_SIZE as u32) + 1
        }
    }

    /// Zwraca liczbę wierszy kafli w warstwie.
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

    /// Pobiera wartość piksela o współrzędnych (x, y).
    /// Zwraca `None` jeśli współrzędne wykraczają poza rozmiar warstwy.
    /// Jeśli kafel nie jest zaalokowany (`None`), zwraca `Some(Rgba8::TRANSPARENT)`.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let tile_size = TILE_SIZE as u32;
        let tx = x / tile_size;
        let ty = y / tile_size;
        let lx = (x % tile_size) as usize;
        let ly = (y % tile_size) as usize;

        let tile_idx = self.tile_index(tx, ty)?;
        let pixel_idx = ly * TILE_SIZE + lx;

        match &self.tiles[tile_idx] {
            Some(tile) => Some(tile[pixel_idx]),
            None => Some(Rgba8::TRANSPARENT),
        }
    }

    /// Ustawia wartość piksela o współrzędnych (x, y).
    /// W przypadku braku kafla alokuje go tylko wtedy, gdy kolor nie jest przezroczysty.
    /// Jeśli kafel jest współdzielony (np. przez migawkę historii), kopiuje go (copy-on-write).
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba8) -> Result<(), OutOfBoundsError> {
        if x >= self.width || y >= self.height {
            return Err(OutOfBoundsError {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }

        let tile_size = TILE_SIZE as u32;
        let tx = x / tile_size;
        let ty = y / tile_size;
        let lx = (x % tile_size) as usize;
        let ly = (y % tile_size) as usize;

        let across = self.tiles_across();
        let tile_idx = (ty * across + tx) as usize;
        let pixel_idx = ly * TILE_SIZE + lx;

        match &mut self.tiles[tile_idx] {
            Some(tile) => {
                let tile_data = Arc::make_mut(tile);
                tile_data[pixel_idx] = color;
            }
            None => {
                // Jeśli piksel jest przezroczysty, a kafel pusty — nie ma sensu alokować pamięci.
                if !color.is_transparent() {
                    let mut new_tile = create_empty_tile();
                    let tile_data = Arc::make_mut(&mut new_tile);
                    tile_data[pixel_idx] = color;
                    self.tiles[tile_idx] = Some(new_tile);
                }
            }
        }

        Ok(())
    }

    /// Zwraca referencję do kafla na pozycji (tx, ty), o ile jest zaalokowany.
    pub fn tile_at(&self, tx: u32, ty: u32) -> Option<&Tile> {
        let idx = self.tile_index(tx, ty)?;
        self.tiles[idx].as_ref()
    }

    /// Zwraca liczbę aktualnie zaalokowanych kafli (różnych od None).
    pub fn count_allocated_tiles(&self) -> usize {
        self.tiles.iter().filter(|t| t.is_some()).count()
    }

    /// Czyści warstwę, zwalniając wszystkie zaalokowane kafle.
    pub fn clear(&mut self) {
        self.tiles.fill(None);
    }
}

/// Warstwa dokumentu z nazwą, widocznością, współczynnikiem krycia, trybem mieszania i siatką pikseli.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// Nazwa warstwy.
    pub name: String,
    /// Czy warstwa jest widoczna na płótnie.
    pub visible: bool,
    /// Krycie warstwy w zakresie od 0.0 do 1.0.
    pub opacity: f32,
    /// Tryb mieszania warstwy z warstwami poniżej.
    pub blend: BlendMode,
    /// Siatka pikseli zorganizowana w kafle.
    pub pixels: TiledLayer,
}

impl Layer {
    /// Tworzy nową warstwę o zadanym rozmiarze i nazwie (domyślnie w pełni widoczna, tryb Normal).
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            name: name.into(),
            visible: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            pixels: TiledLayer::new(width, height),
        }
    }

    /// Pobiera wartość piksela z warstwy.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        self.pixels.get_pixel(x, y)
    }

    /// Zapisuje wartość piksela w warstwie.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba8) -> Result<(), OutOfBoundsError> {
        self.pixels.set_pixel(x, y, color)
    }

    /// Tworzy duplikat warstwy o podanej nowej nazwie.
    ///
    /// Kafle są współdzielone za pomocą wskaźników `Arc` (tani klon Copy-on-Write).
    pub fn duplicate(&self, new_name: impl Into<String>) -> Self {
        Self {
            name: new_name.into(),
            visible: self.visible,
            opacity: self.opacity,
            blend: self.blend,
            pixels: self.pixels.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let layer = TiledLayer::new(100, 200);
        assert_eq!(layer.width, 100);
        assert_eq!(layer.height, 200);
        assert_eq!(layer.tiles_across(), 2);
        assert_eq!(layer.tiles_down(), 4);
        assert_eq!(layer.tiles.len(), 8);
        assert_eq!(layer.count_allocated_tiles(), 0);

        let empty_layer = TiledLayer::new(0, 0);
        assert_eq!(empty_layer.tiles_across(), 0);
        assert_eq!(empty_layer.tiles_down(), 0);
        assert_eq!(empty_layer.tiles.len(), 0);
    }

    #[test]
    fn test_tiles_across_and_down() {
        let exact = TiledLayer::new(64, 128);
        assert_eq!(exact.tiles_across(), 1);
        assert_eq!(exact.tiles_down(), 2);

        let plus_one = TiledLayer::new(65, 129);
        assert_eq!(plus_one.tiles_across(), 2);
        assert_eq!(plus_one.tiles_down(), 3);
    }

    #[test]
    fn test_tile_index() {
        let layer = TiledLayer::new(128, 128);
        assert_eq!(layer.tile_index(0, 0), Some(0));
        assert_eq!(layer.tile_index(1, 0), Some(1));
        assert_eq!(layer.tile_index(0, 1), Some(2));
        assert_eq!(layer.tile_index(1, 1), Some(3));
        assert_eq!(layer.tile_index(2, 0), None);
        assert_eq!(layer.tile_index(0, 2), None);
    }

    #[test]
    fn test_get_and_set_pixel() {
        let mut layer = TiledLayer::new(100, 100);

        // Odczyt z pustego kafla zwraca przezroczysty piksel
        assert_eq!(layer.get_pixel(10, 10), Some(Rgba8::TRANSPARENT));
        assert_eq!(layer.count_allocated_tiles(), 0);

        // Zapis przezroczystego piksela do pustego kafla nie alokuje go
        assert!(layer.set_pixel(10, 10, Rgba8::TRANSPARENT).is_ok());
        assert_eq!(layer.count_allocated_tiles(), 0);

        // Zapis nieprzezroczystego piksela alokuje kafel
        let red = Rgba8::new(255, 0, 0, 255);
        assert!(layer.set_pixel(10, 10, red).is_ok());
        assert_eq!(layer.count_allocated_tiles(), 1);
        assert_eq!(layer.get_pixel(10, 10), Some(red));
        assert_eq!(layer.get_pixel(11, 10), Some(Rgba8::TRANSPARENT));

        // Zapis do innego kafla
        let blue = Rgba8::new(0, 0, 255, 255);
        assert!(layer.set_pixel(70, 70, blue).is_ok());
        assert_eq!(layer.count_allocated_tiles(), 2);
        assert_eq!(layer.get_pixel(70, 70), Some(blue));

        // Dostęp poza granicami
        assert_eq!(layer.get_pixel(100, 50), None);
        assert_eq!(layer.get_pixel(50, 100), None);
        let err = layer.set_pixel(100, 50, red).unwrap_err();
        assert_eq!(
            err,
            OutOfBoundsError {
                x: 100,
                y: 50,
                width: 100,
                height: 100
            }
        );
    }

    #[test]
    fn test_tile_at() {
        let mut layer = TiledLayer::new(128, 64);
        assert!(layer.tile_at(0, 0).is_none());
        assert!(layer.tile_at(5, 5).is_none());

        layer
            .set_pixel(5, 5, Rgba8::new(1, 2, 3, 255))
            .expect("poprawne wspolrzedne");
        assert!(layer.tile_at(0, 0).is_some());
        assert!(layer.tile_at(1, 0).is_none());
    }

    #[test]
    fn test_count_allocated_tiles_and_clear() {
        let mut layer = TiledLayer::new(128, 128);
        assert_eq!(layer.count_allocated_tiles(), 0);

        layer
            .set_pixel(0, 0, Rgba8::WHITE)
            .expect("poprawne wspolrzedne");
        layer
            .set_pixel(64, 0, Rgba8::WHITE)
            .expect("poprawne wspolrzedne");
        assert_eq!(layer.count_allocated_tiles(), 2);

        layer.clear();
        assert_eq!(layer.count_allocated_tiles(), 0);
        assert_eq!(layer.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_copy_on_write_behavior() {
        let mut original = TiledLayer::new(128, 64);
        original
            .set_pixel(10, 10, Rgba8::new(255, 0, 0, 255))
            .expect("poprawne wspolrzedne");

        // Klonujemy warstwę (symulacja migawki historii undo)
        let mut snapshot = original.clone();

        // Kafel powinien być współdzielony wskaźnikiem Arc
        let tile_orig = original.tile_at(0, 0).unwrap();
        let tile_snap = snapshot.tile_at(0, 0).unwrap();
        assert!(Arc::ptr_eq(tile_orig, tile_snap));

        // Modyfikacja w snapshot powinna wywołać Arc::make_mut (copy-on-write)
        let green = Rgba8::new(0, 255, 0, 255);
        snapshot
            .set_pixel(10, 10, green)
            .expect("poprawne wspolrzedne");

        // Wartości pikseli powinny być różne
        assert_eq!(original.get_pixel(10, 10), Some(Rgba8::new(255, 0, 0, 255)));
        assert_eq!(snapshot.get_pixel(10, 10), Some(green));

        // Wskaźniki Arc powinny wskazywać na różne alokacje po zapisie
        let tile_orig_after = original.tile_at(0, 0).unwrap();
        let tile_snap_after = snapshot.tile_at(0, 0).unwrap();
        assert!(!Arc::ptr_eq(tile_orig_after, tile_snap_after));
    }

    #[test]
    fn test_out_of_bounds_error_display() {
        let err = OutOfBoundsError {
            x: 10,
            y: 20,
            width: 5,
            height: 15,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("10"));
        assert!(msg.contains("20"));
        assert!(msg.contains("5"));
        assert!(msg.contains("15"));
    }

    #[test]
    fn test_layer_new() {
        let layer = Layer::new("Tło", 120, 80);
        assert_eq!(layer.name, "Tło");
        assert!(layer.visible);
        assert_eq!(layer.opacity, 1.0);
        assert_eq!(layer.blend, BlendMode::Normal);
        assert_eq!(layer.pixels.width, 120);
        assert_eq!(layer.pixels.height, 80);
    }

    #[test]
    fn test_layer_get_set_pixel() {
        let mut layer = Layer::new("Warstwa 1", 100, 100);
        let color = Rgba8::new(12, 34, 56, 255);
        assert_eq!(layer.get_pixel(10, 10), Some(Rgba8::TRANSPARENT));

        layer.set_pixel(10, 10, color).expect("prawidłowy piksel");
        assert_eq!(layer.get_pixel(10, 10), Some(color));

        assert!(layer.set_pixel(150, 150, color).is_err());
        assert_eq!(layer.get_pixel(150, 150), None);
    }

    #[test]
    fn test_layer_duplicate() {
        let mut layer = Layer::new("Oryginał", 100, 100);
        layer.opacity = 0.75;
        layer.visible = false;
        layer
            .set_pixel(10, 10, Rgba8::new(255, 128, 0, 255))
            .unwrap();

        let dup = layer.duplicate("Kopia");
        assert_eq!(dup.name, "Kopia");
        assert_eq!(dup.opacity, 0.75);
        assert!(!dup.visible);
        assert_eq!(dup.get_pixel(10, 10), Some(Rgba8::new(255, 128, 0, 255)));

        // Kafle powinny być współdzielone wskaźnikiem Arc
        let tile_orig = layer.pixels.tiles[0].as_ref().unwrap();
        let tile_dup = dup.pixels.tiles[0].as_ref().unwrap();
        assert!(Arc::ptr_eq(tile_orig, tile_dup));
    }
}
