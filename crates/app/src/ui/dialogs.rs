use std::time::{Duration, Instant};

use crate::i18n::tr;
use crate::state::{DziwakApp, FilterKind, LayerTransformKind};
use eframe::egui;

/// Wyświetla okno dialogowe filtrów obrazu z podglądem na żywo i debouncem (~100 ms).
pub fn render_filter_dialog(app: &mut DziwakApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.filter_dialog.take() else {
        return;
    };

    let now = Instant::now();
    let debounce_duration = Duration::from_millis(100);

    // Obsługa debounce dla oczekującej aktualizacji suwaków
    if dialog.pending_apply && dialog.preview {
        if let Some(t) = dialog.last_apply_time {
            if now.duration_since(t) >= debounce_duration {
                app.filter_dialog = Some(dialog);
                app.apply_filter_preview();
                if let Some(d) = &mut app.filter_dialog {
                    d.last_apply_time = Some(now);
                    d.pending_apply = false;
                }
                dialog = match app.filter_dialog.take() {
                    Some(d) => d,
                    None => return,
                };
            } else {
                let remaining = debounce_duration.saturating_sub(now.duration_since(t));
                ctx.request_repaint_after(remaining);
            }
        }
    }

    let mut is_open = true;
    let mut should_apply = false;
    let mut should_cancel = false;
    let mut slider_changed = false;

    let title = dialog.kind.title();
    egui::Window::new(title)
        .open(&mut is_open)
        .collapsible(false)
        .resizable(false)
        .default_width(280.0_f32)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0_f32, 0.0_f32))
        .show(ctx, |ui| {
            ui.add_space(4.0_f32);

            match &mut dialog.kind {
                FilterKind::BrightnessContrast {
                    brightness,
                    contrast,
                } => {
                    ui.label(tr("Jasność:"));
                    let b_resp = ui.add(egui::Slider::new(brightness, -255..=255));
                    if b_resp.changed() {
                        slider_changed = true;
                    }

                    ui.add_space(4.0_f32);
                    ui.label(tr("Kontrast:"));
                    let c_resp = ui.add(
                        egui::Slider::new(contrast, 0.0_f32..=3.0_f32)
                            .custom_formatter(|v, _| format!("{:.2}", v)),
                    );
                    if c_resp.changed() {
                        slider_changed = true;
                    }
                }
                FilterKind::HueSaturation {
                    hue_shift_deg,
                    sat_mul,
                    light_delta,
                } => {
                    ui.label(tr("Odcień:"));
                    let h_resp = ui
                        .add(egui::Slider::new(hue_shift_deg, -180.0_f32..=180.0_f32).suffix("°"));
                    if h_resp.changed() {
                        slider_changed = true;
                    }

                    ui.add_space(4.0_f32);
                    ui.label(tr("Nasycenie:"));
                    let s_resp = ui.add(
                        egui::Slider::new(sat_mul, 0.0_f32..=3.0_f32)
                            .custom_formatter(|v, _| format!("{:.2}", v)),
                    );
                    if s_resp.changed() {
                        slider_changed = true;
                    }

                    ui.add_space(4.0_f32);
                    ui.label(tr("Jasność:"));
                    let l_resp = ui.add(
                        egui::Slider::new(light_delta, -1.0_f32..=1.0_f32)
                            .custom_formatter(|v, _| format!("{:.2}", v)),
                    );
                    if l_resp.changed() {
                        slider_changed = true;
                    }
                }
                FilterKind::GaussianBlur { radius } => {
                    ui.label(tr("Promień rozmycia:"));
                    let r_resp = ui.add(egui::Slider::new(radius, 1..=30).suffix(" px"));
                    if r_resp.changed() {
                        slider_changed = true;
                    }
                }
                FilterKind::UnsharpMask { radius, amount } => {
                    ui.label(tr("Promień:"));
                    let r_resp = ui.add(egui::Slider::new(radius, 1..=20).suffix(" px"));
                    if r_resp.changed() {
                        slider_changed = true;
                    }

                    ui.add_space(4.0_f32);
                    ui.label(tr("Siła:"));
                    let a_resp = ui.add(
                        egui::Slider::new(amount, 0.1_f32..=5.0_f32)
                            .custom_formatter(|v, _| format!("{:.2}", v)),
                    );
                    if a_resp.changed() {
                        slider_changed = true;
                    }
                }
            }

            ui.add_space(8.0_f32);
            ui.separator();
            ui.add_space(4.0_f32);

            let preview_resp = ui.checkbox(&mut dialog.preview, tr("Podgląd"));
            if preview_resp.changed() {
                slider_changed = true;
            }

            ui.add_space(8.0_f32);

            ui.horizontal(|ui| {
                if ui.button(tr("Zastosuj")).clicked() {
                    should_apply = true;
                }
                if ui.button(tr("Anuluj")).clicked() {
                    should_cancel = true;
                }
            });
        });

    if !is_open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        should_cancel = true;
    }

    app.filter_dialog = Some(dialog);

    if should_apply {
        app.apply_filter_dialog();
    } else if should_cancel {
        app.cancel_filter_dialog();
    } else if slider_changed {
        let can_apply = match app.filter_dialog.as_ref().and_then(|d| d.last_apply_time) {
            None => true,
            Some(t) => now.duration_since(t) >= debounce_duration,
        };

        if can_apply {
            app.apply_filter_preview();
            if let Some(d) = &mut app.filter_dialog {
                d.last_apply_time = Some(now);
                d.pending_apply = false;
            }
        } else if let Some(d) = &mut app.filter_dialog {
            d.pending_apply = true;
            if let Some(t) = d.last_apply_time {
                let remaining = debounce_duration.saturating_sub(now.duration_since(t));
                ctx.request_repaint_after(remaining);
            }
        }
    }
}

/// Wyświetla okno dialogowe transformacji aktywnej warstwy (T6) z podglądem na żywo i debouncem (~100 ms).
pub fn render_transform_dialog(app: &mut DziwakApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.transform_dialog.take() else {
        return;
    };

    let now = Instant::now();
    let debounce_duration = Duration::from_millis(100);

    // Obsługa debounce dla oczekującej aktualizacji
    if dialog.pending_apply && dialog.preview {
        if let Some(t) = dialog.last_apply_time {
            if now.duration_since(t) >= debounce_duration {
                app.transform_dialog = Some(dialog);
                app.apply_transform_preview();
                if let Some(d) = &mut app.transform_dialog {
                    d.last_apply_time = Some(now);
                    d.pending_apply = false;
                }
                dialog = match app.transform_dialog.take() {
                    Some(d) => d,
                    None => return,
                };
            } else {
                let remaining = debounce_duration.saturating_sub(now.duration_since(t));
                ctx.request_repaint_after(remaining);
            }
        }
    }

    let mut is_open = true;
    let mut should_apply = false;
    let mut should_cancel = false;
    let mut changed = false;

    let title = tr(dialog.kind.title());
    egui::Window::new(title)
        .open(&mut is_open)
        .collapsible(false)
        .resizable(false)
        .default_width(280.0_f32)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0_f32, 0.0_f32))
        .show(ctx, |ui| {
            ui.add_space(4.0_f32);

            match &mut dialog.kind {
                LayerTransformKind::Rotate { angle_deg } => {
                    ui.label(tr("Kąt obrotu:"));
                    let resp = ui.add(
                        egui::Slider::new(angle_deg, -180.0_f32..=180.0_f32)
                            .suffix("°")
                            .custom_formatter(|v, _| format!("{:.1}", v)),
                    );
                    if resp.changed() {
                        changed = true;
                    }
                }
                LayerTransformKind::Scale {
                    width,
                    height,
                    orig_width,
                    orig_height,
                    keep_aspect,
                } => {
                    let ow = *orig_width as f32;
                    let oh = *orig_height as f32;

                    ui.label(tr("Szerokość:"));
                    let max_w = (*orig_width * 4).max(2000);
                    let w_resp = ui.add(egui::Slider::new(width, 1..=max_w).suffix(" px"));
                    if w_resp.changed() {
                        if *keep_aspect && ow > 0.0 {
                            *height = ((*width as f32 * oh) / ow).round().max(1.0) as u32;
                        }
                        changed = true;
                    }

                    ui.add_space(4.0_f32);
                    ui.label(tr("Wysokość:"));
                    let max_h = (*orig_height * 4).max(2000);
                    let h_resp = ui.add(egui::Slider::new(height, 1..=max_h).suffix(" px"));
                    if h_resp.changed() {
                        if *keep_aspect && oh > 0.0 {
                            *width = ((*height as f32 * ow) / oh).round().max(1.0) as u32;
                        }
                        changed = true;
                    }

                    ui.add_space(4.0_f32);
                    let aspect_resp = ui.checkbox(keep_aspect, tr("Zachowaj proporcje"));
                    if aspect_resp.changed() && *keep_aspect && ow > 0.0 {
                        *height = ((*width as f32 * oh) / ow).round().max(1.0) as u32;
                        changed = true;
                    }
                }
                LayerTransformKind::Flip { horizontal } => {
                    ui.label(tr("Kierunek:"));
                    let r1 = ui.radio_value(horizontal, true, tr("Poziomo"));
                    let r2 = ui.radio_value(horizontal, false, tr("Pionowo"));
                    if r1.changed() || r2.changed() {
                        changed = true;
                    }
                }
            }

            ui.add_space(8.0_f32);
            ui.separator();
            ui.add_space(4.0_f32);

            let preview_resp = ui.checkbox(&mut dialog.preview, tr("Podgląd"));
            if preview_resp.changed() {
                changed = true;
            }

            ui.add_space(8.0_f32);

            ui.horizontal(|ui| {
                if ui.button(tr("Zastosuj")).clicked() {
                    should_apply = true;
                }
                if ui.button(tr("Anuluj")).clicked() {
                    should_cancel = true;
                }
            });
        });

    if !is_open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        should_cancel = true;
    }

    app.transform_dialog = Some(dialog);

    if should_apply {
        app.apply_transform_dialog();
    } else if should_cancel {
        app.cancel_transform_dialog();
    } else if changed {
        let can_apply = match app
            .transform_dialog
            .as_ref()
            .and_then(|d| d.last_apply_time)
        {
            None => true,
            Some(t) => now.duration_since(t) >= debounce_duration,
        };

        if can_apply {
            app.apply_transform_preview();
            if let Some(d) = &mut app.transform_dialog {
                d.last_apply_time = Some(now);
                d.pending_apply = false;
            }
        } else if let Some(d) = &mut app.transform_dialog {
            d.pending_apply = true;
            if let Some(t) = d.last_apply_time {
                let remaining = debounce_duration.saturating_sub(now.duration_since(t));
                ctx.request_repaint_after(remaining);
            }
        }
    }
}

/// Wyświetla okno egui z ostrzeżeniem o spłaszczeniu warstw przy zapisie do formatu jednowarstwowego (E13).
pub fn render_flatten_warning_dialog(app: &mut DziwakApp, ctx: &egui::Context) {
    let mut save_confirmed = false;
    let mut cancel_confirmed = false;

    if let Some(path) = &app.pending_flatten_save {
        let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("pliku");

        egui::Window::new(tr("Ostrzeżenie o spłaszczeniu warstw"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Dokument zawiera {} warstwy/warstw.\n\
                     Wybrany format zapisu ({filename}) obsługuje tylko jedną warstwę.\n\
                     Wszystkie warstwy zostaną spłaszczone do pojedynczego obrazu.\n\
                     Aby zachować strukturę warstw, zapisz plik w formacie Dziwak (.dziwak).\n\n\
                     Czy chcesz kontynuować zapis?",
                    app.document.layer_count()
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Zapisz mimo to")).clicked() {
                        save_confirmed = true;
                    }
                    if ui.button(tr("Anuluj")).clicked() {
                        cancel_confirmed = true;
                    }
                });
            });
    }

    if save_confirmed {
        if let Some(path) = app.pending_flatten_save.take() {
            app.execute_save(path);
        }
    } else if cancel_confirmed {
        app.pending_flatten_save = None;
    }
}

/// Wyświetla modalne okno z informacją o błędzie.
pub fn render_error_dialog(app: &mut DziwakApp, ctx: &egui::Context) {
    if let Some(error_text) = &app.error_message.clone() {
        egui::Window::new(tr("Komunikat błędu"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(error_text).color(egui::Color32::from_rgb(255, 100, 100)),
                );
                ui.add_space(8.0);
                if ui.button(tr("Zamknij")).clicked() {
                    app.error_message = None;
                }
            });
    }
}

/// Strona projektu.
pub const PROJECT_URL: &str = "https://github.com/wasyleque/dziwak";
/// Darowizna PayPal dla autora (wasyl@o2.pl).
pub const DONATE_URL: &str =
    "https://www.paypal.com/donate/?business=wasyl%40o2.pl&item_name=Dziwak";

/// Otwiera adres w domyślnej przeglądarce (xdg-open), bez blokowania interfejsu.
pub fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Wyświetla okno dialogowe 'O programie'.
pub fn render_about_dialog(app: &mut DziwakApp, ctx: &egui::Context) {
    if app.show_about_dialog {
        let mut is_open = true;
        egui::Window::new(tr("O programie Dziwak"))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.heading(tr("Dziwak"));
                ui.label(tr("Edytor grafiki rastrowej dla Omarchy Linux / Wayland"));
                ui.label(format!("{} {}", tr("Wersja"), env!("CARGO_PKG_VERSION")));
                ui.label(tr("Licencja: GNU General Public License v3.0 or later"));
                ui.label(format!("{}: Piotr Wasilewski", tr("Autor")));
                if ui.link("github.com/wasyleque/dziwak").clicked() {
                    open_url(PROJECT_URL);
                }
                ui.add_space(8.0);
                ui.label(tr("Podoba ci się Dziwak? Wesprzyj jego rozwój:"));
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr("♥ Wesprzyj przez PayPal")).strong(),
                        )
                        .fill(egui::Color32::from_rgb(0, 112, 186)),
                    )
                    .on_hover_text(DONATE_URL)
                    .clicked()
                {
                    open_url(DONATE_URL);
                }
                ui.add_space(8.0);
                if ui.button(tr("OK")).clicked() {
                    app.show_about_dialog = false;
                }
            });
        if !is_open {
            app.show_about_dialog = false;
        }
    }
}

/// Wyświetla okno dialogowe skalowania całego obrazu (T7).
pub fn render_scale_image_dialog(app: &mut DziwakApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.scale_image_dialog.take() else {
        return;
    };

    let mut is_open = true;
    let mut should_apply = false;
    let mut should_cancel = false;

    egui::Window::new(tr("Skaluj obraz"))
        .open(&mut is_open)
        .collapsible(false)
        .resizable(false)
        .default_width(280.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(format!(
                "{}: {}×{} px",
                tr("Oryginalny rozmiar"),
                dialog.orig_width,
                dialog.orig_height
            ));
            ui.add_space(4.0);

            let orig_w = dialog.orig_width.max(1) as f64;
            let orig_h = dialog.orig_height.max(1) as f64;

            ui.horizontal(|ui| {
                ui.label(tr("Szerokość:"));
                let mut w = dialog.width;
                let w_resp = ui.add(egui::DragValue::new(&mut w).range(1..=16384).suffix(" px"));
                if w_resp.changed() {
                    dialog.width = w;
                    if dialog.keep_aspect {
                        dialog.height = ((w as f64 * orig_h / orig_w).round() as u32).max(1);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label(tr("Wysokość:"));
                let mut h = dialog.height;
                let h_resp = ui.add(egui::DragValue::new(&mut h).range(1..=16384).suffix(" px"));
                if h_resp.changed() {
                    dialog.height = h;
                    if dialog.keep_aspect {
                        dialog.width = ((h as f64 * orig_w / orig_h).round() as u32).max(1);
                    }
                }
            });

            ui.checkbox(&mut dialog.keep_aspect, tr("Zachowaj proporcje"));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button(tr("Skaluj")).clicked() {
                    should_apply = true;
                }
                if ui.button(tr("Anuluj")).clicked() {
                    should_cancel = true;
                }
            });
        });

    if !is_open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        should_cancel = true;
    }

    if should_apply {
        app.scale_image(dialog.width, dialog.height);
    } else if !should_cancel {
        app.scale_image_dialog = Some(dialog);
    }
}

/// Wyświetla okno dialogowe zmiany rozmiaru płótna (T7).
pub fn render_canvas_size_dialog(app: &mut DziwakApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.canvas_size_dialog.take() else {
        return;
    };

    let mut is_open = true;
    let mut should_apply = false;
    let mut should_cancel = false;

    egui::Window::new(tr("Rozmiar płótna"))
        .open(&mut is_open)
        .collapsible(false)
        .resizable(false)
        .default_width(280.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(format!(
                "{}: {}×{} px",
                tr("Bieżący rozmiar"),
                dialog.orig_width,
                dialog.orig_height
            ));
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(tr("Szerokość:"));
                ui.add(
                    egui::DragValue::new(&mut dialog.width)
                        .range(1..=16384)
                        .suffix(" px"),
                );
            });

            ui.horizontal(|ui| {
                ui.label(tr("Wysokość:"));
                ui.add(
                    egui::DragValue::new(&mut dialog.height)
                        .range(1..=16384)
                        .suffix(" px"),
                );
            });

            ui.separator();
            ui.label(tr("Przesunięcie:"));
            ui.horizontal(|ui| {
                ui.label("X:");
                ui.add(
                    egui::DragValue::new(&mut dialog.offset_x)
                        .range(-16384..=16384)
                        .suffix(" px"),
                );
                ui.label("Y:");
                ui.add(
                    egui::DragValue::new(&mut dialog.offset_y)
                        .range(-16384..=16384)
                        .suffix(" px"),
                );
            });

            if ui.button(tr("Wyśrodkuj")).clicked() {
                dialog.offset_x = (dialog.width as i32 - dialog.orig_width as i32) / 2;
                dialog.offset_y = (dialog.height as i32 - dialog.orig_height as i32) / 2;
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Zmień rozmiar")).clicked() {
                    should_apply = true;
                }
                if ui.button(tr("Anuluj")).clicked() {
                    should_cancel = true;
                }
            });
        });

    if !is_open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        should_cancel = true;
    }

    if should_apply {
        app.resize_canvas(
            dialog.width,
            dialog.height,
            dialog.offset_x,
            dialog.offset_y,
        );
    } else if !should_cancel {
        app.canvas_size_dialog = Some(dialog);
    }
}

/// Wyświetla okno dialogowe wstawiania tekstu z podglądem na żywo (T10).
pub fn render_text_dialog(app: &mut DziwakApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.text_dialog.take() else {
        return;
    };

    let mut is_open = true;
    let mut should_commit = false;
    let mut should_cancel = false;
    let mut changed = false;

    egui::Window::new(tr("Wstaw tekst"))
        .open(&mut is_open)
        .collapsible(false)
        .resizable(true)
        .default_width(320.0_f32)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0_f32, 0.0_f32))
        .show(ctx, |ui| {
            ui.add_space(4.0_f32);

            ui.label(tr("Tekst:"));
            let text_resp = ui.add(
                egui::TextEdit::multiline(&mut dialog.text)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            if text_resp.changed() {
                changed = true;
            }

            ui.add_space(4.0_f32);
            ui.label(tr("Rodzina czcionki:"));
            let fam_resp = ui.text_edit_singleline(&mut dialog.font_family);
            if fam_resp.changed() {
                changed = true;
            }

            ui.add_space(4.0_f32);
            ui.label(tr("Rozmiar:"));
            let size_resp =
                ui.add(egui::Slider::new(&mut dialog.font_size, 8.0_f32..=200.0_f32).suffix(" px"));
            if size_resp.changed() {
                changed = true;
            }

            ui.add_space(4.0_f32);
            ui.horizontal(|ui| {
                ui.label(tr("Pozycja:"));
                ui.label("X:");
                ui.add(egui::DragValue::new(&mut dialog.pos.0));
                ui.label("Y:");
                ui.add(egui::DragValue::new(&mut dialog.pos.1));
            });

            ui.add_space(4.0_f32);
            ui.horizontal(|ui| {
                ui.label(tr("Kolor:"));
                egui::color_picker::color_picker_color32(
                    ui,
                    &mut dialog.color,
                    egui::color_picker::Alpha::Opaque,
                );
            });

            if changed {
                DziwakApp::render_text_mask_for_dialog(&mut dialog);
            }

            ui.add_space(4.0_f32);
            if let Some(mask) = &dialog.preview_mask {
                ui.label(
                    egui::RichText::new(format!(
                        "{}: {}×{} px",
                        tr("Wymiary"),
                        mask.width,
                        mask.height
                    ))
                    .small()
                    .weak(),
                );
            }
            if let Some(err) = &dialog.error {
                ui.colored_label(egui::Color32::RED, err);
            }

            ui.add_space(8.0_f32);
            ui.separator();
            ui.horizontal(|ui| {
                let can_apply = dialog.preview_mask.is_some();
                if ui
                    .add_enabled(can_apply, egui::Button::new(tr("Zatwierdź (nowa warstwa)")))
                    .clicked()
                {
                    should_commit = true;
                }
                if ui.button(tr("Anuluj")).clicked() {
                    should_cancel = true;
                }
            });
        });

    if should_commit {
        app.text_dialog = Some(dialog);
        app.commit_text();
    } else if !should_cancel && is_open {
        app.text_dialog = Some(dialog);
    } else {
        app.cancel_text();
    }
}

/// Renderuje wszystkie okna dialogowe aplikacji.
pub fn render_dialogs(app: &mut DziwakApp, ctx: &egui::Context) {
    render_filter_dialog(app, ctx);
    render_transform_dialog(app, ctx);
    render_scale_image_dialog(app, ctx);
    render_canvas_size_dialog(app, ctx);
    render_text_dialog(app, ctx);
    render_flatten_warning_dialog(app, ctx);
    render_error_dialog(app, ctx);
    render_about_dialog(app, ctx);
}
