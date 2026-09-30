use crate::i18n::tr;
use crate::state::{ActiveTool, DziwakApp, FilterKind, SelectionShape};
use eframe::egui;

/// Renderuje menu górne aplikacji w stylu GIMP 2.10.
pub fn render_menu_bar(
    app: &mut DziwakApp,
    ctx: &egui::Context,
    should_fit_to_viewport: &mut bool,
) {
    egui::TopBottomPanel::top("top_menu_panel").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            // 1. Plik
            ui.menu_button(tr("Plik"), |ui| {
                if ui.button(tr("Otwórz... (Ctrl+O)")).clicked() {
                    app.open_file_dialog();
                    ui.close();
                }
                if ui.button(tr("Zapisz jako... (Ctrl+Shift+S)")).clicked() {
                    app.save_file_dialog();
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Zakończ")).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            // 2. Edycja
            ui.menu_button(tr("Edycja"), |ui| {
                let undo_btn = ui.add_enabled(
                    app.history.can_undo(),
                    egui::Button::new(tr("Cofnij (Ctrl+Z)")),
                );
                if undo_btn.clicked() {
                    if app.handle_undo() {
                        *should_fit_to_viewport = true;
                    }
                    ui.close();
                }

                let redo_btn = ui.add_enabled(
                    app.history.can_redo(),
                    egui::Button::new(tr("Ponów (Ctrl+Shift+Z / Ctrl+Y)")),
                );
                if redo_btn.clicked() {
                    if app.handle_redo() {
                        *should_fit_to_viewport = true;
                    }
                    ui.close();
                }
                ui.separator();
                ui.menu_button(tr("Język"), |ui| {
                    for lang in crate::i18n::Lang::ALL {
                        let current = crate::i18n::lang() == lang;
                        if ui.radio(current, lang.native_name()).clicked() {
                            crate::i18n::set_lang(lang);
                            crate::i18n::save_lang(lang);
                            ui.close();
                        }
                    }
                });
            });

            // 3. Zaznaczenie
            ui.menu_button(tr("Zaznaczenie"), |ui| {
                if ui.button(tr("Zaznacz wszystko (Ctrl+A)")).clicked() {
                    app.selection
                        .select_rect(0, 0, app.document.width, app.document.height);
                    app.selection_shape = SelectionShape::Rect;
                    app.selection_drag_start = None;
                    app.selection_drag_current = None;
                    ui.close();
                }
                let deselect_btn = ui.add_enabled(
                    app.selection.has_selection,
                    egui::Button::new(tr("Odznacz (Ctrl+Shift+A)")),
                );
                if deselect_btn.clicked() {
                    app.selection.clear();
                    app.selection_drag_start = None;
                    app.selection_drag_current = None;
                    ui.close();
                }
                if ui.button(tr("Odwróć zaznaczenie (Ctrl+I)")).clicked() {
                    app.selection.invert();
                    app.selection_shape = SelectionShape::Rect;
                    ui.close();
                }
                #[cfg(feature = "ai")]
                {
                    ui.separator();
                    if ui
                        .add_enabled(
                            app.ai_rx.is_none(),
                            egui::Button::new(tr("Pierwszy plan (AI)")),
                        )
                        .on_hover_text(tr("Zaznacza główny obiekt modelem U²-Net-p na CPU"))
                        .clicked()
                    {
                        app.start_ai_background(true);
                        app.selection_shape = SelectionShape::Rect;
                        ui.close();
                    }
                }
            });

            // 4. Widok (Powiększ, Pomniejsz, 1:1, Dopasuj)
            ui.menu_button(tr("Widok"), |ui| {
                if ui.button(tr("Powiększ (+)")).clicked() {
                    app.transform.zoom = (app.transform.zoom * 1.25).clamp(0.05, 64.0);
                    app.tile_renderer.set_zoom(app.transform.zoom);
                    ui.close();
                }
                if ui.button(tr("Pomniejsz (-)")).clicked() {
                    app.transform.zoom = (app.transform.zoom / 1.25).clamp(0.05, 64.0);
                    app.tile_renderer.set_zoom(app.transform.zoom);
                    ui.close();
                }
                if ui.button(tr("Rzeczywisty rozmiar (1:1)")).clicked() {
                    app.transform.zoom = 1.0;
                    app.tile_renderer.set_zoom(1.0);
                    ui.close();
                }
                if ui
                    .add(egui::Button::new(tr("Dopasuj do okna")).shortcut_text("Shift+Ctrl+J"))
                    .clicked()
                {
                    *should_fit_to_viewport = true;
                    ui.close();
                }
                if ui
                    .add(egui::Button::new(tr("Wyśrodkuj obraz")).shortcut_text("Shift+J"))
                    .clicked()
                {
                    app.center_requested = true;
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut app.show_tile_grid, tr("Pokaż siatkę kafli"));
            });

            // 5. Obraz (Spłaszcz obraz)
            ui.menu_button(tr("Obraz"), |ui| {
                let can_flatten = app.document.layer_count() > 1;
                if ui
                    .add_enabled(can_flatten, egui::Button::new(tr("Spłaszcz obraz")))
                    .clicked()
                {
                    app.flatten_image();
                    ui.close();
                }
            });

            // 6. Warstwa (istniejące operacje)
            ui.menu_button(tr("Warstwa"), |ui| {
                if ui.button(tr("Nowa warstwa...")).clicked() {
                    app.add_new_layer();
                    ui.close();
                }
                if ui.button(tr("Duplikuj warstwę")).clicked() {
                    app.duplicate_active_layer();
                    ui.close();
                }
                ui.separator();
                if ui
                    .button(tr("Usuń jednolite tło"))
                    .on_hover_text(tr(
                        "Usuwa tło połączone z brzegami obrazu (tolerancja jak kubełka)",
                    ))
                    .clicked()
                {
                    app.remove_uniform_background();
                    ui.close();
                }
                #[cfg(feature = "ai")]
                if ui
                    .add_enabled(app.ai_rx.is_none(), egui::Button::new(tr("Usuń tło (AI)")))
                    .on_hover_text(tr("Model U²-Net-p na CPU, ok. 1–2 s"))
                    .clicked()
                {
                    app.start_ai_background(false);
                    ui.close();
                }
                let can_remove = app.document.layer_count() > 1;
                if ui
                    .add_enabled(can_remove, egui::Button::new(tr("Usuń warstwę")))
                    .clicked()
                {
                    app.remove_active_layer();
                    ui.close();
                }
                ui.separator();
                let can_move_up = app.active_layer_index + 1 < app.document.layer_count();
                if ui
                    .add_enabled(can_move_up, egui::Button::new(tr("Przesuń w górę")))
                    .clicked()
                {
                    app.move_active_layer_up();
                    ui.close();
                }
                let can_move_down = app.active_layer_index > 0
                    && app.active_layer_index < app.document.layer_count();
                if ui
                    .add_enabled(can_move_down, egui::Button::new(tr("Przesuń w dół")))
                    .clicked()
                {
                    app.move_active_layer_down();
                    ui.close();
                }
            });

            // 7. Kolory (filtry kolorów)
            ui.menu_button(tr("Kolory"), |ui| {
                let has_layers = !app.document.layers.is_empty();
                if ui
                    .add_enabled(has_layers, egui::Button::new(tr("Jasność i kontrast...")))
                    .clicked()
                {
                    app.open_filter_dialog(FilterKind::BrightnessContrast {
                        brightness: 0,
                        contrast: 1.0_f32,
                    });
                    ui.close();
                }
                if ui
                    .add_enabled(has_layers, egui::Button::new(tr("Odcień i nasycenie...")))
                    .clicked()
                {
                    app.open_filter_dialog(FilterKind::HueSaturation {
                        hue_shift_deg: 0.0_f32,
                        sat_mul: 1.0_f32,
                        light_delta: 0.0_f32,
                    });
                    ui.close();
                }
            });

            // 8. Narzędzia
            ui.menu_button(tr("Narzędzia"), |ui| {
                if ui.button(tr("Przesuwanie (Spacja)")).clicked() {
                    app.active_tool = ActiveTool::Pan;
                    ui.close();
                }
                if ui.button(tr("Pędzel (B)")).clicked() {
                    app.active_tool = ActiveTool::Brush;
                    ui.close();
                }
                if ui.button(tr("Gumka (E)")).clicked() {
                    app.active_tool = ActiveTool::Eraser;
                    ui.close();
                }
                if ui.button(tr("Kubełek (G)")).clicked() {
                    app.active_tool = ActiveTool::Bucket;
                    ui.close();
                }
                if ui.button(tr("Gradient (Shift+G)")).clicked() {
                    app.active_tool = ActiveTool::Gradient;
                    ui.close();
                }
                if ui.button(tr("Pipeta (I)")).clicked() {
                    app.active_tool = ActiveTool::Eyedropper;
                    ui.close();
                }
                if ui.button(tr("Zaznaczenie prostokątne (M)")).clicked() {
                    app.active_tool = ActiveTool::SelectRect;
                    ui.close();
                }
                if ui.button(tr("Zaznaczenie eliptyczne (Shift+M)")).clicked() {
                    app.active_tool = ActiveTool::SelectEllipse;
                    ui.close();
                }
                if ui.button(tr("Różdżka (U)")).clicked() {
                    app.active_tool = ActiveTool::MagicWand;
                    ui.close();
                }
            });

            // 9. Filtry (rozmycie/wyostrzanie)
            ui.menu_button(tr("Filtry"), |ui| {
                // Usuwanie tła na górze menu Filtry, żeby było łatwe do znalezienia
                ui.menu_button(tr("Usuwanie tła"), |ui| {
                    #[cfg(feature = "ai")]
                    {
                        if ui
                            .add_enabled(
                                app.ai_rx.is_none(),
                                egui::Button::new(tr("Usuń tło (AI)")),
                            )
                            .on_hover_text(tr("Model U²-Net-p na CPU, ok. 1–2 s"))
                            .clicked()
                        {
                            app.start_ai_background(false);
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                app.ai_rx.is_none(),
                                egui::Button::new(tr("Zaznacz pierwszy plan (AI)")),
                            )
                            .clicked()
                        {
                            app.start_ai_background(true);
                            app.selection_shape = SelectionShape::Rect;
                            ui.close();
                        }
                    }
                    if ui.button(tr("Usuń jednolite tło")).clicked() {
                        app.remove_uniform_background();
                        ui.close();
                    }
                });
                ui.separator();
                let has_layers = !app.document.layers.is_empty();
                if ui
                    .add_enabled(has_layers, egui::Button::new(tr("Rozmycie Gaussa...")))
                    .clicked()
                {
                    app.open_filter_dialog(FilterKind::GaussianBlur { radius: 3 });
                    ui.close();
                }
                if ui
                    .add_enabled(has_layers, egui::Button::new(tr("Wyostrzanie...")))
                    .clicked()
                {
                    app.open_filter_dialog(FilterKind::UnsharpMask {
                        radius: 2,
                        amount: 1.0_f32,
                    });
                    ui.close();
                }
            });

            // 10. Okna
            ui.menu_button(tr("Okna"), |ui| {
                if ui.button(tr("Dokowalne okna: Pędzle")).clicked() {
                    app.right_tab = crate::state::RightTab::Brushes;
                    ui.close();
                }
                if ui.button(tr("Dokowalne okna: Historia cofania")).clicked() {
                    app.right_tab = crate::state::RightTab::History;
                    ui.close();
                }
            });

            // 11. Pomoc (O programie)
            ui.menu_button(tr("Pomoc"), |ui| {
                if ui.button(tr("O programie Dziwak...")).clicked() {
                    app.show_about_dialog = true;
                    ui.close();
                }
            });
        });
    });
}
