//! Podziałka linijek płótna.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    pub pos: f32,
    pub major: bool,
    pub label: Option<i64>,
}

const STEPS: [u32; 12] = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000];

/// Krok (w pikselach obrazu) między opisanymi kreskami, tak by na ekranie było co najmniej 50 px.
pub fn ruler_step(zoom: f32) -> u32 {
    if zoom <= 0.0 {
        return 5000;
    }

    for &step in &STEPS {
        if step as f32 * zoom >= 50.0 {
            return step;
        }
    }

    5000
}

/// Kreski linijki o długości `length` px ekranu. `offset` = pozycja ekranowa (względem początku linijki) punktu 0 obrazu.
pub fn ruler_ticks(offset: f32, zoom: f32, length: f32, out: &mut Vec<Tick>) {
    out.clear();

    if zoom <= 0.0 || length <= 0.0 {
        return;
    }

    let major = ruler_step(zoom) as f64;
    let minor = major / 5.0;
    let show_minor = minor * zoom as f64 >= 4.0;
    let step = if show_minor { minor } else { major };

    let start_v = (-offset as f64) / zoom as f64;
    let end_v = (length as f64 - offset as f64) / zoom as f64;

    let mut k = (start_v / step).floor() as i64;

    loop {
        let v = k as f64 * step;
        if v > end_v {
            break;
        }

        let pos = offset + (v * zoom as f64) as f32;

        if pos >= 0.0 {
            let is_major = (v / major).round() * major == v;

            out.push(Tick {
                pos,
                major: is_major,
                label: if is_major {
                    Some(v.round() as i64)
                } else {
                    None
                },
            });
        }

        k += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ruler_step() {
        assert_eq!(ruler_step(1.0), 50);
        assert_eq!(ruler_step(0.1), 500);
        assert_eq!(ruler_step(10.0), 5);
        assert_eq!(ruler_step(0.0), 5000);
    }

    #[test]
    fn test_ruler_ticks() {
        let mut ticks = Vec::new();

        // Test case 1: Basic case
        ruler_ticks(0.0, 1.0, 200.0, &mut ticks);
        assert!(!ticks.is_empty());
        assert_eq!(ticks[0].pos, 0.0);
        assert!(ticks[0].major);
        assert_eq!(ticks[0].label, Some(0));

        // Find the tick with label 50
        let tick_50 = ticks.iter().find(|t| t.label == Some(50)).unwrap();
        assert_eq!(tick_50.pos, 50.0);

        // Test case 2: Offset case
        ticks.clear();
        ruler_ticks(100.0, 1.0, 300.0, &mut ticks);
        assert!(!ticks.is_empty());

        // Find the tick with label -50 (which should be at position 50)
        let tick_neg_50 = ticks.iter().find(|t| t.label == Some(-50)).unwrap();
        assert_eq!(tick_neg_50.pos, 50.0);

        // Test case 3: Zoom 0
        ticks.clear();
        ruler_ticks(0.0, 0.0, 200.0, &mut ticks);
        assert!(ticks.is_empty());
    }
}
