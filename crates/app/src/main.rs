mod i18n;
mod i18n_table;
mod state;
mod theme;
mod ui;

use std::path::PathBuf;

use eframe::egui;
use state::DziwakApp;

fn main() -> Result<(), eframe::Error> {
    i18n::init();
    let file_arg = std::env::args().nth(1).map(PathBuf::from);

    let initial_title = match &file_arg {
        Some(path) => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(i18n::tr("bez nazwy"));
            format!("Dziwak — {name}")
        }
        None => "Dziwak — bez nazwy".to_string(),
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("dziwak")
            .with_title(initial_title)
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([640.0, 480.0]),
        // Bez vsync: pod Waylandem eglSwapBuffers z vsync czeka na sygnał klatki od kompozytora,
        // którego zasłonięte/nieaktywne okno nie dostaje — główny wątek wisiał i Hyprland
        // pokazywał „nie odpowiada”. Dziwak rysuje tylko przy zmianach, więc vsync nie jest potrzebny.
        vsync: false,
        ..Default::default()
    };

    eframe::run_native(
        "Dziwak",
        native_options,
        Box::new(|cc| {
            // Domyślnie wygląd GIMP 2.10 Dark z akcentem z motywu Omarchy;
            // DZIWAK_THEME=omarchy przywraca pełne kolory motywu Omarchy.
            let omarchy = theme::load_current_theme();
            let full_omarchy = std::env::var("DZIWAK_THEME").is_ok_and(|v| v == "omarchy");
            match omarchy {
                Some(theme) if full_omarchy => theme::apply_theme(&cc.egui_ctx, &theme),
                _ => {
                    let accent = omarchy
                        .map(|t| egui::Color32::from_rgb(t.accent[0], t.accent[1], t.accent[2]));
                    ui::gimp_theme::apply_gimp_style(&cc.egui_ctx, accent);
                }
            }
            let mut app = DziwakApp::new();
            if let Some(path) = file_arg {
                app.open_file_from_path(path);
            }
            Ok(Box::new(app))
        }),
    )
}

impl eframe::App for DziwakApp {
    /// Nieprzezroczyste tło okna (#262626) — domyślne eframe jest półprzezroczyste,
    /// przez co pod Hyprlandem prześwitywała tapeta.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.149, 0.149, 0.149, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ui::render_app(self, ctx, frame);
    }
}
