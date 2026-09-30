//! Rasteryzacja tekstu (narzędzie Tekst) czcionką TrueType/OpenType przez ab_glyph.
use crate::document::Rect;
use crate::layer::Layer;
use crate::pixel::Rgba8;
use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};

/// Maska tekstu: pokrycie 0..=255, wymiary width x height.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMask {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Rasteryzuje `text` (linie rozdzielone \n) czcionką z bajtów `font_data` o wysokości `size_px`. None gdy czcionka jest zła, tekst pusty lub size_px <= 0.
pub fn rasterize_text(font_data: &[u8], text: &str, size_px: f32) -> Option<TextMask> {
    if text.is_empty() || size_px <= 0.0 {
        return None;
    }

    let font = FontRef::try_from_slice(font_data).ok()?;
    let scale = PxScale::from(size_px);
    let sf = font.as_scaled(scale);

    let line_h = sf.ascent() - sf.descent() + sf.line_gap();

    // Collect glyphs
    let mut outlined = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut max_x = 0.0f32;
    let total_height = lines.len() as f32 * line_h;

    for (li, line) in lines.iter().enumerate() {
        let mut x = 0.0f32;
        let base_y = sf.ascent() + li as f32 * line_h;
        let mut prev = None;

        for c in line.chars() {
            let id = font.glyph_id(c);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }

            let glyph = id.with_scale_and_position(scale, point(x, base_y));
            x += sf.h_advance(id);
            prev = Some(id);

            if let Some(og) = font.outline_glyph(glyph) {
                outlined.push(og);
            }
        }

        max_x = max_x.max(x);
    }

    // Calculate dimensions
    let width = max_x.ceil().max(1.0) as u32;
    let height = total_height.ceil().max(1.0) as u32;

    let mut data = vec![0u8; (width * height) as usize];

    // Draw glyphs
    for og in outlined {
        let b = og.px_bounds();
        og.draw(|gx, gy, cov| {
            let px = b.min.x as i32 + gx as i32;
            let py = b.min.y as i32 + gy as i32;

            if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                let i = (py as u32 * width + px as u32) as usize;
                let v = (cov * 255.0).round().clamp(0.0, 255.0) as u8;
                data[i] = data[i].max(v);
            }
        });
    }

    Some(TextMask {
        width,
        height,
        data,
    })
}

/// Nanosi maskę na warstwę w pozycji (x, y) kolorem `color` (premultiplied), mieszając normalnie. Zwraca prostokąt zmian.
pub fn stamp_mask(
    layer: &mut Layer,
    mask: &TextMask,
    x: i32,
    y: i32,
    color: Rgba8,
) -> Option<Rect> {
    let mut x0 = i32::MAX;
    let mut y0 = i32::MAX;
    let mut x1 = i32::MIN;
    let mut y1 = i32::MIN;

    let layer_width = layer.pixels.width as i32;
    let layer_height = layer.pixels.height as i32;

    for my in 0..mask.height {
        for mx in 0..mask.width {
            let m = mask.data[(my * mask.width + mx) as usize];
            if m > 0 {
                let tx = x + mx as i32;
                let ty = y + my as i32;

                // Skip if outside layer bounds
                if tx < 0 || ty < 0 || tx >= layer_width || ty >= layer_height {
                    continue;
                }

                // Update bounding box
                x0 = x0.min(tx);
                y0 = y0.min(ty);
                x1 = x1.max(tx);
                y1 = y1.max(ty);

                let src = crate::blend::apply_opacity_u8(color, m as u32);
                let dst = layer
                    .get_pixel(tx as u32, ty as u32)
                    .unwrap_or(Rgba8::TRANSPARENT);
                let _ = layer.set_pixel(tx as u32, ty as u32, crate::blend::blend_normal(dst, src));
            }
        }
    }

    if x0 != i32::MAX {
        Some(Rect::new(
            x0 as u32,
            y0 as u32,
            (x1 - x0 + 1) as u32,
            (y1 - y0 + 1) as u32,
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &str = "/usr/share/fonts/noto/NotoSans-Regular.ttf";

    fn font() -> Option<Vec<u8>> {
        std::fs::read(FONT).ok()
    }

    #[test]
    fn test_rasterize_basic() {
        let f = font();
        if f.is_none() {
            return;
        }
        let f = f.unwrap();

        let mask = rasterize_text(&f, "Ab", 32.0);
        assert!(mask.is_some());
        let mask = mask.unwrap();
        assert!(mask.width > 10);
        assert!(mask.height >= 32);

        // Check that some pixel has high coverage
        let max_coverage = mask.data.iter().max().unwrap();
        assert!(*max_coverage > 200);
    }

    #[test]
    fn test_multiline() {
        let f = font();
        if f.is_none() {
            return;
        }
        let f = f.unwrap();

        let mask1 = rasterize_text(&f, "A", 32.0).unwrap();
        let mask2 = rasterize_text(&f, "A\nB", 32.0).unwrap();
        assert!(mask2.height > mask1.height);
    }

    #[test]
    fn test_invalid() {
        // Invalid font data
        assert_eq!(rasterize_text(b"xyz", "A", 20.0), None);

        // Empty text
        let f = font();
        if f.is_none() {
            return;
        }
        let f = f.unwrap();
        assert_eq!(rasterize_text(&f, "", 20.0), None);
    }

    #[test]
    fn test_stamp() {
        let f = font();
        if f.is_none() {
            return;
        }
        let f = f.unwrap();

        let mut layer = Layer::new("t", 100, 60);
        let mask = rasterize_text(&f, "A", 40.0).unwrap();
        let rect = stamp_mask(&mut layer, &mask, 5, 5, Rgba8::new(255, 0, 0, 255));

        assert!(rect.is_some());

        // Check that at least one pixel has high alpha
        let r = rect.unwrap();
        let any_opaque = (r.y..r.y + r.height)
            .flat_map(|y| (r.x..r.x + r.width).map(move |x| (x, y)))
            .any(|(x, y)| layer.get_pixel(x, y).is_some_and(|p| p.a() > 200));
        assert!(any_opaque);
    }
}
