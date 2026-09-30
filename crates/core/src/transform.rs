//! Przekształcenia warstw i buforów.

use crate::layer::TiledLayer;
use crate::pixel::Rgba8;

/// Odbija warstwę w pionie.
/// Piksel (x, y) trafia na (w-1-x, y).
pub fn flip_horizontal(src: &TiledLayer) -> TiledLayer {
    let mut dst = TiledLayer::new(src.width, src.height);

    for y in 0..src.height {
        for x in 0..src.width {
            if let Some(px) = src.get_pixel(x, y) {
                if px.a() != 0 {
                    let new_x = src.width - 1 - x;
                    let _ = dst.set_pixel(new_x, y, px);
                }
            }
        }
    }

    dst
}

/// Odbija warstwę w poziomie.
/// Piksel (x, y) trafia na (x, h-1-y).
pub fn flip_vertical(src: &TiledLayer) -> TiledLayer {
    let mut dst = TiledLayer::new(src.width, src.height);

    for y in 0..src.height {
        for x in 0..src.width {
            if let Some(px) = src.get_pixel(x, y) {
                if px.a() != 0 {
                    let new_y = src.height - 1 - y;
                    let _ = dst.set_pixel(x, new_y, px);
                }
            }
        }
    }

    dst
}

/// Obraca warstwę o 90 stopni zgodnie z ruchem wskazówek zegara.
/// Wynik ma wymiary (h, w).
/// Piksel (x, y) trafia na (h-1-y, x).
pub fn rotate90_cw(src: &TiledLayer) -> TiledLayer {
    let mut dst = TiledLayer::new(src.height, src.width);

    for y in 0..src.height {
        for x in 0..src.width {
            if let Some(px) = src.get_pixel(x, y) {
                if px.a() != 0 {
                    let new_x = src.height - 1 - y;
                    let new_y = x;
                    let _ = dst.set_pixel(new_x, new_y, px);
                }
            }
        }
    }

    dst
}

/// Obraca warstwę o 180 stopni.
/// Piksel (x, y) trafia na (w-1-x, h-1-y).
pub fn rotate180(src: &TiledLayer) -> TiledLayer {
    let mut dst = TiledLayer::new(src.width, src.height);

    for y in 0..src.height {
        for x in 0..src.width {
            if let Some(px) = src.get_pixel(x, y) {
                if px.a() != 0 {
                    let new_x = src.width - 1 - x;
                    let new_y = src.height - 1 - y;
                    let _ = dst.set_pixel(new_x, new_y, px);
                }
            }
        }
    }

    dst
}

/// Skalowanie bufora premultiplied (sw x sh) do (dw x dh) interpolacją dwuliniową.
pub fn resample_bilinear(src: &[Rgba8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<Rgba8> {
    if src.len() != sw * sh || sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return vec![Rgba8::TRANSPARENT; dw * dh];
    }

    let mut result = Vec::with_capacity(dw * dh);

    for y in 0..dh {
        for x in 0..dw {
            let fx = ((x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let fy = ((y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5).clamp(0.0, (sh - 1) as f32);

            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let y0 = fy.floor() as usize;
            let y1 = (y0 + 1).min(sh - 1);

            let tx = fx - x0 as f32;
            let ty = fy - y0 as f32;

            let src00 = src[y0 * sw + x0];
            let src10 = src[y0 * sw + x1];
            let src01 = src[y1 * sw + x0];
            let src11 = src[y1 * sw + x1];

            let r = src00.r() as f32 * (1.0 - tx) * (1.0 - ty)
                + src10.r() as f32 * tx * (1.0 - ty)
                + src01.r() as f32 * (1.0 - tx) * ty
                + src11.r() as f32 * tx * ty;
            let g = src00.g() as f32 * (1.0 - tx) * (1.0 - ty)
                + src10.g() as f32 * tx * (1.0 - ty)
                + src01.g() as f32 * (1.0 - tx) * ty
                + src11.g() as f32 * tx * ty;
            let b = src00.b() as f32 * (1.0 - tx) * (1.0 - ty)
                + src10.b() as f32 * tx * (1.0 - ty)
                + src01.b() as f32 * (1.0 - tx) * ty
                + src11.b() as f32 * tx * ty;
            let a = src00.a() as f32 * (1.0 - tx) * (1.0 - ty)
                + src10.a() as f32 * tx * (1.0 - ty)
                + src01.a() as f32 * (1.0 - tx) * ty
                + src11.a() as f32 * tx * ty;

            result.push(Rgba8::new(
                r.round().clamp(0.0, 255.0) as u8,
                g.round().clamp(0.0, 255.0) as u8,
                b.round().clamp(0.0, 255.0) as u8,
                a.round().clamp(0.0, 255.0) as u8,
            ));
        }
    }

    result
}

/// Obrót bufora w x h o kąt `angle` (radiany, zgodnie z ruchem wskazówek zegara) wokół środka; wynik mieści cały obraz. Zwraca (bufor, szerokość, wysokość).
pub fn rotate_bilinear(
    src: &[Rgba8],
    w: usize,
    h: usize,
    angle: f32,
) -> (Vec<Rgba8>, usize, usize) {
    if src.len() != w * h || w == 0 || h == 0 {
        return (Vec::new(), 0, 0);
    }

    let (mut s, mut c) = angle.sin_cos();
    if s.abs() < 1e-5 {
        s = 0.0;
    }
    if c.abs() < 1e-5 {
        c = 0.0;
    }
    let nw = ((w as f32 * c.abs() + h as f32 * s.abs()).ceil() as usize).max(1);
    let nh = ((w as f32 * s.abs() + h as f32 * c.abs()).ceil() as usize).max(1);

    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let ncx = nw as f32 / 2.0;
    let ncy = nh as f32 / 2.0;

    let mut result = Vec::with_capacity(nw * nh);

    for y in 0..nh {
        for x in 0..nw {
            let dx = x as f32 + 0.5 - ncx;
            let dy = y as f32 + 0.5 - ncy;

            let sx = c * dx + s * dy + cx - 0.5;
            let sy = -s * dx + c * dy + cy - 0.5;

            if sx < -0.5 || sy < -0.5 || sx > w as f32 - 0.5 || sy > h as f32 - 0.5 {
                result.push(Rgba8::TRANSPARENT);
            } else {
                let x0 = sx.floor() as usize;
                let x1 = (x0 + 1).min(w - 1);
                let y0 = sy.floor() as usize;
                let y1 = (y0 + 1).min(h - 1);

                let tx = sx - x0 as f32;
                let ty = sy - y0 as f32;

                let src00 = src[y0 * w + x0];
                let src10 = src[y0 * w + x1];
                let src01 = src[y1 * w + x0];
                let src11 = src[y1 * w + x1];

                let r = src00.r() as f32 * (1.0 - tx) * (1.0 - ty)
                    + src10.r() as f32 * tx * (1.0 - ty)
                    + src01.r() as f32 * (1.0 - tx) * ty
                    + src11.r() as f32 * tx * ty;
                let g = src00.g() as f32 * (1.0 - tx) * (1.0 - ty)
                    + src10.g() as f32 * tx * (1.0 - ty)
                    + src01.g() as f32 * (1.0 - tx) * ty
                    + src11.g() as f32 * tx * ty;
                let b = src00.b() as f32 * (1.0 - tx) * (1.0 - ty)
                    + src10.b() as f32 * tx * (1.0 - ty)
                    + src01.b() as f32 * (1.0 - tx) * ty
                    + src11.b() as f32 * tx * ty;
                let a = src00.a() as f32 * (1.0 - tx) * (1.0 - ty)
                    + src10.a() as f32 * tx * (1.0 - ty)
                    + src01.a() as f32 * (1.0 - tx) * ty
                    + src11.a() as f32 * tx * ty;

                result.push(Rgba8::new(
                    r.round().clamp(0.0, 255.0) as u8,
                    g.round().clamp(0.0, 255.0) as u8,
                    b.round().clamp(0.0, 255.0) as u8,
                    a.round().clamp(0.0, 255.0) as u8,
                ));
            }
        }
    }

    (result, nw, nh)
}

/// Obraca warstwę o 90 stopni przeciwnie do ruchu wskazówek zegara (w lewo).
/// Wynik ma wymiary (h, w).
/// Piksel (x, y) trafia na (y, w - 1 - x).
pub fn rotate90_ccw(src: &TiledLayer) -> TiledLayer {
    let mut dst = TiledLayer::new(src.height, src.width);

    for y in 0..src.height {
        for x in 0..src.width {
            if let Some(px) = src.get_pixel(x, y) {
                if px.a() != 0 {
                    let new_x = y;
                    let new_y = src.width - 1 - x;
                    let _ = dst.set_pixel(new_x, new_y, px);
                }
            }
        }
    }

    dst
}

/// Obraca warstwę o dowolny kąt `angle_rad` (w radianach, zgodnie z ruchem wskazówek zegara)
/// wokół jej środka z interpolacją dwuliniową, zachowując rozmiary warstwy.
pub fn rotate_layer_centered(src: &TiledLayer, angle_rad: f32) -> TiledLayer {
    let mut dst = TiledLayer::new(src.width, src.height);
    if src.width == 0 || src.height == 0 {
        return dst;
    }
    if angle_rad.abs() < 1e-6 {
        return src.clone();
    }

    let src_buf = src.to_vec();
    let (rotated, rw, rh) =
        rotate_bilinear(&src_buf, src.width as usize, src.height as usize, angle_rad);
    if rw == 0 || rh == 0 {
        return dst;
    }

    let off_x = (src.width as i64 - rw as i64) / 2;
    let off_y = (src.height as i64 - rh as i64) / 2;

    for ry in 0..rh {
        let ny = ry as i64 + off_y;
        if ny < 0 || ny >= src.height as i64 {
            continue;
        }
        let row = ry * rw;
        for rx in 0..rw {
            let px = rotated[row + rx];
            if px.a() == 0 {
                continue;
            }
            let nx = rx as i64 + off_x;
            if nx >= 0 && nx < src.width as i64 {
                let _ = dst.set_pixel(nx as u32, ny as u32, px);
            }
        }
    }

    dst
}

/// Skaluje zawartość warstwy do rozmiaru (dw x dh) z interpolacją dwuliniową,
/// umieszczając wynik wycentrowany na warstwie o oryginalnych wymiarach (src.width, src.height).
pub fn scale_layer_centered(src: &TiledLayer, dw: u32, dh: u32) -> TiledLayer {
    let mut dst = TiledLayer::new(src.width, src.height);
    if src.width == 0 || src.height == 0 || dw == 0 || dh == 0 {
        return dst;
    }

    let src_buf = src.to_vec();
    let resampled = resample_bilinear(
        &src_buf,
        src.width as usize,
        src.height as usize,
        dw as usize,
        dh as usize,
    );

    let off_x = (src.width as i64 - dw as i64) / 2;
    let off_y = (src.height as i64 - dh as i64) / 2;

    for y in 0..dh {
        let ny = y as i64 + off_y;
        if ny < 0 || ny >= src.height as i64 {
            continue;
        }
        let row = (y * dw) as usize;
        for x in 0..dw {
            let px = resampled[row + x as usize];
            if px.a() == 0 {
                continue;
            }
            let nx = x as i64 + off_x;
            if nx >= 0 && nx < src.width as i64 {
                let _ = dst.set_pixel(nx as u32, ny as u32, px);
            }
        }
    }

    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flip_horizontal() {
        let mut src = TiledLayer::new(3, 2);
        let red = Rgba8::new(255, 0, 0, 255);
        src.set_pixel(0, 0, red).unwrap();

        let dst = flip_horizontal(&src);
        assert_eq!(dst.width, 3);
        assert_eq!(dst.height, 2);
        assert_eq!(dst.get_pixel(2, 0), Some(red));
        assert_eq!(dst.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_flip_vertical() {
        let mut src = TiledLayer::new(3, 2);
        let red = Rgba8::new(255, 0, 0, 255);
        src.set_pixel(0, 0, red).unwrap();

        let dst = flip_vertical(&src);
        assert_eq!(dst.width, 3);
        assert_eq!(dst.height, 2);
        assert_eq!(dst.get_pixel(0, 1), Some(red));
        assert_eq!(dst.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_rotate90_cw() {
        let mut src = TiledLayer::new(3, 2);
        let red = Rgba8::new(255, 0, 0, 255);
        src.set_pixel(0, 0, red).unwrap();

        let dst = rotate90_cw(&src);
        assert_eq!(dst.width, 2);
        assert_eq!(dst.height, 3);
        assert_eq!(dst.get_pixel(1, 0), Some(red));
        assert_eq!(dst.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_rotate90_ccw() {
        let mut src = TiledLayer::new(3, 2);
        let red = Rgba8::new(255, 0, 0, 255);
        src.set_pixel(0, 0, red).unwrap();

        let dst = rotate90_ccw(&src);
        assert_eq!(dst.width, 2);
        assert_eq!(dst.height, 3);
        assert_eq!(dst.get_pixel(0, 2), Some(red));
        assert_eq!(dst.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));

        // Obrót w prawo i potem w lewo przywraca oryginał
        let cw = rotate90_cw(&src);
        let restored = rotate90_ccw(&cw);
        assert_eq!(restored.get_pixel(0, 0), Some(red));
    }

    #[test]
    fn test_rotate_layer_centered() {
        let mut src = TiledLayer::new(10, 10);
        let red = Rgba8::new(255, 0, 0, 255);
        src.set_pixel(5, 5, red).unwrap();

        // Obrót wokół środka o pi (180 st)
        let rotated = rotate_layer_centered(&src, std::f32::consts::PI);
        assert_eq!(rotated.width, 10);
        assert_eq!(rotated.height, 10);
        // Dla 10x10 obrót o 180 st wokół środka (5.0, 5.0) przenosi piksel (5, 5) na (4, 4)
        let px = rotated.get_pixel(4, 4).unwrap();
        assert!(px.a() > 100);
    }

    #[test]
    fn test_scale_layer_centered() {
        let mut src = TiledLayer::new(10, 10);
        let red = Rgba8::new(255, 0, 0, 255);
        for y in 0..10 {
            for x in 0..10 {
                src.set_pixel(x, y, red).unwrap();
            }
        }

        // Skalowanie do 4x4 (wyśrodkowane w 10x10)
        let scaled = scale_layer_centered(&src, 4, 4);
        assert_eq!(scaled.width, 10);
        assert_eq!(scaled.height, 10);
        // Środek (5, 5) powinien być czerwony
        assert_eq!(scaled.get_pixel(5, 5), Some(red));
        // Rogi (0, 0) powinny być przezroczyste
        assert_eq!(scaled.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_rotate180() {
        let mut src = TiledLayer::new(3, 2);
        let red = Rgba8::new(255, 0, 0, 255);
        src.set_pixel(0, 0, red).unwrap();

        let dst = rotate180(&src);
        assert_eq!(dst.width, 3);
        assert_eq!(dst.height, 2);
        assert_eq!(dst.get_pixel(2, 1), Some(red));
        assert_eq!(dst.get_pixel(0, 0), Some(Rgba8::TRANSPARENT));
    }

    #[test]
    fn test_rotate180_twice() {
        let mut src = TiledLayer::new(3, 2);
        let red = Rgba8::new(255, 0, 0, 255);
        src.set_pixel(0, 0, red).unwrap();

        let dst1 = rotate180(&src);
        let dst2 = rotate180(&dst1);
        assert_eq!(dst2.get_pixel(0, 0), Some(red));
    }
}
