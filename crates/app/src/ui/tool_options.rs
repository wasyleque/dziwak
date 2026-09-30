use crate::i18n::tr;
use crate::state::{ActiveTool, DziwakApp};
use eframe::egui;

/// Renderuje kontrolki opcji aktywnego narzędzia.
pub fn render_tool_options(app: &mut DziwakApp, ui: &mut egui::Ui) {
    match app.active_tool {
        ActiveTool::Brush | ActiveTool::Eraser => {
            ui.label(tr("Rozmiar:"));
            ui.add(egui::Slider::new(&mut app.brush_size, 1.0..=200.0).suffix(" px"));
            ui.separator();

            ui.label(tr("Twardość:"));
            ui.add(egui::Slider::new(&mut app.brush_hardness, 0.0..=1.0));
            ui.separator();

            ui.label(tr("Krycie:"));
            ui.add(egui::Slider::new(&mut app.brush_opacity, 0.0..=1.0));
            ui.separator();
        }
        ActiveTool::Bucket | ActiveTool::MagicWand => {
            ui.label(tr("Tolerancja:"));
            ui.add(egui::Slider::new(&mut app.fill_tolerance, 0..=255));
            ui.checkbox(
                &mut app.fill_sample_all_layers,
                tr("Próbkuj wszystkie warstwy"),
            );
            ui.separator();
        }
        ActiveTool::Eyedropper => {
            ui.label(tr("Próbkowanie:"));
            egui::ComboBox::from_id_salt("pipette_radius_combo")
                .selected_text(match app.pipette_radius {
                    0 => "1×1 px",
                    1 => "3×3 px",
                    2 => "5×5 px",
                    _ => "1×1 px",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.pipette_radius, 0, tr("1×1 px"));
                    ui.selectable_value(&mut app.pipette_radius, 1, tr("3×3 px"));
                    ui.selectable_value(&mut app.pipette_radius, 2, tr("5×5 px"));
                });
            ui.separator();
        }
        ActiveTool::Gradient => {
            ui.label(tr("Gradient liniowy:"));
            ui.label(
                egui::RichText::new(
                    "Przeciągnij myszą na płótnie od koloru pierwszoplanowego do tła.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.separator();
        }
        ActiveTool::SelectRect | ActiveTool::SelectEllipse => {
            ui.label(tr("Zaznaczenie:"));
            ui.label(
                egui::RichText::new(
                    "Przeciągnij na płótnie (z Shift: równe boki).\nCtrl+A: wszystko, Ctrl+Shift+A: odznacz.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.separator();
        }
        ActiveTool::Pan => {
            ui.label(tr("Przesuwanie:"));
            ui.label(
                egui::RichText::new(tr(
                    "Przeciągnij myszą z LPM lub ŚPM, aby nawigować po płótnie.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.separator();
        }
    }

    if app.document.layer_count() > 1 {
        ui.label(tr("Aktywna warstwa:"));
        let current_layer_name = app
            .document
            .layer(app.active_layer_index.min(app.document.layer_count() - 1))
            .map(|l| l.name.as_str())
            .unwrap_or("—");
        egui::ComboBox::from_id_salt("active_layer_combo")
            .selected_text(current_layer_name)
            .width(150.0)
            .show_ui(ui, |ui| {
                for (idx, layer) in app.document.layers.iter().enumerate().rev() {
                    ui.selectable_value(&mut app.active_layer_index, idx, &layer.name);
                }
            });
    }
}
