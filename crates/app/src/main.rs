// Na Windowsie bez okna konsoli.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod i18n;
mod i18n_table;
mod state;
mod theme;
mod ui;

use std::path::PathBuf;

use eframe::egui;
use state::DziwakApp;

fn main() -> Result<(), eframe::Error> {
    install_crash_log();
    i18n::init();
    let file_args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();

    let initial_title = match file_args.last() {
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
            if !file_args.is_empty() {
                app.open_files_from_cli(file_args);
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

/// Zapisuje przyczynę paniki do ~/.cache/dziwak/crash.log (program z menu nie ma terminala).
fn install_crash_log() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .map(|d| d.join("dziwak"));
        if let Some(dir) = dir {
            let _ = std::fs::create_dir_all(&dir);
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let entry = format!(
                "[{secs}] Dziwak {}: {info}\n{}\n\n",
                env!("CARGO_PKG_VERSION"),
                std::backtrace::Backtrace::force_capture()
            );
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("crash.log"))
            {
                use std::io::Write;
                let _ = f.write_all(entry.as_bytes());
            }
        }
        default_hook(info);
    }));
}
