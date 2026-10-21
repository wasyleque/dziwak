//! Maska pędzla okrągłego oraz nanoszenie śladów (dab) na warstwę.

use crate::blend::{apply_opacity_u8, blend_normal, div255_round};
use crate::document::Rect;
use crate::layer::Layer;
use crate::pixel::Rgba8;
use crate::selection::Selection;
use crate::tile::{TILE_PIXELS, TILE_SIZE};
use std::collections::HashMap;

/// Tryb działania pędzla (malowanie lub wymazywanie).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrushMode {
    /// Rysowanie kolorem (kompozycja normalna).
    #[default]
    Paint,
    /// Wymazywanie (redukcja krycia i kanałów koloru).
    Erase,
}

/// Krycie pędzla (0..=255) w odległości `dist` od środka.
/// Wewnątrz `radius * hardness` pełne 255, dalej liniowy spadek do 0 na `radius`.
pub fn brush_falloff(dist: f32, radius: f32, hardness: f32) -> u8 {
    if radius <= 0.0 || dist > radius {
        return 0;
    }

    let hardness = hardness.clamp(0.0, 1.0);
    let inner = radius * hardness;

    if dist <= inner {
        255
    } else {
        let t = (radius - dist) / (radius - inner);
        (t * 255.0).round().clamp(0.0, 255.0) as u8
    }
}

/// Stawia pojedynczy ślad ('dab') pędzla na warstwie.
///
/// Iteruje wyłącznie po bounding boxie okręgu o środku `(cx, cy)` i promieniu `radius`.
/// Dla każdego piksela oblicza krycie: `brush_falloff(dist, radius, hardness) * opacity`.
/// Jeśli podano zaznaczenie (`selection`), mnoży krycie przez `coverage(x, y) / 255`.
/// W trybie `Paint` miesza kolor za pomocą `blend_normal` (premultiplied alpha).
/// W trybie `Erase` zmniejsza alfę i kanały RGB: `dst * (255 - krycie) / 255`.
///
/// Zwraca `Some(Rect)` opisujący obszar zmian (do oznaczenia brudnych kafli) lub `None`,
/// jeśli ślad leży całkowicie poza warstwą lub nic nie zmienił.
#[allow(clippy::too_many_arguments)]
pub fn apply_dab(
    layer: &mut Layer,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    opacity: f32,
    color: Rgba8,
    mode: BrushMode,
    selection: Option<&Selection>,
) -> Option<Rect> {
    if radius <= 0.0 || opacity <= 0.0 || layer.pixels.width == 0 || layer.pixels.height == 0 {
        return None;
    }

    let w = layer.pixels.width;
    let h = layer.pixels.height;

    let min_x = (cx - radius).floor().max(0.0) as u32;
    let max_x = ((cx + radius).ceil().max(0.0) as u32).min(w.saturating_sub(1));
    let min_y = (cy - radius).floor().max(0.0) as u32;
    let max_y = ((cy + radius).ceil().max(0.0) as u32).min(h.saturating_sub(1));

    if min_x > max_x || min_y > max_y || min_x >= w || min_y >= h {
        return None;
    }

    let opacity = opacity.clamp(0.0, 1.0);
    let mut any_changed = false;

    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        let dy = py - cy;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let dx = px - cx;
            let dist = dx.hypot(dy);

            if dist > radius {
                continue;
            }

            let falloff = brush_falloff(dist, radius, hardness);
            if falloff == 0 {
                continue;
            }

            let mut dab_opacity = (falloff as f32 * opacity).round().clamp(0.0, 255.0) as u8;
            if let Some(sel) = selection {
                let cov = sel.coverage(x, y);
                if cov == 0 {
                    continue;
                }
                dab_opacity = div255_round(dab_opacity as u32 * cov as u32) as u8;
            }

            if dab_opacity == 0 {
                continue;
            }

            match mode {
                BrushMode::Paint => {
                    let src = apply_opacity_u8(color, dab_opacity as u32);
                    let dst = layer.get_pixel(x, y).unwrap_or(Rgba8::TRANSPARENT);
                    let new_pixel = blend_normal(dst, src);
                    if new_pixel != dst {
                        let _ = layer.set_pixel(x, y, new_pixel);
                        any_changed = true;
                    }
                }
                BrushMode::Erase => {
                    let dst = layer.get_pixel(x, y).unwrap_or(Rgba8::TRANSPARENT);
                    if !dst.is_transparent() {
                        let factor = 255 - dab_opacity as u32;
                        let new_pixel = apply_opacity_u8(dst, factor);
                        if new_pixel != dst {
                            let _ = layer.set_pixel(x, y, new_pixel);
                            any_changed = true;
                        }
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

/// Bufor farby narzędzia Rozmazywanie (jak „accumulator” w GIMP): kwadrat `size` x `size` pikseli.
#[derive(Debug, Clone, PartialEq)]
pub struct SmudgeBuffer {
    pub size: usize,
    pub data: Vec<Rgba8>,
}

/// Początek pociągnięcia: pędzel „nabiera” piksele spod siebie.
pub fn smudge_begin(layer: &Layer, cx: f32, cy: f32, radius: f32) -> SmudgeBuffer {
    let half = radius.max(0.5).ceil() as i64;
    let size = (2 * half + 1) as usize;
    let (ox, oy) = (cx.round() as i64 - half, cy.round() as i64 - half);
    let mut data = Vec::with_capacity(size * size);
    for j in 0..size as i64 {
        for i in 0..size as i64 {
            let (x, y) = (ox + i, oy + j);
            let px = if x >= 0 && y >= 0 {
                layer
                    .get_pixel(x as u32, y as u32)
                    .unwrap_or(Rgba8::TRANSPARENT)
            } else {
                Rgba8::TRANSPARENT
            };
            data.push(px);
        }
    }
    SmudgeBuffer { size, data }
}

/// Jeden dab rozmazywania: bufor = rate * bufor + (1 - rate) * płótno, potem płótno miesza się z buforem
/// z kryciem falloff * pokrycie zaznaczenia. `rate` 0..1 — im wyższy, tym dłużej ciągnie się farba.
#[allow(clippy::too_many_arguments)]
pub fn smudge_dab(
    layer: &mut Layer,
    buf: &mut SmudgeBuffer,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    rate: f32,
    selection: Option<&Selection>,
) -> Option<Rect> {
    if buf.size == 0 || buf.data.len() != buf.size * buf.size || radius <= 0.0 {
        return None;
    }
    let rate = rate.clamp(0.0, 1.0);
    let half = (buf.size / 2) as i64;
    let (ox, oy) = (cx.round() as i64 - half, cy.round() as i64 - half);
    let (w, h) = (layer.pixels.width as i64, layer.pixels.height as i64);
    let mut changed: Option<(u32, u32, u32, u32)> = None;
    for j in 0..buf.size as i64 {
        for i in 0..buf.size as i64 {
            let (x, y) = (ox + i, oy + j);
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let dist = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            if dist > radius {
                continue;
            }
            let falloff = brush_falloff(dist, radius, hardness) as f32 / 255.0;
            let cov = selection.map_or(1.0, |s| s.coverage(x as u32, y as u32) as f32 / 255.0);
            let k = falloff * cov;
            if k <= 0.0 {
                continue;
            }
            let idx = (j as usize) * buf.size + i as usize;
            let canvas = layer
                .get_pixel(x as u32, y as u32)
                .unwrap_or(Rgba8::TRANSPARENT);
            let acc = mix_pixels(canvas, buf.data[idx], rate);
            buf.data[idx] = acc;
            let out = mix_pixels(canvas, acc, k);
            if out != canvas {
                let _ = layer.set_pixel(x as u32, y as u32, out);
                let (ux, uy) = (x as u32, y as u32);
                changed = Some(match changed {
                    None => (ux, uy, ux, uy),
                    Some((a, b, c, d)) => (a.min(ux), b.min(uy), c.max(ux), d.max(uy)),
                });
            }
        }
    }
    changed.map(|(a, b, c, d)| Rect::new(a, b, c - a + 1, d - b + 1))
}

/// Zakres tonalny rozjaśniania/ściemniania (jak „Zakres” w GIMP).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToneRange {
    Shadows,
    #[default]
    Midtones,
    Highlights,
}

/// Maska pociągnięcia jak w GIMP (tryb nieprzyrostowy): pamięta na piksel najwyższe krycie śladu
/// w bieżącym pociągnięciu, więc nakładające się ślady nie kumulują efektu.
/// Rzadka — kafle 64×64 powstają tylko tam, gdzie przeszedł pędzel.
#[derive(Debug, Default, Clone)]
pub struct StrokeMask {
    tiles: HashMap<(u32, u32), Box<[u8; TILE_PIXELS]>>,
}

impl StrokeMask {
    pub fn new() -> Self {
        Self::default()
    }

    /// Podnosi krycie piksela do co najmniej `k`; zwraca nowe krycie, jeśli wzrosło.
    pub fn raise(&mut self, x: u32, y: u32, k: u8) -> Option<u8> {
        if k == 0 {
            return None;
        }
        let ts = TILE_SIZE as u32;
        let tile = self
            .tiles
            .entry((x / ts, y / ts))
            .or_insert_with(|| Box::new([0; TILE_PIXELS]));
        let v = &mut tile[((y % ts) * ts + x % ts) as usize];
        if k > *v {
            *v = k;
            Some(k)
        } else {
            None
        }
    }
}

/// Tablica krzywej rozjaśniania (exposure > 0) / ściemniania (exposure < 0) — wzory GIMP (gimpdodgeburn):
/// factor = 1 + |exposure| / 3; półtony: in^(1/factor) / in^factor, światła: in·factor / in/factor,
/// cienie: 1 − (1 − in)/factor / 1 − (1 − in)·factor. Czerń i biel zostają na miejscu w półtonach.
pub fn dodge_burn_lut(exposure: f32, range: ToneRange) -> [u8; 256] {
    let dodge = exposure >= 0.0;
    let factor = 1.0 + exposure.abs().min(1.0) / 3.0;
    let mut lut = [0u8; 256];
    for (v, out) in lut.iter_mut().enumerate() {
        let i = v as f32 / 255.0;
        let o = match (range, dodge) {
            (ToneRange::Midtones, true) => i.powf(1.0 / factor),
            (ToneRange::Midtones, false) => i.powf(factor),
            (ToneRange::Highlights, true) => i * factor,
            (ToneRange::Highlights, false) => i / factor,
            (ToneRange::Shadows, true) => 1.0 - (1.0 - i) / factor,
            (ToneRange::Shadows, false) => 1.0 - (1.0 - i) * factor,
        };
        *out = (o * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    lut
}

/// Rozjaśnia/ściemnia piksel premultiplied krzywą z [`dodge_burn_lut`] (alfa bez zmian).
pub fn dodge_burn_pixel(px: Rgba8, lut: &[u8; 256]) -> Rgba8 {
    if px.a() == 0 {
        return px;
    }
    let [r, g, b, a] = px.to_straight();
    Rgba8::from_straight(lut[r as usize], lut[g as usize], lut[b as usize], a)
}

/// Ślad rozjaśniania (exposure > 0) / ściemniania (exposure < 0) jak w GIMP: wynik liczony jest
/// z `source` (stan z początku pociągnięcia), a krycie to maksimum śladów w `mask` — jedno pociągnięcie
/// rozjaśnia najwyżej o jedną „ekspozycję”, kolejne pociągnięcia dokładają następną.
#[allow(clippy::too_many_arguments)]
pub fn dodge_burn_dab(
    layer: &mut Layer,
    source: &crate::layer::TiledLayer,
    mask: &mut StrokeMask,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    exposure: f32,
    range: ToneRange,
    selection: Option<&Selection>,
) -> Option<Rect> {
    if radius <= 0.0
        || exposure == 0.0
        || layer.pixels.width == 0
        || layer.pixels.height == 0
        || source.width != layer.pixels.width
        || source.height != layer.pixels.height
    {
        return None;
    }

    let (w, h) = (layer.pixels.width as i64, layer.pixels.height as i64);
    let x0 = ((cx - radius).floor() as i64).max(0);
    let y0 = ((cy - radius).floor() as i64).max(0);
    let x1 = ((cx + radius).ceil() as i64).min(w - 1);
    let y1 = ((cy + radius).ceil() as i64).min(h - 1);
    if x0 > x1 || y0 > y1 {
        return None;
    }

    let lut = dodge_burn_lut(exposure, range);
    let mut changed: Option<(u32, u32, u32, u32)> = None;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dist = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let falloff = brush_falloff(dist, radius, hardness) as f32 / 255.0;
            let cov = selection.map_or(1.0, |s| s.coverage(x as u32, y as u32) as f32 / 255.0);
            let k = (falloff * cov * 255.0).round() as u8;
            let Some(m) = mask.raise(x as u32, y as u32, k) else {
                continue;
            };
            let src = source
                .get_pixel(x as u32, y as u32)
                .unwrap_or(Rgba8::TRANSPARENT);
            let out = mix_pixels(src, dodge_burn_pixel(src, &lut), m as f32 / 255.0);
            let _ = layer.set_pixel(x as u32, y as u32, out);
            let (ux, uy) = (x as u32, y as u32);
            changed = Some(match changed {
                None => (ux, uy, ux, uy),
                Some((a, b, c, d)) => (a.min(ux), b.min(uy), c.max(ux), d.max(uy)),
            });
        }
    }

    changed.map(|(a, b, c, d)| Rect::new(a, b, c - a + 1, d - b + 1))
}

/// Mieszanie dwóch pikseli premultiplied (rozmazywanie): t = 0 daje a, t = 1 daje b.
pub fn mix_pixels(a: Rgba8, b: Rgba8, t: f32) -> Rgba8 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgba8::new(
        m(a.r(), b.r()),
        m(a.g(), b.g()),
        m(a.b(), b.b()),
        m(a.a(), b.a()),
    )
}

/// Klonowanie: maluje na `layer` pikselami z `source` przesuniętymi o (dx, dy) (piksel docelowy (x, y) bierze źródło (x + dx, y + dy)).
/// Krycie jak w apply_dab: brush_falloff(odległość, radius, hardness) * opacity, przycięte do zaznaczenia.
#[allow(clippy::too_many_arguments)]
pub fn clone_dab(
    layer: &mut Layer,
    source: &crate::layer::TiledLayer,
    cx: f32,
    cy: f32,
    dx: i32,
    dy: i32,
    radius: f32,
    hardness: f32,
    opacity: f32,
    selection: Option<&Selection>,
) -> Option<Rect> {
    if radius <= 0.0 || opacity <= 0.0 || layer.pixels.width == 0 || layer.pixels.height == 0 {
        return None;
    }
    let (w, h) = (layer.pixels.width as i64, layer.pixels.height as i64);
    let x0 = ((cx - radius).floor() as i64).max(0);
    let y0 = ((cy - radius).floor() as i64).max(0);
    let x1 = ((cx + radius).ceil() as i64).min(w - 1);
    let y1 = ((cy + radius).ceil() as i64).min(h - 1);
    if x0 > x1 || y0 > y1 {
        return None;
    }
    let mut changed: Option<(u32, u32, u32, u32)> = None;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dist = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let falloff = brush_falloff(dist, radius, hardness) as f32 / 255.0;
            let cov = selection.map_or(1.0, |s| s.coverage(x as u32, y as u32) as f32 / 255.0);
            let k = (falloff * opacity.clamp(0.0, 1.0) * cov * 255.0).round() as u32;
            if k == 0 {
                continue;
            }
            let (sx, sy) = (x + dx as i64, y + dy as i64);
            if sx < 0 || sy < 0 || sx >= source.width as i64 || sy >= source.height as i64 {
                continue;
            }
            let src = source
                .get_pixel(sx as u32, sy as u32)
                .unwrap_or(Rgba8::TRANSPARENT);
            let dst = layer
                .get_pixel(x as u32, y as u32)
                .unwrap_or(Rgba8::TRANSPARENT);
            let out = crate::brush::mix_pixels(dst, src, k as f32 / 255.0);
            if out != dst {
                let _ = layer.set_pixel(x as u32, y as u32, out);
                let (ux, uy) = (x as u32, y as u32);
                changed = Some(match changed {
                    None => (ux, uy, ux, uy),
                    Some((a, b, c, d)) => (a.min(ux), b.min(uy), c.max(ux), d.max(uy)),
                });
            }
        }
    }
    changed.map(|(a, b, c, d)| Rect::new(a, b, c - a + 1, d - b + 1))
}

/// Rozmywanie (amount > 0) lub wyostrzanie (amount < 0) pędzlem w okręgu o środku (cx, cy), jak „Rozmycie/Wyostrzanie” w GIMP.
/// Rozmywanie działa przyrostowo na bieżącej warstwie (filtr 3×3 z siłą `amount` na ślad — pocieranie rozmywa coraz mocniej,
/// a rozmycie jest stabilne). Wyostrzanie liczy maskę wyostrzającą ze stanu `source` (z początku pociągnięcia) z kryciem
/// = maksimum śladów w `mask`, więc jedno pociągnięcie wyostrza najwyżej raz i nie „wypala” szumu zdjęcia.
#[allow(clippy::too_many_arguments)]
pub fn blur_sharpen_dab(
    layer: &mut Layer,
    source: &crate::layer::TiledLayer,
    mask: &mut StrokeMask,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    amount: f32,
    selection: Option<&Selection>,
) -> Option<Rect> {
    if radius <= 0.0
        || amount == 0.0
        || layer.pixels.width == 0
        || layer.pixels.height == 0
        || source.width != layer.pixels.width
        || source.height != layer.pixels.height
    {
        return None;
    }

    let (w, h) = (layer.pixels.width as i64, layer.pixels.height as i64);
    let x0 = ((cx - radius).floor() as i64).max(0);
    let y0 = ((cy - radius).floor() as i64).max(0);
    let x1 = ((cx + radius).ceil() as i64).min(w - 1);
    let y1 = ((cy + radius).ceil() as i64).min(h - 1);
    if x0 > x1 || y0 > y1 {
        return None;
    }

    let amount = amount.clamp(-1.0, 1.0);
    let sharpen = amount < 0.0;
    // Bufor z zapasem 1 px na filtr 3×3: przy wyostrzaniu ze źródła, przy rozmywaniu z bieżącej warstwy.
    let pad = 1i64;
    let bx0 = (x0 - pad).max(0);
    let by0 = (y0 - pad).max(0);
    let bx1 = (x1 + pad).min(w - 1);
    let by1 = (y1 + pad).min(h - 1);
    let bw = (bx1 - bx0 + 1) as usize;
    let bh = (by1 - by0 + 1) as usize;
    let read_from = if sharpen { source } else { &layer.pixels };
    let mut src_buf = Vec::with_capacity(bw * bh);
    for y in by0..=by1 {
        for x in bx0..=bx1 {
            src_buf.push(
                read_from
                    .get_pixel(x as u32, y as u32)
                    .unwrap_or(Rgba8::TRANSPARENT),
            );
        }
    }

    let kernel = [0.25f32, 0.5f32, 0.25f32];
    let mut tmp_buf = vec![Rgba8::TRANSPARENT; bw * bh];
    let mut blurred_buf = vec![Rgba8::TRANSPARENT; bw * bh];
    crate::blur::blur_pass(&src_buf, &mut tmp_buf, bw, bh, &kernel, true);
    crate::blur::blur_pass(&tmp_buf, &mut blurred_buf, bw, bh, &kernel, false);

    // Siła maski wyostrzającej: 0,5 → szczegóły ×1,25 na pociągnięcie, 1,0 → ×1,5.
    let s = -amount * 0.5;
    let mut changed: Option<(u32, u32, u32, u32)> = None;
    for y in y0..=y1 {
        let local_y = (y - by0) as usize;
        for x in x0..=x1 {
            let dist = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let falloff = brush_falloff(dist, radius, hardness) as f32 / 255.0;
            let cov = selection.map_or(1.0, |s| s.coverage(x as u32, y as u32) as f32 / 255.0);
            let k = falloff * cov;
            if k <= 0.0 {
                continue;
            }
            let idx = local_y * bw + (x - bx0) as usize;
            let orig = src_buf[idx];
            let blurred = blurred_buf[idx];

            let out = if sharpen {
                let Some(m) = mask.raise(x as u32, y as u32, (k * 255.0).round() as u8) else {
                    continue;
                };
                // RGB w premultiplied przycięte do alfy, alfa bez zmian.
                let max_c = orig.a() as f32;
                let calc = |o: u8, b: u8| -> u8 {
                    let o_f = o as f32;
                    (o_f + (o_f - b as f32) * s).round().clamp(0.0, max_c) as u8
                };
                let target = Rgba8::new(
                    calc(orig.r(), blurred.r()),
                    calc(orig.g(), blurred.g()),
                    calc(orig.b(), blurred.b()),
                    orig.a(),
                );
                mix_pixels(orig, target, m as f32 / 255.0)
            } else {
                mix_pixels(orig, blurred, k * amount)
            };

            let cur = layer
                .get_pixel(x as u32, y as u32)
                .unwrap_or(Rgba8::TRANSPARENT);
            if out != cur {
                let _ = layer.set_pixel(x as u32, y as u32, out);
                let (ux, uy) = (x as u32, y as u32);
                changed = Some(match changed {
                    None => (ux, uy, ux, uy),
                    Some((a, b, c, d)) => (a.min(ux), b.min(uy), c.max(ux), d.max(uy)),
                });
            }
        }
    }

    changed.map(|(a, b, c, d)| Rect::new(a, b, c - a + 1, d - b + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brush_falloff_center() {
        assert_eq!(brush_falloff(0.0, 10.0, 0.5), 255);
    }

    #[test]
    fn test_brush_falloff_outside_radius() {
        assert_eq!(brush_falloff(15.0, 10.0, 0.5), 0);
    }

    #[test]
    fn test_brush_falloff_hardness_1() {
        assert_eq!(brush_falloff(5.0, 10.0, 1.0), 255);
        assert_eq!(brush_falloff(10.0, 10.0, 1.0), 255);
        assert_eq!(brush_falloff(10.1, 10.0, 1.0), 0);
    }

    #[test]
    fn test_brush_falloff_hardness_0() {
        assert!((127..=129).contains(&brush_falloff(5.0, 10.0, 0.0)));
    }

    #[test]
    fn test_brush_falloff_hardness_05() {
        let result = brush_falloff(7.5, 10.0, 0.5);
        assert!((127..=129).contains(&result));
    }

    #[test]
    fn test_brush_falloff_radius_0() {
        assert_eq!(brush_falloff(0.0, 0.0, 0.5), 0);
    }

    #[test]
    fn test_apply_dab_center_of_tile() {
        // Warstwa 128×128 (4 kafle po 64×64: (0,0), (1,0), (0,1), (1,1))
        let mut layer = Layer::new("Warstwa", 128, 128);
        assert_eq!(layer.pixels.count_allocated_tiles(), 0);

        // Ślad dokładnie na środku pierwszego kafla (32.0, 32.0), promień 8
        let rect = apply_dab(
            &mut layer,
            32.0,
            32.0,
            8.0,
            1.0,
            1.0,
            Rgba8::new(255, 0, 0, 255),
            BrushMode::Paint,
            None,
        );

        assert!(rect.is_some());
        let r = rect.unwrap();
        assert!(r.x >= 24 && r.x + r.width <= 41);
        assert!(r.y >= 24 && r.y + r.height <= 41);

        // Piksel centralny powinien być czerwony
        assert_eq!(layer.get_pixel(32, 32), Some(Rgba8::new(255, 0, 0, 255)));

        // Zaalokowany powinien być WYŁĄCZNIE kafel 0 (pierwszy kafel)
        assert_eq!(layer.pixels.count_allocated_tiles(), 1);
        assert!(layer.pixels.tiles[0].is_some());
        assert!(layer.pixels.tiles[1].is_none());
        assert!(layer.pixels.tiles[2].is_none());
        assert!(layer.pixels.tiles[3].is_none());
    }

    #[test]
    fn test_apply_dab_four_tile_boundary() {
        // Warstwa 128×128 (4 kafle spotykające się w punkcie (64, 64))
        let mut layer = Layer::new("Warstwa", 128, 128);
        assert_eq!(layer.pixels.count_allocated_tiles(), 0);

        // Ślad na granicy 4 kafli (64.0, 64.0) o promieniu 10
        let rect = apply_dab(
            &mut layer,
            64.0,
            64.0,
            10.0,
            1.0,
            1.0,
            Rgba8::new(0, 255, 0, 255),
            BrushMode::Paint,
            None,
        );

        assert!(rect.is_some());
        let r = rect.unwrap();
        // Prostokąt musi przecinać granicę x=64 i y=64
        assert!(r.x < 64 && r.x + r.width > 64);
        assert!(r.y < 64 && r.y + r.height > 64);

        // Wszystkie 4 kafle powinny zostać zaalokowane
        assert_eq!(layer.pixels.count_allocated_tiles(), 4);
        assert!(layer.pixels.tiles[0].is_some());
        assert!(layer.pixels.tiles[1].is_some());
        assert!(layer.pixels.tiles[2].is_some());
        assert!(layer.pixels.tiles[3].is_some());

        // Piksele stykające się na rogach 4 kafli powinny być zielone
        let green = Some(Rgba8::new(0, 255, 0, 255));
        assert_eq!(layer.get_pixel(63, 63), green); // Kafel (0, 0)
        assert_eq!(layer.get_pixel(64, 63), green); // Kafel (1, 0)
        assert_eq!(layer.get_pixel(63, 64), green); // Kafel (0, 1)
        assert_eq!(layer.get_pixel(64, 64), green); // Kafel (1, 1)
    }

    #[test]
    fn test_apply_dab_erase() {
        let mut layer = Layer::new("Warstwa", 64, 64);
        // Najpierw malujemy nieprzezroczysty czerwony punkt
        apply_dab(
            &mut layer,
            32.0,
            32.0,
            5.0,
            1.0,
            1.0,
            Rgba8::new(255, 0, 0, 255),
            BrushMode::Paint,
            None,
        );
        assert_eq!(layer.get_pixel(32, 32), Some(Rgba8::new(255, 0, 0, 255)));

        // Następnie wymazujemy gumką z twardością 1.0 i kryciem 1.0
        let rect = apply_dab(
            &mut layer,
            32.0,
            32.0,
            5.0,
            1.0,
            1.0,
            Rgba8::TRANSPARENT,
            BrushMode::Erase,
            None,
        );
        assert!(rect.is_some());
        assert_eq!(layer.get_pixel(32, 32), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_apply_dab_outside_bounds() {
        let mut layer = Layer::new("Warstwa", 64, 64);
        let rect = apply_dab(
            &mut layer,
            -50.0,
            -50.0,
            5.0,
            1.0,
            1.0,
            Rgba8::WHITE,
            BrushMode::Paint,
            None,
        );
        assert!(rect.is_none());
    }

    #[test]
    fn test_apply_dab_clipped_to_selection() {
        let mut layer = Layer::new("Warstwa", 100, 100);
        let mut sel = Selection::new(100, 100);
        // Zaznaczamy tylko prawą dolną ćwiartkę (50..100, 50..100)
        sel.select_rect(50, 50, 50, 50);

        // Ślad pędzla ze środkiem w punkcie (50.0, 50.0), promień 8
        let red = Rgba8::new(255, 0, 0, 255);
        let rect = apply_dab(
            &mut layer,
            50.0,
            50.0,
            8.0,
            1.0,
            1.0,
            red,
            BrushMode::Paint,
            Some(&sel),
        );

        assert!(rect.is_some());

        // Piksel (52, 52) leży wewnątrz zaznaczenia i wewnątrz promienia pędzla -> czerwony
        assert_eq!(layer.get_pixel(52, 52), Some(red));

        // Piksel (48, 48) leży wewnątrz promienia, ale POZA zaznaczeniem -> pozostał przezroczysty
        assert_eq!(layer.get_pixel(48, 48), Some(Rgba8::TRANSPARENT));
        assert_eq!(layer.get_pixel(48, 52), Some(Rgba8::TRANSPARENT));
        assert_eq!(layer.get_pixel(52, 48), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_dodge_burn_and_mix() {
        let px = Rgba8::new(100, 100, 100, 255);
        let mid = ToneRange::Midtones;
        // GIMP, półtony, ekspozycja 50%: 100 → 255·(100/255)^(1/1,1667) ≈ 114
        let d = dodge_burn_pixel(px, &dodge_burn_lut(0.5, mid));
        assert!((112..=116).contains(&d.r()) && d.a() == 255, "{d:?}");
        let b = dodge_burn_pixel(px, &dodge_burn_lut(-0.5, mid));
        assert!((85..=89).contains(&b.r()), "{b:?}");
        // półtony nie ruszają czerni ani bieli — brak białych/czarnych plam
        let lut = dodge_burn_lut(1.0, mid);
        assert_eq!((lut[0], lut[255]), (0, 255));
        let lut = dodge_burn_lut(-1.0, mid);
        assert_eq!((lut[0], lut[255]), (0, 255));
        assert_eq!(
            dodge_burn_pixel(Rgba8::TRANSPARENT, &dodge_burn_lut(1.0, mid)),
            Rgba8::TRANSPARENT
        );
        let (a, c) = (Rgba8::new(0, 0, 0, 255), Rgba8::new(200, 100, 50, 255));
        assert_eq!(mix_pixels(a, c, 0.0), a);
        assert_eq!(mix_pixels(a, c, 1.0), c);
        assert_eq!(mix_pixels(a, c, 0.5), Rgba8::new(100, 50, 25, 255));
    }

    #[test]
    fn test_smudge_drags_color() {
        let mut layer = Layer::new("t", 30, 10);
        // Wypełnij kolumny x < 8 kolorem (255, 0, 0, 255), pozostałe (255, 255, 255, 255)
        for y in 0..10 {
            for x in 0..30 {
                let color = if x < 8 {
                    Rgba8::new(255, 0, 0, 255)
                } else {
                    Rgba8::new(255, 255, 255, 255)
                };
                layer.set_pixel(x, y, color).unwrap();
            }
        }

        let mut buf = smudge_begin(&layer, 5.0, 5.0, 3.0);
        for step in 0..15 {
            let x = 6.0 + step as f32;
            smudge_dab(&mut layer, &mut buf, x, 5.0, 3.0, 1.0, 0.8, None);
        }

        let p = layer.get_pixel(14, 5).unwrap();
        assert!(p.g() < 200, "kolor nie został przeciągnięty: {:?}", p);

        // Piksel (14, 0) poza pędzlem pozostaje biały
        let p = layer.get_pixel(14, 0).unwrap();
        assert_eq!(p, Rgba8::new(255, 255, 255, 255));
    }

    #[test]
    fn test_dodge_burn_dab() {
        let mut layer = Layer::new("t", 10, 10);

        // Wypełniamy całą warstwę szarym kolorem
        for y in 0..10 {
            for x in 0..10 {
                layer
                    .set_pixel(x, y, Rgba8::new(100, 100, 100, 255))
                    .unwrap();
            }
        }

        // Wykonujemy dodge_burn_dab z rozjaśnieniem (amount = 0.5)
        let source = layer.pixels.clone();
        let mut mask = StrokeMask::new();
        let rect = dodge_burn_dab(
            &mut layer,
            &source,
            &mut mask,
            5.0,
            5.0,
            2.0,
            1.0,
            0.5,
            ToneRange::Midtones,
            None,
        );

        assert!(rect.is_some());

        // Piksel centralny powinien być jaśniejszy
        let once = layer.get_pixel(5, 5).unwrap();
        assert!(once.r() > 105, "{once:?}");

        // Kolejne ślady w tym samym pociągnięciu nie kumulują efektu (maska pociągnięcia)
        for _ in 0..20 {
            dodge_burn_dab(
                &mut layer,
                &source,
                &mut mask,
                5.0,
                5.0,
                2.0,
                1.0,
                0.5,
                ToneRange::Midtones,
                None,
            );
        }
        assert_eq!(layer.get_pixel(5, 5).unwrap(), once);

        // Piksel w rogu nie powinien się zmienić
        assert_eq!(
            layer.get_pixel(0, 0).unwrap(),
            Rgba8::new(100, 100, 100, 255)
        );
    }

    #[test]
    fn test_clone_dab() {
        let mut layer = Layer::new("t", 20, 20);
        let mut source = crate::layer::TiledLayer::new(20, 20);
        source
            .set_pixel(15, 10, Rgba8::new(255, 0, 0, 255))
            .unwrap();

        let rect = clone_dab(&mut layer, &source, 5.5, 10.5, 10, 0, 2.0, 1.0, 1.0, None);

        assert!(rect.is_some());
        assert_eq!(layer.get_pixel(5, 10), Some(Rgba8::new(255, 0, 0, 255)));
    }

    #[test]
    fn test_blur_sharpen_dab() {
        let mut layer = Layer::new("t", 10, 10);
        let dark = Rgba8::new(40, 40, 40, 255);
        for y in 0..10 {
            for x in 0..10 {
                layer.set_pixel(x, y, dark).unwrap();
            }
        }
        // Single white pixel in the center on opaque dark background
        let white = Rgba8::new(255, 255, 255, 255);
        layer.set_pixel(5, 5, white).unwrap();

        // Blur dab over the center
        let source = layer.pixels.clone();
        let rect = blur_sharpen_dab(
            &mut layer,
            &source,
            &mut StrokeMask::new(),
            5.5,
            5.5,
            3.0,
            1.0,
            1.0,
            None,
        );
        assert!(rect.is_some());
        // Center pixel should be diffused/blurred (lower than 255)
        let center = layer.get_pixel(5, 5).unwrap();
        assert!(center.r() < 255 && center.r() > 40);
        // Neighboring pixel should now be brighter than the dark background
        let neighbor = layer.get_pixel(4, 5).unwrap();
        assert!(neighbor.r() > 40);

        // Sharpen dab increases center contrast again
        let source_sharp = layer.pixels.clone();
        let rect_sharp = blur_sharpen_dab(
            &mut layer,
            &source_sharp,
            &mut StrokeMask::new(),
            5.5,
            5.5,
            3.0,
            1.0,
            -1.0,
            None,
        );
        assert!(rect_sharp.is_some());
        assert!(layer.get_pixel(5, 5).unwrap().r() > center.r());
    }

    #[test]
    fn test_blur_sharpen_dab_no_runaway_and_premultiplied_clamp() {
        let mut init_layer = Layer::new("t", 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let a = if (4..12).contains(&x) && (4..12).contains(&y) {
                    140u8
                } else {
                    40u8
                };
                let r = if x == 8 && y == 8 {
                    130u8
                } else if (6..=10).contains(&x) && (6..=10).contains(&y) {
                    70u8
                } else {
                    20u8
                };
                let g = r / 2;
                let b = r.saturating_sub(10);
                init_layer.set_pixel(x, y, Rgba8::new(r, g, b, a)).unwrap();
            }
        }

        let source = init_layer.pixels.clone();

        // Zaznaczenie z pokryciem 50% (k < 1)
        let mut sel = Selection::new(16, 16);
        let sel_mask = vec![128u8; 16 * 16];
        sel.combine_mask(&sel_mask, crate::selection::SelectMode::Replace);

        // Pojedynczy dab
        let mut single_layer = init_layer.clone();
        blur_sharpen_dab(
            &mut single_layer,
            &source,
            &mut StrokeMask::new(),
            8.5,
            8.5,
            4.0,
            1.0,
            -0.8,
            Some(&sel),
        );

        // 20 dabów w tym samym miejscu w jednym pociągnięciu daje to samo co jeden (maska pociągnięcia)
        let mut stroke = StrokeMask::new();

        let mut multi_layer = init_layer.clone();
        for _ in 0..20 {
            blur_sharpen_dab(
                &mut multi_layer,
                &source,
                &mut stroke,
                8.5,
                8.5,
                4.0,
                1.0,
                -0.8,
                Some(&sel),
            );
        }

        for y in 0..16 {
            for x in 0..16 {
                let p_single = single_layer.get_pixel(x, y).unwrap();
                let p_multi = multi_layer.get_pixel(x, y).unwrap();

                // Dla każdego piksela po wyostrzaniu w premultiplied musi zachodzić r, g, b <= a
                assert!(p_single.r() <= p_single.a());
                assert!(p_single.g() <= p_single.a());
                assert!(p_single.b() <= p_single.a());
                assert!(p_multi.r() <= p_multi.a());
                assert!(p_multi.g() <= p_multi.a());
                assert!(p_multi.b() <= p_multi.a());

                // 20 dabów w jednym pociągnięciu = jeden dab (tolerancja ±2)
                assert!((p_single.r() as i16 - p_multi.r() as i16).abs() <= 2);
                assert!((p_single.g() as i16 - p_multi.g() as i16).abs() <= 2);
                assert!((p_single.b() as i16 - p_multi.b() as i16).abs() <= 2);
                assert!((p_single.a() as i16 - p_multi.a() as i16).abs() <= 2);
            }
        }
    }
}

#[cfg(test)]
mod sharpen_strength_tests {
    use super::*;
    use crate::layer::Layer;

    /// Odsetek pikseli „wypalonych” (kanał 0 lub 255) po `strokes` pociągnięciach wyostrzania.
    fn burned_after(strokes: usize, amount: f32) -> f32 {
        let (w, h) = (48u32, 48u32);
        let mut layer = Layer::new("t", w, h);
        let mut seed = 12345u32;
        for y in 0..h {
            for x in 0..w {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let n = ((seed >> 16) % 21) as i32 - 10; // szum ±10 jak w JPEG
                let v = (120 + n) as u8;
                layer
                    .set_pixel(
                        x,
                        y,
                        Rgba8::new(v, (v as i32 + 15) as u8, (v as i32 - 20) as u8, 255),
                    )
                    .unwrap();
            }
        }
        for _ in 0..strokes {
            let src = layer.pixels.clone();
            let mut mask = StrokeMask::new();
            for k in 0..20 {
                blur_sharpen_dab(
                    &mut layer,
                    &src,
                    &mut mask,
                    14.0 + k as f32,
                    24.0,
                    10.0,
                    0.8,
                    -amount,
                    None,
                );
            }
        }
        let mut burned = 0;
        let mut total = 0;
        for y in 16..32 {
            for x in 16..32 {
                let p = layer.get_pixel(x, y).unwrap();
                for c in [p.r(), p.g(), p.b()] {
                    total += 1;
                    if c == 0 || c == 255 {
                        burned += 1;
                    }
                }
            }
        }
        burned as f32 / total as f32
    }

    #[test]
    fn repeated_sharpen_strokes_do_not_burn_out_noise() {
        // 10 pociągnięć w tym samym miejscu przy sile 0,5 na zaszumionym obrazie (jak JPEG)
        assert!(burned_after(10, 0.5) < 0.02);
        // pełna siła też nie może po kilku pociągnięciach zamienić zdjęcia w sieczkę
        assert!(burned_after(3, 1.0) < 0.02);
    }
}
