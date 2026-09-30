//! Kolory z aktualnego motywu Omarchy.
use dziwak_core::color::hex_to_rgb;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OmarchyTheme {
    pub dark: bool,
    pub accent: [u8; 3],
    pub background: [u8; 3],
    pub lighter_background: [u8; 3],
    pub foreground: [u8; 3],
    pub selection: [u8; 3],
}

/// Parsuje zawartość colors.toml (linie `klucz = "wartość"`, komentarze od #, puste linie pomijane).
/// Zwraca None, jeśli brakuje któregoś z kluczy accent, background, lighter_background, foreground, selection lub hex jest zły.
/// `mode = "light"` -> dark=false, brak lub inna wartość -> dark=true.
pub fn parse_theme(text: &str) -> Option<OmarchyTheme> {
    let mut dark = true;
    let mut accent = None;
    let mut background = None;
    let mut lighter_background = None;
    let mut foreground = None;
    let mut selection = None;

    for line in text.lines() {
        let line = line.trim();

        // pomiń puste linie i komentarze
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // rozdziel klucz i wartość
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');

        match key {
            "mode" => {
                dark = value != "light";
            }
            "accent" => {
                accent = hex_to_rgb(value).ok();
            }
            "background" => {
                background = hex_to_rgb(value).ok();
            }
            "lighter_background" => {
                lighter_background = hex_to_rgb(value).ok();
            }
            "foreground" => {
                foreground = hex_to_rgb(value).ok();
            }
            "selection" => {
                selection = hex_to_rgb(value).ok();
            }
            _ => {}
        }
    }

    let (
        Some(accent),
        Some(background),
        Some(lighter_background),
        Some(foreground),
        Some(selection),
    ) = (
        accent,
        background,
        lighter_background,
        foreground,
        selection,
    )
    else {
        return None;
    };

    Some(OmarchyTheme {
        dark,
        accent,
        background,
        lighter_background,
        foreground,
        selection,
    })
}

/// Aplikuje motyw Omarchy do kontekstu egui (E12).
pub fn apply_theme(ctx: &eframe::egui::Context, theme: &OmarchyTheme) {
    let mut visuals = if theme.dark {
        eframe::egui::Visuals::dark()
    } else {
        eframe::egui::Visuals::light()
    };

    let bg = eframe::egui::Color32::from_rgb(
        theme.background[0],
        theme.background[1],
        theme.background[2],
    );
    let lighter_bg = eframe::egui::Color32::from_rgb(
        theme.lighter_background[0],
        theme.lighter_background[1],
        theme.lighter_background[2],
    );
    let fg = eframe::egui::Color32::from_rgb(
        theme.foreground[0],
        theme.foreground[1],
        theme.foreground[2],
    );
    let sel =
        eframe::egui::Color32::from_rgb(theme.selection[0], theme.selection[1], theme.selection[2]);
    let acc = eframe::egui::Color32::from_rgb(theme.accent[0], theme.accent[1], theme.accent[2]);

    visuals.panel_fill = bg;
    visuals.window_fill = bg;
    visuals.extreme_bg_color = lighter_bg;
    visuals.selection.bg_fill = sel;
    visuals.hyperlink_color = acc;
    visuals.widgets.active.bg_fill = acc;
    visuals.widgets.active.weak_bg_fill = acc;
    visuals.override_text_color = Some(fg);

    ctx.set_visuals(visuals);
}

/// Ścieżka do colors.toml aktualnego motywu: $HOME/.local/state/omarchy/current/theme/colors.toml
pub fn current_theme_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("omarchy")
            .join("current")
            .join("theme")
            .join("colors.toml")
    })
}

/// Wczytuje i parsuje aktualny motyw; None gdy brak pliku lub błąd.
pub fn load_current_theme() -> Option<OmarchyTheme> {
    let path = current_theme_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    parse_theme(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_theme_dark() {
        let text = "
# Komentarz
mode = \"dark\"
accent = \"#89b4fa\"
selection = \"#45475a\"
background = \"#1e1e2e\"
lighter_background = \"#313244\"
foreground = \"#cdd6f4\"

";

        let theme = parse_theme(text);
        assert!(theme.is_some());
        let theme = theme.unwrap();
        assert!(theme.dark);
        assert_eq!(theme.accent, [0x89, 0xb4, 0xfa]);
        assert_eq!(theme.background, [0x1e, 0x1e, 0x2e]);
        assert_eq!(theme.lighter_background, [0x31, 0x32, 0x44]);
        assert_eq!(theme.foreground, [0xcd, 0xd6, 0xf4]);
        assert_eq!(theme.selection, [0x45, 0x47, 0x5a]);
    }

    #[test]
    fn test_parse_theme_light() {
        let text = "mode = \"light\"\naccent = \"#000000\"\nbackground = \"#ffffff\"\nlighter_background = \"#eeeeee\"\nforeground = \"#111111\"\nselection = \"#cccccc\"\nto nie jest linia klucz-wartosc";
        let theme = parse_theme(text).expect("pelny motyw");
        assert!(!theme.dark);
    }

    #[test]
    fn test_parse_theme_missing_key() {
        let text = "mode = \"dark\"
accent = \"#89b4fa\"
selection = \"#45475a\"
background = \"#1e1e2e\"
lighter_background = \"#313244\"
"; // brakuje foreground

        let theme = parse_theme(text);
        assert!(theme.is_none());
    }

    #[test]
    fn test_parse_theme_invalid_hex() {
        let text = "mode = \"dark\"
accent = \"#89b4fg\"  # nieprawidłowy kolor
selection = \"#45475a\"
background = \"#1e1e2e\"
lighter_background = \"#313244\"
foreground = \"#cdd6f4\"
";

        let theme = parse_theme(text);
        assert!(theme.is_none());
    }

    #[test]
    fn test_apply_theme_to_ctx() {
        let theme = OmarchyTheme {
            dark: false,
            accent: [10, 20, 30],
            background: [40, 50, 60],
            lighter_background: [70, 80, 90],
            foreground: [100, 110, 120],
            selection: [130, 140, 150],
        };
        let ctx = eframe::egui::Context::default();
        apply_theme(&ctx, &theme);

        let visuals = ctx.style().visuals.clone();
        assert!(!visuals.dark_mode);
        assert_eq!(
            visuals.panel_fill,
            eframe::egui::Color32::from_rgb(40, 50, 60)
        );
        assert_eq!(
            visuals.window_fill,
            eframe::egui::Color32::from_rgb(40, 50, 60)
        );
        assert_eq!(
            visuals.extreme_bg_color,
            eframe::egui::Color32::from_rgb(70, 80, 90)
        );
        assert_eq!(
            visuals.selection.bg_fill,
            eframe::egui::Color32::from_rgb(130, 140, 150)
        );
        assert_eq!(
            visuals.hyperlink_color,
            eframe::egui::Color32::from_rgb(10, 20, 30)
        );
        assert_eq!(
            visuals.widgets.active.bg_fill,
            eframe::egui::Color32::from_rgb(10, 20, 30)
        );
        assert_eq!(
            visuals.override_text_color,
            Some(eframe::egui::Color32::from_rgb(100, 110, 120))
        );
    }
}
