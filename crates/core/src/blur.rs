//! Rozmycie separowalne.

use crate::pixel::Rgba8;

/// Jedno przejście splotu 1D (poziome lub pionowe) bufora `w`x`h` w premultiplied alpha.
/// Piksele spoza obrazu = najbliższy piksel brzegowy (clamp).
pub fn blur_pass(
    src: &[Rgba8],
    dst: &mut [Rgba8],
    w: usize,
    h: usize,
    kernel: &[f32],
    horizontal: bool,
) {
    // Sprawdzenie warunków wejściowych
    if src.len() != w * h || dst.len() != w * h || kernel.is_empty() || w == 0 || h == 0 {
        return;
    }

    let radius = kernel.len() / 2;

    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];

            for (k, &kernel_value) in kernel.iter().enumerate() {
                let off = k as isize - radius as isize;

                let sample_x = if horizontal {
                    x as isize + off
                } else {
                    x as isize
                };

                let sample_y = if horizontal {
                    y as isize
                } else {
                    y as isize + off
                };

                // Clamp coordinates to image boundaries
                let clamped_x = sample_x.max(0).min((w - 1) as isize) as usize;
                let clamped_y = sample_y.max(0).min((h - 1) as isize) as usize;

                let src_idx = clamped_y * w + clamped_x;
                let pixel = src[src_idx];

                acc[0] += pixel.r() as f32 * kernel_value;
                acc[1] += pixel.g() as f32 * kernel_value;
                acc[2] += pixel.b() as f32 * kernel_value;
                acc[3] += pixel.a() as f32 * kernel_value;
            }

            let dst_idx = y * w + x;
            dst[dst_idx] = Rgba8::new(
                acc[0].round().clamp(0.0, 255.0) as u8,
                acc[1].round().clamp(0.0, 255.0) as u8,
                acc[2].round().clamp(0.0, 255.0) as u8,
                acc[3].round().clamp(0.0, 255.0) as u8,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blur_pass_uniform_color() {
        // Test dla jednolitego koloru po przejściu poziomym i pionowym
        let w = 5;
        let h = 5;
        let src = vec![Rgba8::new(100, 150, 200, 255); w * h];
        let mut dst = vec![Rgba8::default(); w * h];
        let kernel = [0.25, 0.5, 0.25];

        // Poziome przejście
        blur_pass(&src, &mut dst, w, h, &kernel, true);
        assert_eq!(src, dst);

        // Pionowe przejście
        blur_pass(&src, &mut dst, w, h, &kernel, false);
        assert_eq!(src, dst);
    }

    #[test]
    fn test_blur_pass_single_pixel() {
        // Test dla pojedynczego białego piksela w środku czarnego obrazu 5x1
        let w = 5;
        let h = 1;
        let mut src = vec![Rgba8::new(0, 0, 0, 0); w * h];
        src[2] = Rgba8::new(255, 255, 255, 255); // Biały piksel w środku
        let mut dst = vec![Rgba8::default(); w * h];
        let kernel = [0.25, 0.5, 0.25];

        blur_pass(&src, &mut dst, w, h, &kernel, true);

        // Sprawdzenie wyników
        assert_eq!(dst[0], Rgba8::new(0, 0, 0, 0)); // Brak zmian na krawędziach
        assert_eq!(dst[1], Rgba8::new(64, 64, 64, 64)); // Alfa 64 w x=1
        assert_eq!(dst[2], Rgba8::new(128, 128, 128, 128)); // Alfa 128 w x=2 (środkowy)
        assert_eq!(dst[3], Rgba8::new(64, 64, 64, 64)); // Alfa 64 w x=3
        assert_eq!(dst[4], Rgba8::new(0, 0, 0, 0)); // Brak zmian na krawędziach
    }

    #[test]
    fn test_blur_pass_identity_kernel() {
        // Test dla jądra [1.0] które kopiuje src do dst
        let w = 3;
        let h = 3;
        let src = vec![Rgba8::new(100, 150, 200, 255); w * h];
        let mut dst = vec![Rgba8::default(); w * h];
        let kernel = [1.0];

        blur_pass(&src, &mut dst, w, h, &kernel, true);
        assert_eq!(src, dst);
    }

    #[test]
    fn test_blur_pass_invalid_dimensions() {
        // Test dla błędnych długości buforów - nie powinno panikować
        let src = vec![Rgba8::new(100, 150, 200, 255); 9];
        let mut dst = vec![Rgba8::default(); 9];

        // Próba wywołania z niezgodnymi wymiarami
        blur_pass(&src, &mut dst, 3, 4, &[0.25, 0.5, 0.25], true);
        // Nie powinno paniknąć - funkcja powinna po prostu zwrócić bez zmian

        // Próba wywołania z pustym kernel
        blur_pass(&src, &mut dst, 3, 3, &[], true);
        // Nie powinno paniknąć - funkcja powinna po prostu zwrócić bez zmian
    }
}
