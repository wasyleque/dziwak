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

use crate::state::{ActiveTool, DialogResult, DziwakApp, SelectionShape};
use eframe::egui;

/// Główna funkcja renderująca interfejs użytkownika aplikacji.
pub fn render_app(app: &mut DziwakApp, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // Wynik usuwania tła AI z wątku roboczego; odpytuj co 100 ms tylko gdy AI liczy
    if app.poll_ai_background() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    if !app.document.layers.is_empty() {
        app.active_layer_index = app.active_layer_index.min(app.document.layers.len() - 1);
    }

    // Obsługa skrótów klawiszowych: Ctrl+O (Otwórz), Ctrl+Shift+S (Zapisz jako)
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
        app.open_file_dialog();
    }
    if ctx.input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::S,
        )
    }) {
        app.save_file_dialog();
    }

    // Skróty jednoklawiszowe narzędzi i operacji kolorów (aktywne gdy nie wprowadzamy tekstu)
    if !ctx.wants_keyboard_input() && app.editing_layer_index.is_none() {
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
    }

    // Skróty zaznaczenia: Ctrl+Shift+A (Odznacz), Ctrl+A (Zaznacz wszystko), Ctrl+I (Odwróć)
    if ctx.input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
            egui::Key::A,
        )
    }) {
        app.selection.clear();
        app.selection_drag_start = None;
        app.selection_drag_current = None;
    } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
        app.selection
            .select_rect(0, 0, app.document.width, app.document.height);
        app.selection_shape = SelectionShape::Rect;
        app.selection_drag_start = None;
        app.selection_drag_current = None;
    }

    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::I)) {
        app.selection.invert();
        app.selection_shape = SelectionShape::Rect;
    }

    let mut should_fit_to_viewport = false;

    // Widok: Shift+Ctrl+J = dopasuj do okna, Shift+J = wyśrodkuj obraz (jak w GIMP-ie)
    if !ctx.wants_keyboard_input() {
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
        } else if app.active_tool == ActiveTool::SelectFree && !app.free_select_points.is_empty() {
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
                app.center_requested = true;
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
            DialogResult::Open(path) => match dziwak_core::load_image(&path) {
                Ok(loaded_doc) => {
                    // Zapisujemy bieżący stan dokumentu do historii przed otwarciem nowego
                    app.push_history();

                    app.document = loaded_doc;
                    app.current_file_path = Some(path);
                    app.is_modified = false;
                    app.on_document_changed();
                    should_fit_to_viewport = true;
                    app.error_message = None;
                }
                Err(err) => {
                    app.error_message = Some(format!(
                        "{}:\n{err}",
                        crate::i18n::tr("Błąd wczytywania pliku")
                    ));
                }
            },
            DialogResult::Save(path) => {
                let is_dziwak = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("dziwak"));

                if !is_dziwak && app.document.layer_count() > 1 {
                    app.pending_flatten_save = Some(path);
                } else {
                    app.execute_save(path);
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

    // Tab chowa/pokazuje doki jak w GIMP-ie (gdy żadne pole tekstowe nie ma fokusu)
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
        app.docks_hidden = !app.docks_hidden;
    }

    if !app.docks_hidden {
        // Lewy dock: skrzynka narzędzi (siatka 28x28), FG/BG, opcje narzędzia
        toolbox::render_left_dock(app, ctx);

        // Prawy dock: zakładki 'Pędzle' / 'Historia cofania' oraz panel 'Warstwy'
        layers::render_right_dock(app, ctx, &mut should_fit_to_viewport);
    }

    // Pasek stanu na dole okna
    statusbar::render_statusbar(app, ctx);

    // Główny obszar roboczy z płótnem
    canvas::render_canvas(app, ctx, should_fit_to_viewport);

    // Okna dialogowe
    dialogs::render_dialogs(app, ctx);
}
