//! Maska pędzla okrągłego oraz nanoszenie śladów (dab) na warstwę.

use crate::blend::{apply_opacity_u8, blend_normal, div255_round};
use crate::document::Rect;
use crate::layer::Layer;
use crate::pixel::Rgba8;
use crate::selection::Selection;

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

/// Rozjaśnianie (amount > 0) lub ściemnianie (amount < 0) piksela premultiplied; amount w -1.0..=1.0.
pub fn dodge_burn_pixel(px: Rgba8, amount: f32) -> Rgba8 {
    if px.a() == 0 {
        return px;
    }
    let [r, g, b, a] = px.to_straight();
    let amount = amount.clamp(-1.0, 1.0);
    let f = |v: u8| -> u8 {
        let v = v as f32;
        let n = if amount >= 0.0 {
            v + (255.0 - v) * amount
        } else {
            v * (1.0 + amount)
        };
        n.round().clamp(0.0, 255.0) as u8
    };
    Rgba8::from_straight(f(r), f(g), f(b), a)
}

/// Rozjaśnianie (amount > 0) / ściemnianie (amount < 0) pędzlem: każdy piksel w okręgu dostaje dodge_burn_pixel(px, amount * falloff * pokrycie zaznaczenia).
#[allow(clippy::too_many_arguments)]
pub fn dodge_burn_dab(
    layer: &mut Layer,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    amount: f32,
    selection: Option<&Selection>,
) -> Option<Rect> {
    if radius <= 0.0 || amount == 0.0 || layer.pixels.width == 0 || layer.pixels.height == 0 {
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
            let k = falloff * cov;
            if k <= 0.0 {
                continue;
            }

            let dst = layer
                .get_pixel(x as u32, y as u32)
                .unwrap_or(Rgba8::TRANSPARENT);
            let out = dodge_burn_pixel(dst, amount * k);
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

/// Rozmywanie (amount > 0) lub wyostrzanie (amount < 0) pędzlem w obszarze okręgu o środku (cx, cy).
/// Wykorzystuje blur_pass do rozmycia małego bufora wokół pędzla i mix_pixels.
#[allow(clippy::too_many_arguments)]
pub fn blur_sharpen_dab(
    layer: &mut Layer,
    cx: f32,
    cy: f32,
    radius: f32,
    hardness: f32,
    amount: f32,
    selection: Option<&Selection>,
) -> Option<Rect> {
    if radius <= 0.0 || amount == 0.0 || layer.pixels.width == 0 || layer.pixels.height == 0 {
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

    // Pobieramy bufor z zapasem 2 px z każdej strony dla splotu gaussa
    let pad = 2i64;
    let bx0 = (x0 - pad).max(0);
    let by0 = (y0 - pad).max(0);
    let bx1 = (x1 + pad).min(w - 1);
    let by1 = (y1 + pad).min(h - 1);
    let bw = (bx1 - bx0 + 1) as usize;
    let bh = (by1 - by0 + 1) as usize;

    let mut src_buf = Vec::with_capacity(bw * bh);
    for y in by0..=by1 {
        for x in bx0..=bx1 {
            src_buf.push(
                layer
                    .get_pixel(x as u32, y as u32)
                    .unwrap_or(Rgba8::TRANSPARENT),
            );
        }
    }

    let kernel = [0.25f32, 0.5f32, 0.25f32];
    let mut tmp_buf = vec![Rgba8::TRANSPARENT; bw * bh];
    let mut blurred_buf = vec![Rgba8::TRANSPARENT; bw * bh];

    // Przejście poziome i pionowe
    crate::blur::blur_pass(&src_buf, &mut tmp_buf, bw, bh, &kernel, true);
    crate::blur::blur_pass(&tmp_buf, &mut blurred_buf, bw, bh, &kernel, false);

    let amount_clamped = amount.clamp(-1.0, 1.0);
    let mut changed: Option<(u32, u32, u32, u32)> = None;

    for y in y0..=y1 {
        let py = y as f32 + 0.5;
        let dy = py - cy;
        let local_y = (y - by0) as usize;

        for x in x0..=x1 {
            let px = x as f32 + 0.5;
            let dx = px - cx;
            let dist = dx.hypot(dy);

            if dist > radius {
                continue;
            }

            let falloff = brush_falloff(dist, radius, hardness) as f32 / 255.0;
            let cov = selection.map_or(1.0, |s| s.coverage(x as u32, y as u32) as f32 / 255.0);
            let k = falloff * cov;
            if k <= 0.0 {
                continue;
            }

            let local_x = (x - bx0) as usize;
            let idx = local_y * bw + local_x;
            let orig = src_buf[idx];
            let blurred = blurred_buf[idx];

            let target = if amount_clamped >= 0.0 {
                // Rozmycie: mieszamy oryginał z rozmytym
                mix_pixels(orig, blurred, amount_clamped)
            } else {
                // Wyostrzanie: unsharp mask: orig + (orig - blurred) * (-amount)
                let factor = -amount_clamped;
                let clamp_u8 = |val: f32| val.round().clamp(0.0, 255.0) as u8;
                let calc = |o: u8, b: u8| -> u8 {
                    let diff = o as f32 - b as f32;
                    clamp_u8(o as f32 + diff * factor)
                };
                Rgba8::new(
                    calc(orig.r(), blurred.r()),
                    calc(orig.g(), blurred.g()),
                    calc(orig.b(), blurred.b()),
                    calc(orig.a(), blurred.a()),
                )
            };

            let out = mix_pixels(orig, target, k);
            if out != orig {
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
        let d = dodge_burn_pixel(px, 0.5);
        assert!((177..=179).contains(&d.r()) && d.a() == 255);
        let b = dodge_burn_pixel(px, -0.5);
        assert!((49..=51).contains(&b.r()));
        assert_eq!(
            dodge_burn_pixel(Rgba8::TRANSPARENT, 1.0),
            Rgba8::TRANSPARENT
        );
        let (a, c) = (Rgba8::new(0, 0, 0, 255), Rgba8::new(200, 100, 50, 255));
        assert_eq!(mix_pixels(a, c, 0.0), a);
        assert_eq!(mix_pixels(a, c, 1.0), c);
        assert_eq!(mix_pixels(a, c, 0.5), Rgba8::new(100, 50, 25, 255));
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
        let rect = dodge_burn_dab(&mut layer, 5.0, 5.0, 2.0, 1.0, 0.5, None);

        assert!(rect.is_some());

        // Piksel centralny powinien być jaśniejszy
        assert!(layer.get_pixel(5, 5).unwrap().r() > 150);

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
        // Single white pixel in the center
        let white = Rgba8::new(255, 255, 255, 255);
        layer.set_pixel(5, 5, white).unwrap();

        // Blur dab over the center
        let rect = blur_sharpen_dab(&mut layer, 5.5, 5.5, 3.0, 1.0, 1.0, None);
        assert!(rect.is_some());
        // Center pixel should be diffused/blurred (lower than 255)
        let center = layer.get_pixel(5, 5).unwrap();
        assert!(center.r() < 255 && center.r() > 0);
        // Neighboring pixel should now have some brightness
        let neighbor = layer.get_pixel(4, 5).unwrap();
        assert!(neighbor.r() > 0);

        // Sharpen dab
        let rect_sharp = blur_sharpen_dab(&mut layer, 5.5, 5.5, 3.0, 1.0, -1.0, None);
        assert!(rect_sharp.is_some());
    }
}
