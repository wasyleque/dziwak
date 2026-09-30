//! Binarny format dokumentu Dziwaka (bez kompresji).

use std::sync::Arc;

use crate::blend::BlendMode;
use crate::document::Document;
use crate::layer::Layer;
use crate::pixel::Rgba8;
use crate::tile::TILE_PIXELS;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FormatError {
    #[error("to nie jest plik Dziwaka")]
    BadMagic,
    #[error("nieobsługiwana wersja {0}")]
    UnsupportedVersion(u16),
    #[error("plik jest ucięty")]
    Truncated,
    #[error("niepoprawna nazwa warstwy")]
    InvalidUtf8,
    #[error("nieznany tryb mieszania {0}")]
    InvalidBlend(u8),
    #[error("błąd kompresji")]
    Compression,
    #[error("błąd pliku: {0}")]
    Io(String),
}

pub const MAGIC: &[u8; 4] = b"DZWK";
pub const VERSION: u16 = 1;

/// Koduje dokument do tablicy bajtów.
pub fn encode(doc: &Document) -> Vec<u8> {
    let mut result = Vec::with_capacity(1024); // Szacunkowa pojemność

    // Zapisz magic
    result.extend_from_slice(MAGIC);

    // Zapisz wersję
    result.extend_from_slice(&VERSION.to_le_bytes());

    // Zapisz rozmiar dokumentu
    result.extend_from_slice(&doc.width.to_le_bytes());
    result.extend_from_slice(&doc.height.to_le_bytes());

    // Zapisz liczbę warstw
    let layer_count = doc.layers.len() as u32;
    result.extend_from_slice(&layer_count.to_le_bytes());

    // Zapisz każdą warstwę
    for layer in &doc.layers {
        // Zapisz długość nazwy warstwy
        let name_bytes = layer.name.as_bytes();
        result.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());

        // Zapisz nazwę warstwy
        result.extend_from_slice(name_bytes);

        // Zapisz widoczność
        result.push(if layer.visible { 1 } else { 0 });

        // Zapisz krycie
        result.extend_from_slice(&layer.opacity.to_le_bytes());

        // Zapisz tryb mieszania
        let blend_idx = BlendMode::ALL
            .iter()
            .position(|&m| m == layer.blend)
            .map(|i| i as u8)
            .unwrap_or(0); // Domyślnie Normalny tryb
        result.push(blend_idx);

        // Zapisz kafle warstwy
        for tile in &layer.pixels.tiles {
            if let Some(tile_data) = tile {
                // Zapisz flagę 1 (kafelek istnieje)
                result.push(1);

                // Zapisz dane kafla
                for pixel in tile_data.iter() {
                    result.extend_from_slice(&pixel.0);
                }
            } else {
                // Zapisz flagę 0 (kafelek nie istnieje)
                result.push(0);
            }
        }
    }

    result
}

/// Czyta dokument z tablicy bajtów.
pub fn decode(bytes: &[u8]) -> Result<Document, FormatError> {
    let mut reader = Reader { buf: bytes, pos: 0 };

    // Sprawdź magic
    let magic = reader.take(4)?;
    if magic != MAGIC {
        return Err(FormatError::BadMagic);
    }

    // Odczytaj wersję
    let version = reader.read_u16()?;
    if version != VERSION {
        return Err(FormatError::UnsupportedVersion(version));
    }

    // Odczytaj rozmiar dokumentu
    let width = reader.read_u32()?;
    let height = reader.read_u32()?;

    // Odczytaj liczbę warstw
    let layer_count = reader.read_u32()? as usize;

    let mut doc = Document::new(width, height);

    // Odczytaj każdą warstwę
    for _ in 0..layer_count {
        // Odczytaj długość nazwy warstwy
        let name_len = reader.read_u32()? as usize;

        // Odczytaj nazwę warstwy
        let name_bytes = reader.take(name_len)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| FormatError::InvalidUtf8)?
            .to_owned();

        // Utwórz nową warstwę
        let mut layer = Layer::new(name, width, height);

        // Odczytaj widoczność
        let visible = reader.read_u8()? != 0;
        layer.visible = visible;

        // Odczytaj krycie
        layer.opacity = reader.read_f32()?;

        // Odczytaj tryb mieszania
        let blend_idx = reader.read_u8()?;
        let blend_mode = BlendMode::ALL
            .get(blend_idx as usize)
            .copied()
            .ok_or(FormatError::InvalidBlend(blend_idx))?;
        layer.blend = blend_mode;

        // Odczytaj kafle warstwy
        for i in 0..layer.pixels.tiles.len() {
            let exists = reader.read_u8()? != 0;

            if exists {
                let mut data = [Rgba8::TRANSPARENT; TILE_PIXELS];

                // Odczytaj dane kafla
                for pixel in data.iter_mut() {
                    let bytes = reader.take(4)?;
                    *pixel = Rgba8([bytes[0], bytes[1], bytes[2], bytes[3]]);
                }

                layer.pixels.tiles[i] = Some(Arc::new(data));
            } else {
                layer.pixels.tiles[i] = None;
            }
        }

        doc.layers.push(layer);
    }

    Ok(doc)
}

/// Zapisuje dokument do pliku .dziwak (encode + kompresja LZ4). Zapis atomowy: najpierw plik z rozszerzeniem .tmp, potem rename.
pub fn save_dziwak(doc: &Document, path: &std::path::Path) -> Result<(), FormatError> {
    let compressed = lz4_flex::compress_prepend_size(&encode(doc));
    let tmp = path.with_extension("dziwak.tmp");
    std::fs::write(&tmp, &compressed).map_err(|e| FormatError::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| FormatError::Io(e.to_string()))
}

/// Wczytuje dokument z pliku .dziwak.
pub fn load_dziwak(path: &std::path::Path) -> Result<Document, FormatError> {
    let data = std::fs::read(path).map_err(|e| FormatError::Io(e.to_string()))?;
    let raw = lz4_flex::decompress_size_prepended(&data).map_err(|_| FormatError::Compression)?;
    decode(&raw)
}

/// Prywatna struktura do odczytu danych binarnych.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Pobiera określoną liczbę bajtów z bufora.
    fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        if self.pos + n > self.buf.len() {
            return Err(FormatError::Truncated);
        }

        let result = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(result)
    }

    /// Odczytuje 1 bajt.
    fn read_u8(&mut self) -> Result<u8, FormatError> {
        let bytes = self.take(1)?;
        Ok(bytes[0])
    }

    /// Odczytuje 2 bajty jako u16 w little-endian.
    fn read_u16(&mut self) -> Result<u16, FormatError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Odczytuje 4 bajty jako u32 w little-endian.
    fn read_u32(&mut self) -> Result<u32, FormatError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Odczytuje 4 bajty jako f32 w little-endian.
    fn read_f32(&mut self) -> Result<f32, FormatError> {
        let bytes = self.take(4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_trip() {
        // Utwórz dokument do testowania
        let mut doc = Document::new(100, 70);

        // Dodaj pierwszą warstwę
        let mut layer1 = Layer::new("Dół", 100, 70);
        layer1.visible = true;
        layer1.opacity = 1.0;
        layer1.blend = BlendMode::Normal;
        doc.layers.push(layer1);

        // Dodaj drugą warstwę
        let mut layer2 = Layer::new("Góra ąę", 100, 70);
        layer2.visible = false;
        layer2.opacity = 0.5;
        layer2.blend = BlendMode::Multiply;

        // Ustaw kilka pikseli w różnych kaflach
        layer2
            .set_pixel(10, 10, Rgba8::new(255, 0, 0, 255))
            .unwrap();
        layer2
            .set_pixel(70, 65, Rgba8::new(0, 255, 0, 255))
            .unwrap();
        layer2
            .set_pixel(99, 69, Rgba8::new(0, 0, 255, 255))
            .unwrap();

        doc.layers.push(layer2);

        // Zakoduj dokument
        let encoded = encode(&doc);

        // Odkoduj dokument
        let decoded = decode(&encoded).unwrap();

        // Sprawdź podstawowe właściwości
        assert_eq!(decoded.width, 100);
        assert_eq!(decoded.height, 70);
        assert_eq!(decoded.layers.len(), 2);

        // Sprawdź pierwszą warstwę
        let layer1 = &decoded.layers[0];
        assert_eq!(layer1.name, "Dół");
        assert!(layer1.visible);
        assert_eq!(layer1.opacity, 1.0);
        assert_eq!(layer1.blend, BlendMode::Normal);

        // Sprawdź drugą warstwę
        let layer2 = &decoded.layers[1];
        assert_eq!(layer2.name, "Góra ąę");
        assert!(!layer2.visible);
        assert_eq!(layer2.opacity, 0.5);
        assert_eq!(layer2.blend, BlendMode::Multiply);

        // Sprawdź ustawione piksele
        assert_eq!(layer2.get_pixel(10, 10), Some(Rgba8::new(255, 0, 0, 255)));
        assert_eq!(layer2.get_pixel(70, 65), Some(Rgba8::new(0, 255, 0, 255)));
        assert_eq!(layer2.get_pixel(99, 69), Some(Rgba8::new(0, 0, 255, 255)));

        // Sprawdź, że puste kafle pozostają None
        let mut empty_tiles_count = 0;
        for tile in &layer2.pixels.tiles {
            if tile.is_none() {
                empty_tiles_count += 1;
            }
        }
        assert!(empty_tiles_count > 0); // Powinno być kilka pustych kaflów
    }

    #[test]
    fn test_bad_magic() {
        let bad_data = [b'N', b'O', b'T', b'D', 0, 1, 0, 0, 0, 0, 0, 0, 0, 0];
        let result = decode(&bad_data);
        assert_eq!(result, Err(FormatError::BadMagic));
    }

    #[test]
    fn test_truncated() {
        let truncated_data = [b'D', b'Z', b'W', b'K', 1, 0]; // Brakuje danych
        let result = decode(&truncated_data);
        assert_eq!(result, Err(FormatError::Truncated));
    }

    #[test]
    fn test_unsupported_version() {
        let data = [b'D', b'Z', b'W', b'K', 99, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let result = decode(&data);
        assert_eq!(result, Err(FormatError::UnsupportedVersion(99)));
    }

    #[test]
    fn test_save_load_dziwak() {
        // Utwórz dokument do testowania
        let mut doc = Document::new(200, 150);

        // Dodaj warstwę
        let mut layer = Layer::new("Test", 200, 150);
        layer.visible = true;
        layer.opacity = 0.8;
        layer.blend = BlendMode::Screen;

        // Ustaw kilka pikseli
        layer.set_pixel(10, 10, Rgba8::new(255, 0, 0, 255)).unwrap();
        layer.set_pixel(50, 50, Rgba8::new(0, 255, 0, 255)).unwrap();
        layer
            .set_pixel(100, 100, Rgba8::new(0, 0, 255, 255))
            .unwrap();

        doc.layers.push(layer);

        // Utwórz ścieżkę testową
        let path = std::env::temp_dir().join(format!("dziwak_test_{}.dziwak", std::process::id()));

        // Zapisz dokument
        save_dziwak(&doc, &path).unwrap();

        // Sprawdź, że plik .tmp nie istnieje
        let tmp_path = path.with_extension("dziwak.tmp");
        assert!(!tmp_path.exists());

        // Wczytaj dokument
        let loaded_doc = load_dziwak(&path).unwrap();

        // Porównaj wymiary
        assert_eq!(loaded_doc.width, 200);
        assert_eq!(loaded_doc.height, 150);
        assert_eq!(loaded_doc.layers.len(), 1);

        // Porównaj warstwę
        let loaded_layer = &loaded_doc.layers[0];
        assert_eq!(loaded_layer.name, "Test");
        assert!(loaded_layer.visible);
        assert_eq!(loaded_layer.opacity, 0.8);
        assert_eq!(loaded_layer.blend, BlendMode::Screen);

        // Porównaj piksele
        assert_eq!(
            loaded_layer.get_pixel(10, 10),
            Some(Rgba8::new(255, 0, 0, 255))
        );
        assert_eq!(
            loaded_layer.get_pixel(50, 50),
            Some(Rgba8::new(0, 255, 0, 255))
        );
        assert_eq!(
            loaded_layer.get_pixel(100, 100),
            Some(Rgba8::new(0, 0, 255, 255))
        );

        // Usuń plik testowy
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_load_garbage() {
        let path =
            std::env::temp_dir().join(format!("dziwak_garbage_{}.dziwak", std::process::id()));

        // Zapisz plik z nieprawidłowymi danymi
        std::fs::write(&path, b"xyz").unwrap();

        // Spróbuj wczytać dokument - powinno zwrócić błąd
        let result = load_dziwak(&path);
        assert!(result.is_err());

        // Usuń plik testowy
        std::fs::remove_file(&path).unwrap();
    }
}
