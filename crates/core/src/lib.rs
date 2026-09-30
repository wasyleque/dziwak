//! Rdzeń Dziwaka: dokument, warstwy, kafle, narzędzia, historia. Bez zależności od UI.

pub mod adjust;
pub mod blend;
pub mod blur;
pub mod brush;
pub mod color;
pub mod document;
pub mod eyedropper;
pub mod fill;
pub mod filters;
pub mod format;
pub mod gaussian;
pub mod gradient;
pub mod history;
pub mod hsl;
pub mod io;
pub mod layer;
pub mod pixel;
pub mod polygon;
pub mod ruler;
pub mod selection;
pub mod stroke;
pub mod text;
pub mod thumbnail;
pub mod tile;
pub mod transform;

pub use adjust::{brightness_contrast, gaussian_blur, hue_saturation, unsharp_mask};
pub use blend::{apply_opacity, apply_opacity_u8, blend_normal, div255_round, BlendMode};
pub use brush::{apply_dab, blur_sharpen_dab, brush_falloff, clone_dab, dodge_burn_dab, BrushMode};
pub use document::{CompositionError, Document, Rect};
pub use history::{
    count_allocated_tiles, count_unshared_tiles, History, LayerSnapshot, Snapshot,
    DEFAULT_MAX_HISTORY_BYTES, TILE_BYTES,
};
pub use io::{format_from_path, load_image, save_image, ImageError};
pub use layer::{Layer, OutOfBoundsError, TiledLayer};
pub use pixel::Rgba8;
pub use selection::Selection;
pub use stroke::interpolate_stroke;
pub use text::{rasterize_text, stamp_mask, TextMask};
pub use tile::{
    create_empty_tile, create_filled_tile, is_tile_empty, Tile, TILE_PIXELS, TILE_SIZE,
};
pub use transform::{
    flip_horizontal, flip_vertical, resample_bilinear, rotate180, rotate90_ccw, rotate90_cw,
    rotate_bilinear, rotate_layer_centered, scale_layer_centered,
};
