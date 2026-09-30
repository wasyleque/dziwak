use crate::i18n::tr;
use crate::state::{ActiveTool, ColorPickerTarget, DziwakApp};
use crate::ui::icons::{paint_tool_icon_by_kind, paint_ui_icon, ToolIcon, UiIcon};
use eframe::egui;

/// Definicja pojedynczego narzędzia w skrzynce narzędzi.
pub struct ToolItem {
    pub icon: ToolIcon,
    pub tooltip: &'static str,
    pub active_tool: Option<ActiveTool>,
}

/// Grupa narzędzi w skrzynce narzędzi w stylu GIMP 2.10.
pub struct ToolGroup {
    pub name: &'static str,
    pub id: &'static str,
    pub tools: &'static [ToolItem],
}

pub static TOOL_GROUPS: &[ToolGroup] = &[
    ToolGroup {
        name: "Zaznaczenia",
        id: "group_selection",
        tools: &[
            ToolItem {
                icon: ToolIcon::SelectRect,
                tooltip: "Zaznaczenie prostokątne (R)",
                active_tool: Some(ActiveTool::SelectRect),
            },
            ToolItem {
                icon: ToolIcon::SelectEllipse,
                tooltip: "Zaznaczenie eliptyczne (E)",
                active_tool: Some(ActiveTool::SelectEllipse),
            },
            ToolItem {
                icon: ToolIcon::SelectFree,
                tooltip: "Zaznaczenie odręczne (F)",
                active_tool: Some(ActiveTool::SelectFree),
            },
            ToolItem {
                icon: ToolIcon::MagicWand,
                tooltip: "Różdżka (U)",
                active_tool: Some(ActiveTool::MagicWand),
            },
            ToolItem {
                icon: ToolIcon::SelectColor,
                tooltip: "Zaznaczenie wg koloru (Shift+O)",
                active_tool: Some(ActiveTool::SelectColor),
            },
        ],
    },
    ToolGroup {
        name: "Transformacje",
        id: "group_transform",
        tools: &[
            ToolItem {
                icon: ToolIcon::Move,
                tooltip: "Przesuwanie (M)",
                active_tool: Some(ActiveTool::Move),
            },
            ToolItem {
                icon: ToolIcon::Crop,
                tooltip: "Kadrowanie (Shift+C)",
                active_tool: Some(ActiveTool::Crop),
            },
            ToolItem {
                icon: ToolIcon::Rotate,
                tooltip: "Obrót (Shift+R)",
                active_tool: Some(ActiveTool::Rotate),
            },
            ToolItem {
                icon: ToolIcon::Scale,
                tooltip: "Skalowanie (Shift+T)",
                active_tool: Some(ActiveTool::Scale),
            },
            ToolItem {
                icon: ToolIcon::Flip,
                tooltip: "Odbicie (Shift+F)",
                active_tool: Some(ActiveTool::Flip),
            },
        ],
    },
    ToolGroup {
        name: "Malowanie",
        id: "group_paint",
        tools: &[
            ToolItem {
                icon: ToolIcon::Pencil,
                tooltip: "Ołówek (N)",
                active_tool: Some(ActiveTool::Pencil),
            },
            ToolItem {
                icon: ToolIcon::Brush,
                tooltip: "Pędzel (P)",
                active_tool: Some(ActiveTool::Brush),
            },
            ToolItem {
                icon: ToolIcon::Airbrush,
                tooltip: "Aerograf (A)",
                active_tool: Some(ActiveTool::Airbrush),
            },
            ToolItem {
                icon: ToolIcon::Eraser,
                tooltip: "Gumka (Shift+E)",
                active_tool: Some(ActiveTool::Eraser),
            },
            ToolItem {
                icon: ToolIcon::Bucket,
                tooltip: "Kubełek (Shift+B)",
                active_tool: Some(ActiveTool::Bucket),
            },
            ToolItem {
                icon: ToolIcon::Gradient,
                tooltip: "Gradient (G)",
                active_tool: Some(ActiveTool::Gradient),
            },
            ToolItem {
                icon: ToolIcon::Clone,
                tooltip: "Klonowanie (C)",
                active_tool: Some(ActiveTool::Clone),
            },
            ToolItem {
                icon: ToolIcon::Smudge,
                tooltip: "Rozmazywanie (Shift+S)",
                active_tool: Some(ActiveTool::Smudge),
            },
            ToolItem {
                icon: ToolIcon::DodgeBurn,
                tooltip: "Rozjaśnianie/Ściemnianie (Shift+D)",
                active_tool: Some(ActiveTool::DodgeBurn),
            },
            ToolItem {
                icon: ToolIcon::BlurSharpen,
                tooltip: "Rozmywanie/Wyostrzanie (Shift+U)",
                active_tool: Some(ActiveTool::BlurSharpen),
            },
        ],
    },
    ToolGroup {
        name: "Kolory i inne",
        id: "group_other",
        tools: &[
            ToolItem {
                icon: ToolIcon::Eyedropper,
                tooltip: "Pipeta (O)",
                active_tool: Some(ActiveTool::Eyedropper),
            },
            ToolItem {
                icon: ToolIcon::Text,
                tooltip: "Tekst (T)",
                active_tool: Some(ActiveTool::Text),
            },
            ToolItem {
                icon: ToolIcon::Zoom,
                tooltip: "Lupa (Z)",
                active_tool: Some(ActiveTool::Zoom),
            },
            ToolItem {
                icon: ToolIcon::Measure,
                tooltip: "Miarka (Shift+M)",
                active_tool: Some(ActiveTool::Measure),
            },
            ToolItem {
                icon: ToolIcon::Pan,
                tooltip: "Przesuwanie widoku (Spacja)",
                active_tool: Some(ActiveTool::Pan),
            },
        ],
    },
];

/// Renderuje siatkę przycisków narzędzi podzieloną na grupy w stylu GIMP 2.10.
pub fn render_tool_grid(app: &mut DziwakApp, ui: &mut egui::Ui) {
    let btn_size = egui::vec2(28.0, 28.0);
    let spacing = 4.0;
    let avail_w = ui.available_width();
    let cols = ((avail_w + spacing) / (btn_size.x + spacing))
        .floor()
        .max(1.0) as usize;

    for group in TOOL_GROUPS {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(tr(group.name)).size(10.5).weak());
        ui.add_space(1.0);

        egui::Grid::new(group.id)
            .spacing([spacing, spacing])
            .show(ui, |ui| {
                for (idx, tool) in group.tools.iter().enumerate() {
                    let is_selected = tool.active_tool.is_some_and(|t| app.active_tool == t);
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
                    } else if tool.active_tool.is_some() {
                        ui.visuals().weak_text_color()
                    } else {
                        ui.visuals().weak_text_color().gamma_multiply(0.45)
                    };

                    paint_tool_icon_by_kind(ui.painter(), rect, tool.icon, icon_color);

                    if resp.clicked() {
                        if let Some(target_tool) = tool.active_tool {
                            app.set_active_tool(target_tool);
                        }
                    }

                    resp.on_hover_text(tr(tool.tooltip));

                    if (idx + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_groups_definitions() {
        assert_eq!(TOOL_GROUPS.len(), 4);
        assert_eq!(TOOL_GROUPS[0].name, "Zaznaczenia");
        assert_eq!(TOOL_GROUPS[1].name, "Transformacje");
        assert_eq!(TOOL_GROUPS[2].name, "Malowanie");
        assert_eq!(TOOL_GROUPS[3].name, "Kolory i inne");

        for group in TOOL_GROUPS {
            assert!(!group.tools.is_empty());
            for tool in group.tools {
                assert!(!tool.tooltip.is_empty());
            }
        }
    }
}
