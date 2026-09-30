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
}
