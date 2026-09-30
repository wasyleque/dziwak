use crate::i18n::tr;
use crate::state::{ActiveTool, ColorPickerTarget, DziwakApp};
use crate::ui::icons::{paint_tool_icon, paint_ui_icon, UiIcon};
use eframe::egui;

const TOOLS: [(ActiveTool, &str); 9] = [
    (ActiveTool::Pan, "Przesuwanie (Spacja)"),
    (ActiveTool::Brush, "Pędzel (B)"),
    (ActiveTool::Eraser, "Gumka (E)"),
    (ActiveTool::Bucket, "Kubełek (G)"),
    (ActiveTool::Gradient, "Gradient (Shift+G)"),
    (ActiveTool::Eyedropper, "Pipeta (I)"),
    (ActiveTool::SelectRect, "Zaznaczenie prostokątne (M)"),
    (
        ActiveTool::SelectEllipse,
        "Zaznaczenie eliptyczne (Shift+M)",
    ),
    (ActiveTool::MagicWand, "Różdżka (U)"),
];

/// Renderuje siatkę przycisków narzędzi 28x28.
pub fn render_tool_grid(app: &mut DziwakApp, ui: &mut egui::Ui) {
    let btn_size = egui::vec2(28.0, 28.0);
    let spacing = 4.0;
    let avail_w = ui.available_width();
    let cols = ((avail_w + spacing) / (btn_size.x + spacing))
        .floor()
        .max(1.0) as usize;

    egui::Grid::new("toolbox_grid")
        .spacing([spacing, spacing])
        .show(ui, |ui| {
            for (idx, &(tool, tooltip)) in TOOLS.iter().enumerate() {
                let is_selected = app.active_tool == tool;
                let (rect, resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());

                // Ręczne tło dla aktywnego / wskazanego narzędzia
                let bg_color = if is_selected {
                    ui.visuals().selection.bg_fill
                } else if resp.hovered() {
                    ui.visuals().widgets.hovered.bg_fill
                } else {
                    ui.visuals().widgets.inactive.bg_fill
                };

                let stroke = if is_selected {
                    ui.visuals().selection.stroke
                } else if resp.hovered() {
                    ui.visuals().widgets.hovered.bg_stroke
                } else {
                    egui::Stroke::new(1.0_f32, egui::Color32::from_black_alpha(40))
                };

                ui.painter()
                    .rect(rect, 3.0, bg_color, stroke, egui::StrokeKind::Inside);

                let icon_color = if is_selected {
                    ui.visuals().strong_text_color()
                } else if resp.hovered() {
                    ui.visuals().text_color()
                } else {
                    ui.visuals().weak_text_color()
                };

                paint_tool_icon(ui.painter(), rect, tool, icon_color);

                // Tekst pomocniczy, jeśli ikona nie rysuje jeszcze kształtu
                let letter = match tool {
                    ActiveTool::Pan => "✥",
                    ActiveTool::Brush => "🖌",
                    ActiveTool::Eraser => "⌫",
                    ActiveTool::Bucket => "🪣",
                    ActiveTool::Gradient => "▨",
                    ActiveTool::Eyedropper => "💉",
                    ActiveTool::SelectRect => "▭",
                    ActiveTool::SelectEllipse => "⬭",
                    ActiveTool::MagicWand => "✦",
                };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    letter,
                    egui::FontId::proportional(14.0),
                    icon_color,
                );

                if resp.clicked() {
                    app.active_tool = tool;
                }

                resp.on_hover_text(tr(tooltip));

                if (idx + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
}

/// Renderuje kwadraty kolorów FG/BG nachodzące na siebie z ikonami zamiany (X) i resetu (D).
pub fn render_fg_bg_widget(app: &mut DziwakApp, ui: &mut egui::Ui) {
    let widget_size = egui::vec2(60.0, 44.0);
    let (rect, _resp) = ui.allocate_exact_size(widget_size, egui::Sense::hover());

    let p0 = rect.min;

    // Przycisk ResetColors (D) - mała ikonka w lewym dolnym rogu (14x14)
    let reset_rect = egui::Rect::from_min_size(p0 + egui::vec2(2.0, 26.0), egui::vec2(14.0, 14.0));
    let reset_resp = ui.allocate_rect(reset_rect, egui::Sense::click());
    if reset_resp.hovered() {
        ui.painter()
            .rect_filled(reset_rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let reset_color = if reset_resp.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    paint_ui_icon(ui.painter(), reset_rect, UiIcon::ResetColors, reset_color);
    // Mała miniatura czarno-biała
    ui.painter().rect_filled(
        egui::Rect::from_min_size(reset_rect.min + egui::vec2(2.0, 2.0), egui::vec2(5.0, 5.0)),
        0.0,
        egui::Color32::BLACK,
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_size(reset_rect.min + egui::vec2(6.0, 6.0), egui::vec2(5.0, 5.0)),
        0.0,
        egui::Color32::WHITE,
    );
    ui.painter().rect_stroke(
        reset_rect,
        1.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(100)),
        egui::StrokeKind::Inside,
    );

    if reset_resp.clicked() {
        app.brush_color = egui::Color32::from_rgb(0, 0, 0);
        app.bg_color = egui::Color32::WHITE;
    }
    reset_resp.on_hover_text(tr("Domyślne kolory: czarny / biały (D)"));

    // Przycisk SwapColors (X) - mała ikonka w prawym górnym rogu (14x14)
    let swap_rect = egui::Rect::from_min_size(p0 + egui::vec2(42.0, 2.0), egui::vec2(14.0, 14.0));
    let swap_resp = ui.allocate_rect(swap_rect, egui::Sense::click());
    if swap_resp.hovered() {
        ui.painter()
            .rect_filled(swap_rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let swap_color = if swap_resp.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    paint_ui_icon(ui.painter(), swap_rect, UiIcon::SwapColors, swap_color);
    ui.painter().text(
        swap_rect.center(),
        egui::Align2::CENTER_CENTER,
        "⇄",
        egui::FontId::proportional(12.0),
        swap_color,
    );
    if swap_resp.clicked() {
        std::mem::swap(&mut app.brush_color, &mut app.bg_color);
    }
    swap_resp.on_hover_text(tr("Zamień kolory (X)"));

    // Kwadrat koloru tła (BG) - 22x22, odsunięty w prawo i w dół
    let bg_square = egui::Rect::from_min_size(p0 + egui::vec2(22.0, 14.0), egui::vec2(22.0, 22.0));
    let bg_resp = ui.allocate_rect(bg_square, egui::Sense::click());
    ui.painter().rect_filled(bg_square, 2.0, app.bg_color);
    ui.painter().rect_stroke(
        bg_square,
        2.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(80)),
        egui::StrokeKind::Inside,
    );
    if bg_resp.clicked() {
        app.color_picker_target = match app.color_picker_target {
            Some(ColorPickerTarget::Background) => None,
            _ => Some(ColorPickerTarget::Background),
        };
    }
    bg_resp.on_hover_text(tr("Kolor tła (kliknij, aby edytować)"));

    // Kwadrat koloru pierwszoplanowego (FG) - 22x22, nałożony z przodu w lewym górnym rogu
    let fg_square = egui::Rect::from_min_size(p0 + egui::vec2(8.0, 4.0), egui::vec2(22.0, 22.0));
    let fg_resp = ui.allocate_rect(fg_square, egui::Sense::click());
    ui.painter().rect_filled(fg_square, 2.0, app.brush_color);
    ui.painter().rect_stroke(
        fg_square,
        2.0,
        egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
        egui::StrokeKind::Inside,
    );
    ui.painter().rect_stroke(
        fg_square.expand(1.0),
        2.0,
        egui::Stroke::new(1.0_f32, egui::Color32::BLACK),
        egui::StrokeKind::Outside,
    );
    if fg_resp.clicked() {
        app.color_picker_target = match app.color_picker_target {
            Some(ColorPickerTarget::Foreground) => None,
            _ => Some(ColorPickerTarget::Foreground),
        };
    }
    fg_resp.on_hover_text(tr("Kolor pierwszoplanowy (kliknij, aby edytować)"));
}

/// Renderuje lewy dok: skrzynka narzędzi, kwadraty FG/BG oraz opcje aktywnego narzędzia.
pub fn render_left_dock(app: &mut DziwakApp, ctx: &egui::Context) {
    egui::SidePanel::left("left_dock")
        .default_width(200.0)
        .width_range(170.0..=260.0)
        .resizable(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(tr("Narzędzia")).strong());
                ui.add_space(2.0);
                render_tool_grid(app, ui);

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    render_fg_bg_widget(app, ui);
                });

                ui.add_space(6.0);
                ui.separator();
                ui.label(egui::RichText::new(tr("Opcje narzędzia")).strong());
                ui.add_space(2.0);
                super::tool_options::render_tool_options(app, ui);
            });
        });

    // Okienko wyboru koloru w popupie jeśli kliknięto FG lub BG
    if let Some(target) = app.color_picker_target {
        let mut is_open = true;
        let title = match target {
            ColorPickerTarget::Foreground => "Wybór koloru pierwszoplanowego",
            ColorPickerTarget::Background => "Wybór koloru tła",
        };

        egui::Window::new(title)
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(210.0, 80.0))
            .show(ctx, |ui| match target {
                ColorPickerTarget::Foreground => {
                    egui::color_picker::color_picker_color32(
                        ui,
                        &mut app.brush_color,
                        egui::color_picker::Alpha::Opaque,
                    );
                }
                ColorPickerTarget::Background => {
                    egui::color_picker::color_picker_color32(
                        ui,
                        &mut app.bg_color,
                        egui::color_picker::Alpha::Opaque,
                    );
                }
            });

        if !is_open {
            app.color_picker_target = None;
        }
    }
}
