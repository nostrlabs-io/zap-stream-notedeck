use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

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
    ("el", "Ελληνικά"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("zh", "中文"),
    ("ar", "العربية"),
];

pub const RTL_LOCALES: &[&str] = &["ar"];

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
        "el" => include_str!("../assets/i18n/el.tsv"),
        "ja" => include_str!("../assets/i18n/ja.tsv"),
        "ko" => include_str!("../assets/i18n/ko.tsv"),
        "ru" => include_str!("../assets/i18n/ru.tsv"),
        "uk" => include_str!("../assets/i18n/uk.tsv"),
        "zh" => include_str!("../assets/i18n/zh.tsv"),
        "ar" => include_str!("../assets/i18n/ar.tsv"),
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

pub fn is_rtl() -> bool {
    RTL_LOCALES.contains(&current_code())
}

fn visual_cache() -> &'static Mutex<HashMap<&'static str, &'static str>> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub fn t(key: &'static str) -> &'static str {
    let logical = lookup(CURRENT.load(Ordering::Relaxed), key);
    if !is_rtl() {
        return logical;
    }
    let mut cache = visual_cache().lock().unwrap();
    cache
        .entry(logical)
        .or_insert_with(|| Box::leak(crate::bidi::visual(logical).into_owned().into_boxed_str()))
}

pub fn tl(key: &'static str) -> &'static str {
    lookup(CURRENT.load(Ordering::Relaxed), key)
}

pub fn tf(key: &'static str, args: &[(&str, &str)]) -> String {
    let mut out = lookup(CURRENT.load(Ordering::Relaxed), key).to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    if is_rtl() {
        crate::bidi::visual(&out).into_owned()
    } else {
        out
    }
}

pub fn detect_system_locale() -> Option<&'static str> {
    let env = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .ok();
    resolve_system_locale(env, sys_locale::get_locale)
}

fn resolve_system_locale(env: Option<String>, os: impl FnOnce() -> Option<String>) -> Option<&'static str> {
    let raw = env.filter(|v| locale_index(v).is_some()).or_else(os)?;
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

    fn locale_files() -> Vec<(String, HashMap<String, String>)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/i18n");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "tsv") {
                let code = path.file_stem().unwrap().to_string_lossy().to_string();
                let map = std::fs::read_to_string(&path)
                    .unwrap()
                    .lines()
                    .filter_map(|l| l.split_once('='))
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                out.push((code, map));
            }
        }
        out
    }

    #[test]
    fn every_locale_file_has_every_english_key_except_the_short_legacy_ones() {
        let files = locale_files();
        let en = &files.iter().find(|(c, _)| c == "en").unwrap().1;
        for (code, map) in &files {
            if ["ar", "el", "ja", "ko", "ru", "uk", "zh"].contains(&code.as_str()) {
                for key in en.keys() {
                    assert!(map.contains_key(key), "{code} is missing key {key}");
                }
            }
        }
    }

    #[test]
    fn every_locale_file_keeps_english_placeholders() {
        let files = locale_files();
        let en = &files.iter().find(|(c, _)| c == "en").unwrap().1;
        for (code, map) in &files {
            for (key, value) in map {
                let en_value = &en[key];
                for name in ["{n}", "{name}", "{host}", "{left}", "{time}", "{when}", "{names}", "{users}", "{moderator}", "{duration}", "{error}", "{code}", "{rate}", "{unit}"] {
                    assert_eq!(en_value.contains(name), value.contains(name), "{code} {key} placeholder {name}");
                }
            }
        }
    }

    #[test]
    fn every_offered_locale_has_a_source_file() {
        for (code, _) in LOCALES {
            assert!(!source(code).is_empty(), "{code} has no source");
        }
    }

    #[test]
    fn arabic_is_offered_detected_and_rtl() {
        assert!(LOCALES.iter().any(|(c, _)| *c == "ar"));
        assert_eq!(locale_index("ar_SA.UTF-8"), locale_index("ar"));
        assert!(RTL_LOCALES.iter().all(|c| locale_index(c).is_some()));
    }

    #[test]
    fn arabic_strings_come_back_in_visual_order() {
        let ar = locale_index("ar").unwrap();
        let logical = lookup(ar, "home.live_now");
        let visual = crate::bidi::visual(logical);
        let reversed: Vec<&str> = logical.split(' ').rev().collect();
        assert_eq!(visual.split(' ').collect::<Vec<_>>(), reversed);
    }

    #[test]
    fn falls_back_to_english_then_key() {
        let de = locale_index("de").unwrap();
        assert_eq!(lookup(de, "button.login"), "Anmelden");
        assert_eq!(lookup(de, "home.live_now"), "Live now");
        assert_eq!(lookup(de, "no.such.key"), "no.such.key");
    }

    #[test]
    fn system_locale_falls_back_to_the_os_when_lang_is_unset() {
        assert_eq!(resolve_system_locale(None, || Some("ja-JP".into())), Some("ja"));
        assert_eq!(resolve_system_locale(Some("C.UTF-8".into()), || Some("zh-Hans-CN".into())), Some("zh"));
        assert_eq!(resolve_system_locale(Some("uk_UA.UTF-8".into()), || Some("ru-RU".into())), Some("uk"));
        assert_eq!(resolve_system_locale(None, || None), None);
    }

    #[test]
    fn locale_codes_with_region_resolve() {
        assert_eq!(locale_index("de_DE.UTF-8"), Some(3));
        assert_eq!(locale_index("pt-BR"), locale_index("pt"));
        assert_eq!(locale_index("ja_JP"), locale_index("ja"));
        assert_eq!(locale_index("xx_XX"), None);
    }

    #[test]
    fn placeholders_are_filled() {
        let out = tf("home.watching", &[("host", "a"), ("n", "3")]);
        assert!(out.contains('a') && out.contains('3') && !out.contains('{'));
    }
}

#[cfg(test)]
mod key_usage_tests {
    use super::*;
    use std::path::Path;

    fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    fn keys_in(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        for call in ["i18n::t(\"", "i18n::tf(\"", " t(\"", "(t(\""] {
            let mut rest = text;
            while let Some(i) = rest.find(call) {
                rest = &rest[i + call.len()..];
                if let Some(end) = rest.find('"') {
                    found.push(rest[..end].to_string());
                }
            }
        }
        found
    }

    #[test]
    fn every_key_used_in_code_exists_in_english() {
        let mut files = Vec::new();
        rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let english = &tables()[0];
        let mut missing = Vec::new();
        let mut used = 0;
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for key in keys_in(&text) {
                used += 1;
                if !key.contains('{') && !english.contains_key(key.as_str()) {
                    missing.push(format!("{}: {}", file.display(), key));
                }
            }
        }
        assert!(used > 50, "scanner found only {used} keys");
        assert!(missing.is_empty(), "keys missing from en.tsv: {missing:?}");
    }
}
