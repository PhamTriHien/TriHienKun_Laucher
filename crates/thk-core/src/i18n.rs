use crate::models::Language;
use std::collections::HashMap;

const VI_JSON: &str = include_str!("../../../assets/locales/vi.json");
const EN_JSON: &str = include_str!("../../../assets/locales/en.json");

#[derive(Debug, Clone)]
pub struct I18n {
    vi: HashMap<String, String>,
    en: HashMap<String, String>,
    current: Language,
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(Language::Vi)
    }
}

impl I18n {
    pub fn new(lang: Language) -> Self {
        let vi = serde_json::from_str(VI_JSON).unwrap_or_default();
        let en = serde_json::from_str(EN_JSON).unwrap_or_default();
        Self {
            vi,
            en,
            current: lang,
        }
    }

    pub fn set_language(&mut self, lang: Language) {
        self.current = lang;
    }

    pub fn language(&self) -> Language {
        self.current
    }

    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        let primary = match self.current {
            Language::Vi => &self.vi,
            Language::En => &self.en,
        };
        primary
            .get(key)
            .or_else(|| self.vi.get(key))
            .or_else(|| self.en.get(key))
            .map(String::as_str)
            .unwrap_or(key)
    }

    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut base = self.t(key).to_string();
        for &(k, v) in args {
            base = base.replace(&format!("{{{k}}}"), v);
        }
        base
    }

    pub fn tr<'a>(&self, vi: &'a str, en: &'a str) -> &'a str {
        match self.current {
            Language::Vi => vi,
            Language::En => en,
        }
    }

    pub fn mismatched_keys(&self) -> Vec<String> {
        let mut diff = Vec::new();
        for k in self.vi.keys() {
            if !self.en.contains_key(k) {
                diff.push(format!("missing_in_en:{k}"));
            }
        }
        for k in self.en.keys() {
            if !self.vi.contains_key(k) {
                diff.push(format!("missing_in_vi:{k}"));
            }
        }
        diff.sort();
        diff
    }
}
