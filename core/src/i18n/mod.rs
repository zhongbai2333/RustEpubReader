//! Shared application catalogs and formatting. Source files also ship to native clients.
use std::collections::HashMap;

const ZH_CN: &str = include_str!("zh_cn.json");
const EN: &str = include_str!("en.json");

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    #[default]
    Auto,
    ZhCN,
    En,
}
impl Language {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "System",
            Self::ZhCN => "中文",
            Self::En => "English",
        }
    }
    pub fn all() -> &'static [Language] {
        &[Self::Auto, Self::ZhCN, Self::En]
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::ZhCN => "zh_cn",
            Self::En => "en",
        }
    }
    pub fn from_code(code: &str) -> Self {
        let code = code.trim().to_ascii_lowercase().replace('_', "-");
        match code.split('-').next().unwrap_or("") {
            "zh" => Self::ZhCN,
            "en" => Self::En,
            _ => Self::Auto,
        }
    }
    pub fn resolve(&self, system: &str) -> Self {
        if self != &Self::Auto {
            return self.clone();
        }
        if system
            .to_ascii_lowercase()
            .replace('_', "-")
            .split('-')
            .next()
            == Some("zh")
        {
            Self::ZhCN
        } else {
            Self::En
        }
    }
}

/// Formats only the template: argument contents never become additional placeholders.
/// Supports sequential {}, indexed {0}, named {name}, and escaped {{/}}.
pub fn format_template(template: &str, args: &[&str], named: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut cursor = 0;
    let mut sequential = 0;
    while cursor < template.len() {
        let tail = &template[cursor..];
        if tail.starts_with("{{") {
            out.push('{');
            cursor += 2;
            continue;
        }
        if tail.starts_with("}}") {
            out.push('}');
            cursor += 2;
            continue;
        }
        if tail.starts_with('{') {
            if let Some(end) = tail.find('}') {
                let slot = &tail[1..end];
                let replacement = if slot.is_empty() {
                    let i = sequential;
                    sequential += 1;
                    args.get(i).copied()
                } else if let Ok(i) = slot.parse::<usize>() {
                    args.get(i).copied()
                } else {
                    named
                        .iter()
                        .find(|(name, _)| *name == slot)
                        .map(|(_, value)| *value)
                };
                if let Some(value) = replacement {
                    out.push_str(value);
                    cursor += end + 1;
                    continue;
                }
            }
        }
        let ch = tail.chars().next().unwrap();
        out.push(ch);
        cursor += ch.len_utf8();
    }
    out
}

#[derive(Debug, Clone)]
pub struct I18n {
    preference: Language,
    language: Language,
    translations: HashMap<String, String>,
    message_cache: std::sync::Arc<std::sync::Mutex<HashMap<String, String>>>,
}
impl I18n {
    pub fn new(preference: Language) -> Self {
        Self::with_system(preference, &sys_locale::get_locale().unwrap_or_default())
    }
    pub fn with_system(preference: Language, system: &str) -> Self {
        let language = preference.resolve(system);
        let mut value = Self {
            preference,
            language,
            translations: HashMap::new(),
            message_cache: Default::default(),
        };
        value.translations = Self::catalog(&value.language);
        value
    }
    fn parse_json(input: &str) -> Result<HashMap<String, String>, serde_json::Error> {
        serde_json::from_str(input)
    }
    fn catalog(language: &Language) -> HashMap<String, String> {
        // Invalid shipped English is a packaging failure; validation runs before all builds.
        let mut fallback = Self::parse_json(EN).expect("Invalid bundled English catalog");
        if language == &Language::ZhCN {
            if let Ok(selected) = Self::parse_json(ZH_CN) {
                fallback.extend(selected);
            }
        }
        fallback
    }
    pub fn language(&self) -> &Language {
        &self.language
    }
    pub fn preference(&self) -> &Language {
        &self.preference
    }
    pub fn choice_label(&self, value: &Language) -> &str {
        if value == &Language::Auto {
            self.t("language.auto")
        } else {
            value.label()
        }
    }
    pub fn set_language(&mut self, preference: Language) {
        self.preference = preference;
        self.refresh_with_system(&sys_locale::get_locale().unwrap_or_default());
    }
    pub fn refresh_with_system(&mut self, system: &str) -> bool {
        let resolved = self.preference.resolve(system);
        if resolved == self.language {
            return false;
        }
        self.translations = Self::catalog(&resolved);
        self.language = resolved;
        true
    }
    pub fn refresh_system_language(&mut self) -> bool {
        self.preference == Language::Auto
            && self.refresh_with_system(&sys_locale::get_locale().unwrap_or_default())
    }
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.translations
            .get(key)
            .map(String::as_str)
            .unwrap_or_else(|| {
                #[cfg(debug_assertions)]
                eprintln!("[i18n] missing key: {key}");
                if self.language == Language::ZhCN {
                    "暂时不可用"
                } else {
                    "Unavailable"
                }
            })
    }
    pub fn tf(&self, key: &str, args: &[&str]) -> String {
        format_template(self.t(key), args, &[])
    }
    pub fn tf1(&self, key: &str, arg: &str) -> String {
        self.tf(key, &[arg])
    }
    pub fn tf2(&self, key: &str, arg1: &str, arg2: &str) -> String {
        self.tf(key, &[arg1, arg2])
    }
    pub fn tf_named(&self, key: &str, args: &[(&str, &str)]) -> String {
        format_template(self.t(key), &[], args)
    }
    pub fn count(&self, key: &str, count: usize) -> String {
        let variant = format!("{key}.{}", if count == 1 { "one" } else { "other" });
        self.tf1(
            if self.translations.contains_key(&variant) {
                &variant
            } else {
                key
            },
            &count.to_string(),
        )
    }
    /// Re-render a stored application status. Never pass book text or user notes here.
    pub fn message(&self, value: &str) -> String {
        let cache_key = format!("{}:{value}", self.language.code());
        if let Some(cached) = self.message_cache.lock().unwrap().get(&cache_key) {
            return cached.clone();
        }
        let translated = self.resolve_message(value);
        let mut cache = self.message_cache.lock().unwrap();
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(cache_key, translated.clone());
        translated
    }
    fn resolve_message(&self, value: &str) -> String {
        let catalogs = [
            Self::parse_json(EN).unwrap_or_default(),
            Self::parse_json(ZH_CN).unwrap_or_default(),
        ];
        for catalog in &catalogs {
            for (key, template) in catalog {
                if value == template {
                    return self.t(key).to_owned();
                }
            }
        }
        let slots = regex::Regex::new(r"\{([0-9]*)\}").unwrap();
        for catalog in &catalogs {
            let mut templates: Vec<_> = catalog.iter().collect();
            templates.sort_by_key(|(_, value)| std::cmp::Reverse(value.len()));
            for (key, template) in templates {
                if !slots.is_match(template) {
                    continue;
                }
                let mut pattern = String::from("^");
                let mut last = 0;
                let mut indices = Vec::new();
                for capture in slots.captures_iter(template) {
                    let m = capture.get(0).unwrap();
                    pattern.push_str(&regex::escape(&template[last..m.start()]));
                    pattern.push_str("(.*?)");
                    indices.push(capture[1].parse::<usize>().unwrap_or(indices.len()));
                    last = m.end();
                }
                pattern.push_str(&regex::escape(&template[last..]));
                pattern.push('$');
                if let Ok(re) = regex::Regex::new(&pattern) {
                    if let Some(capture) = re.captures(value) {
                        let mut args = vec![""; indices.iter().max().copied().unwrap_or(0) + 1];
                        for (group, index) in indices.iter().enumerate() {
                            args[*index] = capture.get(group + 1).unwrap().as_str();
                        }
                        return self.tf(key, &args);
                    }
                }
            }
        }
        value.to_owned()
    }
    pub fn get_all_translations(&self) -> &HashMap<String, String> {
        &self.translations
    }
}
impl Default for I18n {
    fn default() -> Self {
        Self::new(Language::Auto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_preference_survives_resolution_and_refresh() {
        let mut i = I18n::with_system(Language::Auto, "en-US");
        assert_eq!(i.preference(), &Language::Auto);
        assert_eq!(i.language(), &Language::En);
        assert!(i.refresh_with_system("zh-Hant-HK"));
        assert_eq!(i.preference(), &Language::Auto);
        i.set_language(Language::En);
        assert!(!i.refresh_with_system("zh-CN"));
    }
    #[test]
    fn formats_reordered_named_and_literal_arguments_once() {
        assert_eq!(
            format_template("{} / {}", &["literal {}", "second"], &[]),
            "literal {} / second"
        );
        assert_eq!(
            format_template("{1} / {0}", &["😀", "{1}"], &[]),
            "{1} / 😀"
        );
        assert_eq!(
            format_template("{{{name}}}", &[], &[("name", "{}")]),
            "{{}}"
        );
    }
    #[test]
    fn plural_counts_and_stored_status_relocalize() {
        let en = I18n::with_system(Language::En, "zh");
        let zh = I18n::with_system(Language::ZhCN, "en");
        assert_eq!(en.count("ui.library_count", 1), "1 book");
        assert_eq!(en.count("ui.library_count", 2), "2 books");
        assert_eq!(en.message(zh.t("tts.playing")), en.t("tts.playing"));
        assert_eq!(
            en.message(&zh.tf1("common.books_imported_count", "1")),
            "Books imported: 1"
        );
        assert_eq!(Language::Auto.resolve("fr-FR"), Language::En);
        assert_eq!(format_template("{4}", &["x"], &[]), "{4}");
    }
    #[test]
    fn shipped_catalogs_match_and_are_valid() {
        let zh = I18n::parse_json(ZH_CN).unwrap();
        let en = I18n::parse_json(EN).unwrap();
        assert_eq!(zh.len(), en.len());
        assert!(zh.keys().all(|key| en.contains_key(key)));
        assert!(I18n::parse_json("broken").is_err());
        assert_eq!(Language::from_code("zh_CN"), Language::ZhCN);
        assert_eq!(Language::from_code("en-GB"), Language::En);
        assert_eq!(Language::from_code("system"), Language::Auto);
        assert_eq!(
            I18n::with_system(Language::En, "zh").t("missing"),
            "Unavailable"
        );
    }
}
