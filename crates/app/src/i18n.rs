//! Tłumaczenia interfejsu (polski, angielski, hiszpański) bez zewnętrznych zależności.
//!
//! Teksty w kodzie są po polsku i przechodzą przez [`tr`]; brakujące tłumaczenie
//! zwraca polski oryginał. Tabela tłumaczeń jest w `i18n_table.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Pl,
    En,
    Es,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Pl, Lang::En, Lang::Es];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Pl => "pl",
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        match code.get(..2)?.to_ascii_lowercase().as_str() {
            "pl" => Some(Lang::Pl),
            "en" => Some(Lang::En),
            "es" => Some(Lang::Es),
            _ => None,
        }
    }

    /// Nazwa języka w nim samym (do menu wyboru).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Pl => "Polski",
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }

    fn to_u8(self) -> u8 {
        self as u8
    }

    fn from_u8(v: u8) -> Lang {
        match v {
            1 => Lang::En,
            2 => Lang::Es,
            _ => Lang::Pl,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    Lang::from_u8(CURRENT.load(Ordering::Relaxed))
}

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang.to_u8(), Ordering::Relaxed);
}

fn table() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    static TABLE: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        crate::i18n_table::TABLE
            .iter()
            .map(|&(pl, en, es)| (pl, (en, es)))
            .collect()
    })
}

/// Tłumaczy polski tekst interfejsu na bieżący język (brak tłumaczenia = oryginał).
pub fn tr(pl: &'static str) -> &'static str {
    match lang() {
        Lang::Pl => pl,
        Lang::En => table().get(pl).map_or(pl, |t| t.0),
        Lang::Es => table().get(pl).map_or(pl, |t| t.1),
    }
}

/// Język z ustawień systemu (LC_ALL, LC_MESSAGES, LANG); domyślnie angielski.
pub fn system_lang() -> Lang {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .and_then(|v| Lang::from_code(&v))
        .unwrap_or(Lang::En)
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|d| d.join("dziwak/settings"))
}

/// Język zapisany w ~/.config/dziwak/settings (linia `lang=xx`).
pub fn saved_lang() -> Option<Lang> {
    let text = std::fs::read_to_string(settings_path()?).ok()?;
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("lang="))
        .find_map(Lang::from_code)
}

/// Zapisuje wybrany język w ustawieniach (błędy zapisu są ignorowane — to tylko wygoda).
pub fn save_lang(lang: Lang) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("lang={}\n", lang.code()));
}

/// Ustawia język przy starcie: zapisany w ustawieniach, a jeśli brak — systemowy.
pub fn init() {
    set_lang(saved_lang().unwrap_or_else(system_lang));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_code() {
        assert_eq!(Lang::from_code("pl_PL.UTF-8"), Some(Lang::Pl));
        assert_eq!(Lang::from_code("es_ES"), Some(Lang::Es));
        assert_eq!(Lang::from_code("EN"), Some(Lang::En));
        assert_eq!(Lang::from_code("de_DE"), None);
        assert_eq!(Lang::from_code("x"), None);
    }

    #[test]
    fn test_table_has_no_duplicates_or_empty() {
        let mut seen = std::collections::HashSet::new();
        for &(pl, en, es) in crate::i18n_table::TABLE {
            assert!(seen.insert(pl), "duplikat: {pl}");
            assert!(!en.is_empty() && !es.is_empty(), "puste tłumaczenie: {pl}");
        }
    }

    #[test]
    fn test_tr_fallback_and_switch() {
        set_lang(Lang::Pl);
        assert_eq!(tr("Plik"), "Plik");
        set_lang(Lang::En);
        assert_eq!(tr("Plik"), "File");
        assert_eq!(tr("tekst bez tłumaczenia"), "tekst bez tłumaczenia");
        set_lang(Lang::Pl);
    }
}
