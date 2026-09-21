use super::Event;
use anyhow::Result;
use objc2::rc::Retained;
use objc2_web_kit::WKBackForwardListItem;
use sliderust::model::{Pad, navigation_allowed, web_url};
use tao::{event_loop::EventLoopProxy, window::Window};
use wry::{
    NewWindowResponse, PageLoadEvent, Rect, WebView, WebViewBuilder, WebViewBuilderExtDarwin,
    dpi::{LogicalPosition, LogicalSize},
};

// 控制面板的版面數字；ui/style.css 的 :root 自訂屬性必須是同一組，由 chrome.rs 的測試綁定。
pub const RAIL_WIDTH: f64 = 64.0;
pub const TOOLBAR_HEIGHT: f64 = 48.0;
pub const STATUS_HEIGHT: f64 = 26.0;
/// 網頁左右各留一段，讓無框視窗邊緣的拖曳調整區不被原生網頁視圖蓋住。
pub const PAGE_GUTTER: f64 = 8.0;
/// 控制面板在網頁四周佔掉的寬與高。
pub const CHROME_WIDTH: f64 = RAIL_WIDTH + PAGE_GUTTER * 2.0;
pub const CHROME_HEIGHT: f64 = TOOLBAR_HEIGHT + STATUS_HEIGHT;

pub fn bounds(width: f64, height: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(RAIL_WIDTH + PAGE_GUTTER, TOOLBAR_HEIGHT).into(),
        size: LogicalSize::new(
            (width - CHROME_WIDTH).max(1.0),
            (height - CHROME_HEIGHT).max(1.0),
        )
        .into(),
    }
}

pub fn build(
    window: &Window,
    pad: &Pad,
    proxy: EventLoopProxy<Event>,
    width: f64,
    height: f64,
    ephemeral: bool,
    user_agent: &str,
) -> Result<WebView> {
    let id = pad.id;
    let loaded = proxy.clone();
    let popup = proxy.clone();
    let crashed = proxy.clone();
    Ok(WebViewBuilder::new()
        .with_url(&pad.url)
        .with_bounds(bounds(width, height))
        .with_incognito(ephemeral)
        .with_user_agent(user_agent)
        .with_devtools(false)
        .with_accept_first_mouse(true)
        // 遠端 WebView 不註冊 IPC，也不注入具權限的腳本。
        .with_navigation_handler(|address| navigation_allowed(&address))
        .with_new_window_req_handler(move |address, _| {
            if web_url(&address).is_ok() {
                let _ = popup.send_event(Event::Popup(id, address));
            }
            NewWindowResponse::Deny
        })
        .with_on_web_content_process_terminate_handler(move || {
            let _ = crashed.send_event(Event::Crashed(id));
        })
        .with_document_title_changed_handler(move |title| {
            let _ = proxy.send_event(Event::Title(id, title.chars().take(100).collect()));
        })
        .with_on_page_load_handler(move |event, address| {
            let _ = loaded.send_event(Event::Load(
                id,
                matches!(event, PageLoadEvent::Finished),
                address,
            ));
        })
        .build_as_child(window)?)
}

/// 歷史中離目前最近的一個 http(s) 頁面。
/// 每次失敗的載入都會留下一筆 about:blank，所以「回到原本的頁面」不能只退一步。
fn previous_page(view: &WebView) -> Option<Retained<WKBackForwardListItem>> {
    use wry::WebViewExtMacOS;
    // 已在主執行緒，Wry 保持 WKWebView 的所有權。
    let visited = unsafe { view.webview().backForwardList().backList() }.to_vec();
    visited.into_iter().rev().find(|item| {
        unsafe { item.URL() }
            .absoluteString()
            .is_some_and(|address| web_url(&address.to_string()).is_ok())
    })
}

pub fn has_previous_page(view: &WebView) -> bool {
    previous_page(view).is_some()
}

/// 跳回最近的一個 http(s) 頁面；沒有就不動並回傳 false。
pub fn go_to_previous_page(view: &WebView) -> bool {
    use wry::WebViewExtMacOS;
    let Some(page) = previous_page(view) else {
        return false;
    };
    unsafe { view.webview().goToBackForwardListItem(&page) };
    true
}

/// 首次導覽尚未開始時原生 URL 可能為空，不能使用內部 unwrap 的便利方法。
pub fn current_url(view: &WebView) -> Option<String> {
    use wry::WebViewExtMacOS;
    unsafe { view.webview().URL() }
        .and_then(|url| url.absoluteString())
        .map(|address| address.to_string())
}
