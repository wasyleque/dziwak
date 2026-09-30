use crate::i18n::tr;
use crate::state::DziwakApp;
use eframe::egui;

/// Renderuje dolny pasek stanu aplikacji w stylu GIMP 2.10.
pub fn render_statusbar(app: &mut DziwakApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_panel").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if app.ai_rx.is_some() {
                ui.spinner();
                ui.label(tr("AI: usuwanie tła…"));
                ui.separator();
            }

            // Pozycja kursora: x, y px
            if let Some(pos) = app.cursor_canvas_pos {
                ui.label(format!("{:.0}, {:.0} px", pos.x, pos.y));
            } else {
                ui.label(tr("—, — px"));
            }
            ui.separator();

            // Zoom combo (12.5%..1600%)
            let current_zoom_pct = format!("{:.0}%", app.transform.zoom * 100.0);
            egui::ComboBox::from_id_salt("statusbar_zoom_combo")
                .selected_text(current_zoom_pct)
                .width(75.0)
                .show_ui(ui, |ui| {
                    let zoom_presets: [f32; 8] = [0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];
                    for z in zoom_presets {
                        let label = format!("{:.1}%", z * 100.0);
                        let is_selected = (app.transform.zoom - z).abs() < 1e-4;
                        if ui.selectable_label(is_selected, label).clicked() {
                            app.transform.zoom = z;
                            app.tile_renderer.set_zoom(z);
                        }
                    }
                });
            ui.separator();

            // Rozmiar obrazu: szer. x wys.
            ui.label(format!(
                "{} × {} px",
                app.document.width, app.document.height
            ));
            ui.separator();

            // Pamięć historii: Historia: N MB
            let mem_mb = app.history.memory_usage(Some(&app.document)) as f64 / (1024.0 * 1024.0);
            ui.label(format!("{}: {:.1} MB", tr("Historia"), mem_mb));
            ui.separator();

            // Opcjonalna siatka kafli
            ui.checkbox(&mut app.show_tile_grid, tr("Siatka kafli"));
        });
    });
}
