//! Miniatury warstw.
use crate::layer::TiledLayer;
use crate::pixel::Rgba8;

/// Rozmiar miniatury mieszczącej się w kwadracie max x max z zachowaniem proporcji (każdy bok >= 1).
pub fn thumbnail_size(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w == 0 || h == 0 || max == 0 {
        return (0, 0);
    }

    if w >= h {
        let tw = max.min(w);
        let th = ((h as u64 * tw as u64) / w as u64).max(1) as u32;
        (tw, th)
    } else {
        let th = max.min(h);
        let tw = ((w as u64 * th as u64) / h as u64).max(1) as u32;
        (tw, th)
    }
}

/// Miniatura warstwy do bufora `out` (czyszczonego i wypełnianego tw*th pikselami). Każdy piksel = średnia z siatki 4x4 próbek w odpowiadającym mu prostokącie źródła.
pub fn layer_thumbnail(layer: &TiledLayer, max: u32, out: &mut Vec<Rgba8>) -> (u32, u32) {
    let (tw, th) = thumbnail_size(layer.width, layer.height, max);
    out.clear();

    if tw == 0 {
        return (0, 0);
    }

    // Precompute the size of each source rectangle
    let bw = layer.width as f32 / tw as f32;
    let bh = layer.height as f32 / th as f32;

    for ty in 0..th {
        for tx in 0..tw {
            let x0 = tx as f32 * layer.width as f32 / tw as f32;
            let y0 = ty as f32 * layer.height as f32 / th as f32;

            // Sample 4x4 grid and compute average
            let mut r_sum = 0u32;
            let mut g_sum = 0u32;
            let mut b_sum = 0u32;
            let mut a_sum = 0u32;

            for sy in 0..4 {
                for sx in 0..4 {
                    // Calculate sample position
                    let px = (x0 + (sx as f32 + 0.5) * bw / 4.0).round() as u32;
                    let py = (y0 + (sy as f32 + 0.5) * bh / 4.0).round() as u32;

                    // Clamp to valid range
                    let px = px.min(layer.width - 1);
                    let py = py.min(layer.height - 1);

                    // Get pixel and accumulate channels
                    let pixel = layer.get_pixel(px, py).unwrap_or(Rgba8::TRANSPARENT);
                    r_sum += pixel.r() as u32;
                    g_sum += pixel.g() as u32;
                    b_sum += pixel.b() as u32;
                    a_sum += pixel.a() as u32;
                }
            }

            // Compute average and round properly
            let r = ((r_sum + 8) / 16).min(255) as u8;
            let g = ((g_sum + 8) / 16).min(255) as u8;
            let b = ((b_sum + 8) / 16).min(255) as u8;
            let a = ((a_sum + 8) / 16).min(255) as u8;

            out.push(Rgba8::new(r, g, b, a));
        }
    }

    (tw, th)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::TiledLayer;

    #[test]
    fn test_thumbnail_size() {
        assert_eq!(thumbnail_size(64, 32, 32), (32, 16));
        assert_eq!(thumbnail_size(10, 100, 20), (2, 20));
        assert_eq!(thumbnail_size(5, 5, 32), (5, 5));
        assert_eq!(thumbnail_size(0, 5, 32), (0, 0));
        assert_eq!(thumbnail_size(10, 0, 32), (0, 0));
        assert_eq!(thumbnail_size(10, 10, 0), (0, 0));
    }

    #[test]
    fn test_layer_thumbnail() {
        // Test with a filled layer
        let mut layer = TiledLayer::new(100, 50);
        for y in 0..50 {
            for x in 0..100 {
                layer.set_pixel(x, y, Rgba8::new(10, 20, 30, 255)).unwrap();
            }
        }

        let mut out = Vec::new();
        let (w, h) = layer_thumbnail(&layer, 20, &mut out);
        assert_eq!(w, 20);
        assert_eq!(h, 10);
        assert_eq!(out.len(), 200); // 20 * 10

        // All pixels should be the same color
        for pixel in &out {
            assert_eq!(pixel.r(), 10);
            assert_eq!(pixel.g(), 20);
            assert_eq!(pixel.b(), 30);
            assert_eq!(pixel.a(), 255);
        }

        // Test with empty layer
        let empty_layer = TiledLayer::new(100, 50);
        let mut out = Vec::new();
        let (w, h) = layer_thumbnail(&empty_layer, 20, &mut out);
        assert_eq!(w, 20);
        assert_eq!(h, 10);
        assert_eq!(out.len(), 200); // 20 * 10

        // All pixels should be transparent
        for pixel in &out {
            assert_eq!(*pixel, Rgba8::TRANSPARENT);
        }
    }
}
