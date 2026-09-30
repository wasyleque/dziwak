//! Reprezentacja piksela RGBA8 z premultiplied alpha w przestrzeni sRGB.

/// Piksel w formacie RGBA8 z premultiplied alpha w przestrzeni sRGB.
/// Wartości kanałów R, G, B są przemnożone przez znormalizowany kanał Alfa (0..=255).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Rgba8(pub [u8; 4]);

impl Rgba8 {
    /// Przezroczysty piksel (zerowe wszystkie kanały).
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);
    /// W pełni nieprzezroczysty czarny piksel.
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    /// W pełni nieprzezroczysty biały piksel.
    pub const WHITE: Self = Self([255, 255, 255, 255]);

    /// Tworzy piksel z już przemnożonymi wartościami (premultiplied RGBA).
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }

    /// Tworzy piksel z tablicy 4 bajtów premultiplied RGBA.
    #[inline]
    pub const fn from_array(arr: [u8; 4]) -> Self {
        Self(arr)
    }

    /// Zwraca bajty piksela jako tablicę [r, g, b, a].
    #[inline]
    pub const fn to_array(self) -> [u8; 4] {
        self.0
    }

    /// Konwertuje prosty (straight) kolor RGBA na premultiplied RGBA.
    #[inline]
    pub const fn from_straight(r: u8, g: u8, b: u8, a: u8) -> Self {
        if a == 0 {
            Self::TRANSPARENT
        } else if a == 255 {
            Self([r, g, b, 255])
        } else {
            let r_pm = ((r as u16 * a as u16 + 127) / 255) as u8;
            let g_pm = ((g as u16 * a as u16 + 127) / 255) as u8;
            let b_pm = ((b as u16 * a as u16 + 127) / 255) as u8;
            Self([r_pm, g_pm, b_pm, a])
        }
    }

    /// Konwertuje premultiplied RGBA na prosty (straight) kolor RGBA.
    #[inline]
    pub const fn to_straight(self) -> [u8; 4] {
        let a = self.0[3];
        if a == 0 {
            [0, 0, 0, 0]
        } else if a == 255 {
            self.0
        } else {
            let a_u32 = a as u32;
            let r = ((self.0[0] as u32 * 255 + a_u32 / 2) / a_u32) as u8;
            let g = ((self.0[1] as u32 * 255 + a_u32 / 2) / a_u32) as u8;
            let b = ((self.0[2] as u32 * 255 + a_u32 / 2) / a_u32) as u8;
            [r, g, b, a]
        }
    }

    /// Zwraca kanał czerwony.
    #[inline]
    pub const fn r(self) -> u8 {
        self.0[0]
    }

    /// Zwraca kanał zielony.
    #[inline]
    pub const fn g(self) -> u8 {
        self.0[1]
    }

    /// Zwraca kanał niebieski.
    #[inline]
    pub const fn b(self) -> u8 {
        self.0[2]
    }

    /// Zwraca kanał alfa.
    #[inline]
    pub const fn a(self) -> u8 {
        self.0[3]
    }

    /// Sprawdza, czy piksel jest całkowicie przezroczysty.
    #[inline]
    pub const fn is_transparent(self) -> bool {
        self.0[3] == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_channels() {
        let px = Rgba8::new(10, 20, 30, 40);
        assert_eq!(px.r(), 10);
        assert_eq!(px.g(), 20);
        assert_eq!(px.b(), 30);
        assert_eq!(px.a(), 40);
    }

    #[test]
    fn test_array_conversion() {
        let arr = [1, 2, 3, 4];
        let px = Rgba8::from_array(arr);
        assert_eq!(px.to_array(), arr);
    }

    #[test]
    fn test_from_straight_and_to_straight() {
        // Pełne krycie: wartości powinny być identyczne
        let opaque = Rgba8::from_straight(120, 60, 200, 255);
        assert_eq!(opaque.to_array(), [120, 60, 200, 255]);
        assert_eq!(opaque.to_straight(), [120, 60, 200, 255]);

        // Zerowe krycie: zawsze zwraca [0, 0, 0, 0]
        let trans = Rgba8::from_straight(255, 128, 64, 0);
        assert_eq!(trans, Rgba8::TRANSPARENT);
        assert_eq!(trans.to_straight(), [0, 0, 0, 0]);

        // Połowa krycia (alfa = 128)
        let half = Rgba8::from_straight(255, 0, 100, 128);
        assert_eq!(half.a(), 128);
        // Sprawdź zaokrąglenie w to_straight (powinno być bardzo bliskie pierwotnemu)
        let straight_back = half.to_straight();
        assert_eq!(straight_back[3], 128);
        assert!((straight_back[0] as i16 - 255).abs() <= 1);
        assert_eq!(straight_back[1], 0);
        assert!((straight_back[2] as i16 - 100).abs() <= 1);
    }

    #[test]
    fn test_is_transparent() {
        assert!(Rgba8::TRANSPARENT.is_transparent());
        assert!(Rgba8::new(255, 255, 255, 0).is_transparent());
        assert!(!Rgba8::BLACK.is_transparent());
        assert!(!Rgba8::WHITE.is_transparent());
        assert!(!Rgba8::new(0, 0, 0, 1).is_transparent());
    }

    #[test]
    fn test_constants() {
        assert_eq!(Rgba8::TRANSPARENT.to_array(), [0, 0, 0, 0]);
        assert_eq!(Rgba8::BLACK.to_array(), [0, 0, 0, 255]);
        assert_eq!(Rgba8::WHITE.to_array(), [255, 255, 255, 255]);
    }
}
