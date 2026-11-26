//! UI localization generated from the upstream `.lproj` bundles.
//!
//! `i18n.json` maps English source strings to every available translation.
//! Call sites pass `(english, simplified_chinese)`; the Chinese fallback keeps
//! working for strings that upstream has not translated.

use std::collections::HashMap;
use std::sync::OnceLock;

static LANG: OnceLock<String> = OnceLock::new();
static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

const BUNDLED: &str = include_str!("../assets/i18n.json");

fn normalize(raw: &str) -> String {
    let raw = raw.trim().replace('_', "-");
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("zh") {
        if lower.contains("hant") || lower.contains("tw") || lower.contains("hk") {
            "zh-Hant".into()
        } else {
            "zh-Hans".into()
        }
    } else if lower.starts_with("en") {
        "en".into()
    } else {
        lower.split('-').next().unwrap_or("en").to_string()
    }
}

pub fn language() -> &'static str {
    LANG.get_or_init(|| {
        if let Ok(value) = std::env::var("IMAGEOPTIM_LANG") {
            return normalize(&value);
        }
        #[cfg(target_os = "macos")]
        if let Ok(out) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(start) = text.find('"')
                && let Some(end) = text[start + 1..].find('"')
            {
                return normalize(&text[start + 1..start + 1 + end]);
            }
        }
        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(key) {
                return normalize(&value);
            }
        }
        "en".into()
    })
}

pub fn is_chinese() -> bool {
    language().starts_with("zh")
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    TABLE.get_or_init(|| {
        let mut out: HashMap<&'static str, &'static str> = HashMap::new();
        let Ok(all) = serde_json::from_str::<HashMap<String, HashMap<String, String>>>(BUNDLED)
        else {
            return out;
        };
        let lang = language();
        let key = if all.contains_key(lang) {
            Some(lang)
        } else {
            lang.split('-')
                .next()
                .filter(|base| all.contains_key(*base))
        };
        let Some(key) = key else {
            return out;
        };
        for (en, translated) in &all[key] {
            let en: &'static str = Box::leak(en.clone().into_boxed_str());
            let translated: &'static str = Box::leak(translated.clone().into_boxed_str());
            out.insert(en, translated);
        }
        out
    })
}

/// Return the translation for `en`, falling back to `zh` on Chinese systems and
/// to the English source everywhere else.
pub fn t(en: &'static str, zh: &'static str) -> &'static str {
    if let Some(value) = table().get(en) {
        return value;
    }
    if is_chinese() { zh } else { en }
}

/// Like [`t`], rewriting upstream brand strings to `ImageOptim-Rust` while
/// preserving all 27 upstream translations.
pub fn t_app(en: &'static str, zh: &'static str) -> String {
    t(en, zh).replace("ImageOptim", "ImageOptim-Rust")
}
