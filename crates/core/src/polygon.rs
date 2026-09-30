//! Maska wielokąta (zaznaczenie odręczne).
/// Maska w*h (0..=255) wnętrza wielokąta (reguła even-odd), wygładzana 4 próbkami w pionie na piksel.
pub fn polygon_mask(points: &[(f32, f32)], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];

    if points.len() < 3 || w == 0 || h == 0 {
        return out;
    }

    let n = points.len();
    let mut counts = vec![0u8; w];
    let mut xs: Vec<f32> = Vec::new();

    for y in 0..h {
        counts.fill(0);

        for k in 0..4 {
            let sy = y as f32 + (k as f32 + 0.5) / 4.0;
            xs.clear();

            for i in 0..n {
                let j = (i + 1) % n;
                let (xi, yi) = points[i];
                let (xj, yj) = points[j];

                if (yi <= sy && sy < yj) || (yj <= sy && sy < yi) {
                    let x = xi + (sy - yi) * (xj - xi) / (yj - yi);
                    xs.push(x);
                }
            }

            xs.sort_by(|a, b| a.total_cmp(b));

            for m in 0..(xs.len() / 2) {
                let x_start = xs[2 * m];
                let x_end = xs[2 * m + 1];

                for (px, count) in counts.iter_mut().enumerate() {
                    let cx = px as f32 + 0.5;
                    if x_start <= cx && cx < x_end {
                        *count += 1;
                    }
                }
            }
        }

        for px in 0..w {
            out[y * w + px] = (counts[px] as u32 * 255 / 4) as u8;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polygon_mask_square() {
        let points = [(1.0, 1.0), (5.0, 1.0), (5.0, 5.0), (1.0, 5.0)];
        let mask = polygon_mask(&points, 8, 8);

        // Test center pixel - should be fully inside
        assert_eq!(mask[3 * 8 + 3], 255);

        // Test corner pixels - should be outside
        assert_eq!(mask[0], 0);
        assert_eq!(mask[6 * 8 + 6], 0);
    }

    #[test]
    fn test_polygon_mask_triangle() {
        let points = [(0.0, 0.0), (8.0, 0.0), (0.0, 8.0)];
        let mask = polygon_mask(&points, 8, 8);

        // Sum should be between 28 and 36 (half of 64 pixels)
        let sum: u32 = mask.iter().map(|&x| x as u32).sum();
        assert!((28 * 255..=36 * 255).contains(&sum));
    }

    #[test]
    fn test_polygon_mask_insufficient_points() {
        // Less than 3 points
        let mask = polygon_mask(&[(0.0, 0.0), (1.0, 1.0)], 8, 8);
        assert_eq!(mask, vec![0u8; 64]);

        let mask = polygon_mask(&[], 8, 8);
        assert_eq!(mask, vec![0u8; 64]);
    }

    #[test]
    fn test_polygon_mask_zero_dimensions() {
        let mask = polygon_mask(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)], 0, 8);
        assert_eq!(mask, vec![0u8; 0]);

        let mask = polygon_mask(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)], 8, 0);
        assert_eq!(mask, vec![0u8; 0]);
    }
}
