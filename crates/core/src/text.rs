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

/// Wyrównanie poziome tekstu w ramce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Wyrównanie do lewej.
    #[default]
    Left,
    /// Wyśrodkowanie.
    Center,
    /// Wyrównanie do prawej.
    Right,
}

/// Zwraca pozycje poziome (x w px) kursora dla każdej linii tekstu (rozdzielonej `\n`).
/// Dla linii o `N` znakach wektor ma `N + 1` elementów (od `0.0` przed pierwszym znakiem).
pub fn caret_positions(font_data: &[u8], text: &str, size_px: f32) -> Vec<Vec<f32>> {
    if size_px <= 0.0 {
        return Vec::new();
    }
    let Ok(font) = FontRef::try_from_slice(font_data) else {
        return Vec::new();
    };
    let scale = PxScale::from(size_px);
    let sf = font.as_scaled(scale);

    text.split('\n')
        .map(|line| {
            let mut positions = Vec::with_capacity(line.chars().count() + 1);
            let mut x = 0.0f32;
            positions.push(0.0);
            let mut prev = None;
            for c in line.chars() {
                let id = font.glyph_id(c);
                if let Some(p) = prev {
                    x += sf.kern(p, id);
                }
                x += sf.h_advance(id);
                prev = Some(id);
                positions.push(x);
            }
            positions
        })
        .collect()
}

/// Zwraca domyślną wysokość linii dla podanej czcionki i rozmiaru `size_px`.
pub fn font_line_height(font_data: &[u8], size_px: f32) -> f32 {
    if size_px <= 0.0 {
        return 0.0;
    }
    let Ok(font) = FontRef::try_from_slice(font_data) else {
        return 0.0;
    };
    let sf = font.as_scaled(PxScale::from(size_px));
    sf.ascent() - sf.descent() + sf.line_gap()
}

/// Rasteryzuje `text` (linie rozdzielone `\n`) czcionką z bajtów `font_data` o wysokości `size_px`
/// z uwzględnieniem wyrównania `align`, dodatkowego odstępu linii `line_spacing` i minimalnej szerokości `min_width`.
pub fn rasterize_text_with_options(
    font_data: &[u8],
    text: &str,
    size_px: f32,
    align: TextAlign,
    line_spacing: f32,
    min_width: f32,
) -> Option<TextMask> {
    if text.is_empty() || size_px <= 0.0 {
        return None;
    }

    let font = FontRef::try_from_slice(font_data).ok()?;
    let scale = PxScale::from(size_px);
    let sf = font.as_scaled(scale);

    let base_line_h = sf.ascent() - sf.descent() + sf.line_gap();
    let step_line_h = (base_line_h + line_spacing).max(1.0);

    let carets = caret_positions(font_data, text, size_px);
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.is_empty() {
        return None;
    }

    let max_line_w = carets
        .iter()
        .filter_map(|c| c.last().copied())
        .fold(0.0f32, f32::max);
    let box_w = max_line_w.max(min_width.max(0.0));

    let mut outlined = Vec::new();
    let total_height = if lines.len() <= 1 {
        base_line_h
    } else {
        (lines.len() - 1) as f32 * step_line_h + base_line_h
    };

    for (li, line) in lines.iter().enumerate() {
        let line_w = carets
            .get(li)
            .and_then(|c| c.last().copied())
            .unwrap_or(0.0);
        let offset_x = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => ((box_w - line_w) * 0.5).max(0.0),
            TextAlign::Right => (box_w - line_w).max(0.0),
        };

        let mut x = offset_x;
        let base_y = sf.ascent() + li as f32 * step_line_h;
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
    }

    let effective_w = match align {
        TextAlign::Left => max_line_w,
        TextAlign::Center | TextAlign::Right => box_w,
    };
    let width = effective_w.ceil().max(1.0) as u32;
    let height = total_height.ceil().max(1.0) as u32;

    let mut data = vec![0u8; (width * height) as usize];

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

/// Rasteryzuje `text` (linie rozdzielone \n) czcionką z bajtów `font_data` o wysokości `size_px`. None gdy czcionka jest zła, tekst pusty lub size_px <= 0.
pub fn rasterize_text(font_data: &[u8], text: &str, size_px: f32) -> Option<TextMask> {
    rasterize_text_with_options(font_data, text, size_px, TextAlign::Left, 0.0, 0.0)
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

    #[test]
    fn test_caret_positions_and_options() {
        assert!(caret_positions(b"bad", "Ab", 24.0).is_empty());
        assert_eq!(font_line_height(b"bad", 24.0), 0.0);

        let Some(f) = font() else {
            return;
        };

        let pos = caret_positions(&f, "Ab\nC\n", 32.0);
        assert_eq!(pos.len(), 3);
        assert_eq!(pos[0].len(), 3);
        assert_eq!(pos[0][0], 0.0);
        assert!(pos[0][1] > 0.0);
        assert!(pos[0][2] > pos[0][1]);
        assert_eq!(pos[1].len(), 2);
        assert_eq!(pos[1][0], 0.0);
        assert!(pos[1][1] > 0.0);
        assert_eq!(pos[2], vec![0.0]);

        let lh = font_line_height(&f, 32.0);
        assert!(lh >= 32.0);

        let m_right =
            rasterize_text_with_options(&f, "A", 32.0, TextAlign::Right, 10.0, 120.0).unwrap();
        assert_eq!(m_right.width, 120);
    }
}
