//! Konwersje RGB <-> HSL (wartości nie-premultiplied).

/// Konwertuje kolor RGB do HSL.
///
/// # Arguments
/// * `rgb` - tablica 3 wartości u8 reprezentujących kanały R, G, B
///
/// # Returns
/// Tablica 3 wartości f32 reprezentujących H, S, L
/// * H (hue): kąt w stopniach 0.0..360.0
/// * S (saturation): nasycenie 0.0..=1.0
/// * L (lightness): jasność 0.0..=1.0
pub fn rgb_to_hsl(rgb: [u8; 3]) -> [f32; 3] {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    let s = if max == min {
        0.0
    } else {
        (max - min) / (1.0 - (2.0 * l - 1.0).abs())
    };

    let h = if max == min {
        0.0
    } else if max == r {
        ((g - b) / (max - min)).rem_euclid(6.0)
    } else if max == g {
        (b - r) / (max - min) + 2.0
    } else {
        (r - g) / (max - min) + 4.0
    };

    [h * 60.0, s, l]
}

/// Konwertuje kolor HSL do RGB.
///
/// # Arguments
/// * `hsl` - tablica 3 wartości f32 reprezentujących H, S, L
///
/// # Returns
/// Tablica 3 wartości u8 reprezentujących kanały R, G, B
pub fn hsl_to_rgb(hsl: [f32; 3]) -> [u8; 3] {
    let h = hsl[0].rem_euclid(360.0);
    let s = hsl[1].clamp(0.0, 1.0);
    let l = hsl[2].clamp(0.0, 1.0);

    // Funkcja pomocnicza do konwersji Hue do RGB
    let hue_to_rgb = |p: f32, q: f32, t: f32| -> f32 {
        let t = if t < 0.0 {
            t + 1.0
        } else if t > 1.0 {
            t - 1.0
        } else {
            t
        };

        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };

    let p = 2.0 * l - q;

    let r = hue_to_rgb(p, q, h / 360.0 + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h / 360.0);
    let b = hue_to_rgb(p, q, h / 360.0 - 1.0 / 3.0);

    [
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_hsl_red() {
        let rgb = [255, 0, 0];
        let hsl = rgb_to_hsl(rgb);
        assert_eq!(hsl[0], 0.0); // H
        assert_eq!(hsl[1], 1.0); // S
        assert_eq!(hsl[2], 0.5); // L
    }

    #[test]
    fn test_rgb_to_hsl_green() {
        let rgb = [0, 255, 0];
        let hsl = rgb_to_hsl(rgb);
        assert_eq!(hsl[0], 120.0); // H
        assert_eq!(hsl[1], 1.0); // S
        assert_eq!(hsl[2], 0.5); // L
    }

    #[test]
    fn test_rgb_to_hsl_gray() {
        let rgb = [128, 128, 128];
        let hsl = rgb_to_hsl(rgb);
        assert_eq!(hsl[1], 0.0); // S
    }

    #[test]
    fn test_round_trip() {
        let test_colors = [
            [255, 0, 0],
            [18, 52, 86],
            [200, 150, 100],
            [0, 0, 0],
            [255, 255, 255],
        ];

        for rgb in test_colors.iter() {
            let hsl = rgb_to_hsl(*rgb);
            let result_rgb = hsl_to_rgb(hsl);

            // Dopuszczalna różnica 1 na kanał
            assert!((result_rgb[0] as i16 - rgb[0] as i16).abs() <= 1);
            assert!((result_rgb[1] as i16 - rgb[1] as i16).abs() <= 1);
            assert!((result_rgb[2] as i16 - rgb[2] as i16).abs() <= 1);
        }
    }

    #[test]
    fn test_hue_wrapping() {
        let hsl1 = [480.0, 1.0, 0.5];
        let hsl2 = [120.0, 1.0, 0.5];

        let rgb1 = hsl_to_rgb(hsl1);
        let rgb2 = hsl_to_rgb(hsl2);

        assert_eq!(rgb1, rgb2);
    }
}
