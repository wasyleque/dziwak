use crate::i18n::tr;
use crate::state::DziwakApp;
use eframe::egui;

/// Renderuje panel historii cofania (zakładka w prawym doku).
pub fn render_history_panel(
    app: &mut DziwakApp,
    ui: &mut egui::Ui,
    should_fit_to_viewport: &mut bool,
) {
    let undo_count = app.history.undo.len();
    let redo_count = app.history.redo.len();

    if undo_count == 0 && redo_count == 0 {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(tr("Brak operacji w historii"))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        });
        return;
    }

    let mut target_undo_steps = None;
    let mut target_redo_steps = None;

    egui::ScrollArea::vertical()
        .max_height(140.0)
        .show(ui, |ui| {
            // Wpisy do cofnięcia (przeszłość)
            for (idx, _snap) in app.history.undo.iter().enumerate() {
                let label = if idx == 0 {
                    "Stan początkowy".to_string()
                } else {
                    format!("{} {}", tr("Operacja"), idx)
                };

                let resp = ui.selectable_label(false, format!("↶ {label}"));
                if resp.clicked() {
                    // Kliknięcie cofa do wskazanego stanu
                    target_undo_steps = Some(undo_count - idx);
                }
            }

            // Bieżący aktywny stan
            let _ = ui.selectable_label(true, egui::RichText::new(tr("● Bieżący stan")).strong());

            // Wpisy do ponowienia (przyszłość - od najbliższego w dół)
            for (idx, _snap) in app.history.redo.iter().enumerate().rev() {
                let label = format!("{} {}", tr("Ponów"), idx + 1);
                let resp = ui.add_enabled(
                    true,
                    egui::Button::new(
                        egui::RichText::new(format!("↷ {label}")).color(egui::Color32::GRAY),
                    )
                    .frame(false),
                );
                if resp.clicked() {
                    target_redo_steps = Some(app.history.redo.len() - idx);
                }
            }
        });

    if let Some(steps) = target_undo_steps {
        for _ in 0..steps {
            if app.handle_undo() {
                *should_fit_to_viewport = true;
            }
        }
    } else if let Some(steps) = target_redo_steps {
        for _ in 0..steps {
            if app.handle_redo() {
                *should_fit_to_viewport = true;
            }
        }
    }
}
