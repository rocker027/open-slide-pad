//! 從 Chrome 書籤挑選網站加入側欄：讀檔、把候選推給控制面板、再依索引一次寫入設定。
use super::App;
use anyhow::Result;
use serde_json::json;
use sliderust::{bookmarks, model::MAX_PADS};

impl App {
    /// 每次開啟都重讀檔案。候選清單只留在 Rust；控制面板拿到的是一次性的副本，不進 render 的狀態。
    pub(super) fn show_import(&mut self) -> Result<()> {
        // 真實目錄在這裡才解析：找不到使用者目錄只讓匯入失敗，不影響 App 啟動。
        let root = match &self.chrome_root {
            Some(root) => root.clone(),
            None => bookmarks::chrome_root()?,
        };
        let candidates = bookmarks::collect(&root)?;
        let entries: Vec<_> = candidates
            .iter()
            .map(|bookmark| {
                json!({
                    "title": bookmark.title,
                    "url": bookmark.url,
                    "added": self.settings.pads.iter().any(|pad| pad.url == bookmark.url),
                })
            })
            .collect();
        let payload = json!({
            "entries": entries,
            "remaining": MAX_PADS.saturating_sub(self.settings.pads.len()),
        });
        self.import_candidates = candidates;
        self.focus_chrome(&format!("window.showImport({payload})"), true)
    }

    /// 控制面板只回傳索引：IPC 訊息有 16 KiB 上限，二十個長網址就會超過。
    /// 與 Add 一樣不發成功提示：回到首頁後清單本身就是回饋，也不會蓋掉 commit 的耐久性警告。
    pub(super) fn import(&mut self, indices: &[usize]) -> Result<()> {
        let selected = bookmarks::select(&self.import_candidates, indices)?;
        self.commit(self.settings.import(&selected)?)?;
        self.import_candidates.clear();
        self.home = true;
        self.overlay = false;
        self.chrome.evaluate_script("window.closeOverlay()")?;
        Ok(())
    }
}
