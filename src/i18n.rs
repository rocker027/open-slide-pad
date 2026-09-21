use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::LazyLock};

pub const ENGLISH_CATALOG: &str = include_str!("../ui/locales/en.json");
static ENGLISH: LazyLock<BTreeMap<String, String>> =
    LazyLock::new(|| serde_json::from_str(ENGLISH_CATALOG).expect("valid translation catalog"));
// 由長到短排序只算一次：先比對具體訊息，避免「{count} 個網站」吃掉「最多保留 {count} 個網站」。
static TEMPLATES: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    let mut templates: Vec<_> = ENGLISH
        .iter()
        .filter(|(key, _)| key.contains('{'))
        .map(|(key, translated)| (key.as_str(), translated.as_str()))
        .collect();
    templates.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));
    templates
});

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-TW")]
    TraditionalChinese,
}

impl Language {
    pub fn is_default(&self) -> bool {
        *self == Self::English
    }

    pub fn text(self, source: &str) -> String {
        self.translate(source, 0)
    }

    fn translate(self, source: &str, depth: usize) -> String {
        if self == Self::TraditionalChinese || depth > 4 {
            return source.to_owned();
        }
        if let Some(translated) = ENGLISH.get(source) {
            return translated.clone();
        }
        for (template, translated) in TEMPLATES.iter() {
            if let Some(arguments) = capture_arguments(template, source) {
                return arguments.into_iter().fold(
                    translated.to_string(),
                    |text, (name, value)| {
                        text.replace(&format!("{{{name}}}"), &self.translate(value, depth + 1))
                    },
                );
            }
        }
        // anyhow 的 context chain 保留技術原因，只翻譯我們已定義的訊息。
        if let Some((context, cause)) = source.split_once(": ") {
            return format!(
                "{}: {}",
                self.translate(context, depth + 1),
                self.translate(cause, depth + 1)
            );
        }
        if let Some((first, second)) = source.split_once('；') {
            return format!(
                "{}; {}",
                self.translate(first, depth + 1),
                self.translate(second, depth + 1)
            );
        }
        source.to_owned()
    }
}

fn capture_arguments<'a>(
    mut template: &'a str,
    mut source: &'a str,
) -> Option<Vec<(&'a str, &'a str)>> {
    let mut arguments = Vec::new();
    while let Some(open) = template.find('{') {
        source = source.strip_prefix(&template[..open])?;
        let close = template[open..].find('}')? + open;
        let name = &template[open + 1..close];
        template = &template[close + 1..];
        let suffix = template.split('{').next()?;
        let end = if suffix.is_empty() {
            source.len()
        } else {
            source.find(suffix)?
        };
        arguments.push((name, &source[..end]));
        source = &source[end..];
    }
    (source == template).then_some(arguments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_default_and_unknown_languages_are_rejected() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(
            serde_json::to_string(&Language::TraditionalChinese).unwrap(),
            "\"zh-TW\""
        );
        assert!(serde_json::from_str::<Language>("\"fr\"").is_err());
    }

    #[test]
    fn localizes_messages_parameters_and_preserves_external_details() {
        assert_eq!(Language::English.text("設定"), "Settings");
        assert_eq!(Language::TraditionalChinese.text("設定"), "設定");
        assert_eq!(
            Language::English.text("最多保留 20 個網站"),
            "You can save up to 20 sites"
        );
        assert_eq!(
            Language::English.text("未變更快捷鍵：無法保存設定: disk full"),
            "Shortcut was not changed: Could not save settings: disk full"
        );
        assert_eq!(Language::English.text("external detail"), "external detail");
        assert_eq!(
            Language::English.text("快捷鍵已更新，但舊快捷鍵 ⌘ F18 解除失敗：busy"),
            "Shortcut updated, but the old shortcut ⌘ F18 could not be released: busy"
        );
    }
}
