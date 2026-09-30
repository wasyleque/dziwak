//! Narzędzia korekcji i filtrów warstwy (jasność/kontrast, odcień/nasycenie, rozmycie Gaussa, wyostrzanie).
//!
//! Funkcje operują na siatce kafli `TiledLayer` z uwzględnieniem opcjonalnego zaznaczenia `Selection`.
//! Wyniki są mieszane z oryginałem proporcjonalnie do stopnia pokrycia (`coverage` w skali 0..=255).
//!
//! Puste kafle są pomijane tam gdzie to możliwe (poza rozmyciem), a operacje w pętlach pikseli
//! nie dokonują alokacji pamięci na stercie.

use std::sync::Arc;

use crate::blend::div255_round;
use crate::document::Rect;
use crate::layer::{Layer, TiledLayer};
use crate::pixel::Rgba8;
use crate::selection::Selection;
use crate::tile::{create_empty_tile, is_tile_empty, TILE_SIZE};

/// Miesza dwa piksele `p0` i `p1` według współczynnika `t` (0..=255):
/// dla `t = 0` zwraca `p0`, dla `t = 255` zwraca `p1`.
#[inline]
pub fn lerp_pixel(p0: Rgba8, p1: Rgba8, t: u8) -> Rgba8 {
    if t == 0 {
        return p0;
    }
    if t == 255 {
        return p1;
    }
    let t = t as u32;
    let inv = 255 - t;
    let r = div255_round(p0.r() as u32 * inv + p1.r() as u32 * t) as u8;
    let g = div255_round(p0.g() as u32 * inv + p1.g() as u32 * t) as u8;
    let b = div255_round(p0.b() as u32 * inv + p1.b() as u32 * t) as u8;
    let a = div255_round(p0.a() as u32 * inv + p1.a() as u32 * t) as u8;
    Rgba8::new(r, g, b, a)
}

/// Koryguje jasność i kontrast warstwy za pomocą tablicy LUT.
///
/// Dla każdego piksela kolor jest odpremultiplikowany do przestrzeni straight RGB,
/// modyfikowany przez LUT z `filters::brightness_contrast_lut`, a następnie
/// ponownie premultiplikowany i mieszany z pikselem początkowym wg `coverage`.
///
/// Zwraca prostokąt zmian `Some(Rect)` lub `None`, jeśli nie wprowadzono żadnych zmian.
pub fn brightness_contrast(
    layer: &mut TiledLayer,
    sel: Option<&Selection>,
    brightness: i16,
    contrast: f32,
) -> Option<Rect> {
    if layer.width == 0 || layer.height == 0 {
        return None;
    }
    if brightness == 0 && (contrast - 1.0).abs() < 1e-6 {
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

    let lut = crate::filters::brightness_contrast_lut(brightness, contrast);

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

            // Sprawdzamy, czy w tym kaflu jakikolwiek piksel ulegnie zmianie
            let mut tile_needs_change = false;
            {
                let tile = layer.tiles[tile_idx].as_ref().unwrap();
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
                        let new_r = lut[r as usize];
                        let new_g = lut[g as usize];
                        let new_b = lut[b as usize];
                        let adjusted = Rgba8::from_straight(new_r, new_g, new_b, a);
                        let final_px = lerp_pixel(orig, adjusted, cov);

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

            let tile = layer.tiles[tile_idx].as_mut().unwrap();
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
                    let new_r = lut[r as usize];
                    let new_g = lut[g as usize];
                    let new_b = lut[b as usize];
                    let adjusted = Rgba8::from_straight(new_r, new_g, new_b, a);
                    let final_px = lerp_pixel(orig, adjusted, cov);

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

/// Koryguje odcień, nasycenie i jasność warstwy za pomocą przestrzeni barw HSL.
///
/// * `hue_shift_deg`: przesunięcie odcienia w stopniach (np. -180.0..=180.0).
/// * `sat_mul`: mnożnik nasycenia (np. 0.0 = skala szarości, 1.0 = bez zmian, >1.0 = nasycenie).
/// * `light_delta`: zmiana jasności (-1.0..=1.0).
pub fn hue_saturation(
    layer: &mut TiledLayer,
    sel: Option<&Selection>,
    hue_shift_deg: f32,
    sat_mul: f32,
    light_delta: f32,
) -> Option<Rect> {
    if layer.width == 0 || layer.height == 0 {
        return None;
    }
    if hue_shift_deg.rem_euclid(360.0).abs() < 1e-4
        && (sat_mul - 1.0).abs() < 1e-4
        && light_delta.abs() < 1e-4
    {
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
                let tile = layer.tiles[tile_idx].as_ref().unwrap();
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
                        let hsl = crate::hsl::rgb_to_hsl([r, g, b]);
                        let new_hsl = [
                            (hsl[0] + hue_shift_deg).rem_euclid(360.0),
                            (hsl[1] * sat_mul).clamp(0.0, 1.0),
                            (hsl[2] + light_delta).clamp(0.0, 1.0),
                        ];
                        let [new_r, new_g, new_b] = crate::hsl::hsl_to_rgb(new_hsl);
                        let adjusted = Rgba8::from_straight(new_r, new_g, new_b, a);
                        let final_px = lerp_pixel(orig, adjusted, cov);

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

            let tile = layer.tiles[tile_idx].as_mut().unwrap();
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
                    let hsl = crate::hsl::rgb_to_hsl([r, g, b]);
                    let new_hsl = [
                        (hsl[0] + hue_shift_deg).rem_euclid(360.0),
                        (hsl[1] * sat_mul).clamp(0.0, 1.0),
                        (hsl[2] + light_delta).clamp(0.0, 1.0),
                    ];
                    let [new_r, new_g, new_b] = crate::hsl::hsl_to_rgb(new_hsl);
                    let adjusted = Rgba8::from_straight(new_r, new_g, new_b, a);
                    let final_px = lerp_pixel(orig, adjusted, cov);

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

/// Rozmywa warstwę za pomocą filtru Gaussa o promieniu `radius`.
///
/// Wykorzystuje separowalne jądro 1D z `gaussian::gaussian_kernel_1d` oraz `blur::blur_pass`
/// w dwóch przejściach (poziomym i pionowym) na buforze obejmującym bounding box zaznaczenia
/// (lub całą warstwę) z marginesem `radius`.
pub fn gaussian_blur(
    layer: &mut TiledLayer,
    sel: Option<&Selection>,
    radius: usize,
) -> Option<Rect> {
    if layer.width == 0 || layer.height == 0 || radius == 0 || layer.count_allocated_tiles() == 0 {
        return None;
    }

    // Wyznaczamy obszar docelowy zmian (target_rect)
    let target_rect = match sel {
        Some(s) if s.has_selection => s.bounds()?,
        _ => Rect::new(0, 0, layer.width, layer.height),
    };

    let rad_u32 = radius as u32;
    let buf_x0 = target_rect.x.saturating_sub(rad_u32);
    let buf_y0 = target_rect.y.saturating_sub(rad_u32);
    let buf_x1 = (target_rect.x + target_rect.width + rad_u32).min(layer.width);
    let buf_y1 = (target_rect.y + target_rect.height + rad_u32).min(layer.height);

    if buf_x0 >= buf_x1 || buf_y0 >= buf_y1 {
        return None;
    }

    let buf_w = (buf_x1 - buf_x0) as usize;
    let buf_h = (buf_y1 - buf_y0) as usize;

    let mut src_buf = vec![Rgba8::TRANSPARENT; buf_w * buf_h];
    let mut tmp_buf = vec![Rgba8::TRANSPARENT; buf_w * buf_h];

    // Pobieramy piksele z warstwy do bufora źródłowego
    let tile_size = TILE_SIZE as u32;
    let min_tx = buf_x0 / tile_size;
    let max_tx = (buf_x1 - 1) / tile_size;
    let min_ty = buf_y0 / tile_size;
    let max_ty = (buf_y1 - 1) / tile_size;
    let across = layer.tiles_across();

    for ty in min_ty..=max_ty {
        let ty0 = ty * tile_size;
        let ty1 = (ty0 + tile_size).min(layer.height);
        let clip_y0 = ty0.max(buf_y0);
        let clip_y1 = ty1.min(buf_y1);
        if clip_y0 >= clip_y1 {
            continue;
        }

        for tx in min_tx..=max_tx {
            let tx0 = tx * tile_size;
            let tx1 = (tx0 + tile_size).min(layer.width);
            let clip_x0 = tx0.max(buf_x0);
            let clip_x1 = tx1.min(buf_x1);
            if clip_x0 >= clip_x1 {
                continue;
            }

            let tile_idx = (ty * across + tx) as usize;
            if let Some(tile) = &layer.tiles[tile_idx] {
                for py in clip_y0..clip_y1 {
                    let tile_row = (py - ty0) as usize * TILE_SIZE;
                    let buf_row = (py - buf_y0) as usize * buf_w;
                    let lx0 = (clip_x0 - tx0) as usize;
                    let lx1 = (clip_x1 - tx0) as usize;
                    let bx0 = (clip_x0 - buf_x0) as usize;
                    let count = lx1 - lx0;
                    src_buf[buf_row + bx0..buf_row + bx0 + count]
                        .copy_from_slice(&tile[tile_row + lx0..tile_row + lx1]);
                }
            }
        }
    }

    let kernel = crate::gaussian::gaussian_kernel_1d(radius, 0.0);
    crate::blur::blur_pass(&src_buf, &mut tmp_buf, buf_w, buf_h, &kernel, true);
    crate::blur::blur_pass(&tmp_buf, &mut src_buf, buf_w, buf_h, &kernel, false);

    // Zapisujemy rozmyty bufor z powrotem na warstwę w obszarze target_rect
    let target_min_tx = target_rect.x / tile_size;
    let target_max_tx =
        ((target_rect.x + target_rect.width - 1) / tile_size).min(across.saturating_sub(1));
    let target_min_ty = target_rect.y / tile_size;
    let target_max_ty = ((target_rect.y + target_rect.height - 1) / tile_size)
        .min(layer.tiles_down().saturating_sub(1));

    let mut min_x = u32::MAX;
    let mut max_x = 0;
    let mut min_y = u32::MAX;
    let mut max_y = 0;
    let mut any_changed = false;

    for ty in target_min_ty..=target_max_ty {
        let ty0 = ty * tile_size;
        let ty1 = (ty0 + tile_size).min(layer.height);
        let clip_y0 = ty0.max(target_rect.y);
        let clip_y1 = ty1.min(target_rect.y + target_rect.height);
        if clip_y0 >= clip_y1 {
            continue;
        }

        for tx in target_min_tx..=target_max_tx {
            let tx0 = tx * tile_size;
            let tx1 = (tx0 + tile_size).min(layer.width);
            let clip_x0 = tx0.max(target_rect.x);
            let clip_x1 = tx1.min(target_rect.x + target_rect.width);
            if clip_x0 >= clip_x1 {
                continue;
            }

            let tile_idx = (ty * across + tx) as usize;

            // Sprawdzamy, czy w tym kaflu jakikolwiek piksel ulegnie zmianie
            let mut tile_needs_change = false;
            for py in clip_y0..clip_y1 {
                let buf_row = (py - buf_y0) as usize * buf_w;
                for px in clip_x0..clip_x1 {
                    let cov = sel.map_or(255, |s| s.coverage(px, py));
                    if cov == 0 {
                        continue;
                    }
                    let orig = match &layer.tiles[tile_idx] {
                        Some(t) => {
                            let ly = (py - ty0) as usize;
                            let lx = (px - tx0) as usize;
                            t[ly * TILE_SIZE + lx]
                        }
                        None => Rgba8::TRANSPARENT,
                    };
                    let blurred = src_buf[buf_row + (px - buf_x0) as usize];
                    let final_px = lerp_pixel(orig, blurred, cov);
                    if final_px != orig {
                        tile_needs_change = true;
                        break;
                    }
                }
                if tile_needs_change {
                    break;
                }
            }

            if !tile_needs_change {
                continue;
            }

            if layer.tiles[tile_idx].is_none() {
                layer.tiles[tile_idx] = Some(create_empty_tile());
            }
            let tile = layer.tiles[tile_idx].as_mut().unwrap();
            let tile_data = Arc::make_mut(tile);

            for py in clip_y0..clip_y1 {
                let ly = (py - ty0) as usize;
                let row = ly * TILE_SIZE;
                let buf_row = (py - buf_y0) as usize * buf_w;
                for px in clip_x0..clip_x1 {
                    let cov = sel.map_or(255, |s| s.coverage(px, py));
                    if cov == 0 {
                        continue;
                    }
                    let lx = (px - tx0) as usize;
                    let orig = tile_data[row + lx];
                    let blurred = src_buf[buf_row + (px - buf_x0) as usize];
                    let final_px = lerp_pixel(orig, blurred, cov);

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

            if is_tile_empty(tile_data) {
                layer.tiles[tile_idx] = None;
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

/// Wyostrza warstwę za pomocą maski wyostrzającej (Unsharp Mask).
///
/// * `radius`: promień rozmycia Gaussa maski nieostrej.
/// * `amount`: siła wyostrzenia (np. 0.5..=2.0).
pub fn unsharp_mask(
    layer: &mut TiledLayer,
    sel: Option<&Selection>,
    radius: usize,
    amount: f32,
) -> Option<Rect> {
    if layer.width == 0
        || layer.height == 0
        || radius == 0
        || amount <= 0.0
        || layer.count_allocated_tiles() == 0
    {
        return None;
    }

    let target_rect = match sel {
        Some(s) if s.has_selection => s.bounds()?,
        _ => Rect::new(0, 0, layer.width, layer.height),
    };

    let rad_u32 = radius as u32;
    let buf_x0 = target_rect.x.saturating_sub(rad_u32);
    let buf_y0 = target_rect.y.saturating_sub(rad_u32);
    let buf_x1 = (target_rect.x + target_rect.width + rad_u32).min(layer.width);
    let buf_y1 = (target_rect.y + target_rect.height + rad_u32).min(layer.height);

    if buf_x0 >= buf_x1 || buf_y0 >= buf_y1 {
        return None;
    }

    let buf_w = (buf_x1 - buf_x0) as usize;
    let buf_h = (buf_y1 - buf_y0) as usize;

    let mut orig_buf = vec![Rgba8::TRANSPARENT; buf_w * buf_h];
    let mut tmp_buf = vec![Rgba8::TRANSPARENT; buf_w * buf_h];

    let tile_size = TILE_SIZE as u32;
    let min_tx = buf_x0 / tile_size;
    let max_tx = (buf_x1 - 1) / tile_size;
    let min_ty = buf_y0 / tile_size;
    let max_ty = (buf_y1 - 1) / tile_size;
    let across = layer.tiles_across();

    for ty in min_ty..=max_ty {
        let ty0 = ty * tile_size;
        let ty1 = (ty0 + tile_size).min(layer.height);
        let clip_y0 = ty0.max(buf_y0);
        let clip_y1 = ty1.min(buf_y1);
        if clip_y0 >= clip_y1 {
            continue;
        }

        for tx in min_tx..=max_tx {
            let tx0 = tx * tile_size;
            let tx1 = (tx0 + tile_size).min(layer.width);
            let clip_x0 = tx0.max(buf_x0);
            let clip_x1 = tx1.min(buf_x1);
            if clip_x0 >= clip_x1 {
                continue;
            }

            let tile_idx = (ty * across + tx) as usize;
            if let Some(tile) = &layer.tiles[tile_idx] {
                for py in clip_y0..clip_y1 {
                    let tile_row = (py - ty0) as usize * TILE_SIZE;
                    let buf_row = (py - buf_y0) as usize * buf_w;
                    let lx0 = (clip_x0 - tx0) as usize;
                    let lx1 = (clip_x1 - tx0) as usize;
                    let bx0 = (clip_x0 - buf_x0) as usize;
                    let count = lx1 - lx0;
                    orig_buf[buf_row + bx0..buf_row + bx0 + count]
                        .copy_from_slice(&tile[tile_row + lx0..tile_row + lx1]);
                }
            }
        }
    }

    let kernel = crate::gaussian::gaussian_kernel_1d(radius, 0.0);
    // Rozmycie Gaussa do bufora pomocniczego
    crate::blur::blur_pass(&orig_buf, &mut tmp_buf, buf_w, buf_h, &kernel, true);
    let mut blurred_buf = vec![Rgba8::TRANSPARENT; buf_w * buf_h];
    crate::blur::blur_pass(&tmp_buf, &mut blurred_buf, buf_w, buf_h, &kernel, false);

    let target_min_tx = target_rect.x / tile_size;
    let target_max_tx =
        ((target_rect.x + target_rect.width - 1) / tile_size).min(across.saturating_sub(1));
    let target_min_ty = target_rect.y / tile_size;
    let target_max_ty = ((target_rect.y + target_rect.height - 1) / tile_size)
        .min(layer.tiles_down().saturating_sub(1));

    let mut min_x = u32::MAX;
    let mut max_x = 0;
    let mut min_y = u32::MAX;
    let mut max_y = 0;
    let mut any_changed = false;

    for ty in target_min_ty..=target_max_ty {
        let ty0 = ty * tile_size;
        let ty1 = (ty0 + tile_size).min(layer.height);
        let clip_y0 = ty0.max(target_rect.y);
        let clip_y1 = ty1.min(target_rect.y + target_rect.height);
        if clip_y0 >= clip_y1 {
            continue;
        }

        for tx in target_min_tx..=target_max_tx {
            let tx0 = tx * tile_size;
            let tx1 = (tx0 + tile_size).min(layer.width);
            let clip_x0 = tx0.max(target_rect.x);
            let clip_x1 = tx1.min(target_rect.x + target_rect.width);
            if clip_x0 >= clip_x1 {
                continue;
            }

            let tile_idx = (ty * across + tx) as usize;

            let mut tile_needs_change = false;
            for py in clip_y0..clip_y1 {
                let buf_row = (py - buf_y0) as usize * buf_w;
                for px in clip_x0..clip_x1 {
                    let cov = sel.map_or(255, |s| s.coverage(px, py));
                    if cov == 0 {
                        continue;
                    }
                    let buf_idx = buf_row + (px - buf_x0) as usize;
                    let orig = orig_buf[buf_idx];
                    if orig.is_transparent() {
                        continue;
                    }

                    let blurred = blurred_buf[buf_idx];
                    let diff_r = orig.r() as f32 - blurred.r() as f32;
                    let diff_g = orig.g() as f32 - blurred.g() as f32;
                    let diff_b = orig.b() as f32 - blurred.b() as f32;

                    let sr = (orig.r() as f32 + amount * diff_r)
                        .round()
                        .clamp(0.0, orig.a() as f32) as u8;
                    let sg = (orig.g() as f32 + amount * diff_g)
                        .round()
                        .clamp(0.0, orig.a() as f32) as u8;
                    let sb = (orig.b() as f32 + amount * diff_b)
                        .round()
                        .clamp(0.0, orig.a() as f32) as u8;
                    let sharpened = Rgba8::new(sr, sg, sb, orig.a());
                    let final_px = lerp_pixel(orig, sharpened, cov);

                    if final_px != orig {
                        tile_needs_change = true;
                        break;
                    }
                }
                if tile_needs_change {
                    break;
                }
            }

            if !tile_needs_change {
                continue;
            }

            if layer.tiles[tile_idx].is_none() {
                layer.tiles[tile_idx] = Some(create_empty_tile());
            }
            let tile = layer.tiles[tile_idx].as_mut().unwrap();
            let tile_data = Arc::make_mut(tile);

            for py in clip_y0..clip_y1 {
                let ly = (py - ty0) as usize;
                let row = ly * TILE_SIZE;
                let buf_row = (py - buf_y0) as usize * buf_w;
                for px in clip_x0..clip_x1 {
                    let cov = sel.map_or(255, |s| s.coverage(px, py));
                    if cov == 0 {
                        continue;
                    }
                    let lx = (px - tx0) as usize;
                    let orig = tile_data[row + lx];
                    if orig.is_transparent() {
                        continue;
                    }

                    let buf_idx = buf_row + (px - buf_x0) as usize;
                    let blurred = blurred_buf[buf_idx];
                    let diff_r = orig.r() as f32 - blurred.r() as f32;
                    let diff_g = orig.g() as f32 - blurred.g() as f32;
                    let diff_b = orig.b() as f32 - blurred.b() as f32;

                    let sr = (orig.r() as f32 + amount * diff_r)
                        .round()
                        .clamp(0.0, orig.a() as f32) as u8;
                    let sg = (orig.g() as f32 + amount * diff_g)
                        .round()
                        .clamp(0.0, orig.a() as f32) as u8;
                    let sb = (orig.b() as f32 + amount * diff_b)
                        .round()
                        .clamp(0.0, orig.a() as f32) as u8;
                    let sharpened = Rgba8::new(sr, sg, sb, orig.a());
                    let final_px = lerp_pixel(orig, sharpened, cov);

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

            if is_tile_empty(tile_data) {
                layer.tiles[tile_idx] = None;
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

/// Wariant `brightness_contrast` przyjmujący referencję do całej struktury `Layer`.
pub fn brightness_contrast_layer(
    layer: &mut Layer,
    sel: Option<&Selection>,
    brightness: i16,
    contrast: f32,
) -> Option<Rect> {
    brightness_contrast(&mut layer.pixels, sel, brightness, contrast)
}

/// Wariant `hue_saturation` przyjmujący referencję do całej struktury `Layer`.
pub fn hue_saturation_layer(
    layer: &mut Layer,
    sel: Option<&Selection>,
    hue_shift_deg: f32,
    sat_mul: f32,
    light_delta: f32,
) -> Option<Rect> {
    hue_saturation(&mut layer.pixels, sel, hue_shift_deg, sat_mul, light_delta)
}

/// Wariant `gaussian_blur` przyjmujący referencję do całej struktury `Layer`.
pub fn gaussian_blur_layer(
    layer: &mut Layer,
    sel: Option<&Selection>,
    radius: usize,
) -> Option<Rect> {
    gaussian_blur(&mut layer.pixels, sel, radius)
}

/// Wariant `unsharp_mask` przyjmujący referencję do całej struktury `Layer`.
pub fn unsharp_mask_layer(
    layer: &mut Layer,
    sel: Option<&Selection>,
    radius: usize,
    amount: f32,
) -> Option<Rect> {
    unsharp_mask(&mut layer.pixels, sel, radius, amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brightness_contrast_identity() {
        let mut layer = TiledLayer::new(100, 100);
        layer
            .set_pixel(10, 10, Rgba8::new(100, 150, 200, 255))
            .unwrap();
        let orig = layer.clone();

        let rect = brightness_contrast(&mut layer, None, 0, 1.0);
        assert_eq!(rect, None);
        assert_eq!(layer, orig);
    }

    #[test]
    fn test_brightness_contrast_applies_change() {
        let mut layer = TiledLayer::new(64, 64);
        layer
            .set_pixel(10, 10, Rgba8::new(100, 100, 100, 255))
            .unwrap();

        let rect = brightness_contrast(&mut layer, None, 50, 1.0);
        assert_eq!(rect, Some(Rect::new(10, 10, 1, 1)));
        assert_eq!(
            layer.get_pixel(10, 10),
            Some(Rgba8::new(150, 150, 150, 255))
        );
    }

    #[test]
    fn test_selection_restricts_change() {
        let mut layer = TiledLayer::new(100, 100);
        // Ustawiamy dwa piksele: (15, 15) i (50, 50)
        layer
            .set_pixel(15, 15, Rgba8::new(100, 100, 100, 255))
            .unwrap();
        layer
            .set_pixel(50, 50, Rgba8::new(100, 100, 100, 255))
            .unwrap();

        // Zaznaczamy tylko obszar wokół (15, 15)
        let mut sel = Selection::new(100, 100);
        sel.select_rect(10, 10, 20, 20);

        let rect = brightness_contrast(&mut layer, Some(&sel), 50, 1.0);
        assert_eq!(rect, Some(Rect::new(15, 15, 1, 1)));

        // Piksel wewnątrz zaznaczenia uległ zmianie
        assert_eq!(
            layer.get_pixel(15, 15),
            Some(Rgba8::new(150, 150, 150, 255))
        );
        // Piksel poza zaznaczeniem pozostał nienaruszony
        assert_eq!(
            layer.get_pixel(50, 50),
            Some(Rgba8::new(100, 100, 100, 255))
        );
    }

    #[test]
    fn test_gaussian_blur_uniform_layer() {
        let mut layer = TiledLayer::new(64, 64);
        let color = Rgba8::new(120, 60, 180, 255);
        for y in 0..64 {
            for x in 0..64 {
                layer.set_pixel(x, y, color).unwrap();
            }
        }
        let orig = layer.clone();

        // Rozmycie jednolitej warstwy nie zmienia jej
        let rect = gaussian_blur(&mut layer, None, 3);
        assert_eq!(rect, None);
        assert_eq!(layer, orig);
    }

    #[test]
    fn test_gaussian_blur_edge() {
        let mut layer = TiledLayer::new(64, 64);
        // Lewa połowa czarna, prawa biała
        for y in 0..64 {
            for x in 0..32 {
                layer.set_pixel(x, y, Rgba8::BLACK).unwrap();
            }
            for x in 32..64 {
                layer.set_pixel(x, y, Rgba8::WHITE).unwrap();
            }
        }

        let rect = gaussian_blur(&mut layer, None, 2);
        assert!(rect.is_some());
        // Piksele na granicy (31, 32) powinny stać się szare
        let p31 = layer.get_pixel(31, 32).unwrap();
        let p32 = layer.get_pixel(32, 32).unwrap();
        assert!(p31.r() > 0 && p31.r() < 255);
        assert!(p32.r() > 0 && p32.r() < 255);
    }

    #[test]
    fn test_hue_saturation_shift() {
        let mut layer = TiledLayer::new(64, 64);
        layer.set_pixel(5, 5, Rgba8::new(255, 0, 0, 255)).unwrap();

        // Przesunięcie odcienia czerwonego o 120 stopni powinno dać kolor zielony
        let rect = hue_saturation(&mut layer, None, 120.0, 1.0, 0.0);
        assert_eq!(rect, Some(Rect::new(5, 5, 1, 1)));

        let px = layer.get_pixel(5, 5).unwrap();
        assert_eq!(px.r(), 0);
        assert_eq!(px.g(), 255);
        assert_eq!(px.b(), 0);
    }

    #[test]
    fn test_hue_saturation_identity() {
        let mut layer = TiledLayer::new(64, 64);
        layer
            .set_pixel(5, 5, Rgba8::new(200, 100, 50, 255))
            .unwrap();
        let orig = layer.clone();

        let rect = hue_saturation(&mut layer, None, 0.0, 1.0, 0.0);
        assert_eq!(rect, None);
        assert_eq!(layer, orig);
    }

    #[test]
    fn test_unsharp_mask_uniform_layer() {
        let mut layer = TiledLayer::new(64, 64);
        let color = Rgba8::new(75, 125, 175, 255);
        for y in 0..64 {
            for x in 0..64 {
                layer.set_pixel(x, y, color).unwrap();
            }
        }
        let orig = layer.clone();

        let rect = unsharp_mask(&mut layer, None, 2, 1.0);
        assert_eq!(rect, None);
        assert_eq!(layer, orig);
    }

    #[test]
    fn test_unsharp_mask_sharpening_effect() {
        let mut layer = TiledLayer::new(64, 64);
        // Tworzymy łagodny gradient skokowy
        for y in 0..64 {
            for x in 0..32 {
                layer.set_pixel(x, y, Rgba8::new(80, 80, 80, 255)).unwrap();
            }
            for x in 32..64 {
                layer
                    .set_pixel(x, y, Rgba8::new(160, 160, 160, 255))
                    .unwrap();
            }
        }

        let rect = unsharp_mask(&mut layer, None, 2, 1.0);
        assert!(rect.is_some());

        // Po wyostrzeniu strona ciemniejsza przy krawędzi staje się jeszcze ciemniejsza,
        // a jaśniejsza staje się jeszcze jaśniejsza (zwiększenie kontrastu lokalnego)
        let dark_edge = layer.get_pixel(31, 32).unwrap().r();
        let light_edge = layer.get_pixel(32, 32).unwrap().r();

        assert!(
            dark_edge < 80,
            "Ciemna krawędź powinna być ciemniejsza niż 80, otrzymano {}",
            dark_edge
        );
        assert!(
            light_edge > 160,
            "Jasna krawędź powinna być jaśniejsza niż 160, otrzymano {}",
            light_edge
        );
    }

    #[test]
    fn test_empty_layer_and_empty_tiles() {
        let mut layer = TiledLayer::new(100, 100);
        assert_eq!(layer.count_allocated_tiles(), 0);

        assert_eq!(brightness_contrast(&mut layer, None, 50, 1.5), None);
        assert_eq!(hue_saturation(&mut layer, None, 90.0, 1.5, 0.2), None);
        assert_eq!(gaussian_blur(&mut layer, None, 3), None);
        assert_eq!(unsharp_mask(&mut layer, None, 3, 1.0), None);

        // Brak niepotrzebnych alokacji kafli
        assert_eq!(layer.count_allocated_tiles(), 0);
    }
}
