//! Konwersje kolorów.

/// Konwertuje kolor w formacie hex do tablicy RGB.
/// Akceptuje "#rrggbb" i "rrggbb" (wielkość liter dowolna).
/// Zwraca błąd dla niepoprawnego formatu.
pub fn hex_to_rgb(hex: &str) -> Result<[u8; 3], &'static str> {
    let clean_hex = hex.strip_prefix('#').unwrap_or(hex);

    if clean_hex.len() != 6 || !clean_hex.is_ascii() {
        return Err("zły format koloru");
    }

    let r = u8::from_str_radix(&clean_hex[0..2], 16).map_err(|_| "zły format koloru")?;
    let g = u8::from_str_radix(&clean_hex[2..4], 16).map_err(|_| "zły format koloru")?;
    let b = u8::from_str_radix(&clean_hex[4..6], 16).map_err(|_| "zły format koloru")?;

    Ok([r, g, b])
}

/// Konwertuje kolor RGB do formatu hex.
pub fn rgb_to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_rgb() {
        assert_eq!(hex_to_rgb("#ff0000"), Ok([255, 0, 0]));
        assert_eq!(hex_to_rgb("00FF7f"), Ok([0, 255, 127]));
        assert_eq!(hex_to_rgb("#fff"), Err("zły format koloru"));
        assert_eq!(hex_to_rgb("#gg0000"), Err("zły format koloru"));
    }

    #[test]
    fn test_rgb_to_hex() {
        assert_eq!(rgb_to_hex(18, 52, 86), "#123456");
    }

    #[test]
    fn test_round_trip() {
        let r = 18;
        let g = 52;
        let b = 86;
        let hex = rgb_to_hex(r, g, b);
        assert_eq!(hex_to_rgb(&hex), Ok([r, g, b]));
    }

    #[test]
    fn test_hex_to_rgb_non_ascii_no_panic() {
        assert!(hex_to_rgb("aébcd").is_err());
    }
}
