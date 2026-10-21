pub mod canvas;
pub mod dialogs;
pub mod gimp_theme;
pub mod history_panel;
pub mod icons;
pub mod layers;
pub mod menus;
pub mod statusbar;
pub mod tool_options;
pub mod toolbox;

use crate::state::{ActiveTool, CloseDecision, DialogResult, DziwakApp, SelectionShape};
use eframe::egui;

/// Główna funkcja renderująca interfejs użytkownika aplikacji.
pub fn render_app(app: &mut DziwakApp, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // Przechwycenie prośby o zamknięcie okna (Super+W, przycisk X, Plik → Zakończ, Ctrl+Q)
    if ctx.input(|i| i.viewport().close_requested())
        && app.on_close_requested() == CloseDecision::AskUser
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
    }

    // Wynik usuwania tła AI z wątku roboczego; odpytuj co 100 ms tylko gdy AI liczy
    if app.poll_ai_background() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    if !app.doc.document.layers.is_empty() {
        app.doc.active_layer_index = app
            .doc
            .active_layer_index
            .min(app.doc.document.layers.len() - 1);
    }

    // Obsługa skrótów klawiszowych: Ctrl+N (Nowy), Ctrl+O (Otwórz), Ctrl+W (Zamknij zakładkę),
    // Ctrl+Tab / Ctrl+Shift+Tab (przełączanie zakładek), Ctrl+Shift+S (Zapisz jako), Ctrl+Q (Zakończ)
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
        app.new_tab();
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
        app.open_file_dialog();
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)) {
        let active_id = app.doc.id;
        app.request_close_tab(active_id);
    }
    if ctx.input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::Tab,
        ) || i.consume_key(
            egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT),
            egui::Key::Tab,
        )
    }) {
        app.prev_tab();
    } else if ctx.input_mut(|i| {
        i.consume_key(egui::Modifiers::COMMAND, egui::Key::Tab)
            || i.consume_key(egui::Modifiers::CTRL, egui::Key::Tab)
    }) {
        app.next_tab();
    }
    if ctx.input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::S,
        )
    }) {
        app.close_after_save = false;
        app.save_file_dialog();
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Q)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // Obsługa pisania bezpośrednio na płótnie w aktywnej ramce narzędzia Tekst (T10)
    if app.doc.text_editor.is_some() {
        app.ensure_text_editor_valid();
    }
    let text_box_active = app.is_editing_text()
        && !ctx.wants_keyboard_input()
        && app.doc.editing_layer_index.is_none()
        && !app.show_close_confirm_dialog;

    if text_box_active {
        let events = ctx.input(|i| i.events.clone());
        let mut any_text_event = false;
        for event in events {
            match event {
                egui::Event::Text(s) => {
                    app.text_insert_str(&s);
                    any_text_event = true;
                }
                egui::Event::Paste(s) => {
                    app.text_insert_str(&s);
                    any_text_event = true;
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => match key {
                    egui::Key::Enter if modifiers.command || modifiers.ctrl => {
                        app.commit_text();
                        any_text_event = true;
                    }
                    egui::Key::Enter => {
                        app.text_insert_newline();
                        any_text_event = true;
                    }
                    egui::Key::Escape => {
                        app.cancel_text();
                        any_text_event = true;
                    }
                    egui::Key::Backspace => {
                        app.text_backspace();
                        any_text_event = true;
                    }
                    egui::Key::Delete => {
                        app.text_delete();
                        any_text_event = true;
                    }
                    egui::Key::ArrowLeft => {
                        app.text_move_left();
                        any_text_event = true;
                    }
                    egui::Key::ArrowRight => {
                        app.text_move_right();
                        any_text_event = true;
                    }
                    egui::Key::ArrowUp => {
                        app.text_move_up();
                        any_text_event = true;
                    }
                    egui::Key::ArrowDown => {
                        app.text_move_down();
                        any_text_event = true;
                    }
                    egui::Key::Home => {
                        app.text_move_home();
                        any_text_event = true;
                    }
                    egui::Key::End => {
                        app.text_move_end();
                        any_text_event = true;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if any_text_event {
            ctx.request_repaint();
        }
    }

    // Skróty jednoklawiszowe narzędzi i operacji kolorów (aktywne gdy nie wprowadzamy tekstu)
    if !ctx.wants_keyboard_input()
        && !text_box_active
        && !app.is_editing_text()
        && app.doc.editing_layer_index.is_none()
        && !app.show_close_confirm_dialog
    {
        // Gradient: G
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::G)) {
            app.active_tool = ActiveTool::Gradient;
        }

        // Kubełek: Shift+B
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::B)) {
            app.active_tool = ActiveTool::Bucket;
        }

        // Klonowanie: C
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::C)) {
            app.active_tool = ActiveTool::Clone;
        }

        // Reset kolorów: D (czarny/biały)
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::D)) {
            app.brush_color = egui::Color32::BLACK;
            app.bg_color = egui::Color32::WHITE;
        }

        // Pipeta: O
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::O)) {
            app.active_tool = ActiveTool::Eyedropper;
        }

        // Zaznaczenie wg koloru: Shift+O
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::O)) {
            app.active_tool = ActiveTool::SelectColor;
        }

        // Zamiana koloru pierwszoplanowego i tła: X
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::X)) {
            std::mem::swap(&mut app.brush_color, &mut app.bg_color);
        }

        // Różdżka: U
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::U)) {
            app.active_tool = ActiveTool::MagicWand;
        }

        // Zaznaczenie eliptyczne: E, prostokątne: R
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::E)) {
            app.active_tool = ActiveTool::SelectEllipse;
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::R)) {
            app.active_tool = ActiveTool::SelectRect;
        }

        // Ołówek: N
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::N)) {
            app.active_tool = ActiveTool::Pencil;
        }

        // Pędzel: P
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::P)) {
            app.active_tool = ActiveTool::Brush;
        }

        // Aerograf: A
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::A)) {
            app.active_tool = ActiveTool::Airbrush;
        }

        // Przesuwanie: M
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::M)) {
            app.active_tool = ActiveTool::Move;
        }

        // Gumka: Shift+E
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::E)) {
            app.active_tool = ActiveTool::Eraser;
        }

        // Kadrowanie: Shift+C
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::C)) {
            app.active_tool = ActiveTool::Crop;
        }

        // Obrót: Shift+R
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::R)) {
            app.set_active_tool(ActiveTool::Rotate);
        }

        // Skalowanie: Shift+T
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::T)) {
            app.set_active_tool(ActiveTool::Scale);
        }

        // Odbicie: Shift+F
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F)) {
            app.set_active_tool(ActiveTool::Flip);
        }

        // Rozmazywanie: Shift+S
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::S)) {
            app.active_tool = ActiveTool::Smudge;
        }

        // Rozjaśnianie / ściemnianie: Shift+D
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::D)) {
            app.active_tool = ActiveTool::DodgeBurn;
        }

        // Rozmywanie / wyostrzanie: Shift+U
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::U)) {
            app.active_tool = ActiveTool::BlurSharpen;
        }

        // Zaznaczenie odręczne: F
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F)) {
            app.active_tool = ActiveTool::SelectFree;
        }

        // Tekst: T
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::T)) {
            app.set_active_tool(ActiveTool::Text);
        }

        // Lupa: Z
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Z)) {
            app.active_tool = ActiveTool::Zoom;
        }

        // Miarka: Shift+M
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::M)) {
            app.active_tool = ActiveTool::Measure;
        }

        // Wyczyść: Delete
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
            app.clear_selection_or_layer();
        }

        // Wypełnij kolorem pierwszoplanowym: Ctrl+,
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
            app.fill_with_fg_color();
        }

        // Wypełnij kolorem tła: Ctrl+.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Period)) {
            app.fill_with_bg_color();
        }
    }

    // Skróty zaznaczenia: Ctrl+Shift+A (Odznacz), Ctrl+A (Zaznacz wszystko), Ctrl+I (Odwróć)
    if ctx.input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::A,
        )
    }) {
        app.doc.selection.clear();
        app.doc.selection_drag_start = None;
        app.doc.selection_drag_current = None;
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
        app.doc
            .selection
            .select_rect(0, 0, app.doc.document.width, app.doc.document.height);
        app.doc.selection_shape = SelectionShape::Rect;
        app.doc.selection_drag_start = None;
        app.doc.selection_drag_current = None;
    }

    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::I)) {
        app.doc.selection.invert();
        app.doc.selection_shape = SelectionShape::Rect;
    }

    let mut should_fit_to_viewport = false;

    // Widok: Shift+Ctrl+J = dopasuj do okna, Shift+J = wyśrodkuj obraz (jak w GIMP-ie)
    if !ctx.wants_keyboard_input()
        && !text_box_active
        && !app.is_editing_text()
        && !app.show_close_confirm_dialog
    {
        if app.active_tool == ActiveTool::Crop {
            let enter_pressed =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            if enter_pressed {
                if app.commit_crop() {
                    should_fit_to_viewport = true;
                }
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                app.cancel_crop();
            }
        } else if app.active_tool == ActiveTool::SelectFree
            && !app.doc.free_select_points.is_empty()
        {
            let enter_pressed =
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            if enter_pressed {
                app.finish_free_select();
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                app.cancel_free_select();
            }
        }

        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::J) {
                should_fit_to_viewport = true;
            } else if i.consume_key(egui::Modifiers::SHIFT, egui::Key::J) {
                app.doc.center_requested = true;
            }
        });
    }

    // Skróty historii: Ctrl+Shift+Z lub Ctrl+Y (Ponów), Ctrl+Z (Cofnij)
    let redo_requested = ctx.input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::Z,
        ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
    });
    if redo_requested {
        should_fit_to_viewport |= app.handle_redo();
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
        should_fit_to_viewport |= app.handle_undo();
    }

    // Odbieranie wyników okien dialogowych z osobnego wątku (bez blokowania pętli UI)
    while let Ok(result) = app.dialog_rx.try_recv() {
        match result {
            DialogResult::Open(path) => {
                if app.open_file_in_new_tab(path).is_some() {
                    should_fit_to_viewport = true;
                }
            }
            DialogResult::Save { tab_id, path } => {
                if app.switch_to_tab(tab_id)
                    && app.request_save_to_path(path)
                    && app.close_after_save
                    && app.complete_close_after_save()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            DialogResult::SaveCancelled { tab_id } => {
                if app.doc.id == tab_id || app.close_confirm_tab_id == Some(tab_id) {
                    app.close_after_save = false;
                    app.closing_entire_app = false;
                    app.close_confirm_tab_id = None;
                    app.close_discarded_tabs.clear();
                }
            }
        }
    }

    // Aktualizacja tytułu okna: 'Dziwak — nazwa_pliku' + gwiazdka gdy zmodyfikowany (E13)
    let current_title = app.window_title();
    if app.last_window_title != current_title {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(current_title.clone()));
        app.last_window_title = current_title;
    }

    // Górne menu aplikacji (Plik, Edycja, Zaznaczenie, Widok, Obraz, Warstwa, Kolory, Narzędzia, Filtry, Okna, Pomoc)
    menus::render_menu_bar(app, ctx, &mut should_fit_to_viewport);

    // Tab chowa/pokazuje doki jak w GIMP-ie (gdy żadne pole tekstowe nie ma fokusu i bez modyfikatorów Ctrl/Shift)
    if !ctx.wants_keyboard_input()
        && !text_box_active
        && !app.is_editing_text()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
    {
        app.docks_hidden = !app.docks_hidden;
    }

    if !app.docks_hidden {
        // Lewy dock: skrzynka narzędzi (siatka 28x28), FG/BG, opcje narzędzia
        toolbox::render_left_dock(app, ctx);

        // Prawy dock: zakładki 'Pędzle' / 'Historia cofania' oraz panel 'Warstwy'
        layers::render_right_dock(app, ctx, &mut should_fit_to_viewport);
    }

    if app.doc.text_editor.is_some() {
        app.ensure_text_editor_valid();
    }

    // Pasek stanu na dole okna
    statusbar::render_statusbar(app, ctx);

    // Pasek zakładek obrazów nad płótnem (w stylu GIMP 2.10 w trybie jednego okna)
    render_tabs_bar(app, ctx);

    // Główny obszar roboczy z płótnem
    canvas::render_canvas(app, ctx, should_fit_to_viewport);

    // Okna dialogowe
    dialogs::render_dialogs(app, ctx);
}

/// Renderuje pasek zakładek obrazów nad obszarem płótna (nazwa pliku, gwiazdka przy modyfikacji, krzyżyk).
pub fn render_tabs_bar(app: &mut DziwakApp, ctx: &egui::Context) {
    let mut activate_id: Option<u64> = None;
    let mut close_id: Option<u64> = None;
    let mut create_new = false;

    egui::TopBottomPanel::top("document_tabs_panel")
        .exact_height(26.0)
        .show(ctx, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("document_tabs_scroll")
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let active_id = app.doc.id;
                        for &tab_id in &app.tab_order {
                            let Some(tab) = app.tab_by_id(tab_id) else {
                                continue;
                            };
                            let is_active = tab_id == active_id;
                            let name = tab.document_name();
                            let label = if tab.is_modified {
                                format!("{name}*")
                            } else {
                                name
                            };

                            let fill = if is_active {
                                egui::Color32::from_rgb(72, 72, 72)
                            } else {
                                egui::Color32::from_rgb(48, 48, 48)
                            };
                            let stroke = if is_active {
                                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 160, 210))
                            } else {
                                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(38, 38, 38))
                            };

                            egui::Frame::NONE
                                .fill(fill)
                                .stroke(stroke)
                                .corner_radius(egui::CornerRadius::same(2))
                                .inner_margin(egui::Margin::symmetric(6, 2))
                                .show(ui, |ui| {
                                    let text = if is_active {
                                        egui::RichText::new(label)
                                            .strong()
                                            .color(egui::Color32::from_rgb(235, 235, 235))
                                    } else {
                                        egui::RichText::new(label)
                                            .color(egui::Color32::from_rgb(185, 185, 185))
                                    };
                                    let tab_resp = ui.add(
                                        egui::Button::new(text)
                                            .frame(false)
                                            .sense(egui::Sense::click()),
                                    );
                                    if tab_resp.clicked() {
                                        activate_id = Some(tab_id);
                                    }
                                    if tab_resp.middle_clicked() {
                                        close_id = Some(tab_id);
                                    }

                                    let close_resp = ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("×")
                                                    .size(12.0)
                                                    .color(egui::Color32::from_rgb(190, 190, 190)),
                                            )
                                            .frame(false),
                                        )
                                        .on_hover_text(crate::i18n::tr(
                                            "Zamknij zakładkę (Ctrl+W)",
                                        ));
                                    if close_resp.clicked() || close_resp.middle_clicked() {
                                        close_id = Some(tab_id);
                                    }
                                });
                        }

                        if ui
                            .add(egui::Button::new("+").small())
                            .on_hover_text(crate::i18n::tr("Nowa zakładka (Ctrl+N)"))
                            .clicked()
                        {
                            create_new = true;
                        }
                    });
                });
        });

    if let Some(id) = close_id {
        app.request_close_tab(id);
    } else if let Some(id) = activate_id {
        let _ = app.switch_to_tab(id);
    } else if create_new {
        app.new_tab();
    }
}

#[cfg(test)]
mod text_tool_ui_tests {
    use super::*;
    use crate::state::{ActiveTool, DziwakApp};

    fn frame(
        app: &mut DziwakApp,
        ctx: &egui::Context,
        f: &mut eframe::Frame,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1366.0, 768.0),
            )),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| render_app(app, ctx, f));
    }

    fn button(pos: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        }
    }

    #[test]
    fn text_tool_end_to_end() {
        let ctx = egui::Context::default();
        let mut f = eframe::Frame::_new_kittest();
        let mut app = DziwakApp::new();
        for _ in 0..3 {
            frame(&mut app, &ctx, &mut f, vec![]);
        }
        app.set_active_tool(ActiveTool::Text);
        let layers_before = app.doc.document.layer_count();
        let c = app.doc.transform.canvas_to_screen(egui::pos2(
            app.doc.document.width as f32 / 3.0,
            app.doc.document.height as f32 / 3.0,
        ));
        let none = egui::Modifiers::NONE;
        frame(&mut app, &ctx, &mut f, vec![egui::Event::PointerMoved(c)]);
        frame(&mut app, &ctx, &mut f, vec![button(c, true, none)]);
        let c2 = c + egui::vec2(220.0, 90.0);
        frame(&mut app, &ctx, &mut f, vec![egui::Event::PointerMoved(c2)]);
        frame(&mut app, &ctx, &mut f, vec![button(c2, false, none)]);
        assert!(
            app.doc.text_editor.is_some(),
            "po przeciągnięciu nie powstała ramka tekstu"
        );
        assert_eq!(
            app.doc.document.layer_count(),
            layers_before + 1,
            "liczba warstw powinna wzrosnąć o 1 już po utworzeniu ramki"
        );
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name, "Tekst",
            "nowa warstwa powinna mieć nazwę 'Tekst'"
        );
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::Text("Abc".into())],
        );
        assert_eq!(
            app.doc.text_editor.as_ref().map(|e| e.text.clone()),
            Some("Abc".to_string()),
            "tekst nie trafił do ramki"
        );
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name, "Tekst: Abc",
            "warstwa powinna mieć nazwę 'Tekst: Abc' po wpisaniu"
        );
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: ctrl,
            }],
        );
        assert!(
            app.doc.text_editor.is_none(),
            "Ctrl+Enter nie zatwierdził tekstu"
        );
        assert_eq!(
            app.doc.document.layer_count(),
            layers_before + 1,
            "nie powstała warstwa tekstu"
        );
        assert_eq!(
            app.doc.document.layers[app.doc.active_layer_index].name,
            "Tekst: Abc"
        );

        // Esc usuwa warstwę tekstową
        let count_after_commit = app.doc.document.layer_count();
        frame(&mut app, &ctx, &mut f, vec![egui::Event::PointerMoved(c)]);
        frame(&mut app, &ctx, &mut f, vec![button(c, true, none)]);
        frame(&mut app, &ctx, &mut f, vec![egui::Event::PointerMoved(c2)]);
        frame(&mut app, &ctx, &mut f, vec![button(c2, false, none)]);
        assert!(app.doc.text_editor.is_some());
        assert_eq!(app.doc.document.layer_count(), count_after_commit + 1);
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::Text("Do anulowania".into())],
        );
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: none,
            }],
        );
        assert!(
            app.doc.text_editor.is_none(),
            "Esc nie anulował edycji tekstu"
        );
        assert_eq!(
            app.doc.document.layer_count(),
            count_after_commit,
            "Esc nie usunął warstwy tekstowej"
        );

        // Pusty tekst przy zatwierdzeniu nie zostawia warstwy
        frame(&mut app, &ctx, &mut f, vec![egui::Event::PointerMoved(c)]);
        frame(&mut app, &ctx, &mut f, vec![button(c, true, none)]);
        frame(&mut app, &ctx, &mut f, vec![egui::Event::PointerMoved(c2)]);
        frame(&mut app, &ctx, &mut f, vec![button(c2, false, none)]);
        assert!(app.doc.text_editor.is_some());
        assert_eq!(app.doc.document.layer_count(), count_after_commit + 1);
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: ctrl,
            }],
        );
        assert!(app.doc.text_editor.is_none());
        assert_eq!(
            app.doc.document.layer_count(),
            count_after_commit,
            "pusty tekst przy zatwierdzeniu nie powinien zostawiać warstwy"
        );
    }

    #[test]
    fn brush_paints_on_canvas() {
        let ctx = egui::Context::default();
        let mut f = eframe::Frame::_new_kittest();
        let mut app = DziwakApp::new();
        for _ in 0..3 {
            frame(&mut app, &ctx, &mut f, vec![]);
        }
        app.set_active_tool(ActiveTool::Brush);
        app.doc.active_layer_index = 0;
        let p = egui::pos2(
            app.doc.document.width as f32 * 0.9,
            app.doc.document.height as f32 * 0.9,
        );
        let before = app.doc.document.layers[0].get_pixel(p.x as u32, p.y as u32);
        let s = app.doc.transform.canvas_to_screen(p);
        let none = egui::Modifiers::NONE;
        frame(&mut app, &ctx, &mut f, vec![egui::Event::PointerMoved(s)]);
        frame(&mut app, &ctx, &mut f, vec![button(s, true, none)]);
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::PointerMoved(s + egui::vec2(5.0, 0.0))],
        );
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![button(s + egui::vec2(5.0, 0.0), false, none)],
        );
        let after = app.doc.document.layers[0].get_pixel(p.x as u32, p.y as u32);
        assert_ne!(
            before, after,
            "pędzel nie zmienił piksela — płótno nie reaguje na mysz"
        );
    }

    #[test]
    fn close_with_unsaved_changes_asks() {
        let ctx = egui::Context::default();
        let mut f = eframe::Frame::_new_kittest();
        let mut app = DziwakApp::new();
        frame(&mut app, &ctx, &mut f, vec![]);
        app.push_history(); // jak po każdej edycji
        assert!(
            app.doc.is_modified,
            "edycja nie oznaczyła dokumentu jako zmienionego"
        );
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1366.0, 768.0),
            )),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let out = ctx.run(input, |ctx| render_app(&mut app, ctx, &mut f));
        let cmds = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.commands.clone())
            .unwrap_or_default();
        assert!(
            cmds.contains(&egui::ViewportCommand::CancelClose),
            "zamknięcie nie zostało wstrzymane: {cmds:?}"
        );
        assert!(
            app.show_close_confirm_dialog,
            "nie pokazano pytania o zapis"
        );
    }

    /// Obraz: lewa połowa czerwona, prawa biała; pociągnięcie rozmazywania z czerwieni w biel.
    fn two_tone_app(ctx: &egui::Context, f: &mut eframe::Frame) -> DziwakApp {
        let mut app = DziwakApp::new();
        let (w, h) = (app.doc.document.width, app.doc.document.height);
        let mut layer = dziwak_core::Layer::new("t", w, h);
        for y in 0..h {
            for x in 0..w {
                let c = if x < w / 2 {
                    [200, 20, 20]
                } else {
                    [240, 240, 240]
                };
                let _ =
                    layer.set_pixel(x, y, dziwak_core::pixel::Rgba8::new(c[0], c[1], c[2], 255));
            }
        }
        app.doc.document.layers = vec![layer];
        app.doc.active_layer_index = 0;
        app.mark_document_dirty();
        for _ in 0..3 {
            frame(&mut app, ctx, f, vec![]);
        }
        app
    }

    /// Przeciąga mysz po płótnie od `from` do `to` (współrzędne obrazu) w `steps` ruchach.
    fn drag_canvas(
        app: &mut DziwakApp,
        ctx: &egui::Context,
        f: &mut eframe::Frame,
        from: egui::Pos2,
        to: egui::Pos2,
        steps: usize,
    ) {
        let none = egui::Modifiers::NONE;
        let s = app.doc.transform.canvas_to_screen(from);
        let e = app.doc.transform.canvas_to_screen(to);
        frame(app, ctx, f, vec![egui::Event::PointerMoved(s)]);
        frame(app, ctx, f, vec![button(s, true, none)]);
        for k in 1..=steps {
            let p = s + (e - s) * (k as f32 / steps as f32);
            frame(app, ctx, f, vec![egui::Event::PointerMoved(p)]);
        }
        frame(app, ctx, f, vec![button(e, false, none)]);
    }

    #[test]
    fn smudge_drags_color_on_canvas() {
        let ctx = egui::Context::default();
        let mut f = eframe::Frame::_new_kittest();
        let mut app = two_tone_app(&ctx, &mut f);
        app.set_active_tool(ActiveTool::Smudge);
        app.brush_size = 20.0;
        let (w, h) = (
            app.doc.document.width as f32,
            app.doc.document.height as f32,
        );
        let y = h * 0.5;
        drag_canvas(
            &mut app,
            &ctx,
            &mut f,
            egui::pos2(w * 0.5 - 15.0, y),
            egui::pos2(w * 0.5 + 25.0, y),
            20,
        );
        let px = app.doc.document.layers[0]
            .get_pixel((w * 0.5 + 8.0) as u32, y as u32)
            .unwrap_or_default();
        assert!(
            px.g() < 200,
            "rozmazywanie nie przeciągnęło czerwieni w biel: {px:?}"
        );
    }

    #[test]
    fn dodge_stroke_does_not_burn_to_white() {
        let ctx = egui::Context::default();
        let mut f = eframe::Frame::_new_kittest();
        let mut app = two_tone_app(&ctx, &mut f);
        app.set_active_tool(ActiveTool::DodgeBurn);
        app.dodge_burn_type = crate::state::DodgeBurnType::Dodge;
        app.brush_size = 20.0;
        let (w, h) = (
            app.doc.document.width as f32,
            app.doc.document.height as f32,
        );
        let y = h * 0.5;
        // tam i z powrotem po czerwonym — jedno pociągnięcie
        drag_canvas(
            &mut app,
            &ctx,
            &mut f,
            egui::pos2(10.0, y),
            egui::pos2(w * 0.3, y),
            30,
        );
        let px = app.doc.document.layers[0]
            .get_pixel((w * 0.15) as u32, y as u32)
            .unwrap_or_default();
        assert!(px.r() >= 200, "rozjaśnianie nie rozjaśniło: {px:?}");
        assert!(px.g() < 90, "rozjaśnianie wypaliło do bieli: {px:?}");
    }

    #[test]
    fn sharpen_brush_does_not_explode() {
        let ctx = egui::Context::default();
        let mut f = eframe::Frame::_new_kittest();
        let mut app = DziwakApp::new();
        // jedna warstwa z gładkim gradientem (jak zdjęcie)
        let (w, h) = (app.doc.document.width, app.doc.document.height);
        let mut layer = dziwak_core::Layer::new("t", w, h);
        for y in 0..h {
            for x in 0..w {
                let v = (x * 255 / w.max(1)) as u8;
                let _ = layer.set_pixel(x, y, dziwak_core::pixel::Rgba8::new(v, 128, 255 - v, 255));
            }
        }
        app.doc.document.layers = vec![layer];
        app.doc.active_layer_index = 0;
        app.mark_document_dirty();
        for _ in 0..3 {
            frame(&mut app, &ctx, &mut f, vec![]);
        }
        let orig = app.doc.document.layers[0].clone();
        app.set_active_tool(ActiveTool::BlurSharpen);
        app.blur_sharpen_type = crate::state::BlurSharpenType::Sharpen;
        let p = egui::pos2(w as f32 * 0.4, h as f32 * 0.5);
        let s = app.doc.transform.canvas_to_screen(p);
        let none = egui::Modifiers::NONE;
        frame(&mut app, &ctx, &mut f, vec![egui::Event::PointerMoved(s)]);
        frame(&mut app, &ctx, &mut f, vec![button(s, true, none)]);
        for k in 1..40 {
            frame(
                &mut app,
                &ctx,
                &mut f,
                vec![egui::Event::PointerMoved(
                    s + egui::vec2(k as f32 * 2.0, (k % 3) as f32),
                )],
            );
        }
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![button(s + egui::vec2(80.0, 0.0), false, none)],
        );
        let mut max_diff = 0i32;
        for y in 0..h {
            for x in 0..w {
                let a = orig.get_pixel(x, y).unwrap_or_default();
                let b = app.doc.document.layers[0]
                    .get_pixel(x, y)
                    .unwrap_or_default();
                for c in 0..4 {
                    max_diff = max_diff.max((a.0[c] as i32 - b.0[c] as i32).abs());
                }
            }
        }
        assert!(
            max_diff <= 12,
            "wyostrzanie gładkiego gradientu zmieniło piksel o {max_diff}"
        );
    }

    #[test]
    fn open_dialog_result_creates_new_tab_and_shortcuts_switch_and_close() {
        let ctx = egui::Context::default();
        let mut f = eframe::Frame::_new_kittest();
        let mut app = DziwakApp::new();
        let initial_tab_id = app.doc.id;
        frame(&mut app, &ctx, &mut f, vec![]);

        let tmp_dir = std::env::temp_dir();
        let path = tmp_dir.join("dziwak_ui_dialog_open_tab.png");
        let doc = crate::state::create_demo_document(40, 30);
        dziwak_core::save_image(&doc, &path).unwrap();

        // Wynik dialogu otwarcia trafia do NOWEJ zakładki
        app.dialog_tx
            .send(DialogResult::Open(path.clone()))
            .unwrap();
        frame(&mut app, &ctx, &mut f, vec![]);

        assert_eq!(app.tab_count(), 2);
        let second_tab_id = app.doc.id;
        assert_ne!(initial_tab_id, second_tab_id);
        assert_eq!(app.doc.current_file_path, Some(path.clone()));
        assert_eq!(app.doc.document.width, 40);
        assert_eq!(app.doc.document.height, 30);

        // Ctrl+Tab przełącza na kolejną zakładkę i NIE chowa doków
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        assert!(!app.docks_hidden);
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: ctrl,
            }],
        );
        assert_eq!(app.doc.id, initial_tab_id);
        assert!(!app.docks_hidden);

        // Ctrl+Shift+Tab przełącza na poprzednią zakładkę
        let ctrl_shift = egui::Modifiers {
            ctrl: true,
            command: true,
            shift: true,
            ..Default::default()
        };
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: ctrl_shift,
            }],
        );
        assert_eq!(app.doc.id, second_tab_id);

        // Ctrl+W zamyka aktywną niezmodyfikowaną zakładkę
        frame(
            &mut app,
            &ctx,
            &mut f,
            vec![egui::Event::Key {
                key: egui::Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: ctrl,
            }],
        );
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.doc.id, initial_tab_id);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn close_app_with_two_modified_tabs_asks_twice_in_ui() {
        let ctx = egui::Context::default();
        let mut f = eframe::Frame::_new_kittest();
        let mut app = DziwakApp::new();
        let tab1_id = app.doc.id;
        app.push_history();

        let tab2_id = app.new_tab();
        app.push_history();
        frame(&mut app, &ctx, &mut f, vec![]);

        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1366.0, 768.0),
            )),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        let out = ctx.run(input, |ctx| render_app(&mut app, ctx, &mut f));
        let cmds = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.commands.clone())
            .unwrap_or_default();
        assert!(cmds.contains(&egui::ViewportCommand::CancelClose));
        assert!(app.show_close_confirm_dialog);
        assert_eq!(app.doc.id, tab1_id);
        assert_eq!(app.close_confirm_tab_id, Some(tab1_id));

        // Po odrzuceniu pierwszej zakładki okno dialogowe pyta o drugą zakładkę
        assert!(!app.confirm_discard_and_close());
        frame(&mut app, &ctx, &mut f, vec![]);
        assert!(app.show_close_confirm_dialog);
        assert_eq!(app.doc.id, tab2_id);
        assert_eq!(app.close_confirm_tab_id, Some(tab2_id));

        // Po odrzuceniu drugiej zakładki program jest gotowy do zamknięcia
        assert!(app.confirm_discard_and_close());
        assert!(!app.show_close_confirm_dialog);
        assert!(app.close_confirmed);
    }
}
