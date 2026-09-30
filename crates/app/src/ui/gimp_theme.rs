//! Wygląd „GIMP 2.10 Dark”.
use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Visuals};

/// Stałe kolorów (pub const, Color32::from_rgb)
pub const PANEL: Color32 = Color32::from_rgb(69, 69, 69);
pub const WINDOW: Color32 = Color32::from_rgb(59, 59, 59);
pub const EXTREME: Color32 = Color32::from_rgb(45, 45, 45);
pub const FAINT: Color32 = Color32::from_rgb(80, 80, 80);
pub const WIDGET: Color32 = Color32::from_rgb(86, 86, 86);
pub const WIDGET_HOVER: Color32 = Color32::from_rgb(106, 106, 106);
pub const WIDGET_ACTIVE: Color32 = Color32::from_rgb(122, 122, 122);
pub const TEXT: Color32 = Color32::from_rgb(190, 190, 190);
pub const TEXT_STRONG: Color32 = Color32::from_rgb(230, 230, 230);
pub const BORDER: Color32 = Color32::from_rgb(40, 40, 40);
pub const ACCENT_DEFAULT: Color32 = Color32::from_rgb(111, 139, 184);

/// Paleta GIMP 2.10 Dark; `accent` (np. z motywu Omarchy) zastępuje domyślny niebieski akcent.
pub fn gimp_dark_visuals(accent: Option<Color32>) -> Visuals {
    let accent = accent.unwrap_or(ACCENT_DEFAULT);
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = WINDOW;
    v.extreme_bg_color = EXTREME;
    v.faint_bg_color = FAINT;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_corner_radius = CornerRadius::same(2);
    v.menu_corner_radius = CornerRadius::same(2);
    v.hyperlink_color = accent;
    v.selection.bg_fill = accent.gamma_multiply(0.6);
    v.selection.stroke = Stroke::new(1.0_f32, TEXT_STRONG);

    // Ustawienia widgetów
    let widgets = [
        (&mut v.widgets.noninteractive, PANEL),
        (&mut v.widgets.inactive, WIDGET),
        (&mut v.widgets.hovered, WIDGET_HOVER),
        (&mut v.widgets.active, WIDGET_ACTIVE),
        (&mut v.widgets.open, WIDGET_ACTIVE),
    ];

    for (w, color) in widgets {
        w.bg_fill = color;
        w.weak_bg_fill = color;
        w.corner_radius = CornerRadius::same(2);
        w.bg_stroke = Stroke::new(1.0_f32, BORDER);
        w.fg_stroke = Stroke::new(1.0_f32, TEXT);
    }

    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_STRONG);
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent);

    v
}

/// Ustawia wygląd i gęstsze odstępy jak w GIMP-ie.
pub fn apply_gimp_style(ctx: &egui::Context, accent: Option<Color32>) {
    ctx.set_visuals(gimp_dark_visuals(accent));
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(4.0, 3.0);
        style.spacing.button_padding = egui::vec2(4.0, 2.0);
        style.spacing.interact_size.y = 18.0;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(10.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(11.0));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gimp_dark_visuals() {
        assert_eq!(gimp_dark_visuals(None).panel_fill, PANEL);
        assert_eq!(
            gimp_dark_visuals(Some(Color32::RED)).hyperlink_color,
            Color32::RED
        );
        assert!(gimp_dark_visuals(None).dark_mode);
    }
}
