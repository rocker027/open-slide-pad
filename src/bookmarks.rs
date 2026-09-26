//! 讀取 Chrome 書籤檔並過濾成可加入側欄的網站。
//! 檔案由另一個 App 寫出，內容一律視為不可信：只認通過 `web_url` 的網址，其餘節點跳過。
use crate::model::web_url;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

/// 重度使用者的書籤檔可達數 MiB；設定檔的 256 KiB 上限不適用。
pub const MAX_BOOKMARK_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// 每個 profile 目錄下的兩個書籤檔：本機／可同步書籤與帳號書籤，格式相同。
const BOOKMARK_FILE_NAMES: [&str; 2] = ["Bookmarks", "AccountBookmarks"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

/// Chrome 使用者資料根目錄；各 profile 是它底下的子目錄。
pub fn chrome_root() -> Result<PathBuf> {
    let base = directories::BaseDirs::new().context("找不到使用者資料目錄")?;
    Ok(base.data_dir().join("Google").join("Chrome"))
}

/// 讀取根目錄下每個 profile 的書籤檔，依網址去重並保留首次出現的順序。根目錄不存在視為沒有書籤。
pub fn collect(root: &Path) -> Result<Vec<Bookmark>> {
    let mut seen = HashSet::new();
    let mut bookmarks = Vec::new();
    for path in profile_files(root)? {
        // 錯誤只帶檔案路徑：書籤名稱與網址不進日誌與提示。
        let context = || format!("無法讀取 Chrome 書籤：{}", path.display());
        let bytes = read_capped(&path).with_context(context)?;
        for bookmark in parse(&bytes).with_context(context)? {
            if seen.insert(bookmark.url.clone()) {
                bookmarks.push(bookmark);
            }
        }
    }
    Ok(bookmarks)
}

/// 依目錄名排序，讓 Default 先於 Profile N；根目錄本身的檔案不算 profile。
fn profile_files(root: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error).context("無法列出 Chrome 使用者資料目錄"),
    };
    let mut profiles: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    profiles.sort();
    Ok(profiles
        .iter()
        .flat_map(|profile| BOOKMARK_FILE_NAMES.map(|name| profile.join(name)))
        .filter(|path| path.is_file())
        .collect())
}

fn read_capped(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_BOOKMARK_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BOOKMARK_FILE_BYTES,
        "檔案超過 {} MiB",
        MAX_BOOKMARK_FILE_BYTES / 1024 / 1024
    );
    Ok(bytes)
}

/// 解析單一書籤檔的內容。只回傳 `type == "url"` 且通過 `web_url` 的節點。
pub fn parse(bytes: &[u8]) -> Result<Vec<Bookmark>> {
    // serde_json 的遞迴上限（128 層）同時限制資料夾深度；超過就整份拒絕。
    let document: Value = serde_json::from_slice(bytes).context("書籤檔不是有效的 JSON")?;
    let mut pending: Vec<&Value> = document
        .get("roots")
        .and_then(Value::as_object)
        .map(|roots| roots.values().rev().collect())
        .unwrap_or_default();
    let mut bookmarks = Vec::new();
    // 走訪用自己的堆疊而不用遞迴；深度已由解析階段限制，這裡只是不讓走訪本身吃呼叫堆疊。
    while let Some(node) = pending.pop() {
        match node.get("type").and_then(Value::as_str) {
            Some("url") => bookmarks.extend(bookmark_from(node)),
            Some("folder") => {
                if let Some(children) = node.get("children").and_then(Value::as_array) {
                    pending.extend(children.iter().rev());
                }
            }
            _ => {}
        }
    }
    Ok(bookmarks)
}

fn bookmark_from(node: &Value) -> Option<Bookmark> {
    let parsed = web_url(node.get("url")?.as_str()?).ok()?;
    let url = parsed.to_string();
    // 百分比編碼會讓輸出比輸入長；以實際要保存的字串再驗一次，保證候選一定通過 Settings::validate。
    web_url(&url).ok()?;
    let name: String = node
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|letter| !letter.is_control())
        .collect();
    let mut title: String = name.trim().chars().take(80).collect();
    if title.is_empty() {
        title = parsed.host_str().unwrap_or("網站").to_owned();
    }
    Some(Bookmark { title, url })
}

/// 控制面板送回的是候選索引；越界代表清單已過期，整批拒絕。重複索引去重並依候選順序排列。
pub fn select(candidates: &[Bookmark], indices: &[usize]) -> Result<Vec<Bookmark>> {
    let unique: BTreeSet<usize> = indices.iter().copied().collect();
    unique
        .iter()
        .map(|&index| {
            candidates
                .get(index)
                .cloned()
                .context("候選清單已過期，請重新開啟匯入畫面")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn url(name: &str, url: &str) -> Value {
        json!({"type": "url", "name": name, "url": url, "id": "1", "guid": "g"})
    }
    fn folder(name: &str, children: Vec<Value>) -> Value {
        json!({"type": "folder", "name": name, "children": children, "id": "2"})
    }
    fn file(bar: Vec<Value>, other: Vec<Value>) -> Vec<u8> {
        json!({
            "checksum": "x",
            "version": 1,
            "roots": {
                "bookmark_bar": folder("書籤列", bar),
                "other": folder("其他書籤", other),
                "synced": folder("行動裝置書籤", vec![]),
            }
        })
        .to_string()
        .into_bytes()
    }
    fn urls(bookmarks: &[Bookmark]) -> Vec<&str> {
        bookmarks.iter().map(|b| b.url.as_str()).collect()
    }

    /// 攻擊面矩陣：每一列是一種把非網頁內容塞進 WebKit 的嘗試。
    #[test]
    fn keeps_only_web_urls_and_drops_dangerous_or_credentialed_ones() {
        let bookmarks = parse(&file(
            vec![
                url("ok https", "https://example.com/a"),
                url("ok http", "http://example.com/b"),
                url("script", "javascript:alert(1)"),
                url("local file", "file:///etc/passwd"),
                url("chrome page", "chrome://settings"),
                url("data document", "data:text/html,<script>alert(1)</script>"),
                url("mail", "mailto:someone@example.com"),
                url("ftp", "ftp://example.com/"),
                url("credentials", "https://user:secret@example.com/"),
                url("no host", "https://"),
                url("untrimmed", " https://example.com/c"),
                url("control", "https://example.com/\u{7}"),
                url("blob", "blob:https://example.com/uuid"),
            ],
            vec![url("nested ok", "https://example.org/")],
        ))
        .unwrap();
        assert_eq!(
            urls(&bookmarks),
            [
                "https://example.com/a",
                "http://example.com/b",
                "https://example.org/"
            ]
        );
    }

    #[test]
    fn titles_are_trimmed_stripped_capped_and_fall_back_to_the_host() {
        let long = "字".repeat(100);
        let bookmarks = parse(&file(
            vec![
                url("  Hi\u{0}there\t ", "https://example.com/1"),
                url(&long, "https://example.com/2"),
                url("", "https://www.example.com/3"),
                url("\u{1}\u{2}", "https://example.net/4"),
            ],
            vec![],
        ))
        .unwrap();
        let titles: Vec<_> = bookmarks.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Hithere", &long[..240], "www.example.com", "example.net"]
        );
        assert_eq!(bookmarks[1].title.chars().count(), 80);
    }

    #[test]
    fn tolerates_unexpected_shapes_without_panicking() {
        let cases: Vec<(Value, usize)> = vec![
            (json!({"roots": []}), 0),
            (json!({"roots": {"bookmark_bar": "text"}}), 0),
            (
                json!({"roots": {"bookmark_bar": {"type": "folder", "children": {}}}}),
                0,
            ),
            (json!({"roots": {"bookmark_bar": {"type": "folder"}}}), 0),
            (json!({"roots": {"bookmark_bar": {"type": "url"}}}), 0),
            (
                json!({"roots": {"bookmark_bar": {"type": "url", "url": 42}}}),
                0,
            ),
            (
                json!({"roots": {"bookmark_bar": {"url": "https://example.com/"}}}),
                0,
            ),
            (
                json!({"roots": {"bookmark_bar": {"type": "url", "name": 7, "url": "https://example.com/"}}}),
                1,
            ),
            (
                json!({"roots": {"bookmark_bar": {"type": "folder", "children": [1, null, "x", {"type": "url", "url": "https://example.com/"}]}}}),
                1,
            ),
            (json!({"no_roots": true}), 0),
            (json!([]), 0),
            (json!(null), 0),
        ];
        for (document, expected) in cases {
            let bookmarks = parse(document.to_string().as_bytes()).unwrap();
            assert_eq!(bookmarks.len(), expected, "{document}");
        }
    }

    #[test]
    fn rejects_malformed_json_oversized_files_and_deep_nesting() {
        assert!(parse(b"{").is_err());
        assert!(parse(b"").is_err());
        let deep = format!(
            "{{\"roots\":{{\"bookmark_bar\":{}{}}}}}",
            "{\"type\":\"folder\",\"children\":[".repeat(200),
            "]}".repeat(200)
        );
        assert!(parse(deep.as_bytes()).is_err());
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("Default");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("Bookmarks"),
            vec![b' '; MAX_BOOKMARK_FILE_BYTES as usize + 1],
        )
        .unwrap();
        let error = collect(dir.path()).unwrap_err().to_string();
        assert!(error.contains("Default"), "{error}");
    }

    #[test]
    fn collects_every_profile_file_in_order_and_dedupes_by_url() {
        let dir = tempfile::tempdir().unwrap();
        let write = |profile: &str, name: &str, bytes: Vec<u8>| {
            let folder = dir.path().join(profile);
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join(name), bytes).unwrap();
        };
        write(
            "Default",
            "Bookmarks",
            file(
                vec![url("a", "https://a.com"), url("b", "https://b.com/")],
                vec![],
            ),
        );
        write(
            "Default",
            "AccountBookmarks",
            file(
                vec![url("b again", "https://b.com/"), url("c", "https://c.com/")],
                vec![],
            ),
        );
        write(
            "Profile 1",
            "Bookmarks",
            file(
                vec![url("a slash", "https://a.com/"), url("d", "https://d.com/")],
                vec![],
            ),
        );
        write("Profile 2", "Preferences", b"{}".to_vec());
        fs::write(
            dir.path().join("Bookmarks"),
            file(vec![url("root", "https://root.com/")], vec![]),
        )
        .unwrap();
        let bookmarks = collect(dir.path()).unwrap();
        assert_eq!(
            urls(&bookmarks),
            [
                "https://a.com/",
                "https://b.com/",
                "https://c.com/",
                "https://d.com/"
            ]
        );
        assert_eq!(bookmarks[0].title, "a");
        assert!(collect(&dir.path().join("missing")).unwrap().is_empty());
    }

    #[test]
    fn errors_name_the_file_but_never_leak_bookmark_content() {
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("Default");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("Bookmarks"),
            b"{\"roots\": {\"bookmark_bar\": \"https://secret.example/\"",
        )
        .unwrap();
        let error = format!("{:#}", collect(dir.path()).unwrap_err());
        assert!(error.contains("Bookmarks"), "{error}");
        assert!(!error.contains("secret"), "{error}");
    }

    /// 中段空白與非 ASCII 會被百分比編碼；輸入剛好 8192 位元組時，保存的字串會超過上限。
    #[test]
    fn candidates_are_validated_after_percent_encoding_so_import_never_rejects_them() {
        let prefix = "https://example.com/?q=";
        let padded = |total: usize, filler: &str| {
            let mut address = prefix.to_owned();
            while address.len() + filler.len() <= total {
                address.push_str(filler);
            }
            address.push('a');
            address
        };
        let spaces = padded(8192, "a ");
        let cjk = padded(8114, "字");
        let short = "https://example.com/?q=a b";
        assert_eq!(spaces.len(), 8192);
        assert!(cjk.len() <= 8192, "CJK 案例須在輸入上限內");
        // 兩個案例都通過第一次 web_url，只會被序列化後的第二次檢查擋下。
        assert!(web_url(&spaces).is_ok() && web_url(&cjk).is_ok());
        let bookmarks = parse(&file(
            vec![
                url("spaces", &spaces),
                url("cjk", &cjk),
                url("short", short),
            ],
            vec![],
        ))
        .unwrap();
        assert_eq!(urls(&bookmarks), ["https://example.com/?q=a%20b"]);
        for bookmark in &bookmarks {
            web_url(&bookmark.url).unwrap();
        }
    }

    #[test]
    fn select_dedupes_and_orders_indices_and_rejects_out_of_range_ones() {
        let candidates = vec![
            Bookmark {
                title: "a".into(),
                url: "https://a.com/".into(),
            },
            Bookmark {
                title: "b".into(),
                url: "https://b.com/".into(),
            },
        ];
        let picked = select(&candidates, &[1, 0, 1]).unwrap();
        assert_eq!(urls(&picked), ["https://a.com/", "https://b.com/"]);
        assert!(select(&candidates, &[]).unwrap().is_empty());
        assert!(
            select(&candidates, &[0, 2]).is_err(),
            "越界代表清單已過期，整批拒絕"
        );
        assert!(select(&[], &[0]).is_err());
    }
}
