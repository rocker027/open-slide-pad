//! 一個已建立原生網頁視圖的網站分頁，以及它的載入狀態。
use anyhow::Result;
use sliderust::load::LoadWatch;
use wry::{WebView, WebViewExtMacOS};

/// 進度條至少前進這麼多才重畫，避免每次輪詢都送一次畫面更新。
const PROGRESS_STEP: f64 = 0.02;

pub(super) struct BrowserPad {
    pub(super) view: WebView,
    pub(super) title: String,
    pub(super) address: String,
    /// App 要求載入、尚未顯示的網址；失敗時「重試」與網址列都用它，不用 WebKit 退回的舊網址。
    pub(super) pending: Option<String>,
    pub(super) load: LoadWatch,
    pub(super) progress: f64,
    pub(super) history: (bool, bool),
}

impl BrowserPad {
    /// 讀原生載入狀態；回傳 true 表示畫面需要更新。
    pub(super) fn refresh_load(&mut self) -> bool {
        // 已在主執行緒，Wry 保持 WKWebView 的所有權。
        let (native_loading, estimated) = unsafe {
            let webview = self.view.webview();
            (webview.isLoading(), webview.estimatedProgress())
        };
        let changed = self.load.observe(native_loading);
        let progress = if self.load.is_loading() {
            estimated
        } else {
            0.0
        };
        let moved = (progress - self.progress).abs() >= PROGRESS_STEP;
        if changed || moved {
            self.progress = progress;
        }
        changed || moved
    }

    /// 重新載入；失敗畫面上則重試當初要求的網址（失敗的載入沒有可 reload 的頁面）。
    pub(super) fn reload(&mut self) -> Result<()> {
        if self.load.failure().is_none() {
            unsafe { self.view.webview().reload() };
            return Ok(());
        }
        let target = self.pending.clone().unwrap_or_else(|| self.address.clone());
        self.view.load_url(&target)?;
        self.load.request();
        Ok(())
    }
}
