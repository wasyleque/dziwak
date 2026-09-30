use std::time::{Duration, Instant};

use crate::i18n::tr;
use crate::state::{DziwakApp, FilterKind};
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

/// Renderuje wszystkie okna dialogowe aplikacji.
pub fn render_dialogs(app: &mut DziwakApp, ctx: &egui::Context) {
    render_filter_dialog(app, ctx);
    render_flatten_warning_dialog(app, ctx);
    render_error_dialog(app, ctx);
    render_about_dialog(app, ctx);
}
