//! Filtry tonalne.

/// Tablica LUT jasności/kontrastu dla kanału 0..=255 (wartości nie-premultiplied).
pub fn brightness_contrast_lut(brightness: i16, contrast: f32) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (v, out) in lut.iter_mut().enumerate() {
        let x = (v as f32 + brightness as f32 - 128.0) * contrast + 128.0;
        *out = x.round().clamp(0.0, 255.0) as u8;
    }
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brightness_contrast_lut_identity() {
        let lut = brightness_contrast_lut(0, 1.0);
        for (v, &out) in lut.iter().enumerate() {
            assert_eq!(out, v as u8);
        }
    }

    #[test]
    fn test_brightness_contrast_lut_brightness() {
        let lut = brightness_contrast_lut(50, 1.0);
        assert_eq!(lut[0], 50);
        assert_eq!(lut[250], 255);
    }

    #[test]
    fn test_brightness_contrast_lut_zero_contrast() {
        let lut = brightness_contrast_lut(0, 0.0);
        assert!(lut.iter().all(|&out| out == 128));
    }

    #[test]
    fn test_brightness_contrast_lut_double_contrast() {
        let lut = brightness_contrast_lut(0, 2.0);
        assert_eq!(lut[128], 128);
        assert_eq!(lut[0], 0);
        assert_eq!(lut[255], 255);
    }
}
