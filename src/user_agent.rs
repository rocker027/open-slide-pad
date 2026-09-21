//! 遠端 WebView 送出的 User-Agent。
//!
//! WKWebView 預設的 UA 沒有 `Version/… Safari/…`，會嗅探 UA 的網站因此把它當成不支援的瀏覽器。
//! 引擎本來就是 Safari 使用的系統 WebKit，補上這兩段才與實際能力一致。

/// 讀不到本機 Safari 版本時使用的版本。
const FALLBACK_SAFARI_VERSION: &str = "18.5";
const MAX_VERSION_LENGTH: usize = 16;

/// 組出與 Safari 相同格式的 UA。平台與 WebKit 版號是 Safari 自己也凍結的固定值，只有 Safari 版本會變。
///
/// `safari_version` 來自本機 Safari 的 `CFBundleShortVersionString`；
/// 缺少，或內容不是以點分隔的數字（例如 `18`、`18.5`、`26.0.1`）時改用內建版本，不把未知內容送進 HTTP 標頭。
pub fn safari_user_agent(safari_version: Option<&str>) -> String {
    let version = safari_version
        .filter(|version| is_dotted_number(version))
        .unwrap_or(FALLBACK_SAFARI_VERSION);
    format!(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15"
    )
}

fn is_dotted_number(version: &str) -> bool {
    version.len() <= MAX_VERSION_LENGTH
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_carries_the_local_safari_version_and_the_tokens_sites_sniff_for() {
        let agent = safari_user_agent(Some("26.0.1"));
        assert!(agent.starts_with("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/"));
        assert!(agent.ends_with(" Version/26.0.1 Safari/605.1.15"));
        // 沒有小數點的版本同樣只含數字，照樣採用。
        assert!(safari_user_agent(Some("26")).contains(" Version/26 "));
    }

    #[test]
    fn unusable_versions_fall_back_instead_of_reaching_the_header() {
        let fallback = safari_user_agent(None);
        assert!(fallback.contains(&format!("Version/{FALLBACK_SAFARI_VERSION} ")));
        for version in [
            "",
            ".",
            "18.",
            ".5",
            "18..5",
            "18.5 beta",
            "18.5\r\nX-Injected: 1",
            "１８.５",
            "12345678901234567",
        ] {
            assert_eq!(safari_user_agent(Some(version)), fallback, "{version:?}");
        }
    }
}
