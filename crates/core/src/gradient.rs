//! Gradient liniowy.

use crate::blend::{apply_opacity_u8, blend_normal};
use crate::document::Rect;
use crate::layer::Layer;
use crate::pixel::Rgba8;
use crate::selection::Selection;

/// Parametr t w 0.0..=1.0 rzutu punktu p na odcinek a->b (przycięty).
/// Gdy a == b zwraca 0.0.
pub fn linear_t(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let d_x = b.0 - a.0;
    let d_y = b.1 - a.1;
    let len2 = d_x * d_x + d_y * d_y;

    if len2 == 0.0 {
        return 0.0;
    }

    let t = ((p.0 - a.0) * d_x + (p.1 - a.1) * d_y) / len2;
    t.clamp(0.0, 1.0)
}

/// Interpolacja liniowa dwóch kolorów premultiplied, t przycięte do 0..=1.
pub fn lerp_rgba(c0: Rgba8, c1: Rgba8, t: f32) -> Rgba8 {
    let t = t.clamp(0.0, 1.0);

    let r0 = c0.r() as f32;
    let g0 = c0.g() as f32;
    let b0 = c0.b() as f32;
    let a0 = c0.a() as f32;

    let r1 = c1.r() as f32;
    let g1 = c1.g() as f32;
    let b1 = c1.b() as f32;
    let a1 = c1.a() as f32;

    let r = (r0 + (r1 - r0) * t).round() as u8;
    let g = (g0 + (g1 - g0) * t).round() as u8;
    let b = (b0 + (b1 - b0) * t).round() as u8;
    let a = (a0 + (a1 - a0) * t).round() as u8;

    Rgba8::new(r, g, b, a)
}

/// Gradient liniowy od a (kolor c0) do b (kolor c1) na całej warstwie, z uwzględnieniem zaznaczenia.
pub fn apply_linear_gradient(
    layer: &mut Layer,
    a: (f32, f32),
    b: (f32, f32),
    c0: Rgba8,
    c1: Rgba8,
    sel: Option<&Selection>,
) -> Option<Rect> {
    let width = layer.pixels.width;
    let height = layer.pixels.height;

    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut changed = false;

    for y in 0..height {
        for x in 0..width {
            // Oblicz krycie na podstawie zaznaczenia
            let k = sel.map_or(255, |s| s.coverage(x, y) as u32);

            // Jeśli krycie jest równe 0, pomijamy ten piksel
            if k == 0 {
                continue;
            }

            // Oblicz parametr t dla punktu (x,y)
            let t = linear_t((x as f32 + 0.5, y as f32 + 0.5), a, b);

            // Oblicz kolor źródłowy na podstawie gradientu
            let src = lerp_rgba(c0, c1, t);

            // Pobierz aktualny kolor piksela
            let dst = layer.get_pixel(x, y).unwrap_or(Rgba8::TRANSPARENT);

            // Zastosuj kolor z uwzględnieniem krycia
            let blended = blend_normal(dst, apply_opacity_u8(src, k));

            // Ustaw nowy kolor piksela
            let _ = layer.set_pixel(x, y, blended);

            // Aktualizuj bounding box
            if x < min_x {
                min_x = x;
            }
            if x > max_x {
                max_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
            changed = true;
        }
    }

    // Zwróć bounding box zmienionych pikseli lub None jeśli nic nie zmieniono
    if changed {
        Some(Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::Layer;

    #[test]
    fn test_linear_t() {
        // Punkty a = 0, b = 1
        assert_eq!(linear_t((0.0, 0.0), (0.0, 0.0), (1.0, 0.0)), 0.0);
        assert_eq!(linear_t((1.0, 0.0), (0.0, 0.0), (1.0, 0.0)), 1.0);

        // Środek
        assert_eq!(linear_t((0.5, 0.0), (0.0, 0.0), (1.0, 0.0)), 0.5);

        // Punkt przed a = 0
        assert_eq!(linear_t((-0.5, 0.0), (0.0, 0.0), (1.0, 0.0)), 0.0);

        // Punkt za b = 1
        assert_eq!(linear_t((1.5, 0.0), (0.0, 0.0), (1.0, 0.0)), 1.0);

        // Punkt przesunięty prostopadle ma to samo t
        assert_eq!(linear_t((0.5, 1.0), (0.0, 0.0), (1.0, 0.0)), 0.5);

        // a == b -> 0
        assert_eq!(linear_t((0.0, 0.0), (0.0, 0.0), (0.0, 0.0)), 0.0);
    }

    #[test]
    fn test_lerp_rgba() {
        // t = 0 -> c0
        let c0 = Rgba8::new(255, 0, 0, 255);
        let c1 = Rgba8::new(0, 255, 0, 255);
        assert_eq!(lerp_rgba(c0, c1, 0.0), c0);

        // t = 1 -> c1
        assert_eq!(lerp_rgba(c0, c1, 1.0), c1);

        // t = 0.5 czarny->biały nieprzezroczysty = (128,128,128,255)
        let black = Rgba8::new(0, 0, 0, 255);
        let white = Rgba8::new(255, 255, 255, 255);
        assert_eq!(lerp_rgba(black, white, 0.5), Rgba8::new(128, 128, 128, 255));

        // t = 2.0 -> c1 (przycięte)
        assert_eq!(lerp_rgba(c0, c1, 2.0), c1);
    }

    #[test]
    fn test_apply_linear_gradient() {
        let mut layer = Layer::new("t", 10, 1);
        let r = apply_linear_gradient(
            &mut layer,
            (0.0, 0.5),
            (10.0, 0.5),
            Rgba8::new(0, 0, 0, 255),
            Rgba8::new(255, 255, 255, 255),
            None,
        )
        .expect("zmiana");
        assert_eq!(r.width, 10);
        assert_eq!(r.height, 1);

        // Sprawdź wartości kanału czerwonego
        let first = layer.get_pixel(0, 0).expect("piksel").r();
        let last = layer.get_pixel(9, 0).expect("piksel").r();

        assert!(first < 30);
        assert!(last > 225);

        // Sprawdź ciąg niemalejący
        for x in 1..9 {
            let current = layer.get_pixel(x, 0).expect("piksel").r();
            let previous = layer.get_pixel(x - 1, 0).expect("piksel").r();
            assert!(current >= previous);
        }
    }
}
