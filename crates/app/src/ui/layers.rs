use crate::i18n::tr;
use crate::state::{ActiveTool, DziwakApp, RightTab, BRUSH_PRESETS};
use crate::ui::icons::{paint_ui_icon, UiIcon};
use dziwak_core::BlendMode;
use eframe::egui;

fn render_layer_action_btn(
    ui: &mut egui::Ui,
    icon: UiIcon,
    label: &'static str,
    enabled: bool,
    tooltip: &'static str,
) -> egui::Response {
    let size = egui::vec2(22.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
    );
    if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let stroke = egui::Stroke::new(
        1.0_f32,
        if enabled {
            ui.visuals().widgets.inactive.bg_stroke.color
        } else {
            egui::Color32::from_gray(50)
        },
    );
    ui.painter()
        .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    let icon_color = if !enabled {
        egui::Color32::from_gray(70)
    } else if resp.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    paint_ui_icon(ui.painter(), rect, icon, icon_color);
    resp.on_hover_text(format!("{} ({})", tr(label), tr(tooltip)))
}

/// Renderuje listę 6 presetów pędzli (rozmiar/twardość).
pub fn render_brushes_presets(app: &mut DziwakApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .max_height(140.0)
        .show(ui, |ui| {
            for preset in &BRUSH_PRESETS {
                let is_current = (app.brush_size - preset.size).abs() < 0.5
                    && (app.brush_hardness - preset.hardness).abs() < 0.05;

                ui.horizontal(|ui| {
                    // Mała wizualizacja średnicy pędzla
                    let (dot_rect, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    let r = (preset.size * 0.35).clamp(2.0, 9.0);
                    ui.painter().circle_filled(
                        dot_rect.center(),
                        r,
                        if is_current {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().text_color()
                        },
                    );

                    let label = format!(
                        "{} — {:.0} px / {:.0}%",
                        tr(preset.name),
                        preset.size,
                        preset.hardness * 100.0
                    );
                    if ui.selectable_label(is_current, label).clicked() {
                        app.brush_size = preset.size;
                        app.brush_hardness = preset.hardness;
                        if !matches!(
                            app.active_tool,
                            ActiveTool::Brush
                                | ActiveTool::Pencil
                                | ActiveTool::Airbrush
                                | ActiveTool::Eraser
                        ) {
                            app.active_tool = ActiveTool::Brush;
                        }
                    }
                });
            }
        });
}

/// Menu kontekstowe aktywnej warstwy (prawy przycisk w panelu Warstwy).
fn layer_context_menu(app: &mut DziwakApp, ui: &mut egui::Ui) {
    let count = app.document.layer_count();
    let idx = app.active_layer_index;
    if ui.button(tr("Nowa warstwa")).clicked() {
        app.add_new_layer();
        ui.close();
    }
    if ui.button(tr("Duplikuj warstwę")).clicked() {
        app.duplicate_active_layer();
        ui.close();
    }
    if ui
        .add_enabled(count > 1, egui::Button::new(tr("Usuń warstwę")))
        .clicked()
    {
        app.remove_active_layer();
        ui.close();
    }
    ui.separator();
    if ui
        .add_enabled(idx + 1 < count, egui::Button::new(tr("Przesuń w górę")))
        .clicked()
    {
        app.move_active_layer_up();
        ui.close();
    }
    if ui
        .add_enabled(idx > 0, egui::Button::new(tr("Przesuń w dół")))
        .clicked()
    {
        app.move_active_layer_down();
        ui.close();
    }
    if ui
        .add_enabled(idx > 0, egui::Button::new(tr("Scal w dół")))
        .clicked()
    {
        app.merge_active_layer_down();
        ui.close();
    }
    ui.separator();
    ui.add_enabled(false, egui::Button::new(tr("Dodaj kanał alfa")))
        .on_disabled_hover_text(tr("Warstwy Dziwaka zawsze mają kanał alfa (RGBA)"));
    if ui.button(tr("Alfa do zaznaczenia")).clicked() {
        app.alpha_to_selection();
        ui.close();
    }
    if ui
        .button(tr("Kolor na przezroczystość"))
        .on_hover_text(tr("Usuwa kolor pierwszoplanowy (tolerancja jak kubełka)"))
        .clicked()
    {
        app.color_to_alpha_active();
        ui.close();
    }
    if ui.button(tr("Usuń jednolite tło")).clicked() {
        app.remove_uniform_background();
        ui.close();
    }
    #[cfg(feature = "ai")]
    if ui
        .add_enabled(app.ai_rx.is_none(), egui::Button::new(tr("Usuń tło (AI)")))
        .clicked()
    {
        app.start_ai_background(false);
        ui.close();
    }
    ui.separator();
    if ui
        .add_enabled(count > 1, egui::Button::new(tr("Spłaszcz obraz")))
        .clicked()
    {
        app.flatten_image();
        ui.close();
    }
}

/// Renderuje zawartość panelu warstw.
pub fn render_layers_content(app: &mut DziwakApp, ui: &mut egui::Ui) {
    // Tryb mieszania aktywnej warstwy
    let active_blend = app
        .document
        .layers
        .get(app.active_layer_index)
        .map(|l| l.blend);
    if let Some(current_blend) = active_blend {
        let mut chosen = current_blend;
        ui.horizontal(|ui| {
            ui.label(tr("Tryb:"));
            egui::ComboBox::from_id_salt("blend_mode_combo")
                .selected_text(tr(current_blend.label()))
                .show_ui(ui, |ui| {
                    for mode in BlendMode::ALL {
                        ui.selectable_value(&mut chosen, mode, tr(mode.label()));
                    }
                });
        });
        if chosen != current_blend {
            app.push_history();
            if let Some(layer) = app.document.layers.get_mut(app.active_layer_index) {
                layer.blend = chosen;
            }
            app.mark_document_dirty();
        }
        ui.separator();
    }

    // Lista warstw od góry (najwyższa warstwa u góry)
    // Suwak krycia wybranej (aktywnej) warstwy (0-100%)
    let active_opacity = app
        .document
        .layers
        .get(app.active_layer_index)
        .map(|l| l.opacity);
    if let Some(current_opacity) = active_opacity {
        ui.horizontal(|ui| {
            ui.label(tr("Krycie:"));
            let mut opacity_pct = (current_opacity * 100.0).round();
            let slider = egui::Slider::new(&mut opacity_pct, 0.0..=100.0).suffix("%");
            let resp = ui.add(slider);

            if resp.drag_started() {
                app.push_history();
                app.is_dragging_opacity = true;
            }
            if resp.changed() {
                if !app.is_dragging_opacity {
                    app.push_history();
                }
                if let Some(layer) = app.document.layers.get_mut(app.active_layer_index) {
                    layer.opacity = (opacity_pct / 100.0).clamp(0.0, 1.0);
                }
                app.mark_document_dirty();
            }
            if resp.drag_stopped() {
                app.is_dragging_opacity = false;
            }
        });
        ui.separator();
    }

    // Zablokuj: piksele, położenie
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr("Zablokuj:")).small());
        ui.add_enabled(false, egui::Checkbox::new(&mut false, tr("piksele")));
        ui.add_enabled(false, egui::Checkbox::new(&mut false, tr("położenie")));
    });
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut layer_to_toggle_vis = None;
        let mut layer_to_select = None;
        let mut start_rename = None;
        let mut commit_rename = None;

        let count = app.document.layer_count();
        for idx in (0..count).rev() {
            let thumb = app.layer_thumbnail_texture(&ui.ctx().clone(), idx);
            let is_active = idx == app.active_layer_index;
            let layer = &app.document.layers[idx];

            let row = ui.horizontal(|ui| {
                // Ikona oka (widoczność) + checkbox widoczności
                let eye_size = egui::vec2(16.0, 16.0);
                let (eye_rect, eye_resp) = ui.allocate_exact_size(eye_size, egui::Sense::click());
                let eye_color = if layer.visible {
                    ui.visuals().text_color()
                } else {
                    egui::Color32::from_gray(80)
                };
                paint_ui_icon(ui.painter(), eye_rect, UiIcon::Eye, eye_color);
                if eye_resp.clicked() {
                    layer_to_toggle_vis = Some(idx);
                }
                eye_resp.on_hover_text(if layer.visible {
                    "Ukryj warstwę"
                } else {
                    "Pokaż warstwę"
                });

                // Miniatura warstwy
                let (thumb_rect, thumb_resp) =
                    ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
                ui.painter()
                    .rect_filled(thumb_rect, 0.0, egui::Color32::from_gray(90));
                if let Some((tex_id, size)) = thumb {
                    let scale = (32.0 / size.x.max(size.y)).min(1.0);
                    let img_rect = egui::Rect::from_center_size(thumb_rect.center(), size * scale);
                    ui.painter().image(
                        tex_id,
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                if thumb_resp.clicked() {
                    layer_to_select = Some(idx);
                }
                let mut menu_resp = thumb_resp.clone();

                if app.editing_layer_index == Some(idx) {
                    let text_resp = ui.text_edit_singleline(&mut app.editing_layer_name);
                    if app.request_focus_rename {
                        text_resp.request_focus();
                        app.request_focus_rename = false;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        app.editing_layer_index = None;
                    } else if text_resp.lost_focus()
                        || ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        commit_rename = Some((idx, app.editing_layer_name.clone()));
                    }
                } else {
                    let name = if is_active {
                        egui::RichText::new(&layer.name).strong()
                    } else {
                        egui::RichText::new(&layer.name)
                    };
                    let label_resp = ui.add(
                        egui::Label::new(name)
                            .selectable(false)
                            .sense(egui::Sense::click())
                            .truncate(),
                    );
                    menu_resp = menu_resp.union(label_resp.clone());
                    if label_resp.clicked() {
                        layer_to_select = Some(idx);
                    }
                    if label_resp.double_clicked() {
                        start_rename = Some((idx, layer.name.clone()));
                    }
                }
                menu_resp
            });

            if is_active {
                ui.painter().rect_filled(
                    row.response.rect,
                    2.0,
                    ui.visuals().selection.bg_fill.gamma_multiply(0.35),
                );
            }

            // Prawy przycisk: menu kontekstowe warstwy (jak w GIMP-ie)
            let row_resp = row.inner;
            if row_resp.secondary_clicked() {
                layer_to_select = Some(idx);
            }
            row_resp.context_menu(|ui| {
                app.active_layer_index = idx;
                layer_context_menu(app, ui);
            });
        }

        if let Some(idx) = layer_to_toggle_vis {
            app.toggle_layer_visibility(idx);
        }
        if let Some(idx) = layer_to_select {
            app.active_layer_index = idx;
        }
        if let Some((idx, name)) = start_rename {
            app.editing_layer_index = Some(idx);
            app.editing_layer_name = name;
            app.request_focus_rename = true;
            app.active_layer_index = idx;
        }
        if let Some((idx, new_name)) = commit_rename {
            app.rename_layer(idx, new_name);
            app.editing_layer_index = None;
        }
    });

    // Przyciski akcji: Dodaj, Usuń, Duplikuj oraz W górę, W dół
    ui.separator();
    ui.horizontal(|ui| {
        if render_layer_action_btn(ui, UiIcon::NewLayer, "Dodaj", true, "Nowa warstwa").clicked() {
            app.add_new_layer();
        }
        let can_move_up = app.active_layer_index + 1 < app.document.layer_count();
        if render_layer_action_btn(ui, UiIcon::Raise, "W górę", can_move_up, "Podnieś warstwę")
            .clicked()
        {
            app.move_active_layer_up();
        }
        let can_move_down =
            app.active_layer_index > 0 && app.active_layer_index < app.document.layer_count();
        if render_layer_action_btn(ui, UiIcon::Lower, "W dół", can_move_down, "Obniż warstwę")
            .clicked()
        {
            app.move_active_layer_down();
        }
        if render_layer_action_btn(ui, UiIcon::Duplicate, "Duplikuj", true, "Duplikuj warstwę")
            .clicked()
        {
            app.duplicate_active_layer();
        }
        let can_remove = app.document.layer_count() > 1;
        if render_layer_action_btn(ui, UiIcon::Delete, "Usuń", can_remove, "Usuń warstwę").clicked()
        {
            app.remove_active_layer();
        }
    });
}

/// Renderuje prawy dok (~260 px): góra zakładki 'Pędzle' / 'Historia cofania', dół 'Warstwy'.
pub fn render_right_dock(
    app: &mut DziwakApp,
    ctx: &egui::Context,
    should_fit_to_viewport: &mut bool,
) {
    egui::SidePanel::right("right_dock")
        .default_width(260.0)
        .width_range(220.0..=360.0)
        .resizable(true)
        .show(ctx, |ui| {
            // Górna część: zakładki
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.right_tab, RightTab::Brushes, tr("Pędzle"));
                ui.selectable_value(
                    &mut app.right_tab,
                    RightTab::History,
                    tr("Historia cofania"),
                );
            });
            ui.separator();

            match app.right_tab {
                RightTab::Brushes => {
                    render_brushes_presets(app, ui);
                }
                RightTab::History => {
                    super::history_panel::render_history_panel(app, ui, should_fit_to_viewport);
                }
            }

            ui.add_space(6.0);
            ui.separator();
            render_layers_content(app, ui);
        });
}
