//! Tryby mieszania warstw i operacje na kolorach z premultiplied alpha.

use crate::pixel::Rgba8;

/// Obsługiwane tryby mieszania warstw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlendMode {
    /// Tryb normalny (Porter-Duff "Over").
    #[default]
    Normal,
    /// Mnożenie
    Multiply,
    /// Ekran (Screen)
    Screen,
    /// Ciemniejszy (Darken)
    Darken,
    /// Jaśniejszy (Lighten)
    Lighten,
    /// Nakładka (Overlay)
    Overlay,
}

impl BlendMode {
    /// Wszystkie tryby w kolejności wyświetlania w UI.
    pub const ALL: [BlendMode; 6] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
    ];

    /// Polska nazwa trybu do wyświetlenia w UI.
    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normalny",
            BlendMode::Multiply => "Mnożenie",
            BlendMode::Screen => "Rozjaśnianie (Screen)",
            BlendMode::Overlay => "Nakładka",
            BlendMode::Darken => "Tylko ciemniejsze",
            BlendMode::Lighten => "Tylko jaśniejsze",
        }
    }
}

/// Szybkie dzielenie przez 255 z zaokrągleniem w arytmetyce całkowitoliczbowej: `((x + 128) * 257) >> 16`.
#[inline]
pub const fn div255_round(x: u32) -> u32 {
    ((x + 128) * 257) >> 16
}

/// Skaluje wszystkie 4 kanały koloru (RGBA premultiplied) przez współczynnik krycia w skali 0..=255.
#[inline]
pub fn apply_opacity_u8(pixel: Rgba8, opacity_u8: u32) -> Rgba8 {
    if opacity_u8 >= 255 {
        return pixel;
    }
    if opacity_u8 == 0 || pixel.is_transparent() {
        return Rgba8::TRANSPARENT;
    }

    let r = div255_round(pixel.r() as u32 * opacity_u8) as u8;
    let g = div255_round(pixel.g() as u32 * opacity_u8) as u8;
    let b = div255_round(pixel.b() as u32 * opacity_u8) as u8;
    let a = div255_round(pixel.a() as u32 * opacity_u8) as u8;

    Rgba8::new(r, g, b, a)
}

/// Skaluje kanały koloru (RGBA premultiplied) przez krycie warstwy (0.0..=1.0).
#[inline]
pub fn apply_opacity(pixel: Rgba8, opacity: f32) -> Rgba8 {
    if opacity >= 1.0 {
        return pixel;
    }
    if opacity <= 0.0 || pixel.is_transparent() {
        return Rgba8::TRANSPARENT;
    }

    let opacity_u8 = (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    apply_opacity_u8(pixel, opacity_u8)
}

/// Mieszanie w trybie Normal (kompozycja Porter-Duff "Over" dla premultiplied alpha).
///
/// Wzór:
/// `out_c = src_c + dst_c * (255 - src_a) / 255`
/// Z zaokrągleniem bez operacji f32: `((x + 128) * 257) >> 16`.
#[inline]
pub fn blend_normal(dst: Rgba8, src: Rgba8) -> Rgba8 {
    let src_a = src.a() as u32;
    if src_a == 0 {
        return dst;
    }
    if src_a == 255 {
        return src;
    }

    let dst_a = dst.a() as u32;
    if dst_a == 0 {
        return src;
    }

    let inv_src_a = 255 - src_a;
    let r = src.r() as u32 + div255_round(dst.r() as u32 * inv_src_a);
    let g = src.g() as u32 + div255_round(dst.g() as u32 * inv_src_a);
    let b = src.b() as u32 + div255_round(dst.b() as u32 * inv_src_a);
    let a = src_a + div255_round(dst_a * inv_src_a);

    Rgba8::new(
        r.min(255) as u8,
        g.min(255) as u8,
        b.min(255) as u8,
        a.min(255) as u8,
    )
}

/// Tryb Multiply dla premultiplied alpha (wzór W3C).
#[inline]
pub fn blend_multiply(dst: Rgba8, src: Rgba8) -> Rgba8 {
    let (sa, da) = (src.a() as u32, dst.a() as u32);
    let ch = |s: u8, d: u8| -> u8 {
        let (s, d) = (s as u32, d as u32);
        div255_round(s * (255 - da) + d * (255 - sa) + s * d).min(255) as u8
    };
    let a = (sa + da - div255_round(sa * da)).min(255) as u8;
    Rgba8::new(
        ch(src.r(), dst.r()),
        ch(src.g(), dst.g()),
        ch(src.b(), dst.b()),
        a,
    )
}

/// Tryb Screen dla premultiplied alpha (wzór W3C).
#[inline]
pub fn blend_screen(dst: Rgba8, src: Rgba8) -> Rgba8 {
    let (sa, da) = (src.a() as u32, dst.a() as u32);
    let ch = |s: u8, d: u8| -> u8 {
        let (s, d) = (s as u32, d as u32);
        (s + d - div255_round(s * d)).min(255) as u8
    };
    let a = (sa + da - div255_round(sa * da)).min(255) as u8;
    Rgba8::new(
        ch(src.r(), dst.r()),
        ch(src.g(), dst.g()),
        ch(src.b(), dst.b()),
        a,
    )
}

/// Tryb Darken dla premultiplied alpha.
#[inline]
pub fn blend_darken(dst: Rgba8, src: Rgba8) -> Rgba8 {
    let (sa, da) = (src.a() as u32, dst.a() as u32);
    let ch = |s: u8, d: u8| -> u8 {
        let (s, d) = (s as u32, d as u32);
        (s + d - div255_round((s * da).max(d * sa))).min(255) as u8
    };
    let a = (sa + da - div255_round(sa * da)).min(255) as u8;
    Rgba8::new(
        ch(src.r(), dst.r()),
        ch(src.g(), dst.g()),
        ch(src.b(), dst.b()),
        a,
    )
}

/// Tryb Lighten dla premultiplied alpha.
#[inline]
pub fn blend_lighten(dst: Rgba8, src: Rgba8) -> Rgba8 {
    let (sa, da) = (src.a() as u32, dst.a() as u32);
    let ch = |s: u8, d: u8| -> u8 {
        let (s, d) = (s as u32, d as u32);
        (s + d - div255_round((s * da).min(d * sa))).min(255) as u8
    };
    let a = (sa + da - div255_round(sa * da)).min(255) as u8;
    Rgba8::new(
        ch(src.r(), dst.r()),
        ch(src.g(), dst.g()),
        ch(src.b(), dst.b()),
        a,
    )
}

/// Tryb Overlay dla premultiplied alpha.
#[inline]
pub fn blend_overlay(dst: Rgba8, src: Rgba8) -> Rgba8 {
    let (sa, da) = (src.a() as u32, dst.a() as u32);
    let ch = |s: u8, d: u8| -> u8 {
        let (s, d, sa, da) = (s as i64, d as i64, sa as i64, da as i64);
        let b = if 2 * d <= da {
            2 * s * d
        } else {
            sa * da - 2 * (da - d) * (sa - s)
        };
        let v = s * (255 - da) + d * (255 - sa) + b;
        div255_round(v.max(0) as u32).min(255) as u8
    };
    let a = (sa + da - div255_round(sa * da)).min(255) as u8;
    Rgba8::new(
        ch(src.r(), dst.r()),
        ch(src.g(), dst.g()),
        ch(src.b(), dst.b()),
        a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_div255_round() {
        assert_eq!(div255_round(0), 0);
        assert_eq!(div255_round(127), 0);
        assert_eq!(div255_round(128), 1);
        assert_eq!(div255_round(255), 1);
        assert_eq!(div255_round(255 * 255), 255);
        // Sprawdź dokładność zaokrąglenia dla wszystkich iloczynów 0..=65025
        for x in 0..=255 * 255 {
            let exact_round = ((x as f64) / 255.0).round() as u32;
            let approx = div255_round(x);
            assert_eq!(
                approx, exact_round,
                "Błąd zaokrąglenia dla x = {}: otrzymano {}, oczekiwano {}",
                x, approx, exact_round
            );
        }
    }

    #[test]
    fn test_blend_mode_default() {
        assert_eq!(BlendMode::default(), BlendMode::Normal);
    }

    #[test]
    fn test_apply_opacity_u8() {
        let white = Rgba8::WHITE;
        assert_eq!(apply_opacity_u8(white, 255), white);
        assert_eq!(apply_opacity_u8(white, 0), Rgba8::TRANSPARENT);

        let half = apply_opacity_u8(white, 128);
        assert_eq!(half.a(), 128);
        assert_eq!(half.r(), 128);
        assert_eq!(half.g(), 128);
        assert_eq!(half.b(), 128);

        let trans = Rgba8::TRANSPARENT;
        assert_eq!(apply_opacity_u8(trans, 200), Rgba8::TRANSPARENT);
    }

    #[test]
    fn test_apply_opacity() {
        let white = Rgba8::WHITE;
        assert_eq!(apply_opacity(white, 1.0), white);
        assert_eq!(apply_opacity(white, 1.5), white);
        assert_eq!(apply_opacity(white, 0.0), Rgba8::TRANSPARENT);
        assert_eq!(apply_opacity(white, -0.5), Rgba8::TRANSPARENT);

        let half_white = apply_opacity(white, 0.5);
        assert_eq!(half_white.a(), 128);
        assert_eq!(half_white.r(), 128);
        assert_eq!(half_white.g(), 128);
        assert_eq!(half_white.b(), 128);

        let trans = Rgba8::TRANSPARENT;
        assert_eq!(apply_opacity(trans, 0.8), Rgba8::TRANSPARENT);
    }

    #[test]
    fn test_blend_normal_transparent_src() {
        let dst = Rgba8::new(100, 150, 200, 255);
        let src = Rgba8::TRANSPARENT;
        assert_eq!(blend_normal(dst, src), dst);
    }

    #[test]
    fn test_blend_normal_transparent_dst() {
        let dst = Rgba8::TRANSPARENT;
        let src = Rgba8::new(100, 150, 200, 180);
        assert_eq!(blend_normal(dst, src), src);
    }

    #[test]
    fn test_blend_normal_opaque_src() {
        let dst = Rgba8::new(50, 50, 50, 255);
        let src = Rgba8::new(200, 100, 0, 255);
        assert_eq!(blend_normal(dst, src), src);
    }

    #[test]
    fn test_blend_normal_semi_transparent() {
        // Podłoże: niebieski 100% [0, 0, 255, 255]
        let dst = Rgba8::new(0, 0, 255, 255);
        // Warstwa górna: czerwony 50% (premultiplied: [128, 0, 0, 128])
        let src = Rgba8::new(128, 0, 0, 128);

        let blended = blend_normal(dst, src);
        assert_eq!(blended.a(), 255);
        assert_eq!(blended.r(), 128);
        assert_eq!(blended.g(), 0);
        // dst_b * (255 - 128) / 255 = 255 * 127 / 255 = 127
        assert_eq!(blended.b(), 127);
    }

    #[test]
    fn test_blend_multiply() {
        // Biały nieprzezroczysty * kolor == kolor
        let dst = Rgba8::WHITE;
        let src = Rgba8::new(100, 150, 200, 255);
        assert_eq!(blend_multiply(dst, src), src);

        // Czarny nieprzezroczysty * kolor == czarny
        let dst = Rgba8::BLACK;
        let src = Rgba8::new(100, 150, 200, 255);
        assert_eq!(blend_multiply(dst, src), Rgba8::BLACK);

        // Src przezroczysty zwraca dst
        let dst = Rgba8::new(100, 150, 200, 255);
        let src = Rgba8::TRANSPARENT;
        assert_eq!(blend_multiply(dst, src), dst);

        // Dst przezroczysty zwraca src
        let dst = Rgba8::TRANSPARENT;
        let src = Rgba8::new(100, 150, 200, 255);
        assert_eq!(blend_multiply(dst, src), src);
    }

    #[test]
    fn test_blend_screen() {
        // Czarny nieprzezroczysty (0,0,0,255) jako src na kolorze (100,150,200,255) daje (100,150,200,255)
        let dst = Rgba8::new(100, 150, 200, 255);
        let src = Rgba8::BLACK;
        assert_eq!(blend_screen(dst, src), dst);

        // Biały nieprzezroczysty daje (255,255,255,255)
        let dst = Rgba8::new(100, 150, 200, 255);
        let src = Rgba8::WHITE;
        assert_eq!(blend_screen(dst, src), Rgba8::WHITE);
    }

    #[test]
    fn test_blend_darken() {
        // Test z podanym przykładem: src (50,200,100,255) na dst (100,150,200,255) daje (50,150,100,255)
        let dst = Rgba8::new(100, 150, 200, 255);
        let src = Rgba8::new(50, 200, 100, 255);
        let result = blend_darken(dst, src);
        assert_eq!(result, Rgba8::new(50, 150, 100, 255));
    }

    #[test]
    fn test_blend_lighten() {
        // Test z podanym przykładem: src (50,200,100,255) na dst (100,150,200,255) daje (100,200,200,255)
        let dst = Rgba8::new(100, 150, 200, 255);
        let src = Rgba8::new(50, 200, 100, 255);
        let result = blend_lighten(dst, src);
        assert_eq!(result, Rgba8::new(100, 200, 200, 255));
    }

    #[test]
    fn test_blend_overlay() {
        // Test: src szary (128,128,128,255) na dst (100,150,200,255) daje wynik różniący się od dst o najwyżej 2 na każdym kanale
        let dst = Rgba8::new(100, 150, 200, 255);
        let src = Rgba8::new(128, 128, 128, 255);
        let result = blend_overlay(dst, src);
        assert!((result.r() as i16 - dst.r() as i16).abs() <= 2);
        assert!((result.g() as i16 - dst.g() as i16).abs() <= 2);
        assert!((result.b() as i16 - dst.b() as i16).abs() <= 2);

        // Test: src czarny (0,0,0,255) na dst (50,50,50,255) daje (0,0,0,255)
        let dst = Rgba8::new(50, 50, 50, 255);
        let src = Rgba8::BLACK;
        let result = blend_overlay(dst, src);
        assert_eq!(result, Rgba8::BLACK);
    }

    #[test]
    fn test_blend_mode_labels() {
        assert_eq!(BlendMode::ALL.len(), 6);

        let mut labels = std::collections::HashSet::new();
        for mode in BlendMode::ALL {
            let label = mode.label();
            assert!(!label.is_empty());
            assert!(labels.insert(label), "Duplicate label found: {}", label);
        }

        assert_eq!(BlendMode::Normal.label(), "Normalny");
    }
}
