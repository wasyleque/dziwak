//! Desaturacja (Kolory → Desaturacja w GIMP 2.10): zamiana kolorów na odcienie szarości w jednym z pięciu trybów.

use std::sync::{Arc, OnceLock};

use crate::adjust::lerp_pixel;
use crate::document::Rect;
use crate::layer::TiledLayer;
use crate::pixel::Rgba8;
use crate::selection::Selection;
use crate::tile::TILE_SIZE;

/// Tryb desaturacji — te same co w GIMP 2.10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DesaturateMode {
    /// Luminancja (domyślna w GIMP): Y = 0,2126 R + 0,7152 G + 0,0722 B liczone w świetle liniowym
    /// (sRGB → liniowe → Y → sRGB). Najwierniej oddaje odbieraną jasność.
    #[default]
    Luminance,
    /// Luma: te same współczynniki, ale na wartościach sRGB (z gammą), bez linearyzacji.
    Luma,
    /// Jasność (HSL): (max + min) / 2.
    Lightness,
    /// Średnia (HSI): (R + G + B) / 3.
    Average,
    /// Wartość (HSV): max(R, G, B).
    Value,
}

fn srgb_to_linear_table() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0.0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
        }
        t
    })
}

/// Szarość (0..=255) dla piksela w postaci prostej (nie premultiplied) `[r, g, b]`.
/// Wynik zaokrąglony do najbliższej liczby całkowitej.
pub fn gray_value(rgb: [u8; 3], mode: DesaturateMode) -> u8 {
    let r = rgb[0];
    let g = rgb[1];
    let b = rgb[2];
    match mode {
        DesaturateMode::Luminance => {
            let lut = srgb_to_linear_table();
            let lr = lut[r as usize];
            let lg = lut[g as usize];
            let lb = lut[b as usize];
            let y = 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;
            let ys = if y <= 0.0031308 {
                12.92 * y
            } else {
                1.055 * y.powf(1.0 / 2.4) - 0.055
            };
            (ys * 255.0).round().clamp(0.0, 255.0) as u8
        }
        DesaturateMode::Luma => {
            let v = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
            v.round().clamp(0.0, 255.0) as u8
        }
        DesaturateMode::Lightness => {
            let mx = r.max(g).max(b);
            let mn = r.min(g).min(b);
            (mx as u16 + mn as u16).div_ceil(2) as u8
        }
        DesaturateMode::Average => {
            let sum = r as u16 + g as u16 + b as u16;
            ((sum + 1) / 3) as u8
        }
        DesaturateMode::Value => r.max(g).max(b),
    }
}

/// Desaturuje warstwę (piksele premultiplied) w podanym trybie.
///
/// Zasady (jak `adjust::hue_saturation` w tym samym crate'cie):
/// - przy aktywnym zaznaczeniu przechodzi tylko po kaflach w `sel.bounds()`, a wynik miesza
///   z oryginałem przez `lerp_pixel(orig, gray, coverage)`; piksele z coverage 0 bez zmian;
/// - piksele w pełni przezroczyste i puste kafle (`None`) są pomijane; alfa pozostaje bez zmian
///   (desaturacja na wartościach prostych: `to_straight` → szarość → `from_straight`);
/// - `Arc::make_mut` tylko dla kafli, w których faktycznie coś się zmienia (copy-on-write historii);
/// - zwraca prostokąt obejmujący zmienione piksele albo `None`, gdy nic się nie zmieniło.
pub fn desaturate(
    layer: &mut TiledLayer,
    sel: Option<&Selection>,
    mode: DesaturateMode,
) -> Option<Rect> {
    if layer.width == 0 || layer.height == 0 {
        return None;
    }

    let tile_size = TILE_SIZE as u32;
    let across = layer.tiles_across();
    let down = layer.tiles_down();

    let (min_tx, max_tx, min_ty, max_ty) = match sel {
        Some(s) if s.has_selection => {
            let b = s.bounds()?;
            (
                b.x / tile_size,
                ((b.x + b.width - 1) / tile_size).min(across.saturating_sub(1)),
                b.y / tile_size,
                ((b.y + b.height - 1) / tile_size).min(down.saturating_sub(1)),
            )
        }
        _ => (0, across.saturating_sub(1), 0, down.saturating_sub(1)),
    };

    let mut min_x = u32::MAX;
    let mut max_x = 0;
    let mut min_y = u32::MAX;
    let mut max_y = 0;
    let mut any_changed = false;

    for ty in min_ty..=max_ty {
        let ty0 = ty * tile_size;
        let ty1 = (ty0 + tile_size).min(layer.height);

        for tx in min_tx..=max_tx {
            let tile_idx = (ty * across + tx) as usize;
            if layer.tiles[tile_idx].is_none() {
                continue;
            }

            let tx0 = tx * tile_size;
            let tx1 = (tx0 + tile_size).min(layer.width);

            let mut tile_needs_change = false;
            {
                let Some(tile) = layer.tiles[tile_idx].as_ref() else {
                    continue;
                };
                for py in ty0..ty1 {
                    let ly = (py - ty0) as usize;
                    let row = ly * TILE_SIZE;
                    for px in tx0..tx1 {
                        let lx = (px - tx0) as usize;
                        let orig = tile[row + lx];
                        if orig.is_transparent() {
                            continue;
                        }
                        let cov = sel.map_or(255, |s| s.coverage(px, py));
                        if cov == 0 {
                            continue;
                        }

                        let [r, g, b, a] = orig.to_straight();
                        let v = gray_value([r, g, b], mode);
                        let gray = Rgba8::from_straight(v, v, v, a);
                        let final_px = lerp_pixel(orig, gray, cov);

                        if final_px != orig {
                            tile_needs_change = true;
                            break;
                        }
                    }
                    if tile_needs_change {
                        break;
                    }
                }
            }

            if !tile_needs_change {
                continue;
            }

            let Some(tile) = layer.tiles[tile_idx].as_mut() else {
                continue;
            };
            let tile_data = Arc::make_mut(tile);

            for py in ty0..ty1 {
                let ly = (py - ty0) as usize;
                let row = ly * TILE_SIZE;
                for px in tx0..tx1 {
                    let lx = (px - tx0) as usize;
                    let orig = tile_data[row + lx];
                    if orig.is_transparent() {
                        continue;
                    }
                    let cov = sel.map_or(255, |s| s.coverage(px, py));
                    if cov == 0 {
                        continue;
                    }

                    let [r, g, b, a] = orig.to_straight();
                    let v = gray_value([r, g, b], mode);
                    let gray = Rgba8::from_straight(v, v, v, a);
                    let final_px = lerp_pixel(orig, gray, cov);

                    if final_px != orig {
                        tile_data[row + lx] = final_px;
                        min_x = min_x.min(px);
                        max_x = max_x.max(px);
                        min_y = min_y.min(py);
                        max_y = max_y.max(py);
                        any_changed = true;
                    }
                }
            }
        }
    }

    if any_changed {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray_input_stays_gray_in_every_mode() {
        for mode in [
            DesaturateMode::Luminance,
            DesaturateMode::Luma,
            DesaturateMode::Lightness,
            DesaturateMode::Average,
            DesaturateMode::Value,
        ] {
            for v in [0u8, 1, 64, 128, 200, 255] {
                assert_eq!(gray_value([v, v, v], mode), v, "{mode:?} {v}");
            }
        }
    }

    #[test]
    fn modes_match_gimp_formulas() {
        let c = [200u8, 100, 50];
        assert_eq!(gray_value(c, DesaturateMode::Value), 200);
        assert_eq!(gray_value(c, DesaturateMode::Lightness), 125);
        assert_eq!(gray_value(c, DesaturateMode::Average), 117);
        // 0,2126·200 + 0,7152·100 + 0,0722·50 = 117,6
        assert_eq!(gray_value(c, DesaturateMode::Luma), 118);
        // liniowo: 0,2126·0,5776 + 0,7152·0,1274 + 0,0722·0,0319 = 0,2162 → sRGB 128
        let y = gray_value(c, DesaturateMode::Luminance);
        assert!((128..=130).contains(&y), "luminance {y}");
        // czysta zieleń jest jasna, czysty niebieski ciemny
        assert_eq!(gray_value([0, 255, 0], DesaturateMode::Luminance), 220);
        assert_eq!(gray_value([0, 0, 255], DesaturateMode::Luminance), 76);
    }

    fn filled(w: u32, h: u32, c: Rgba8) -> TiledLayer {
        let mut l = TiledLayer::new(w, h);
        for y in 0..h {
            for x in 0..w {
                l.set_pixel(x, y, c).unwrap();
            }
        }
        l
    }

    #[test]
    fn desaturate_whole_layer_keeps_alpha() {
        let c = Rgba8::from_straight(200, 100, 50, 128);
        let mut l = filled(70, 10, c);
        let r = desaturate(&mut l, None, DesaturateMode::Average);
        assert_eq!(r, Some(Rect::new(0, 0, 70, 10)));
        let p = l.get_pixel(69, 9).unwrap();
        assert_eq!(p.a(), 128);
        assert_eq!(p.r(), p.g());
        assert_eq!(p.g(), p.b());
        let [sr, _, _, _] = p.to_straight();
        assert!((116..=118).contains(&sr), "{sr}");
        // druga desaturacja niczego już nie zmienia
        assert_eq!(desaturate(&mut l, None, DesaturateMode::Average), None);
    }

    #[test]
    fn desaturate_respects_selection_and_cow() {
        let c = Rgba8::new(255, 0, 0, 255);
        let mut l = filled(130, 10, c);
        let before = l.clone();
        let mut sel = Selection::new(130, 10);
        sel.select_rect(0, 0, 10, 10);
        let r = desaturate(&mut l, Some(&sel), DesaturateMode::Value);
        assert_eq!(r, Some(Rect::new(0, 0, 10, 10)));
        assert_eq!(l.get_pixel(5, 5).unwrap(), Rgba8::new(255, 255, 255, 255));
        assert_eq!(l.get_pixel(50, 5).unwrap(), c);
        // kafle poza zaznaczeniem nadal współdzielone z kopią sprzed zmiany
        assert!(Arc::ptr_eq(
            l.tiles[2].as_ref().unwrap(),
            before.tiles[2].as_ref().unwrap()
        ));
    }

    #[test]
    fn empty_tiles_and_transparent_pixels_untouched() {
        let mut l = TiledLayer::new(100, 100);
        assert_eq!(desaturate(&mut l, None, DesaturateMode::Luminance), None);
        assert!(l.tiles.iter().all(|t| t.is_none()));
    }
}
