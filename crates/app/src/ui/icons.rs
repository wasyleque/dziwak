//! Ikony symboliczne w stylu GIMP 2.10, rysowane wektorowo przez `egui::Painter`.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke};

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

/// Typ ikony narzędzia (pełny zestaw GIMP 2.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolIcon {
    // Zaznaczenia
    SelectRect,
    SelectEllipse,
    SelectFree,
    MagicWand,
    SelectColor,
    // Transformacje
    Move,
    Crop,
    Rotate,
    Scale,
    Flip,
    // Malowanie
    Pencil,
    Brush,
    Airbrush,
    Eraser,
    Bucket,
    Gradient,
    Clone,
    Smudge,
    DodgeBurn,
    BlurSharpen,
    // Kolory i inne
    Eyedropper,
    Text,
    Zoom,
    Measure,
    Pan,
}

/// Rysuje ikonę narzędzia w prostokącie `rect` jednym kolorem dla danego `ToolIcon`.
pub fn paint_tool_icon_by_kind(painter: &Painter, rect: Rect, icon: ToolIcon, color: Color32) {
    let p = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    let s = Stroke::new((rect.width() / 12.0).max(1.0), color);

    match icon {
        ToolIcon::Pan => {
            // Dłoń (narzędzie przesuwania widoku)
            let palm_rect = Rect::from_min_max(p(0.32, 0.42), p(0.68, 0.78));
            painter.rect_stroke(palm_rect, 2.0, s, egui::StrokeKind::Inside);
            // 4 palce
            painter.line_segment([p(0.38, 0.24), p(0.38, 0.42)], s);
            painter.line_segment([p(0.46, 0.20), p(0.46, 0.42)], s);
            painter.line_segment([p(0.54, 0.21), p(0.54, 0.42)], s);
            painter.line_segment([p(0.62, 0.27), p(0.62, 0.42)], s);
            // Kciuk
            painter.line_segment([p(0.22, 0.48), p(0.32, 0.58)], s);
        }
        ToolIcon::Move => {
            // Krzyż strzałek (narzędzie przesuwania warstwy / zaznaczenia)
            painter.line_segment([p(0.2, 0.5), p(0.8, 0.5)], s);
            painter.line_segment([p(0.5, 0.2), p(0.5, 0.8)], s);

            // Grotki na końcach
            painter.line_segment([p(0.15, 0.45), p(0.2, 0.5)], s);
            painter.line_segment([p(0.15, 0.55), p(0.2, 0.5)], s);
            painter.line_segment([p(0.85, 0.45), p(0.8, 0.5)], s);
            painter.line_segment([p(0.85, 0.55), p(0.8, 0.5)], s);
            painter.line_segment([p(0.45, 0.15), p(0.5, 0.2)], s);
            painter.line_segment([p(0.55, 0.15), p(0.5, 0.2)], s);
            painter.line_segment([p(0.45, 0.85), p(0.5, 0.8)], s);
            painter.line_segment([p(0.55, 0.85), p(0.5, 0.8)], s);
        }
        ToolIcon::Crop => {
            // Kadrowanie: dwa przecinające się kątowniki
            painter.line_segment([p(0.18, 0.38), p(0.72, 0.38)], s);
            painter.line_segment([p(0.38, 0.18), p(0.38, 0.72)], s);
            painter.line_segment([p(0.28, 0.62), p(0.82, 0.62)], s);
            painter.line_segment([p(0.62, 0.28), p(0.62, 0.82)], s);
        }
        ToolIcon::Rotate => {
            // Obrót: zakrzywiona strzałka wokół środka
            let center = p(0.5, 0.5);
            let r = 0.26 * rect.width();
            let steps = 12;
            for i in 0..steps {
                let a1 = std::f32::consts::PI * 0.1
                    + (i as f32) * (std::f32::consts::PI * 1.5) / (steps as f32);
                let a2 = std::f32::consts::PI * 0.1
                    + ((i + 1) as f32) * (std::f32::consts::PI * 1.5) / (steps as f32);
                let pt1 = Pos2::new(center.x + a1.cos() * r, center.y + a1.sin() * r);
                let pt2 = Pos2::new(center.x + a2.cos() * r, center.y + a2.sin() * r);
                painter.line_segment([pt1, pt2], s);
            }
            let end_a = std::f32::consts::PI * 1.6;
            let end_pt = Pos2::new(center.x + end_a.cos() * r, center.y + end_a.sin() * r);
            painter.line_segment(
                [end_pt, Pos2::new(end_pt.x - 0.12 * rect.width(), end_pt.y)],
                s,
            );
            painter.line_segment(
                [end_pt, Pos2::new(end_pt.x, end_pt.y + 0.12 * rect.height())],
                s,
            );
        }
        ToolIcon::Scale => {
            // Skalowanie: mały kwadrat wewnątrz dużego i strzałka po przekątnej
            let big_box = [p(0.2, 0.2), p(0.8, 0.2), p(0.8, 0.8), p(0.2, 0.8)];
            painter.add(Shape::closed_line(big_box.to_vec(), s));
            let small_box = [p(0.2, 0.5), p(0.5, 0.5), p(0.5, 0.8), p(0.2, 0.8)];
            painter.add(Shape::closed_line(small_box.to_vec(), s));
            painter.line_segment([p(0.5, 0.5), p(0.76, 0.24)], s);
            painter.line_segment([p(0.62, 0.24), p(0.76, 0.24)], s);
            painter.line_segment([p(0.76, 0.38), p(0.76, 0.24)], s);
        }
        ToolIcon::Flip => {
            // Odbicie: pionowa linia symetrii i dwa trójkąty
            painter.line_segment([p(0.5, 0.15), p(0.5, 0.38)], s);
            painter.line_segment([p(0.5, 0.48), p(0.5, 0.68)], s);
            painter.line_segment([p(0.5, 0.78), p(0.5, 0.85)], s);
            let left_tri = [p(0.2, 0.32), p(0.44, 0.5), p(0.2, 0.68)];
            painter.add(Shape::closed_line(left_tri.to_vec(), s));
            let right_tri = [p(0.8, 0.32), p(0.56, 0.5), p(0.8, 0.68)];
            painter.add(Shape::convex_polygon(
                right_tri.to_vec(),
                color,
                Stroke::NONE,
            ));
        }
        ToolIcon::Brush => {
            // Ukośny trzonek
            painter.line_segment([p(0.8, 0.2), p(0.4, 0.6)], s);
            // Wypełniona kropla/elipsa
            painter.circle_filled(p(0.3, 0.7), 0.15 * rect.width(), color);
        }
        ToolIcon::Pencil => {
            // Ołówek: ukośny korpus z ostrym czubkiem
            let body = [p(0.72, 0.18), p(0.82, 0.28), p(0.46, 0.64), p(0.36, 0.54)];
            painter.add(Shape::closed_line(body.to_vec(), s));
            let tip = [p(0.36, 0.54), p(0.46, 0.64), p(0.22, 0.78)];
            painter.add(Shape::convex_polygon(tip.to_vec(), color, Stroke::NONE));
        }
        ToolIcon::Airbrush => {
            // Aerograf: ukośna dysza i chmura natrysku
            painter.line_segment([p(0.22, 0.78), p(0.48, 0.52)], s);
            painter.line_segment([p(0.42, 0.58), p(0.54, 0.46)], s);
            // Kropki natrysku o rosnącym promieniu
            let dot_r = (rect.width() * 0.04).max(1.0);
            painter.circle_filled(p(0.64, 0.36), dot_r, color);
            painter.circle_filled(p(0.74, 0.46), dot_r * 0.8, color);
            painter.circle_filled(p(0.60, 0.22), dot_r * 0.9, color);
            painter.circle_filled(p(0.80, 0.28), dot_r * 1.2, color);
            painter.circle_filled(p(0.84, 0.16), dot_r, color);
        }
        ToolIcon::Eraser => {
            // Obrócony prostokąt
            let points = [p(0.2, 0.6), p(0.5, 0.3), p(0.8, 0.6), p(0.5, 0.9)];
            painter.add(Shape::closed_line(points.to_vec(), s));
            // Linia środkowa
            painter.line_segment([p(0.2, 0.6), p(0.8, 0.6)], s);
        }
        ToolIcon::Bucket => {
            // Wiadro jako trapez
            let points = [p(0.25, 0.35), p(0.7, 0.35), p(0.62, 0.85), p(0.33, 0.85)];
            painter.add(Shape::closed_line(points.to_vec(), s));
            // Kropla koło w (0.82, 0.7)
            painter.circle_filled(p(0.82, 0.7), 0.1 * rect.width(), color);
        }
        ToolIcon::Gradient => {
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
        ToolIcon::Clone => {
            // Pieczęć (stempel klonowania): uchwyt na górze, podstawa na dole
            painter.circle_filled(p(0.5, 0.22), 0.10 * rect.width(), color);
            painter.line_segment([p(0.5, 0.26), p(0.5, 0.52)], s);
            let base = [p(0.28, 0.52), p(0.72, 0.52), p(0.80, 0.76), p(0.20, 0.76)];
            painter.add(Shape::closed_line(base.to_vec(), s));
            painter.line_segment([p(0.16, 0.84), p(0.84, 0.84)], s);
        }
        ToolIcon::Smudge => {
            // Rozmazywanie: palec wskazujący ze smugą
            painter.line_segment([p(0.5, 0.2), p(0.5, 0.6)], s);
            painter.circle_filled(p(0.5, 0.62), 0.09 * rect.width(), color);
            // Fala smugi
            painter.line_segment([p(0.25, 0.80), p(0.42, 0.74)], s);
            painter.line_segment([p(0.42, 0.74), p(0.58, 0.82)], s);
            painter.line_segment([p(0.58, 0.82), p(0.75, 0.76)], s);
        }
        ToolIcon::DodgeBurn => {
            // Rozjaśnianie / ściemnianie: tradycyjna łopatka ciemniowa
            painter.circle_stroke(p(0.5, 0.38), 0.20 * rect.width(), s);
            painter.line_segment([p(0.5, 0.58), p(0.5, 0.86)], s);
        }
        ToolIcon::BlurSharpen => {
            // Rozmywanie / wyostrzanie: kropla wody
            let drop = [
                p(0.5, 0.18),
                p(0.74, 0.54),
                p(0.68, 0.78),
                p(0.5, 0.85),
                p(0.32, 0.78),
                p(0.26, 0.54),
            ];
            painter.add(Shape::closed_line(drop.to_vec(), s));
        }
        ToolIcon::Eyedropper => {
            // Pipeta
            painter.line_segment([p(0.2, 0.8), p(0.65, 0.35)], s);
            painter.circle_filled(p(0.75, 0.25), 0.12 * rect.width(), color);
        }
        ToolIcon::Text => {
            // Tekst: litera T z szeryfami
            painter.line_segment([p(0.22, 0.25), p(0.78, 0.25)], s);
            painter.line_segment([p(0.50, 0.25), p(0.50, 0.82)], s);
            // Szeryfy
            painter.line_segment([p(0.22, 0.25), p(0.22, 0.35)], s);
            painter.line_segment([p(0.78, 0.25), p(0.78, 0.35)], s);
            painter.line_segment([p(0.38, 0.82), p(0.62, 0.82)], s);
        }
        ToolIcon::Zoom => {
            // Lupa: koło i uchwyt
            painter.circle_stroke(p(0.42, 0.42), 0.22 * rect.width(), s);
            let handle_stroke = Stroke::new(s.width * 1.5, color);
            painter.line_segment([p(0.58, 0.58), p(0.82, 0.82)], handle_stroke);
        }
        ToolIcon::Measure => {
            // Miarka / cyrkiel podziałowy
            painter.circle_filled(p(0.5, 0.22), 0.07 * rect.width(), color);
            painter.line_segment([p(0.5, 0.25), p(0.26, 0.82)], s);
            painter.line_segment([p(0.5, 0.25), p(0.74, 0.82)], s);
            // Łuk / podziałka
            let arc_stroke = Stroke::new((s.width * 0.75).max(1.0), color);
            painter.line_segment([p(0.36, 0.60), p(0.64, 0.60)], arc_stroke);
        }
        ToolIcon::SelectRect => {
            // Przerywany prostokąt wewnątrz przycisku (0.18..0.82)
            let (a, b) = (0.18_f32, 0.82_f32);
            let dash = (b - a) / 5.0;
            for k in [0.0_f32, 2.0, 4.0] {
                let (u0, u1) = (a + k * dash, a + (k + 1.0) * dash);
                painter.line_segment([p(u0, a), p(u1, a)], s);
                painter.line_segment([p(u0, b), p(u1, b)], s);
                painter.line_segment([p(a, u0), p(a, u1)], s);
                painter.line_segment([p(b, u0), p(b, u1)], s);
            }
        }
        ToolIcon::SelectEllipse => {
            // Koło w środku
            painter.circle_stroke(p(0.5, 0.5), 0.35 * rect.width(), s);
        }
        ToolIcon::SelectFree => {
            // Pętla lassa
            let loop_pts = [
                p(0.50, 0.18),
                p(0.76, 0.36),
                p(0.66, 0.68),
                p(0.36, 0.68),
                p(0.24, 0.38),
            ];
            painter.add(Shape::closed_line(loop_pts.to_vec(), s));
            // Węzeł / końcówki
            painter.line_segment([p(0.36, 0.68), p(0.26, 0.84)], s);
            painter.line_segment([p(0.66, 0.68), p(0.72, 0.84)], s);
        }
        ToolIcon::MagicWand => {
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
        ToolIcon::SelectColor => {
            // Zaznaczenie wg koloru: 3 małe próbki barwne
            let sq = 0.22 * rect.width();
            let r1 = Rect::from_min_size(p(0.20, 0.24), egui::vec2(sq, sq));
            let r2 = Rect::from_min_size(p(0.54, 0.24), egui::vec2(sq, sq));
            let r3 = Rect::from_min_size(p(0.37, 0.56), egui::vec2(sq, sq));
            painter.rect_filled(r1, 1.0, color);
            painter.rect_stroke(r2, 1.0, s, egui::StrokeKind::Inside);
            painter.rect_filled(r3, 1.0, color);
        }
    }
}

/// Rysuje ikonę aktywnego narzędzia w prostokącie `rect` jednym kolorem.
#[allow(dead_code)]
pub fn paint_tool_icon(painter: &Painter, rect: Rect, tool: ActiveTool, color: Color32) {
    let icon = match tool {
        ActiveTool::Pan => ToolIcon::Pan,
        ActiveTool::Move => ToolIcon::Move,
        ActiveTool::Brush => ToolIcon::Brush,
        ActiveTool::Pencil => ToolIcon::Pencil,
        ActiveTool::Airbrush => ToolIcon::Airbrush,
        ActiveTool::Eraser => ToolIcon::Eraser,
        ActiveTool::Bucket => ToolIcon::Bucket,
        ActiveTool::Gradient => ToolIcon::Gradient,
        ActiveTool::Eyedropper => ToolIcon::Eyedropper,
        ActiveTool::SelectRect => ToolIcon::SelectRect,
        ActiveTool::SelectEllipse => ToolIcon::SelectEllipse,
        ActiveTool::MagicWand => ToolIcon::MagicWand,
        ActiveTool::Crop => ToolIcon::Crop,
        ActiveTool::SelectFree => ToolIcon::SelectFree,
        ActiveTool::SelectColor => ToolIcon::SelectColor,
        ActiveTool::Rotate => ToolIcon::Rotate,
        ActiveTool::Scale => ToolIcon::Scale,
        ActiveTool::Flip => ToolIcon::Flip,
        ActiveTool::Clone => ToolIcon::Clone,
        ActiveTool::Smudge => ToolIcon::Smudge,
        ActiveTool::DodgeBurn => ToolIcon::DodgeBurn,
        ActiveTool::BlurSharpen => ToolIcon::BlurSharpen,
        ActiveTool::Text => ToolIcon::Text,
        ActiveTool::Zoom => ToolIcon::Zoom,
        ActiveTool::Measure => ToolIcon::Measure,
    };
    paint_tool_icon_by_kind(painter, rect, icon, color);
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, Rect, Vec2};

    #[test]
    fn test_all_tool_icons_paint_without_panic() {
        let ctx = eframe::egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(28.0, 28.0));
                let color = Color32::WHITE;
                let icons = [
                    ToolIcon::SelectRect,
                    ToolIcon::SelectEllipse,
                    ToolIcon::SelectFree,
                    ToolIcon::MagicWand,
                    ToolIcon::SelectColor,
                    ToolIcon::Move,
                    ToolIcon::Crop,
                    ToolIcon::Rotate,
                    ToolIcon::Scale,
                    ToolIcon::Flip,
                    ToolIcon::Pencil,
                    ToolIcon::Brush,
                    ToolIcon::Airbrush,
                    ToolIcon::Eraser,
                    ToolIcon::Bucket,
                    ToolIcon::Gradient,
                    ToolIcon::Clone,
                    ToolIcon::Smudge,
                    ToolIcon::DodgeBurn,
                    ToolIcon::BlurSharpen,
                    ToolIcon::Eyedropper,
                    ToolIcon::Text,
                    ToolIcon::Zoom,
                    ToolIcon::Measure,
                    ToolIcon::Pan,
                ];
                for icon in icons {
                    paint_tool_icon_by_kind(ui.painter(), rect, icon, color);
                }
            });
        });
    }

    #[test]
    fn test_all_ui_icons_paint_without_panic() {
        let ctx = eframe::egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(28.0, 28.0));
                let color = Color32::WHITE;
                let icons = [
                    UiIcon::NewLayer,
                    UiIcon::Raise,
                    UiIcon::Lower,
                    UiIcon::Duplicate,
                    UiIcon::Delete,
                    UiIcon::Eye,
                    UiIcon::SwapColors,
                    UiIcon::ResetColors,
                ];
                for icon in icons {
                    paint_ui_icon(ui.painter(), rect, icon, color);
                }
            });
        });
    }

    #[test]
    fn test_paint_tool_icon_all_active_tools() {
        let ctx = eframe::egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            eframe::egui::CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(28.0, 28.0));
                let color = Color32::WHITE;
                let tools = [
                    ActiveTool::Pan,
                    ActiveTool::Move,
                    ActiveTool::Brush,
                    ActiveTool::Pencil,
                    ActiveTool::Airbrush,
                    ActiveTool::Eraser,
                    ActiveTool::Bucket,
                    ActiveTool::Gradient,
                    ActiveTool::Eyedropper,
                    ActiveTool::SelectRect,
                    ActiveTool::SelectEllipse,
                    ActiveTool::MagicWand,
                    ActiveTool::Crop,
                    ActiveTool::SelectFree,
                    ActiveTool::SelectColor,
                    ActiveTool::Rotate,
                    ActiveTool::Scale,
                    ActiveTool::Flip,
                    ActiveTool::Clone,
                    ActiveTool::Smudge,
                    ActiveTool::DodgeBurn,
                    ActiveTool::BlurSharpen,
                    ActiveTool::Text,
                    ActiveTool::Zoom,
                    ActiveTool::Measure,
                ];
                for tool in tools {
                    paint_tool_icon(ui.painter(), rect, tool, color);
                }
            });
        });
    }
}
