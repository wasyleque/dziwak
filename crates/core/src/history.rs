//! Historia operacji (undo/redo) oparta na migawkach kafli warstw.
//!
//! Dzięki strukturze `Tile = Arc<[Rgba8; 4096]>` klonowanie siatki kafli warstwy
//! jest tanie (kopiowane są jedynie wskaźniki `Arc` z atomowym licznikiem referencji).
//! Fizyczna pamięć jest alokowana wyłącznie dla kafli faktycznie zmodyfikowanych (Copy-on-Write).
//!
//! Pamięć historii jest liczona na podstawie kafli nie współdzielonych (`!Arc::ptr_eq`)
//! z poprzednią migawką lub aktualnym stanem dokumentu (16 KiB na każdy unikalny kafel).
//! W przypadku przekroczenia limitu `max_bytes` najstarsze migawki są automatycznie usuwane.

use std::sync::Arc;

use crate::blend::BlendMode;
use crate::document::Document;
use crate::layer::{Layer, TiledLayer};
use crate::tile::{Tile, TILE_SIZE};

/// Rozmiar jednego kafla w bajtach (64×64 piksele po 4 bajty RGBA8 = 16 KiB).
pub const TILE_BYTES: usize = TILE_SIZE * TILE_SIZE * 4;

/// Domyślny maksymalny limit pamięci historii (128 MiB).
pub const DEFAULT_MAX_HISTORY_BYTES: usize = 128 * 1024 * 1024;

/// Migawka pojedynczej warstwy dokumentu.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSnapshot {
    /// Nazwa warstwy.
    pub name: String,
    /// Widoczność warstwy.
    pub visible: bool,
    /// Krycie warstwy (0.0..=1.0).
    pub opacity: f32,
    /// Tryb mieszania warstwy.
    pub blend: BlendMode,
    /// Szerokość warstwy w pikselach.
    pub width: u32,
    /// Wysokość warstwy w pikselach.
    pub height: u32,
    /// Sklonowana siatka kafli (tani klon wskaźników `Arc`).
    pub tiles: Vec<Option<Tile>>,
}

impl LayerSnapshot {
    /// Tworzy migawkę z istniejącej warstwy.
    pub fn from_layer(layer: &Layer) -> Self {
        Self {
            name: layer.name.clone(),
            visible: layer.visible,
            opacity: layer.opacity,
            blend: layer.blend,
            width: layer.pixels.width,
            height: layer.pixels.height,
            tiles: layer.pixels.tiles.clone(),
        }
    }

    /// Odtwarza warstwę z migawki.
    pub fn to_layer(&self) -> Layer {
        Layer {
            name: self.name.clone(),
            visible: self.visible,
            opacity: self.opacity,
            blend: self.blend,
            pixels: TiledLayer {
                width: self.width,
                height: self.height,
                tiles: self.tiles.clone(),
            },
        }
    }
}

/// Migawka całego dokumentu (wymiary oraz stan wszystkich warstw).
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// Szerokość dokumentu w pikselach.
    pub width: u32,
    /// Wysokość dokumentu w pikselach.
    pub height: u32,
    /// Migawki warstw w dokumencie.
    pub layers: Vec<LayerSnapshot>,
}

impl Snapshot {
    /// Tworzy migawkę z aktualnego stanu dokumentu.
    pub fn from_document(doc: &Document) -> Self {
        Self {
            width: doc.width,
            height: doc.height,
            layers: doc.layers.iter().map(LayerSnapshot::from_layer).collect(),
        }
    }

    /// Odtwarza stan dokumentu z migawki.
    pub fn apply_to(&self, doc: &mut Document) {
        doc.width = self.width;
        doc.height = self.height;
        doc.layers = self.layers.iter().map(LayerSnapshot::to_layer).collect();
    }
}

/// Zwraca liczbę zaalokowanych kafli w `snapshot`, które NIE są współdzielone
/// z odpowiadającymi kaflami w `reference` (tzn. brak kafla w referencji lub `!Arc::ptr_eq`).
pub fn count_unshared_tiles(snapshot: &Snapshot, reference: &Snapshot) -> usize {
    let mut unshared = 0;
    for (layer_idx, layer) in snapshot.layers.iter().enumerate() {
        if let Some(ref_layer) = reference.layers.get(layer_idx) {
            let max_tiles = layer.tiles.len().max(ref_layer.tiles.len());
            for i in 0..max_tiles {
                let t_snap = layer.tiles.get(i).and_then(|opt| opt.as_ref());
                let t_ref = ref_layer.tiles.get(i).and_then(|opt| opt.as_ref());
                match (t_snap, t_ref) {
                    (Some(snap_tile), Some(ref_tile)) => {
                        if !Arc::ptr_eq(snap_tile, ref_tile) {
                            unshared += 1;
                        }
                    }
                    (Some(_), None) => {
                        unshared += 1;
                    }
                    (None, _) => {
                        // Kafel niezaalokowany w snapshot - nie zajmuje pamięci.
                    }
                }
            }
        } else {
            // Cała warstwa nie występuje w referencji - wszystkie jej zaalokowane kafle są unikalne.
            for tile in &layer.tiles {
                if tile.is_some() {
                    unshared += 1;
                }
            }
        }
    }
    unshared
}

/// Zwraca całkowitą liczbę zaalokowanych kafli w migawce.
pub fn count_allocated_tiles(snapshot: &Snapshot) -> usize {
    snapshot
        .layers
        .iter()
        .map(|l| l.tiles.iter().filter(|t| t.is_some()).count())
        .sum()
}

/// Historia operacji (undo / redo) z zarządzaniem pamięcią i limitem bajtów.
#[derive(Clone, Debug)]
pub struct History {
    /// Stos migawek do cofnięcia (najstarsze na początku, najnowsze na końcu).
    pub undo: Vec<Snapshot>,
    /// Stos migawek do ponowienia.
    pub redo: Vec<Snapshot>,
    /// Maksymalny limit pamięci w bajtach przeznaczony na historię.
    pub max_bytes: usize,
}

impl History {
    /// Tworzy nową pustą historię z określonym limitem pamięci w bajtach.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_bytes,
        }
    }

    /// Sprawdza, czy dostępna jest operacja cofnięcia (undo).
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Sprawdza, czy dostępna jest operacja ponowienia (redo).
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Czyści całą historię (zarówno undo, jak i redo).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Zapisuje bieżący stan dokumentu na stosie undo przed wykonaniem nowej modyfikacji.
    ///
    /// Każda nowa zmiana czyści stos redo oraz wymusza przestrzeganie limitu pamięci `max_bytes`
    /// (usuwając najstarsze migawki w razie potrzeby).
    pub fn push(&mut self, doc: &Document) {
        self.redo.clear();
        self.undo.push(Snapshot::from_document(doc));
        self.enforce_memory_limit(Some(doc));
    }

    /// Cofa ostatnią operację, przywracając dokument do poprzedniego stanu.
    ///
    /// Bieżący stan dokumentu trafia na stos redo.
    /// Zwraca `true`, jeśli cofnięcie powiodło się, lub `false`, jeśli stos undo był pusty.
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        if let Some(snapshot) = self.undo.pop() {
            self.redo.push(Snapshot::from_document(doc));
            snapshot.apply_to(doc);
            true
        } else {
            false
        }
    }

    /// Ponawia wcześniej cofniętą operację.
    ///
    /// Bieżący stan dokumentu trafia na stos undo.
    /// Zwraca `true`, jeśli ponowienie powiodło się, lub `false`, jeśli stos redo był pusty.
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        if let Some(snapshot) = self.redo.pop() {
            self.undo.push(Snapshot::from_document(doc));
            snapshot.apply_to(doc);
            self.enforce_memory_limit(Some(doc));
            true
        } else {
            false
        }
    }

    /// Oblicza szacowane zużycie pamięci (w bajtach) przez migawki w historii,
    /// licząc wyłącznie kafle niewspółdzielone (`!Arc::ptr_eq`) z poprzednią migawką
    /// lub aktualnym stanem dokumentu (16 KiB na kafel).
    pub fn memory_usage(&self, current_doc: Option<&Document>) -> usize {
        let current_snap = current_doc.map(Snapshot::from_document);
        let mut total_unshared_tiles = 0;

        // Migawki w stosie undo:
        for i in 0..self.undo.len() {
            if i > 0 {
                // Porównujemy z poprzednią migawką w sekwencji undo:
                total_unshared_tiles += count_unshared_tiles(&self.undo[i], &self.undo[i - 1]);
            } else if let Some(ref doc_snap) = current_snap {
                // Najstarszą migawkę (lub jedyną) porównujemy z aktualnym stanem:
                total_unshared_tiles += count_unshared_tiles(&self.undo[0], doc_snap);
            } else if self.undo.len() > 1 {
                // Brak referencji do doc, ale jest kolejna migawka:
                total_unshared_tiles += count_unshared_tiles(&self.undo[0], &self.undo[1]);
            } else {
                total_unshared_tiles += count_allocated_tiles(&self.undo[0]);
            }
        }

        // Migawki w stosie redo:
        for i in (0..self.redo.len()).rev() {
            if i + 1 < self.redo.len() {
                // Porównujemy z migawką bliżej aktualnego stanu dokumentu:
                total_unshared_tiles += count_unshared_tiles(&self.redo[i], &self.redo[i + 1]);
            } else if let Some(ref doc_snap) = current_snap {
                total_unshared_tiles += count_unshared_tiles(&self.redo[i], doc_snap);
            } else {
                total_unshared_tiles += count_allocated_tiles(&self.redo[i]);
            }
        }

        total_unshared_tiles * TILE_BYTES
    }

    /// Zwraca bieżące zużycie pamięci w bajtach bez przekazywania referencji do dokumentu.
    pub fn total_bytes(&self) -> usize {
        self.memory_usage(None)
    }

    /// Zwraca bieżące zużycie pamięci w bajtach z uwzględnieniem aktualnego stanu dokumentu.
    pub fn total_bytes_with_doc(&self, doc: &Document) -> usize {
        self.memory_usage(Some(doc))
    }

    /// Usuwa najstarsze migawki z historii, jeśli przekroczony jest limit `max_bytes`.
    fn enforce_memory_limit(&mut self, current_doc: Option<&Document>) {
        while self.memory_usage(current_doc) > self.max_bytes && !self.undo.is_empty() {
            self.undo.remove(0);
        }
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_HISTORY_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::Rgba8;

    #[test]
    fn test_undo_redo_round_trip() {
        let mut doc = Document::with_default_layer(64, 64, "Warstwa");
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([255, 0, 0, 255]))
            .unwrap();

        let mut history = History::new(1024 * 1024);

        // Zapisujemy stan przed zmianą
        history.push(&doc);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // Wprowadzamy zmianę (zielony piksel)
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([0, 255, 0, 255]))
            .unwrap();
        assert_eq!(doc.layers[0].get_pixel(0, 0), Some(Rgba8([0, 255, 0, 255])));

        // Cofamy operację -> powinien wrócić czerwony piksel
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[0].get_pixel(0, 0), Some(Rgba8([255, 0, 0, 255])));
        assert!(!history.can_undo());
        assert!(history.can_redo());

        // Ponawiamy operację -> zielony piksel
        assert!(history.redo(&mut doc));
        assert_eq!(doc.layers[0].get_pixel(0, 0), Some(Rgba8([0, 255, 0, 255])));
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_tile_sharing_ptr_eq() {
        // Dokument o wymiarach 128×64 (dokładnie 2 kafle obok siebie)
        let mut doc = Document::with_default_layer(128, 64, "Warstwa");
        // Alokujemy oba kafle poprzez zapis pikseli
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([10, 20, 30, 255]))
            .unwrap();
        doc.layers[0]
            .set_pixel(64, 0, Rgba8([40, 50, 60, 255]))
            .unwrap();

        let mut history = History::new(1024 * 1024);
        history.push(&doc);

        // Modyfikujemy tylko pierwszy kafel (x=0)
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([99, 99, 99, 255]))
            .unwrap();

        let snap_tile0 = history.undo[0].layers[0].tiles[0].as_ref().unwrap();
        let snap_tile1 = history.undo[0].layers[0].tiles[1].as_ref().unwrap();

        let doc_tile0 = doc.layers[0].pixels.tiles[0].as_ref().unwrap();
        let doc_tile1 = doc.layers[0].pixels.tiles[1].as_ref().unwrap();

        // Kafel 1 NIE był modyfikowany - wskaźniki Arc MUSZĄ być równe (współdzielenie pamięci)
        assert!(
            Arc::ptr_eq(snap_tile1, doc_tile1),
            "Niezmodyfikowany kafel powinien być współdzielony przez Arc::ptr_eq"
        );

        // Kafel 0 został zmodyfikowany (CoW make_mut utworzył kopię) - wskaźniki MUSZĄ być różne
        assert!(
            !Arc::ptr_eq(snap_tile0, doc_tile0),
            "Zmodyfikowany kafel nie powinien być Arc::ptr_eq"
        );
    }

    #[test]
    fn test_memory_limit_removes_oldest() {
        // Limit pamięci na maksymalnie 2 niepowtarzalne kafle (2 * 16 KiB = 32 KiB)
        let mut doc = Document::with_default_layer(64, 64, "Warstwa"); // 1 kafel
        let mut history = History::new(2 * TILE_BYTES);

        // Początkowy stan: piksel [1, 0, 0, 255]
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([1, 0, 0, 255]))
            .unwrap();

        // Przed zmianą 1 (na [2, 0, 0, 255]): push stanu [1, 0, 0, 255]
        history.push(&doc);
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([2, 0, 0, 255]))
            .unwrap();

        // Przed zmianą 2 (na [3, 0, 0, 255]): push stanu [2, 0, 0, 255]
        history.push(&doc);
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([3, 0, 0, 255]))
            .unwrap();
        assert_eq!(history.undo.len(), 2);

        // Przed zmianą 3 (na [4, 0, 0, 255]): push stanu [3, 0, 0, 255]
        history.push(&doc);
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([4, 0, 0, 255]))
            .unwrap();

        // Ze względu na limit 2 kafli (32 KiB), najstarsza migawka (stan [1, 0, 0, 255]) została usunięta
        assert_eq!(history.undo.len(), 2);
        assert!(history.total_bytes_with_doc(&doc) <= 2 * TILE_BYTES);

        // Cofamy -> powinna pojawić się zmiana ze stanem [3, 0, 0, 255]
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[0].get_pixel(0, 0), Some(Rgba8([3, 0, 0, 255])));

        // Cofamy ponownie -> powinna pojawić się zmiana ze stanem [2, 0, 0, 255]
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[0].get_pixel(0, 0), Some(Rgba8([2, 0, 0, 255])));

        // Dalsze cofnięcie niemożliwe, bo najstarsza migawka ze stanem 1 została usunięta
        assert!(!history.undo(&mut doc));
    }

    #[test]
    fn test_memory_calculation_shared_tiles() {
        // Dokument 128×128 (4 kafle po 16 KiB = 64 KiB cały obraz)
        let mut doc = Document::with_default_layer(128, 128, "Warstwa");
        // Wypełniamy wszystkie 4 kafle
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([1, 1, 1, 255]))
            .unwrap();
        doc.layers[0]
            .set_pixel(64, 0, Rgba8([2, 2, 2, 255]))
            .unwrap();
        doc.layers[0]
            .set_pixel(0, 64, Rgba8([3, 3, 3, 255]))
            .unwrap();
        doc.layers[0]
            .set_pixel(64, 64, Rgba8([4, 4, 4, 255]))
            .unwrap();

        let mut history = History::new(1024 * 1024);
        history.push(&doc);

        // Modyfikujemy tylko jeden piksel w jednym kaflu
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([9, 9, 9, 255]))
            .unwrap();

        // Obliczamy pamięć: tylko 1 kafel różni się od stanu doc, 3 pozostałe są współdzielone
        let usage = history.total_bytes_with_doc(&doc);
        assert_eq!(
            usage, TILE_BYTES,
            "Tylko 1 zmodyfikowany kafel (16 KiB) powinien być liczony w historii"
        );
    }

    #[test]
    fn test_new_action_clears_redo() {
        let mut doc = Document::with_default_layer(64, 64, "Warstwa");
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([10, 0, 0, 255]))
            .unwrap();

        let mut history = History::new(1024 * 1024);
        // Przed modyfikacją: push stanu 10
        history.push(&doc);

        doc.layers[0]
            .set_pixel(0, 0, Rgba8([20, 0, 0, 255]))
            .unwrap();

        // Cofamy -> wraca 10, redo ma stan 20
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[0].get_pixel(0, 0), Some(Rgba8([10, 0, 0, 255])));
        assert!(history.can_redo());
        assert_eq!(history.redo.len(), 1);

        // Nowa zmiana przed zapisem do doc: push stanu 10 czyści redo
        history.push(&doc);
        doc.layers[0]
            .set_pixel(0, 0, Rgba8([30, 0, 0, 255]))
            .unwrap();

        assert!(!history.can_redo());
        assert_eq!(history.redo.len(), 0);
    }

    #[test]
    fn test_undo_redo_empty() {
        let mut doc = Document::with_default_layer(64, 64, "Warstwa");
        let mut history = History::new(1024 * 1024);

        assert!(!history.undo(&mut doc));
        assert!(!history.redo(&mut doc));
    }

    #[test]
    fn test_layer_metadata_restored() {
        let mut doc = Document::with_default_layer(64, 64, "Tło");
        doc.layers[0].opacity = 0.5;
        doc.layers[0].visible = true;
        doc.layers[0].blend = BlendMode::Normal;

        let mut history = History::new(1024 * 1024);
        history.push(&doc);

        // Zmieniamy metadane warstwy
        doc.layers[0].name = "Zmieniona nazwa".to_string();
        doc.layers[0].opacity = 0.8;
        doc.layers[0].visible = false;

        // Cofamy -> metadane powinny wrócić do pierwotnych wartości
        assert!(history.undo(&mut doc));
        assert_eq!(doc.layers[0].name, "Tło");
        assert!((doc.layers[0].opacity - 0.5).abs() < 1e-5);
        assert!(doc.layers[0].visible);
    }
}
