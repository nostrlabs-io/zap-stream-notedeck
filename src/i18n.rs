use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

type Table = HashMap<&'static str, &'static str>;

pub const LOCALES: &[(&str, &str)] = &[
    ("en", "English"),
    ("cs", "Čeština"),
    ("da", "Dansk"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("fi", "Suomi"),
    ("fr", "Français"),
    ("hu", "Magyar"),
    ("it", "Italiano"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("ro", "Română"),
    ("sv", "Svenska"),
    ("tr", "Türkçe"),
];

static CURRENT: AtomicUsize = AtomicUsize::new(0);

fn source(code: &str) -> &'static str {
    match code {
        "en" => include_str!("../assets/i18n/en.tsv"),
        "cs" => include_str!("../assets/i18n/cs.tsv"),
        "da" => include_str!("../assets/i18n/da.tsv"),
        "de" => include_str!("../assets/i18n/de.tsv"),
        "es" => include_str!("../assets/i18n/es.tsv"),
        "fi" => include_str!("../assets/i18n/fi.tsv"),
        "fr" => include_str!("../assets/i18n/fr.tsv"),
        "hu" => include_str!("../assets/i18n/hu.tsv"),
        "it" => include_str!("../assets/i18n/it.tsv"),
        "nl" => include_str!("../assets/i18n/nl.tsv"),
        "pl" => include_str!("../assets/i18n/pl.tsv"),
        "pt" => include_str!("../assets/i18n/pt.tsv"),
        "ro" => include_str!("../assets/i18n/ro.tsv"),
        "sv" => include_str!("../assets/i18n/sv.tsv"),
        "tr" => include_str!("../assets/i18n/tr.tsv"),
        _ => "",
    }
}

fn parse(text: &'static str) -> Table {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .collect()
}

fn tables() -> &'static Vec<Table> {
    static TABLES: OnceLock<Vec<Table>> = OnceLock::new();
    TABLES.get_or_init(|| LOCALES.iter().map(|(c, _)| parse(source(c))).collect())
}

pub fn locale_index(code: &str) -> Option<usize> {
    let base = code.split(['-', '_']).next().unwrap_or(code).to_lowercase();
    LOCALES.iter().position(|(c, _)| *c == base)
}

pub fn set_locale(code: &str) {
    if let Some(i) = locale_index(code) {
        CURRENT.store(i, Ordering::Relaxed);
    }
}

pub fn current_code() -> &'static str {
    LOCALES[CURRENT.load(Ordering::Relaxed)].0
}

fn lookup(index: usize, key: &'static str) -> &'static str {
    let tables = tables();
    tables[index]
        .get(key)
        .or_else(|| tables[0].get(key))
        .copied()
        .unwrap_or(key)
}

pub fn t(key: &'static str) -> &'static str {
    lookup(CURRENT.load(Ordering::Relaxed), key)
}

pub fn tf(key: &'static str, args: &[(&str, &str)]) -> String {
    let mut out = t(key).to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

pub fn detect_system_locale() -> Option<&'static str> {
    let raw = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .ok()?;
    locale_index(&raw).map(|i| LOCALES[i].0)
}

pub fn load_saved(dir: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(dir.join("zap_locale")).ok().map(|s| s.trim().to_string())
}

pub fn save(dir: &std::path::Path, code: &str) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join("zap_locale"), code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_key_exists_in_english() {
        let tables = tables();
        for (i, (code, _)) in LOCALES.iter().enumerate().skip(1) {
            for key in tables[i].keys() {
                assert!(tables[0].contains_key(key), "{code} has unknown key {key}");
            }
        }
    }

    #[test]
    fn falls_back_to_english_then_key() {
        let de = locale_index("de").unwrap();
        assert_eq!(lookup(de, "button.login"), "Anmelden");
        assert_eq!(lookup(de, "home.live_now"), "Live now");
        assert_eq!(lookup(de, "no.such.key"), "no.such.key");
    }

    #[test]
    fn locale_codes_with_region_resolve() {
        assert_eq!(locale_index("de_DE.UTF-8"), Some(3));
        assert_eq!(locale_index("pt-BR"), locale_index("pt"));
        assert_eq!(locale_index("ja_JP"), None);
    }

    #[test]
    fn placeholders_are_filled() {
        let out = tf("home.watching", &[("host", "a"), ("n", "3")]);
        assert!(out.contains('a') && out.contains('3') && !out.contains('{'));
    }
}
