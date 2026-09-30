//! Wypełnianie obszaru (kubełek).
use crate::blend::{apply_opacity_u8, blend_normal, div255_round};
use crate::document::Rect;
use crate::layer::Layer;
use crate::pixel::Rgba8;
use crate::selection::Selection;

/// Maska obszaru spójnego (4-sąsiedztwo) z pikselem startowym: 255 = w obszarze, 0 = poza.
/// Piksel należy do obszaru, gdy max(|dr|,|dg|,|db|,|da|) względem koloru startowego <= tolerance.
pub fn flood_fill_mask(
    buf: &[Rgba8],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    tolerance: u8,
) -> Vec<u8> {
    // Sprawdzenie warunków wejściowych
    if buf.len() != w * h || x >= w || y >= h {
        return vec![0; w * h];
    }

    let start_color = buf[y * w + x];
    let mut mask = vec![0u8; w * h];
    let mut stack = Vec::new();

    // Dodanie punktu startowego
    stack.push((x, y));

    while let Some((cx, cy)) = stack.pop() {
        // Pomijamy piksele, które już są w obszarze lub nie pasują do koloru startowego
        if mask[cy * w + cx] == 255 || !matches(buf[cy * w + cx], start_color, tolerance) {
            continue;
        }

        // Zaznaczamy punkt jako należący do obszaru
        mask[cy * w + cx] = 255;

        // Wyszukiwanie granic odcinka w poziomie
        let mut left = cx;
        while left > 0 && matches(buf[cy * w + (left - 1)], start_color, tolerance) {
            left -= 1;
            mask[cy * w + left] = 255;
        }

        let mut right = cx;
        while right < w - 1 && matches(buf[cy * w + (right + 1)], start_color, tolerance) {
            right += 1;
            mask[cy * w + right] = 255;
        }

        // Dodawanie pikseli z sąsiadujących wierszy do stosu
        if cy > 0 {
            for x in left..=right {
                if mask[(cy - 1) * w + x] == 0
                    && matches(buf[(cy - 1) * w + x], start_color, tolerance)
                {
                    stack.push((x, cy - 1));
                }
            }
        }

        if cy < h - 1 {
            for x in left..=right {
                if mask[(cy + 1) * w + x] == 0
                    && matches(buf[(cy + 1) * w + x], start_color, tolerance)
                {
                    stack.push((x, cy + 1));
                }
            }
        }
    }

    mask
}

/// Sprawdza czy dwa kolory pasują do siebie z uwagi na tolerancję.
fn matches(a: Rgba8, b: Rgba8, tol: u8) -> bool {
    let dr = a.r().abs_diff(b.r());
    let dg = a.g().abs_diff(b.g());
    let db = a.b().abs_diff(b.b());
    let da = a.a().abs_diff(b.a());

    dr.max(dg).max(db).max(da) <= tol
}

/// Wypełnia piksele z maski (np. z flood_fill_mask) kolorem, z uwzględnieniem zaznaczenia.
pub fn apply_fill(
    layer: &mut Layer,
    mask: &[u8],
    color: Rgba8,
    sel: Option<&Selection>,
) -> Option<Rect> {
    let width = layer.pixels.width as usize;
    let height = layer.pixels.height as usize;

    if mask.len() != width * height {
        return None;
    }

    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut changed = false;

    for y in 0..height {
        for x in 0..width {
            let m = mask[y * width + x] as u32;
            if m == 0 {
                continue;
            }

            let cov = sel.map_or(255, |s| s.coverage(x as u32, y as u32) as u32);
            let k = div255_round(m * cov);

            if k == 0 {
                continue;
            }

            let dst = layer
                .get_pixel(x as u32, y as u32)
                .unwrap_or(Rgba8::TRANSPARENT);
            let src = color;
            let result = blend_normal(dst, apply_opacity_u8(src, k));

            let _ = layer.set_pixel(x as u32, y as u32, result);

            // Update bounding box
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

    if !changed {
        None
    } else {
        Some(Rect {
            x: min_x as u32,
            y: min_y as u32,
            width: (max_x - min_x + 1) as u32,
            height: (max_y - min_y + 1) as u32,
        })
    }
}

/// Mnoży piksele warstwy (premultiplied) przez maskę m/255 — 0 czyni piksel przezroczystym. Maska ma długość width*height.
pub fn mask_alpha(layer: &mut Layer, mask: &[u8]) -> Option<Rect> {
    let width = layer.pixels.width as usize;
    let height = layer.pixels.height as usize;

    if mask.len() != width * height {
        return None;
    }

    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut changed = false;

    for y in 0..height {
        for x in 0..width {
            let m = mask[y * width + x] as u32;

            if m == 255 {
                continue;
            }

            let px = layer
                .get_pixel(x as u32, y as u32)
                .unwrap_or(Rgba8::TRANSPARENT);

            if px.a() == 0 {
                continue;
            }

            let nowy = apply_opacity_u8(px, m);
            let _ = layer.set_pixel(x as u32, y as u32, nowy);

            // Update bounding box
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

    if !changed {
        None
    } else {
        Some(Rect {
            x: min_x as u32,
            y: min_y as u32,
            width: (max_x - min_x + 1) as u32,
            height: (max_y - min_y + 1) as u32,
        })
    }
}

/// Maska tła połączonego z brzegami obrazu: suma (max) flood_fill_mask uruchomionych z 4 rogów z tolerancją `tolerance`.
pub fn border_background_mask(buf: &[Rgba8], w: usize, h: usize, tolerance: u8) -> Vec<u8> {
    if w == 0 || h == 0 || buf.len() != w * h {
        return vec![0; w * h];
    }

    let mut out = vec![0u8; w * h];

    // Run flood fill from each corner
    let corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];

    for &(x, y) in &corners {
        let m = flood_fill_mask(buf, w, h, x, y, tolerance);
        for i in 0..out.len() {
            out[i] = out[i].max(m[i]);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniform_4x4() {
        let buf = vec![Rgba8::new(100, 150, 200, 255); 16];
        let mask = flood_fill_mask(&buf, 4, 4, 0, 0, 0);
        assert_eq!(mask, vec![255; 16]);
    }

    #[test]
    fn test_5x1_with_tolerance() {
        let buf = [
            Rgba8::new(100, 150, 200, 255),
            Rgba8::new(100, 150, 200, 255),
            Rgba8::new(200, 100, 50, 255),
            Rgba8::new(100, 150, 200, 255),
            Rgba8::new(100, 150, 200, 255),
        ];
        let mask = flood_fill_mask(&buf, 5, 1, 0, 0, 0);
        assert_eq!(mask, vec![255, 255, 0, 0, 0]);
    }

    #[test]
    fn test_vertical_wall() {
        let a = Rgba8::new(100, 150, 200, 255);
        let b = Rgba8::new(200, 100, 50, 255);
        // 5x3, kolumna 2 to ściana z koloru B
        let mut buf = [a; 15];
        for y in 0..3 {
            buf[y * 5 + 2] = b;
        }
        let mask = flood_fill_mask(&buf, 5, 3, 0, 0, 0);
        for y in 0..3 {
            assert_eq!(&mask[y * 5..y * 5 + 5], &[255, 255, 0, 0, 0]);
        }
    }

    #[test]
    fn test_tolerance() {
        let buf = [
            Rgba8::new(100, 150, 200, 255),
            Rgba8::new(110, 160, 210, 255),
            Rgba8::new(120, 170, 220, 255),
        ];
        // Tolerancja 10 - kolory 100,150,200 i 110,160,210 powinny pasować
        let mask = flood_fill_mask(&buf, 3, 1, 0, 0, 10);
        assert_eq!(mask[0], 255);
        assert_eq!(mask[1], 255);
        assert_eq!(mask[2], 0); // 120,170,220 nie pasuje przy tolerancji 10

        // Tolerancja 9 - kolory 100,150,200 i 110,160,210 nie powinny pasować
        let mask = flood_fill_mask(&buf, 3, 1, 0, 0, 9);
        assert_eq!(mask[0], 255);
        assert_eq!(mask[1], 0);
        assert_eq!(mask[2], 0);
    }

    #[test]
    fn test_start_outside_image() {
        let buf = vec![Rgba8::new(100, 150, 200, 255); 4];
        let mask = flood_fill_mask(&buf, 2, 2, 5, 5, 0);
        assert_eq!(mask, vec![0; 4]);
    }

    #[test]
    fn test_large_uniform_image() {
        // Test dla obrazu 256x256
        let buf = vec![Rgba8::new(100, 150, 200, 255); 256 * 256];
        let mask = flood_fill_mask(&buf, 256, 256, 0, 0, 0);
        assert_eq!(mask, vec![255; 256 * 256]);
    }

    #[test]
    fn test_apply_fill_full() {
        let mut layer = Layer::new("t", 8, 8);
        let mask = vec![255u8; 64];
        let r = apply_fill(&mut layer, &mask, Rgba8::new(255, 0, 0, 255), None);
        assert_eq!(
            r,
            Some(Rect {
                x: 0,
                y: 0,
                width: 8,
                height: 8
            })
        );
        assert_eq!(layer.get_pixel(5, 5), Some(Rgba8::new(255, 0, 0, 255)));
    }

    #[test]
    fn test_apply_fill_single() {
        let mut layer = Layer::new("t", 8, 8);
        let mut mask = vec![0u8; 64];
        mask[2 * 8 + 3] = 255;
        let r = apply_fill(&mut layer, &mask, Rgba8::new(255, 0, 0, 255), None);
        assert_eq!(
            r,
            Some(Rect {
                x: 3,
                y: 2,
                width: 1,
                height: 1
            })
        );
        assert_eq!(layer.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_apply_fill_bad_mask() {
        let mut layer = Layer::new("t", 8, 8);
        let mask = vec![255u8; 10];
        let r = apply_fill(&mut layer, &mask, Rgba8::new(255, 0, 0, 255), None);
        assert_eq!(r, None);
    }

    #[test]
    fn test_mask_alpha() {
        let mut layer = Layer::new("t", 4, 4);
        // Fill layer with red pixels
        for y in 0..4 {
            for x in 0..4 {
                let _ = layer.set_pixel(x, y, Rgba8::new(255, 0, 0, 255));
            }
        }

        // Test with all zeros mask - should make all pixels transparent
        let mask = vec![0u8; 16];
        let r = mask_alpha(&mut layer, &mask);
        assert_eq!(
            r,
            Some(Rect {
                x: 0,
                y: 0,
                width: 4,
                height: 4
            })
        );

        // Test with all 255 mask - should return None (no changes)
        let mask = vec![255u8; 16];
        let r = mask_alpha(&mut layer, &mask);
        assert_eq!(r, None);
    }

    #[test]
    fn test_border_background_mask() {
        // Create a 5x5 image with white background and black pixel in center
        let mut buf = vec![Rgba8::new(255, 255, 255, 255); 25];
        buf[12] = Rgba8::new(0, 0, 0, 255); // Center pixel (2,2)

        let mask = border_background_mask(&buf, 5, 5, 10);

        // All pixels should be 255 except the center pixel which should be 0
        for (i, &m) in mask.iter().enumerate() {
            if i == 12 {
                assert_eq!(m, 0);
            } else {
                assert_eq!(m, 255);
            }
        }
    }
}
