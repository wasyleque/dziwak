//! Kafel o rozmiarze 64×64 piksele, przechowywany jako `Arc<[Rgba8; 4096]>`.

use std::sync::Arc;

use crate::pixel::Rgba8;

/// Rozmiar boku kafla w pikselach (64 px).
pub const TILE_SIZE: usize = 64;

/// Łączna liczba pikseli w kaflu (64 × 64 = 4096).
pub const TILE_PIXELS: usize = TILE_SIZE * TILE_SIZE;

/// Kafel obrazu: niepodzielny bufor 64×64 pikseli współdzielony wskaźnikiem Arc.
/// Modyfikacja odbywa się przez copy-on-write (`Arc::make_mut`).
pub type Tile = Arc<[Rgba8; TILE_PIXELS]>;

/// Tworzy nowy kafel wypełniony przezroczystością.
pub fn create_empty_tile() -> Tile {
    create_filled_tile(Rgba8::TRANSPARENT)
}

/// Tworzy nowy kafel wypełniony zadanym kolorem.
pub fn create_filled_tile(color: Rgba8) -> Tile {
    Arc::new([color; TILE_PIXELS])
}

/// Sprawdza, czy wszystkie piksele w kaflu są całkowicie przezroczyste.
pub fn is_tile_empty(tile: &[Rgba8; TILE_PIXELS]) -> bool {
    tile.iter().all(|px| px.is_transparent())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_empty_tile() {
        let tile = create_empty_tile();
        assert_eq!(tile.len(), TILE_PIXELS);
        assert!(is_tile_empty(&tile));
        assert_eq!(tile[0], Rgba8::TRANSPARENT);
        assert_eq!(tile[TILE_PIXELS - 1], Rgba8::TRANSPARENT);
    }

    #[test]
    fn test_create_filled_tile() {
        let color = Rgba8::new(255, 0, 0, 255);
        let tile = create_filled_tile(color);
        assert_eq!(tile.len(), TILE_PIXELS);
        assert!(!is_tile_empty(&tile));
        assert_eq!(tile[0], color);
        assert_eq!(tile[TILE_PIXELS - 1], color);
    }

    #[test]
    fn test_is_tile_empty() {
        let empty = create_empty_tile();
        assert!(is_tile_empty(&empty));

        let mut non_empty_tile = create_empty_tile();
        let data = Arc::make_mut(&mut non_empty_tile);
        data[123] = Rgba8::new(0, 0, 0, 1);
        assert!(!is_tile_empty(&non_empty_tile));
    }
}
