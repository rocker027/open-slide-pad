use super::Event;
use anyhow::Result;
use serde::Deserialize;
use tao::{event_loop::EventLoopProxy, window::Window};
use wry::{NewWindowResponse, WebView, WebViewBuilder};

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Ready,
    BeginResize,
    FullHeight,
    SetLanguage {
        language: sliderust::i18n::Language,
    },
    SetShortcut {
        shortcut: sliderust::shortcut::Shortcut,
    },
    Add {
        address: String,
    },
    Select {
        id: u64,
    },
    Remove {
        id: u64,
    },
    Rename {
        id: u64,
        title: String,
    },
    Move {
        id: u64,
        position: usize,
    },
    UndoRemove,
    ShowImport,
    Import {
        indices: Vec<usize>,
    },
    FocusAddress,
    NewPad,
    ShowSettings,
    SelectIndex {
        index: usize,
    },
    Navigate {
        address: String,
    },
    Back,
    Forward,
    Reload,
    Stop,
    DismissFailure,
    Home,
    Hide,
    Pin,
    Overlay {
        open: bool,
    },
    Side,
    Width {
        width: f64,
    },
    HotEdge,
    External,
    Quit,
}

pub fn build(window: &Window, proxy: EventLoopProxy<Event>) -> Result<WebView> {
    Ok(WebViewBuilder::new()
        .with_html(document())
        .with_devtools(false)
        .with_accept_first_mouse(true)
        .with_navigation_handler(|url| url == "about:blank")
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if *request.uri() != "about:blank" || request.body().len() > 16384 {
                return;
            }
            if let Ok(command) = serde_json::from_str::<Command>(request.body()) {
                let _ = proxy.send_event(Event::Command(command));
            }
        })
        .build_as_child(window)?)
}

pub fn toast(view: &WebView, message: &str) {
    if let Ok(encoded) = serde_json::to_string(message)
        && let Err(error) = view.evaluate_script(&format!("window.showToast({encoded})"))
    {
        eprintln!("提示訊息無法顯示：{error}");
    }
}

fn document() -> String {
    include_str!("../../ui/index.html")
        .replace("/* SLIDERUST_STYLE */", include_str!("../../ui/style.css"))
        .replace(
            "// SLIDERUST_I18N",
            &include_str!("../../ui/i18n.js")
                .replace("/* SLIDERUST_ENGLISH */", sliderust::i18n::ENGLISH_CATALOG),
        )
        .replace("// SLIDERUST_SCRIPT", include_str!("../../ui/app.js"))
        .replace("/* SLIDERUST_VERSION */", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::super::browser::{RAIL_WIDTH, STATUS_HEIGHT, TOOLBAR_HEIGHT};

    /// 原生網頁視圖的位置由 Rust 常數決定，控制面板由 CSS 決定；兩邊不一致時網頁會蓋住工具列或留下空隙。
    #[test]
    fn stylesheet_layout_matches_the_native_page_bounds() {
        let stylesheet = include_str!("../../ui/style.css");
        for (property, expected) in [
            ("--rail-width", RAIL_WIDTH),
            ("--toolbar-height", TOOLBAR_HEIGHT),
            ("--status-height", STATUS_HEIGHT),
        ] {
            let declarations: Vec<_> = stylesheet
                .match_indices(&format!("{property}: "))
                .map(|(start, matched)| &stylesheet[start + matched.len()..])
                .filter_map(|rest| rest.split_once(';'))
                .map(|(value, _)| value)
                .collect();
            assert_eq!(declarations, [format!("{expected}px")], "{property}");
        }
    }

    #[test]
    fn document_replaces_every_placeholder_and_embeds_the_package_version() {
        let document = super::document();
        assert!(!document.contains("SLIDERUST_"), "尚有未替換的占位符");
        assert!(document.contains(concat!(
            "const APP_VERSION = '",
            env!("CARGO_PKG_VERSION"),
            "';"
        )));
    }
}
