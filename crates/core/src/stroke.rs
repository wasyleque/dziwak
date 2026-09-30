//! Interpolacja pociągnięć pędzla.

/// Dodaje do `out` punkty co `spacing` px na odcinku p1->p2.
///
/// `carry` to dystans przebyty od ostatniego postawionego punktu. Na początku
/// pociągnięcia podaj `carry = spacing`, aby pierwszy punkt wypadł dokładnie w `p1`.
/// Zwraca nowy `carry` dla następnego odcinka.
pub fn interpolate_stroke(
    p1: (f32, f32),
    p2: (f32, f32),
    spacing: f32,
    carry: f32,
    out: &mut Vec<(f32, f32)>,
) -> f32 {
    let spacing = spacing.max(0.5);
    let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return carry;
    }

    let mut d = (spacing - carry).max(0.0);
    while d <= len {
        let t = d / len;
        out.push((p1.0 + dx * t, p1.1 + dy * t));
        d += spacing;
    }
    len - (d - spacing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xs(out: &[(f32, f32)]) -> Vec<f32> {
        out.iter().map(|p| p.0).collect()
    }

    #[test]
    fn test_single_segment() {
        let mut out = Vec::new();
        let carry = interpolate_stroke((0.0, 0.0), (10.0, 0.0), 2.0, 2.0, &mut out);
        assert_eq!(xs(&out), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(carry, 0.0);
    }

    #[test]
    fn test_carry_keeps_even_spacing() {
        let mut out = Vec::new();
        let c = interpolate_stroke((0.0, 0.0), (3.0, 0.0), 2.0, 2.0, &mut out);
        assert_eq!(c, 1.0);
        let c = interpolate_stroke((3.0, 0.0), (6.0, 0.0), 2.0, c, &mut out);
        assert_eq!(xs(&out), vec![0.0, 2.0, 4.0, 6.0]);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn test_no_duplicate_at_segment_joint() {
        let mut out = Vec::new();
        let c = interpolate_stroke((0.0, 0.0), (2.0, 0.0), 2.0, 2.0, &mut out);
        interpolate_stroke((2.0, 0.0), (4.0, 0.0), 2.0, c, &mut out);
        assert_eq!(xs(&out), vec![0.0, 2.0, 4.0]);
    }

    #[test]
    fn test_zero_length() {
        let mut out = Vec::new();
        let carry = interpolate_stroke((5.0, 5.0), (5.0, 5.0), 2.0, 1.5, &mut out);
        assert!(out.is_empty());
        assert_eq!(carry, 1.5);
    }
}
