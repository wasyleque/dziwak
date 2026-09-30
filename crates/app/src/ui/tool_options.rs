use crate::i18n::tr;
use crate::state::{ActiveTool, DziwakApp};
use eframe::egui;

/// Renderuje kontrolki opcji aktywnego narzędzia.
pub fn render_tool_options(app: &mut DziwakApp, ui: &mut egui::Ui) {
    match app.active_tool {
        ActiveTool::Brush | ActiveTool::Eraser | ActiveTool::Airbrush => {
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
        ActiveTool::Clone => {
            ui.label(tr("Klonowanie:"));
            ui.label(
                egui::RichText::new(tr(
                    "Ctrl+klik na płótnie: wybierz źródło.\nMalowanie LPM: kopiowanie z przesunięciem (wyrównane).",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.separator();

            ui.label(tr("Rozmiar:"));
            ui.add(egui::Slider::new(&mut app.brush_size, 1.0..=200.0).suffix(" px"));
            ui.separator();

            ui.label(tr("Twardość:"));
            ui.add(egui::Slider::new(&mut app.brush_hardness, 0.0..=1.0));
            ui.separator();

            ui.label(tr("Krycie:"));
            ui.add(egui::Slider::new(&mut app.brush_opacity, 0.0..=1.0));
            ui.separator();

            if let Some((sx, sy)) = app.clone_source {
                ui.label(format!("{}: {:.0}×{:.0} px", tr("Źródło"), sx, sy));
            } else {
                ui.label(
                    egui::RichText::new(tr("Źródło nieustawione (użyj Ctrl+klik)"))
                        .small()
                        .color(egui::Color32::YELLOW),
                );
            }
            ui.separator();
        }
        ActiveTool::Smudge => {
            ui.label(tr("Rozmazywanie:"));
            ui.label(
                egui::RichText::new(tr("Przeciągnij myszą, aby rozmazać piksele."))
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.separator();

            ui.label(tr("Rozmiar:"));
            ui.add(egui::Slider::new(&mut app.brush_size, 1.0..=200.0).suffix(" px"));
            ui.separator();

            ui.label(tr("Twardość:"));
            ui.add(egui::Slider::new(&mut app.brush_hardness, 0.0..=1.0));
            ui.separator();

            ui.label(tr("Siła:"));
            ui.add(egui::Slider::new(&mut app.smudge_rate, 0.0..=1.0));
            ui.separator();
        }
        ActiveTool::DodgeBurn => {
            ui.label(tr("Rozjaśnianie / Ściemnianie:"));
            ui.label(
                egui::RichText::new(tr("Ctrl odwraca tryb rozjaśnianie ↔ ściemnianie."))
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.separator();

            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut app.dodge_burn_type,
                    crate::state::DodgeBurnType::Dodge,
                    tr("Rozjaśnianie"),
                );
                ui.radio_value(
                    &mut app.dodge_burn_type,
                    crate::state::DodgeBurnType::Burn,
                    tr("Ściemnianie"),
                );
            });
            ui.separator();

            ui.label(tr("Rozmiar:"));
            ui.add(egui::Slider::new(&mut app.brush_size, 1.0..=200.0).suffix(" px"));
            ui.separator();

            ui.label(tr("Twardość:"));
            ui.add(egui::Slider::new(&mut app.brush_hardness, 0.0..=1.0));
            ui.separator();

            ui.label(tr("Ekspozycja:"));
            ui.add(egui::Slider::new(&mut app.dodge_burn_exposure, 0.0..=1.0));
            ui.separator();
        }
        ActiveTool::BlurSharpen => {
            ui.label(tr("Rozmywanie / Wyostrzanie:"));
            ui.label(
                egui::RichText::new(tr("Ctrl odwraca tryb rozmywanie ↔ wyostrzanie."))
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.separator();

            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut app.blur_sharpen_type,
                    crate::state::BlurSharpenType::Blur,
                    tr("Rozmywanie"),
                );
                ui.radio_value(
                    &mut app.blur_sharpen_type,
                    crate::state::BlurSharpenType::Sharpen,
                    tr("Wyostrzanie"),
                );
            });
            ui.separator();

            ui.label(tr("Rozmiar:"));
            ui.add(egui::Slider::new(&mut app.brush_size, 1.0..=200.0).suffix(" px"));
            ui.separator();

            ui.label(tr("Twardość:"));
            ui.add(egui::Slider::new(&mut app.brush_hardness, 0.0..=1.0));
            ui.separator();

            ui.label(tr("Siła:"));
            ui.add(egui::Slider::new(&mut app.blur_sharpen_rate, 0.0..=1.0));
            ui.separator();
        }
        ActiveTool::Text => {
            ui.label(tr("Narzędzie Tekst:"));
            ui.label(
                egui::RichText::new(tr(
                    "Kliknij na płótnie, aby umieścić tekst, lub otwórz okno edycji tekstu.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.separator();

            if ui.button(tr("Otwórz okno tekstu...")).clicked() && app.text_dialog.is_none() {
                let (x, y) = if let Some(pos) = app.cursor_canvas_pos {
                    (pos.x.round() as i32, pos.y.round() as i32)
                } else {
                    (50, 50)
                };
                app.open_text_dialog(x, y);
            }
            ui.separator();
        }
        ActiveTool::Pencil => {
            ui.label(tr("Rozmiar:"));
            ui.add(egui::Slider::new(&mut app.brush_size, 1.0..=200.0).suffix(" px"));
            ui.separator();

            ui.label(tr("Krycie:"));
            ui.add(egui::Slider::new(&mut app.brush_opacity, 0.0..=1.0));
            ui.separator();
        }
        ActiveTool::Bucket | ActiveTool::MagicWand | ActiveTool::SelectColor => {
            ui.label(tr("Tolerancja:"));
            ui.add(egui::Slider::new(&mut app.fill_tolerance, 0..=255));
            ui.checkbox(
                &mut app.fill_sample_all_layers,
                tr("Próbkuj wszystkie warstwy"),
            );
            if app.active_tool == ActiveTool::SelectColor {
                ui.label(
                    egui::RichText::new(tr(
                        "Kliknij na płótnie, aby zaznaczyć piksele o podobnym kolorze w całym obrazie.\nShift: dodaj, Ctrl: odejmij, Shift+Ctrl: przetnij.",
                    ))
                    .small()
                    .color(egui::Color32::GRAY),
                );
            }
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
        ActiveTool::SelectFree => {
            ui.label(tr("Zaznaczenie odręczne:"));
            ui.label(
                egui::RichText::new(tr(
                    "Klikaj lub przeciągaj myszą, aby utworzyć wielokąt.\nDwuklik lub Enter zamyka zaznaczenie. Esc anuluje.\nShift: dodaj, Ctrl: odejmij, Shift+Ctrl: przetnij.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            if !app.free_select_points.is_empty() {
                ui.label(format!(
                    "{}: {}",
                    tr("Liczba punktów:"),
                    app.free_select_points.len()
                ));
            }
            ui.horizontal(|ui| {
                let can_close = app.free_select_points.len() >= 3;
                if ui
                    .add_enabled(can_close, egui::Button::new(tr("Zamknij (Enter)")))
                    .clicked()
                {
                    app.finish_free_select();
                }
                if ui
                    .add_enabled(
                        !app.free_select_points.is_empty(),
                        egui::Button::new(tr("Anuluj (Esc)")),
                    )
                    .clicked()
                {
                    app.cancel_free_select();
                }
            });
            ui.separator();
        }
        ActiveTool::Move => {
            ui.label(tr("Przesuwanie:"));
            ui.label(
                egui::RichText::new(tr(
                    "Przeciągnij na płótnie, aby przesunąć piksele aktywnej warstwy lub zaznaczenia.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.separator();
        }
        ActiveTool::Crop => {
            ui.label(tr("Kadrowanie:"));
            ui.label(
                egui::RichText::new(tr(
                    "Przeciągnij prostokąt na płótnie, a następnie naciśnij Enter, aby przyciąć obraz. Esc anuluje.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            if let Some(r) = app.crop_rect {
                ui.label(format!("{}×{} px (+{}, +{})", r.width, r.height, r.x, r.y));
            }
            ui.horizontal(|ui| {
                let has_crop = app.crop_rect.is_some() || app.crop_drag_start.is_some();
                if ui
                    .add_enabled(has_crop, egui::Button::new(tr("Przytnij (Enter)")))
                    .clicked()
                {
                    app.commit_crop();
                }
                if ui
                    .add_enabled(
                        app.crop_rect.is_some(),
                        egui::Button::new(tr("Anuluj (Esc)")),
                    )
                    .clicked()
                {
                    app.cancel_crop();
                }
            });
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
        ActiveTool::Rotate => {
            ui.label(tr("Obrót:"));
            ui.label(
                egui::RichText::new(tr(
                    "Obraca aktywną warstwę wokół środka z interpolacją dwuliniową.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            if app.transform_dialog.is_none() && ui.button(tr("Otwórz okno obrotu...")).clicked() {
                app.open_transform_dialog(crate::state::LayerTransformKind::Rotate {
                    angle_deg: 0.0,
                });
            }
            ui.separator();
        }
        ActiveTool::Scale => {
            ui.label(tr("Skalowanie:"));
            ui.label(
                egui::RichText::new(tr(
                    "Skaluje zawartość aktywnej warstwy z interpolacją dwuliniową.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            if app.transform_dialog.is_none()
                && ui.button(tr("Otwórz okno skalowania...")).clicked()
            {
                let (w, h) = (app.document.width, app.document.height);
                app.open_transform_dialog(crate::state::LayerTransformKind::Scale {
                    width: w,
                    height: h,
                    orig_width: w,
                    orig_height: h,
                    keep_aspect: true,
                });
            }
            ui.separator();
        }
        ActiveTool::Flip => {
            ui.label(tr("Odbicie:"));
            ui.label(
                egui::RichText::new(tr("Odbija aktywną warstwę w poziomie lub w pionie."))
                    .small()
                    .color(egui::Color32::GRAY),
            );
            if app.transform_dialog.is_none() && ui.button(tr("Otwórz okno odbicia...")).clicked()
            {
                app.open_transform_dialog(crate::state::LayerTransformKind::Flip {
                    horizontal: true,
                });
            }
            ui.separator();
        }
        ActiveTool::Zoom => {
            ui.label(tr("Lupa:"));
            ui.label(
                egui::RichText::new(tr(
                    "Kliknij LPM, aby powiększyć. Alt/Ctrl+klik, aby pomniejszyć. Przeciągnij, aby powiększyć obszar.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.add_space(4.0_f32);
            ui.horizontal(|ui| {
                if ui.button(tr("Powiększ (+)")).clicked() {
                    let center = egui::pos2(
                        app.document.width as f32 * 0.5,
                        app.document.height as f32 * 0.5,
                    );
                    let screen_center = app.transform.canvas_to_screen(center);
                    app.transform.zoom_around(screen_center, 1.5, 0.05, 64.0);
                    app.tile_renderer.set_zoom(app.transform.zoom);
                    app.auto_fit = false;
                }
                if ui.button(tr("Pomniejsz (-)")).clicked() {
                    let center = egui::pos2(
                        app.document.width as f32 * 0.5,
                        app.document.height as f32 * 0.5,
                    );
                    let screen_center = app.transform.canvas_to_screen(center);
                    app.transform
                        .zoom_around(screen_center, 1.0 / 1.5, 0.05, 64.0);
                    app.tile_renderer.set_zoom(app.transform.zoom);
                    app.auto_fit = false;
                }
            });
            ui.separator();
        }
        ActiveTool::Measure => {
            ui.label(tr("Miarka:"));
            ui.label(
                egui::RichText::new(tr(
                    "Przeciągnij lewym przyciskiem myszy po płótnie, aby zmierzyć odległość i kąt.",
                ))
                .small()
                .color(egui::Color32::GRAY),
            );
            if let Some((len, angle, dx, dy)) = app.measure_stats() {
                ui.add_space(4.0_f32);
                ui.label(format!("{}: {:.1} px", tr("Długość"), len));
                ui.label(format!("{}: {:.1}°", tr("Kąt"), angle));
                ui.label(format!("ΔX: {:.1} px, ΔY: {:.1} px", dx, dy));
                if ui.button(tr("Wyczyść pomiar")).clicked() {
                    app.measure_start = None;
                    app.measure_end = None;
                }
            }
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
