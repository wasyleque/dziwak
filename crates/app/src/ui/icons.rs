//! Ikony symboliczne w stylu GIMP 2.10, rysowane wektorowo przez `egui::Painter`.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

use crate::state::ActiveTool;

/// Ikony interfejsu poza narzędziami (panel warstw, kolory).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiIcon {
    NewLayer,
    Raise,
    Lower,
    Duplicate,
    Delete,
    Eye,
    SwapColors,
    ResetColors,
}

/// Rysuje ikonę narzędzia w prostokącie `rect` jednym kolorem.
pub fn paint_tool_icon(painter: &Painter, rect: Rect, tool: ActiveTool, color: Color32) {
    let p = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    let s = Stroke::new((rect.width() / 12.0).max(1.0), color);

    match tool {
        ActiveTool::Pan => {
            // Krzyż strzałek
            painter.line_segment([p(0.2, 0.5), p(0.8, 0.5)], s); // pozioma linia
            painter.line_segment([p(0.5, 0.2), p(0.5, 0.8)], s); // pionowa linia

            // Małe grotki na końcach
            painter.line_segment([p(0.15, 0.45), p(0.2, 0.5)], s);
            painter.line_segment([p(0.15, 0.55), p(0.2, 0.5)], s);
            painter.line_segment([p(0.85, 0.45), p(0.8, 0.5)], s);
            painter.line_segment([p(0.85, 0.55), p(0.8, 0.5)], s);
            painter.line_segment([p(0.45, 0.15), p(0.5, 0.2)], s);
            painter.line_segment([p(0.55, 0.15), p(0.5, 0.2)], s);
            painter.line_segment([p(0.45, 0.85), p(0.5, 0.8)], s);
            painter.line_segment([p(0.55, 0.85), p(0.5, 0.8)], s);
        }
        ActiveTool::Brush => {
            // Ukośny trzonek
            painter.line_segment([p(0.8, 0.2), p(0.4, 0.6)], s);

            // Wypełniona kropla/elipsa
            painter.circle_filled(p(0.3, 0.7), 0.15 * rect.width(), color);
        }
        ActiveTool::Eraser => {
            // Obrócony prostokąt
            let points = [p(0.2, 0.6), p(0.5, 0.3), p(0.8, 0.6), p(0.5, 0.9)];
            painter.add(Shape::closed_line(points.to_vec(), s));

            // Linia środkowa
            painter.line_segment([p(0.2, 0.6), p(0.8, 0.6)], s);
        }
        ActiveTool::Bucket => {
            // Wiadro jako trapez
            let points = [p(0.25, 0.35), p(0.7, 0.35), p(0.62, 0.85), p(0.33, 0.85)];
            painter.add(Shape::closed_line(points.to_vec(), s));

            // Kropla koło w (0.82, 0.7)
            painter.circle_filled(p(0.82, 0.7), 0.1 * rect.width(), color);
        }
        ActiveTool::Gradient => {
            // Prostokąt
            let points = [p(0.15, 0.15), p(0.85, 0.15), p(0.85, 0.85), p(0.15, 0.85)];
            painter.add(Shape::closed_line(points.to_vec(), s));

            // Wypełniony trójkąt
            let triangle = [p(0.15, 0.85), p(0.85, 0.15), p(0.85, 0.85)];
            painter.add(Shape::convex_polygon(
                triangle.to_vec(),
                color,
                Stroke::NONE,
            ));
        }
        ActiveTool::Eyedropper => {
            // Linia
            painter.line_segment([p(0.2, 0.8), p(0.65, 0.35)], s);

            // Koło wypełnione
            painter.circle_filled(p(0.75, 0.25), 0.12 * rect.width(), color);
        }
        ActiveTool::SelectRect => {
            // Przerywany prostokąt
            let segment_length = 0.15;

            // Górna krawędź
            painter.line_segment([p(0.0, 0.0), p(segment_length, 0.0)], s);
            painter.line_segment(
                [p(segment_length * 2.0, 0.0), p(segment_length * 3.0, 0.0)],
                s,
            );
            painter.line_segment([p(segment_length * 4.0, 0.0), p(1.0, 0.0)], s);

            // Prawa krawędź
            painter.line_segment([p(1.0, 0.0), p(1.0, segment_length)], s);
            painter.line_segment(
                [p(1.0, segment_length * 2.0), p(1.0, segment_length * 3.0)],
                s,
            );
            painter.line_segment([p(1.0, segment_length * 4.0), p(1.0, 1.0)], s);

            // Dolna krawędź
            painter.line_segment([p(1.0, 1.0), p(1.0 - segment_length, 1.0)], s);
            painter.line_segment(
                [
                    p(1.0 - segment_length * 2.0, 1.0),
                    p(1.0 - segment_length * 3.0, 1.0),
                ],
                s,
            );
            painter.line_segment([p(1.0 - segment_length * 4.0, 1.0), p(0.0, 1.0)], s);

            // Lewa krawędź
            painter.line_segment([p(0.0, 1.0), p(0.0, 1.0 - segment_length)], s);
            painter.line_segment(
                [
                    p(0.0, 1.0 - segment_length * 2.0),
                    p(0.0, 1.0 - segment_length * 3.0),
                ],
                s,
            );
            painter.line_segment([p(0.0, 1.0 - segment_length * 4.0), p(0.0, 0.0)], s);
        }
        ActiveTool::SelectEllipse => {
            // Koło w środku
            painter.circle_stroke(p(0.5, 0.5), 0.35 * rect.width(), s);
        }
        ActiveTool::MagicWand => {
            // Różdżka: trzonek i gwiazdka na końcu
            painter.line_segment([p(0.2, 0.85), p(0.62, 0.43)], s);
            let c = p(0.72, 0.3);
            let r = 0.16 * rect.width();
            for (dx, dy) in [(1.0, 0.0), (0.0, 1.0), (0.7, 0.7), (0.7, -0.7)] {
                painter.line_segment(
                    [
                        Pos2::new(c.x - dx * r, c.y - dy * r),
                        Pos2::new(c.x + dx * r, c.y + dy * r),
                    ],
                    s,
                );
            }
        }
    }
}

/// Rysuje ikonę interfejsu w prostokącie `rect` jednym kolorem.
pub fn paint_ui_icon(painter: &Painter, rect: Rect, icon: UiIcon, color: Color32) {
    let p = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    let s = Stroke::new((rect.width() / 12.0).max(1.0), color);

    match icon {
        UiIcon::NewLayer => {
            // Kartka
            let points = [p(0.25, 0.15), p(0.75, 0.15), p(0.75, 0.85), p(0.25, 0.85)];
            painter.add(Shape::closed_line(points.to_vec(), s));

            // Plus
            painter.line_segment([p(0.5, 0.35), p(0.5, 0.65)], s);
            painter.line_segment([p(0.35, 0.5), p(0.65, 0.5)], s);
        }
        UiIcon::Raise => {
            // Grot w górę
            let points = [p(0.5, 0.2), p(0.8, 0.6), p(0.2, 0.6)];
            painter.add(Shape::convex_polygon(points.to_vec(), color, Stroke::NONE));

            // Linia
            painter.line_segment([p(0.5, 0.6), p(0.5, 0.85)], s);
        }
        UiIcon::Lower => {
            // Odbicie Raise w dół
            let points = [p(0.5, 0.8), p(0.8, 0.4), p(0.2, 0.4)];
            painter.add(Shape::convex_polygon(points.to_vec(), color, Stroke::NONE));

            // Linia
            painter.line_segment([p(0.5, 0.4), p(0.5, 0.15)], s);
        }
        UiIcon::Duplicate => {
            // Dwie przesunięte kartki
            let points1 = [p(0.2, 0.15), p(0.65, 0.15), p(0.65, 0.85), p(0.2, 0.85)];
            painter.add(Shape::closed_line(points1.to_vec(), s));

            let points2 = [p(0.35, 0.15), p(0.8, 0.15), p(0.8, 0.85), p(0.35, 0.85)];
            painter.add(Shape::closed_line(points2.to_vec(), s));
        }
        UiIcon::Delete => {
            // Kosz: trapez
            let points = [p(0.25, 0.3), p(0.75, 0.3), p(0.68, 0.88), p(0.32, 0.88)];
            painter.add(Shape::closed_line(points.to_vec(), s));

            // Pokrywka
            painter.line_segment([p(0.18, 0.22), p(0.82, 0.22)], s);
        }
        UiIcon::Eye => {
            // Oczy: migdał
            let points = [p(0.1, 0.5), p(0.5, 0.25), p(0.9, 0.5), p(0.5, 0.75)];
            painter.add(Shape::closed_line(points.to_vec(), s));

            // Wypełnione koło w środku
            painter.circle_filled(p(0.5, 0.5), 0.12 * rect.width(), color);
        }
        UiIcon::SwapColors => {
            // Łuk z dwóch odcinków
            painter.line_segment([p(0.3, 0.25), p(0.75, 0.25)], s);
            painter.line_segment([p(0.75, 0.25), p(0.75, 0.7)], s);

            // Grotki na końcach
            painter.line_segment([p(0.3, 0.25), p(0.3, 0.2)], s);
            painter.line_segment([p(0.3, 0.25), p(0.35, 0.25)], s);

            painter.line_segment([p(0.75, 0.7), p(0.75, 0.75)], s);
            painter.line_segment([p(0.75, 0.7), p(0.8, 0.7)], s);
        }
        UiIcon::ResetColors => {
            // Dwa małe kwadraty
            let points1 = [p(0.15, 0.15), p(0.55, 0.15), p(0.55, 0.55), p(0.15, 0.55)];
            painter.add(Shape::convex_polygon(points1.to_vec(), color, Stroke::NONE));

            let points2 = [p(0.45, 0.45), p(0.85, 0.45), p(0.85, 0.85), p(0.45, 0.85)];
            painter.add(Shape::closed_line(points2.to_vec(), s));
        }
    }
}
