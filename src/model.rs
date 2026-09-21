use crate::{i18n::Language, shortcut::Shortcut};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
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
    // Url::parse 會默默剝掉頭尾空白，通過驗證的字串就不再等於實際交給 WebKit 的字串。
    ensure!(
        input.len() <= 8192 && input == input.trim() && !input.chars().any(char::is_control),
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

/// 遠端 WebView 的導覽放行規則。wry 不提供框架資訊，主框架與 iframe 共用同一條規則。
/// 網站常用空白頁、srcdoc 與自身來源的 blob 文件當 iframe；這些內容都屬於原網站來源。
/// data: 維持拒絕：網址列只顯示通過 web_url 的位址，放行會讓內容掛在前一個網址底下。
pub fn navigation_allowed(address: &str) -> bool {
    // 前置條件對三種形態一視同仁：WebKit 送來的位址已百分比編碼，不會含空白或控制字元。
    let is_malformed = |letter: char| letter.is_whitespace() || letter.is_control();
    if address.len() > 8192 || address.chars().any(is_malformed) {
        return false;
    }
    // HTML 規範：about:blank 可帶任意 query 與 fragment；about:srcdoc 的 query 必須為 null。
    let document = address.split('#').next().unwrap_or(address);
    let is_blank = document.split('?').next() == Some("about:blank");
    is_blank
        || document == "about:srcdoc"
        || web_url(address.strip_prefix("blob:").unwrap_or(address)).is_ok()
}

pub fn normalize_address(input: &str) -> Result<String> {
    ensure!(!input.chars().any(char::is_control), "輸入不可含有控制字元");
    let input = input.trim();
    ensure!(
        !input.is_empty() && input.len() <= 8192,
        "請輸入網址或搜尋內容"
    );
    let address = match url_candidate(input) {
        Some(candidate) => web_url(&candidate)?.to_string(),
        None => {
            let mut search = Url::parse("https://duckduckgo.com/")?;
            search.query_pairs_mut().append_pair("q", input);
            search.to_string()
        }
    };
    // 百分比編碼會讓輸出比輸入長。以實際要載入的字串再驗一次，保證回傳值一定通過 web_url。
    web_url(&address)?;
    Ok(address)
}

/// 判斷輸入是不是網址。是的話回傳要交給 web_url 把關的字串，None 代表當成搜尋。
fn url_candidate(input: &str) -> Option<String> {
    // 明確以「scheme://」開頭就是網址，打錯（例如埠號不合法）要回報錯誤，不改當搜尋。
    if has_scheme_prefix(input) {
        return Some(input.to_owned());
    }
    // 其餘含空白的輸入一律當搜尋：「site:example.com 關鍵字」會被解析成自訂 scheme，但不是網址。
    if input.contains(char::is_whitespace) {
        return None;
    }
    let bare_host = host_with_port(input).is_some();
    // 無空白的其他 scheme（javascript:、mailto:、foo:bar）交給 web_url 拒絕，不改當搜尋。
    if !bare_host && Url::parse(input).is_ok() {
        return Some(input.to_owned());
    }
    let host = host_without_port(input);
    (bare_host || host.contains('.') || host.eq_ignore_ascii_case("localhost"))
        .then(|| format!("{}://{input}", default_scheme(host)))
}

/// 「查詢裡提到 https://…」或網址的查詢字串含「://」都不算；只看開頭是不是合法的 scheme。
fn has_scheme_prefix(input: &str) -> bool {
    input.split_once("://").is_some_and(|(scheme, _)| {
        scheme.starts_with(|first: char| first.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '+' | '.' | '-'))
    })
}

fn authority(input: &str) -> &str {
    input.split(['/', '?', '#']).next().unwrap_or(input)
}

fn host_without_port(input: &str) -> &str {
    let authority = authority(input);
    authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host)
}

/// 辨識沒有 scheme 的「主機:埠」；WHATWG 解析會把主機當成自訂 scheme，須先攔下。
/// 主機須含「.」或為 localhost，避免把「詞:數字」誤判成網址。
fn host_with_port(input: &str) -> Option<&str> {
    let (host, port) = authority(input).rsplit_once(':')?;
    let is_port = (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit());
    let is_host = host
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        && (host.contains('.') || host.eq_ignore_ascii_case("localhost"));
    (is_port && is_host).then_some(host)
}

/// 本機開發伺服器通常沒有憑證，其餘主機預設走 https。
fn default_scheme(host: &str) -> &'static str {
    if host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
        "http"
    } else {
        "https"
    }
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

// 頂層刻意不用 deny_unknown_fields：加欄位不升 version，version 留給破壞性變更。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// 本版不認得的頂層欄位。原樣保留並寫回，舊版開過新版設定後不會把新欄位洗掉。
    #[serde(flatten)]
    pub unknown_fields: BTreeMap<String, serde_json::Value>,
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
            unknown_fields: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// 給控制面板用的副本。不認得的欄位只負責原樣寫回磁碟，不送進 UI：
    /// 它們是不透明資料，鍵名（例如 `__proto__`）不該有機會影響畫面端的物件。
    pub fn without_unknown_fields(&self) -> Self {
        Self {
            unknown_fields: BTreeMap::new(),
            ..self.clone()
        }
    }

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
    fn unknown_top_level_fields_are_kept_while_nested_unknown_fields_are_rejected() {
        let mut json = serde_json::to_value(Settings::default().add("a.com").unwrap()).unwrap();
        json["future_feature"] = serde_json::json!({"enabled": true, "items": [1, 2]});
        let loaded: Settings = serde_json::from_value(json.clone()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(serde_json::to_value(&loaded).unwrap(), json);
        // 經過一般操作後，新版寫入的欄位仍原樣留在設定裡。
        let changed = loaded.add("b.com").unwrap().remove(1).unwrap();
        assert_eq!(
            serde_json::to_value(&changed).unwrap()["future_feature"],
            json["future_feature"]
        );
        let mut nested = json.clone();
        nested["pads"][0]["unknown"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<Settings>(nested).is_err());
        // 送進控制面板的副本不含未知欄位，其餘內容不變。
        let mut hostile = json.clone();
        hostile["__proto__"] = serde_json::json!({"polluted": true});
        let displayed = serde_json::from_value::<Settings>(hostile)
            .unwrap()
            .without_unknown_fields();
        let mut expected = json.clone();
        expected.as_object_mut().unwrap().remove("future_feature");
        assert_eq!(serde_json::to_value(&displayed).unwrap(), expected);
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
    fn embedded_frames_may_use_blank_srcdoc_and_web_origin_blob_documents() {
        for address in [
            "https://example.com/",
            "http://example.com/path?q=1",
            "about:blank",
            "about:srcdoc",
            // HTML 規範：about:blank 可帶任意 query 與 fragment；about:srcdoc 的 query 必須為 null。
            "about:blank#fragment",
            "about:blank?query",
            "about:blank?query#fragment",
            "about:srcdoc#fragment",
            // fragment 裡的「?」不是 query。
            "about:srcdoc#a?b",
            "blob:https://example.com/0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0",
            "blob:http://localhost:3000/0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0",
            // 解析後仍是一般的 http(s) 網址：scheme 大小寫、空的 userinfo。
            "HTTPS://EXAMPLE.COM/",
            "blob:HTTPS://example.com/x",
            "https://@example.com/",
        ] {
            assert!(navigation_allowed(address), "{address}");
        }
        // 長度上限對三種形態一視同仁，以整個位址計算。
        for longest in [
            format!("https://example.com/{}", "a".repeat(8192 - 20)),
            format!("blob:https://example.com/{}", "a".repeat(8192 - 25)),
            format!("about:blank#{}", "a".repeat(8192 - 12)),
        ] {
            assert_eq!(longest.len(), 8192);
            assert!(navigation_allowed(&longest), "{}", &longest[..24]);
            assert!(
                !navigation_allowed(&format!("{longest}a")),
                "{}",
                &longest[..24]
            );
        }
    }

    #[test]
    fn navigation_preconditions_apply_to_every_address_form() {
        // WebKit 送來的位址一律已百分比編碼，不會含空白或控制字元；每種形態都要擋。
        for tail in [
            " ", "a b", "\u{a0}", "\u{3000}", "\u{2028}", "\u{0}", "\u{7f}", "\u{85}", "\n",
        ] {
            for address in [
                format!("https://example.com/{tail}"),
                format!("https://example.com/?q={tail}"),
                format!("blob:https://example.com/x{tail}"),
                format!("about:blank#{tail}"),
                format!("about:blank?{tail}"),
                format!("about:srcdoc#{tail}"),
            ] {
                assert!(!navigation_allowed(&address), "{address:?}");
            }
        }
        // 比對的是整份文件位址，不是「包含」或「開頭是」。
        for address in [
            "about:srcdoc?query",
            "about:srcdoc?query#fragment",
            // 空的 query 不等於沒有 query。
            "about:srcdoc?",
            "about:config#about:blank",
            "javascript:alert(1)#about:blank",
            "data:text/html,x#about:blank",
            "file:///etc/passwd?about:blank",
            "blob:about:blank#x",
            "About:Blank#x",
            "about:blank%23x",
            "about:#blank",
        ] {
            assert!(!navigation_allowed(address), "{address}");
        }
    }

    #[test]
    fn navigation_still_blocks_local_script_data_and_external_app_schemes() {
        for address in [
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "slack://open",
            "x-apple.systempreferences:com.apple.preference.security",
            "about:config",
            "about:blankx",
            "about:blank/",
            "about:",
            "ABOUT:BLANK",
            " about:blank",
            "about:blank#\n",
            "about:srcdoc\n",
            "blob:",
            "blob:null/0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0",
            "blob:file:///etc/passwd",
            "blob:javascript:alert(1)",
            "blob:data:text/html,hi",
            "blob:blob:https://example.com/0f1e2d3c",
            "blob:https://user:pass@example.com/0f1e2d3c",
            "blob:https://example.com/\u{0}",
            "https://user:pass@example.com/",
            "https://user@example.com/",
            "https://:pass@example.com/",
            // Url::parse 會默默剝掉頭尾空白，必須在解析前擋下。
            " https://example.com/",
            "https://example.com/ ",
            "   https://example.com/   ",
            "blob: https://example.com/x",
            "blob:https://example.com/x ",
            " blob:https://example.com/x",
            " javascript:alert(1)",
            "About:Blank",
            "ABOUT:SRCDOC",
            "BLOB:https://example.com/x",
            "\u{a0}https://example.com/",
            "\u{feff}https://example.com/",
            "\u{ff48}\u{ff54}\u{ff54}\u{ff50}\u{ff53}://example.com/",
            "%68ttps://example.com/",
            "about%3Ablank",
            "blob\u{200b}:https://example.com/x",
            "blob:about:blank",
            "blob:ws://example.com/x",
            "filesystem:https://example.com/temporary/x",
            "view-source:https://example.com/",
            "ws://example.com/",
            "https://example.com/\u{7f}",
            "\u{85}https://example.com/",
        ] {
            assert!(!navigation_allowed(address), "{address}");
        }
    }

    #[test]
    fn bare_host_with_port_opens_as_url_and_loopback_defaults_to_http() {
        for (input, expected) in [
            ("localhost:3000", "http://localhost:3000/"),
            ("LocalHost:3000/api?x=1", "http://localhost:3000/api?x=1"),
            ("localhost", "http://localhost/"),
            ("127.0.0.1:8080", "http://127.0.0.1:8080/"),
            ("example.com:8080", "https://example.com:8080/"),
            (
                "example.com:8080/a?b=1#c",
                "https://example.com:8080/a?b=1#c",
            ),
            ("192.168.1.10:8443", "https://192.168.1.10:8443/"),
            ("localhost/api", "http://localhost/api"),
            ("localhost?x=1", "http://localhost/?x=1"),
            ("example.com:0", "https://example.com:0/"),
            ("example.com:65535", "https://example.com:65535/"),
            ("example.com:00080", "https://example.com:80/"),
            // 帶點的自訂 scheme 加數字也只能變成 https 主機，不可當成外部 App scheme。
            (
                "x-apple.systempreferences:1",
                "https://x-apple.systempreferences:1/",
            ),
            ("https:example.com", "https://example.com/"),
            // 空的 userinfo 解析後消失；反斜線之後屬於路徑，主機仍是第一個網域。
            ("@example.com", "https://example.com/"),
            ("example.com\\@evil.com", "https://example.com/@evil.com"),
            // 網址本身的查詢字串含「://」時仍是網址。
            (
                "example.com/r?u=https://x.com",
                "https://example.com/r?u=https://x.com",
            ),
        ] {
            assert_eq!(normalize_address(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn queries_that_start_like_a_scheme_are_searched_when_they_contain_spaces() {
        for input in [
            "site:github.com wry",
            "TypeError: x is not a function",
            "error: cannot find crate",
            "C++: templates",
            "javascript:alert(1) //example.com",
            // 查詢裡提到網址，或主機部分根本不像網域，都不是網址。
            "how to parse https://example.com",
            "foo bar://baz",
            "/etc/passwd.txt",
            "foo/bar.baz",
            // 含「://」但開頭不是合法 scheme：不是網址，也不會被當成某個 scheme 執行。
            "%68ttps://example.com",
            "https%3A//example.com",
            "\u{ff48}\u{ff54}\u{ff54}\u{ff50}\u{ff53}://example.com",
            "\u{feff}https://example.com",
            "1http://example.com",
            "://",
        ] {
            let search = Url::parse(&normalize_address(input).unwrap()).unwrap();
            assert_eq!(search.scheme(), "https", "{input}");
            assert_eq!(search.host_str(), Some("duckduckgo.com"), "{input}");
            assert_eq!(search.query_pairs().next().unwrap().1, input);
        }
    }

    #[test]
    fn scheme_like_input_without_spaces_and_malformed_ports_stay_rejected() {
        for input in [
            "foo:bar",
            "nas:5000",
            "javascript:alert(1)",
            "data:text/html,hi",
            "mailto:a@b.com",
            "file:/etc/passwd",
            "user:pass@example.com",
            "evil.com:80@example.com",
            "example.com:80:80",
            "example.com:99999",
            "example.com:",
            "example.com:80x",
            "example.com:-1",
            "example.com:65536",
            "example.com:\u{ff10}\u{ff18}\u{ff10}",
            "https://example.com:99999/",
            "https://example.com:80:80/",
            "https://example.com:8a/",
            "user@example.com",
            "example.com@evil.com",
            "localhost:3000@evil.com",
            "a:b@example.com:80",
            "example.com:80\\@evil.com",
            "javascript:alert(1)\u{200b}",
            "\u{a0}javascript:alert(1)\u{a0}",
            "C:\\Windows\\win.ini",
            "javascript://example.com/ x",
        ] {
            assert!(normalize_address(input).is_err(), "{input}");
        }
    }

    #[test]
    fn every_accepted_address_is_a_plain_web_url() {
        for input in [
            "localhost:3000",
            "example.com:8080/a",
            "site:github.com wry",
            "javascript:alert(1) //example.com",
            "例子.台灣:8080",
            "notion.so",
            "台灣 Rust",
        ] {
            let address = normalize_address(input).unwrap();
            assert!(web_url(&address).is_ok(), "{input} → {address}");
        }
        // 百分比編碼會讓輸出比輸入長；超過上限的結果必須被拒絕，不能回傳一個過不了 web_url 的字串。
        let at_limit = format!("https://example.com/{}", "台".repeat(908));
        assert_eq!(normalize_address(&at_limit).unwrap().len(), 8192);
        for input in [
            format!("https://example.com/{}", "台".repeat(909)),
            format!("https://example.com/?q={}", "台".repeat(1000)),
            format!("https:example.com/{}", "台".repeat(1000)),
            format!("example.com/{}", "台".repeat(1000)),
            format!("localhost:3000/{}", "台".repeat(1000)),
            "台".repeat(1000),
            "&".repeat(2800),
            format!("a {}", "台".repeat(1000)),
        ] {
            assert!(normalize_address(&input).is_err(), "{} bytes", input.len());
        }
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
