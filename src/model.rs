use crate::{i18n::Language, shortcut::Shortcut};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::Url;

pub const MAX_PADS: usize = 20;

fn default_top_offset() -> f64 {
    8.0
}

fn validate_title(title: &str) -> Result<()> {
    ensure!(
        !title.trim().is_empty()
            && title.chars().count() <= 80
            && !title.chars().any(char::is_control),
        "網站名稱須為 1 至 80 個字，且不可含有控制字元"
    );
    Ok(())
}

/// 僅讓網路網址進入 WebKit，避免輸入被當成本機檔案或外部程式。
pub fn web_url(input: &str) -> Result<Url> {
    ensure!(
        input.len() <= 8192 && !input.chars().any(char::is_control),
        "網址格式不正確"
    );
    let parsed = Url::parse(input)?;
    ensure!(
        matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some(),
        "僅支援 http 與 https 網址"
    );
    ensure!(
        parsed.username().is_empty() && parsed.password().is_none(),
        "網址不可含有帳號密碼"
    );
    Ok(parsed)
}

pub fn normalize_address(input: &str) -> Result<String> {
    ensure!(!input.chars().any(char::is_control), "輸入不可含有控制字元");
    let input = input.trim();
    ensure!(
        !input.is_empty() && input.len() <= 8192,
        "請輸入網址或搜尋內容"
    );
    if input.contains("://") || Url::parse(input).is_ok() {
        return Ok(web_url(input)?.to_string());
    }
    if !input.contains(char::is_whitespace) && (input.contains('.') || input == "localhost") {
        return Ok(web_url(&format!("https://{input}"))?.to_string());
    }
    let mut search = Url::parse("https://duckduckgo.com/")?;
    search.query_pairs_mut().append_pair("q", input);
    Ok(search.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Pad {
    pub id: u64,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Language::is_default")]
    pub language: Language,
    pub pads: Vec<Pad>,
    pub active: Option<u64>,
    pub next_id: u64,
    pub side: Side,
    pub width: f64,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default = "default_top_offset")]
    pub top_offset: f64,
    pub pinned: bool,
    pub hot_edge: bool,
    #[serde(default)]
    pub toggle_shortcut: Shortcut,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            language: Language::default(),
            pads: vec![],
            active: None,
            next_id: 1,
            side: Side::Right,
            width: 520.0,
            height: None,
            top_offset: default_top_offset(),
            pinned: false,
            hot_edge: true,
            toggle_shortcut: Shortcut::default(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "設定版本不支援，原檔已保留");
        self.toggle_shortcut.validate()?;
        ensure!((360.0..=960.0).contains(&self.width), "側欄寬度超出範圍");
        ensure!(
            self.height
                .is_none_or(|height| (320.0..=10_000.0).contains(&height)),
            "側欄高度超出範圍"
        );
        ensure!(
            (0.0..=10_000.0).contains(&self.top_offset),
            "側欄垂直位置超出範圍"
        );
        ensure!(self.pads.len() <= MAX_PADS, "最多保留 {MAX_PADS} 個網站");
        let mut ids = HashSet::new();
        for pad in &self.pads {
            ensure!(
                pad.id > 0 && pad.id < self.next_id && ids.insert(pad.id),
                "網站識別碼不正確"
            );
            validate_title(&pad.title)?;
            web_url(&pad.url)?;
        }
        ensure!(
            self.active.is_none_or(|id| ids.contains(&id)),
            "選取的網站不存在"
        );
        ensure!(
            self.pads.is_empty() == self.active.is_none(),
            "網站選取狀態不一致"
        );
        Ok(())
    }

    pub fn add(&self, input: &str) -> Result<Self> {
        ensure!(self.pads.len() < MAX_PADS, "最多保留 {MAX_PADS} 個網站");
        let url = normalize_address(input)?;
        let title = web_url(&url)?
            .host_str()
            .unwrap_or("網站")
            .chars()
            .take(80)
            .collect();
        let mut updated = self.clone();
        updated.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("識別碼已用盡"))?;
        updated.pads.push(Pad {
            id: self.next_id,
            title,
            url,
        });
        updated.active = Some(self.next_id);
        Ok(updated)
    }

    pub fn remove(&self, id: u64) -> Result<Self> {
        let Some(index) = self.pads.iter().position(|p| p.id == id) else {
            bail!("找不到網站");
        };
        let mut updated = self.clone();
        updated.pads.remove(index);
        if self.active == Some(id) {
            updated.active = updated
                .pads
                .get(index.min(updated.pads.len().saturating_sub(1)))
                .map(|p| p.id);
        }
        Ok(updated)
    }

    /// 以識別碼重新命名；移除前後空白，保留順序與選取狀態。
    pub fn rename(&self, id: u64, title: &str) -> Result<Self> {
        ensure!(
            !title.chars().any(char::is_control),
            "網站名稱不可含有控制字元"
        );
        let title = title.trim();
        validate_title(title)?;
        let index = self
            .pads
            .iter()
            .position(|pad| pad.id == id)
            .ok_or_else(|| anyhow::anyhow!("找不到網站"))?;
        let mut updated = self.clone();
        updated.pads[index].title = title.to_owned();
        Ok(updated)
    }

    /// 將網站移到零起算的最終位置；識別碼與選取狀態不變。
    pub fn move_pad(&self, id: u64, position: usize) -> Result<Self> {
        ensure!(position < self.pads.len(), "網站排序位置超出範圍");
        let index = self
            .pads
            .iter()
            .position(|pad| pad.id == id)
            .ok_or_else(|| anyhow::anyhow!("找不到網站"))?;
        let mut updated = self.clone();
        let pad = updated.pads.remove(index);
        updated.pads.insert(position, pad);
        Ok(updated)
    }

    /// 復原已發出識別碼的網站並選取它；超出目前長度的位置改放末尾。
    pub fn restore_pad(&self, pad: Pad, position: usize) -> Result<Self> {
        ensure!(self.pads.len() < MAX_PADS, "最多保留 {MAX_PADS} 個網站");
        let mut updated = self.clone();
        updated.active = Some(pad.id);
        updated.pads.insert(position.min(updated.pads.len()), pad);
        updated.validate()?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_use_full_height_with_default_top_margin() {
        let original = Settings::default();
        let mut old = serde_json::to_value(&original).unwrap();
        old.as_object_mut().unwrap().remove("height");
        old.as_object_mut().unwrap().remove("top_offset");
        let loaded: Settings = serde_json::from_value(old).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.height, None);
        assert_eq!(loaded.top_offset, 8.0);
        assert_eq!(loaded.version, 1);
    }

    #[test]
    fn dimensions_accept_boundaries_and_reject_nonfinite_or_out_of_range_values() {
        for height in [320.0, 10_000.0] {
            for top_offset in [0.0, 10_000.0] {
                Settings {
                    height: Some(height),
                    top_offset,
                    ..Settings::default()
                }
                .validate()
                .unwrap();
            }
        }
        for height in [319.0, 10_001.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                Settings {
                    height: Some(height),
                    ..Settings::default()
                }
                .validate()
                .is_err()
            );
        }
        for top_offset in [-1.0, 10_001.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                Settings {
                    top_offset,
                    ..Settings::default()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn old_settings_default_shortcut_and_new_settings_round_trip() {
        let original = Settings::default();
        let mut old = serde_json::to_value(&original).unwrap();
        old.as_object_mut().unwrap().remove("toggle_shortcut");
        let loaded: Settings = serde_json::from_value(old).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded, original);
        let changed = Settings {
            toggle_shortcut: Shortcut {
                option: true,
                command: false,
                key: "F12".to_owned(),
                ..Shortcut::default()
            },
            ..original
        };
        let json = serde_json::to_string(&changed).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored, changed);
        assert_eq!(restored.version, 1);
    }

    #[test]
    fn settings_reject_invalid_and_unknown_shortcut_fields() {
        let mut invalid = Settings::default();
        invalid.toggle_shortcut.key = "Enter".to_owned();
        assert!(invalid.validate().is_err());
        let mut json = serde_json::to_value(Settings::default()).unwrap();
        json["toggle_shortcut"]["unknown"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<Settings>(json).is_err());
    }

    #[test]
    fn renaming_trims_unicode_title_without_changing_identity_or_selection() {
        let original = Settings::default()
            .add("a.com")
            .unwrap()
            .add("b.com")
            .unwrap();
        let renamed = original.rename(1, "  我的網站 🦀　").unwrap();
        assert_eq!(renamed.pads[0].title, "我的網站 🦀");
        assert_eq!(renamed.pads[0].id, original.pads[0].id);
        assert_eq!(renamed.pads[0].url, original.pads[0].url);
        assert_eq!(renamed.active, original.active);
        assert_eq!(renamed.next_id, original.next_id);
        assert_eq!(original.pads[0].title, "a.com");
        assert_eq!(
            original.rename(1, &"字".repeat(80)).unwrap().pads[0]
                .title
                .chars()
                .count(),
            80
        );
    }

    #[test]
    fn invalid_rename_rejects_empty_long_control_titles_and_unknown_ids() {
        let original = Settings::default().add("a.com").unwrap();
        let before = original.clone();
        for title in [
            "".to_owned(),
            " 　".to_owned(),
            "字".repeat(81),
            "a\0b".to_owned(),
            "\na".to_owned(),
            "a\t".to_owned(),
            "a\u{0085}".to_owned(),
        ] {
            assert!(original.rename(1, &title).is_err(), "{title:?}");
        }
        assert!(original.rename(999, "網站").is_err());
        assert_eq!(original, before);
    }

    #[test]
    fn stored_titles_reject_whitespace_controls_and_overlong_unicode() {
        let original = Settings::default().add("a.com").unwrap();
        for title in [
            "".to_owned(),
            " 　".to_owned(),
            "字".repeat(81),
            "a\0b".to_owned(),
            "\na".to_owned(),
            "a\t".to_owned(),
            "a\u{0085}".to_owned(),
        ] {
            let mut invalid = original.clone();
            invalid.pads[0].title = title.clone();
            assert!(invalid.validate().is_err(), "{title:?}");
        }
    }

    #[test]
    fn moving_pads_uses_final_positions_and_preserves_stable_ids() {
        let original = Settings::default()
            .add("a.com")
            .unwrap()
            .add("b.com")
            .unwrap()
            .add("c.com")
            .unwrap();
        let moved = original.move_pad(1, 2).unwrap();
        assert_eq!(
            moved.pads.iter().map(|pad| pad.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
        assert_eq!(moved.active, original.active);
        assert_eq!(moved.next_id, original.next_id);
        assert_eq!(moved.move_pad(1, 0).unwrap(), original);
        assert_eq!(original.move_pad(2, 1).unwrap(), original);
        assert!(original.move_pad(1, 3).is_err());
        assert!(original.move_pad(999, 0).is_err());
        assert_eq!(original.pads[0].id, 1);
    }

    #[test]
    fn restoring_removed_pad_clamps_position_selects_it_and_retains_next_id() {
        let original = Settings::default()
            .add("a.com")
            .unwrap()
            .add("b.com")
            .unwrap();
        let removed = original.pads[0].clone();
        let reduced = original.remove(removed.id).unwrap();
        let restored = reduced.restore_pad(removed.clone(), 99).unwrap();
        assert_eq!(
            restored.pads,
            vec![original.pads[1].clone(), removed.clone()]
        );
        assert_eq!(restored.active, Some(removed.id));
        assert_eq!(restored.next_id, original.next_id);
        assert_eq!(restored.add("c.com").unwrap().pads.last().unwrap().id, 3);
        let only = Settings::default().add("a.com").unwrap();
        assert_eq!(
            only.remove(1)
                .unwrap()
                .restore_pad(only.pads[0].clone(), 0)
                .unwrap(),
            only
        );
    }

    #[test]
    fn restoring_rejects_duplicate_unissued_invalid_and_over_capacity_pads() {
        let original = Settings::default().add("a.com").unwrap();
        let before = original.clone();
        let pad = original.pads[0].clone();
        assert!(original.restore_pad(pad.clone(), 0).is_err());
        let empty = original.remove(1).unwrap();
        for invalid in [
            Pad {
                id: 0,
                ..pad.clone()
            },
            Pad {
                id: original.next_id,
                ..pad.clone()
            },
            Pad {
                title: "\n".to_owned(),
                ..pad.clone()
            },
            Pad {
                url: "file:///tmp/a".to_owned(),
                ..pad.clone()
            },
        ] {
            assert!(empty.restore_pad(invalid, 0).is_err());
        }
        let mut full = original.remove(1).unwrap();
        for _ in 0..MAX_PADS {
            full = full.add("example.com").unwrap();
        }
        assert!(full.restore_pad(pad, 0).is_err());
        assert_eq!(original, before);
    }

    #[test]
    fn blocks_executable_local_and_credential_urls() {
        for input in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
            "mailto:a@b.com",
            "https://a:b@example.com",
            "https://example.com\nattack",
            "ftp://example.com",
        ] {
            assert!(normalize_address(input).is_err(), "{input}");
        }
    }
    #[test]
    fn supports_urls_localhost_and_search_without_losing_query() {
        assert_eq!(
            normalize_address("example.com/path?q=hello").unwrap(),
            "https://example.com/path?q=hello"
        );
        assert_eq!(
            normalize_address("http://localhost:8080").unwrap(),
            "http://localhost:8080/"
        );
        let search = Url::parse(&normalize_address("台灣 Rust & mac").unwrap()).unwrap();
        assert_eq!(search.host_str(), Some("duckduckgo.com"));
        assert_eq!(search.query_pairs().next().unwrap().1, "台灣 Rust & mac");
        assert!(normalize_address("   ").is_err());
    }
    #[test]
    fn deleting_active_pad_selects_neighbor_and_last_removal_returns_home() {
        let settings = Settings::default()
            .add("a.com")
            .unwrap()
            .add("b.com")
            .unwrap();
        let reduced = settings.remove(2).unwrap();
        assert_eq!(reduced.active, Some(1));
        let empty = reduced.remove(1).unwrap();
        assert_eq!(empty.active, None);
        empty.validate().unwrap();
        assert_eq!(settings.pads.len(), 2);
    }
    #[test]
    fn rejects_future_schema_duplicate_ids_and_invalid_active_pad() {
        let mut settings = Settings::default().add("a.com").unwrap();
        settings.version = 2;
        assert!(settings.validate().is_err());
        settings.version = 1;
        settings.active = Some(42);
        assert!(settings.validate().is_err());
        settings.active = Some(1);
        settings.pads.push(settings.pads[0].clone());
        assert!(settings.validate().is_err());
        settings.pads.pop();
        settings.width = f64::NAN;
        assert!(settings.validate().is_err());
    }
}
