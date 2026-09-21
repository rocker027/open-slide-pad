use super::Event;
use anyhow::Result;
use sliderust::model::{Pad, navigation_allowed, web_url};
use tao::{event_loop::EventLoopProxy, window::Window};
use wry::{
    NewWindowResponse, PageLoadEvent, Rect, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

pub fn bounds(width: f64, height: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(72.0, 70.0).into(),
        size: LogicalSize::new((width - 80.0).max(1.0), (height - 106.0).max(1.0)).into(),
    }
}

pub fn build(
    window: &Window,
    pad: &Pad,
    proxy: EventLoopProxy<Event>,
    width: f64,
    height: f64,
    ephemeral: bool,
) -> Result<WebView> {
    let id = pad.id;
    let loaded = proxy.clone();
    let popup = proxy.clone();
    Ok(WebViewBuilder::new()
        .with_url(&pad.url)
        .with_bounds(bounds(width, height))
        .with_incognito(ephemeral)
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

/// 首次導覽尚未開始時原生 URL 可能為空，不能使用內部 unwrap 的便利方法。
pub fn current_url(view: &WebView) -> Option<String> {
    use wry::WebViewExtMacOS;
    unsafe { view.webview().URL() }
        .and_then(|url| url.absoluteString())
        .map(|address| address.to_string())
}
