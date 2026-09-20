//! 以原生選單與 responder 契約驗證整合，不送出系統鍵盤輸入。
use super::{App, Event, browser, chrome::Command, native, shortcuts};
use anyhow::{Context, Result, ensure};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSView};
use objc2_foundation::{NSPoint, NSString};
use sliderust::shortcut::Shortcut;
use std::time::{Duration, Instant};
use tao::event_loop::ControlFlow;
use wry::{WebView, WebViewExtMacOS};

#[derive(Default)]
enum Stage {
    #[default]
    Page,
    Spa,
    Address,
    AddressResult,
    NewPad,
    NewPadResult,
    Hidden,
    ShortcutForm,
    ShortcutFormResult,
    ShortcutApplied,
    ShortcutLabel,
    ResizeLayout,
    ResizeLayoutResult,
    Done,
}

#[derive(Default)]
pub struct Probe {
    stage: Stage,
    address_result: Option<bool>,
    new_pad_result: Option<bool>,
    shortcut_form_result: Option<bool>,
    shortcut_label_result: Option<bool>,
    resize_layout_result: Option<bool>,
    started: Option<Instant>,
}

impl Probe {
    pub fn receive(&mut self, name: &str, passed: bool) {
        match name {
            "address" => self.address_result = Some(passed),
            "new_pad" => self.new_pad_result = Some(passed),
            "shortcut_form" => self.shortcut_form_result = Some(passed),
            "shortcut_label" => self.shortcut_label_result = Some(passed),
            "resize_layout" => self.resize_layout_result = Some(passed),
            _ => {}
        }
    }
    pub fn poll(&mut self, app: &mut App, flow: &mut ControlFlow) -> Result<()> {
        let start = *self.started.get_or_insert_with(Instant::now);
        ensure!(start.elapsed() < Duration::from_secs(25), "原生 smoke 逾時");
        match self.stage {
            Stage::Page if app.chrome_ready && app.page_loaded => {
                let view = active_view(app)?;
                view.evaluate_script("history.pushState({}, '', '/sliderust-smoke')")?;
                self.stage = Stage::Spa;
            }
            Stage::Spa => {
                let view = active_view(app)?;
                let ready = app
                    .settings
                    .active
                    .and_then(|id| app.pads.get(&id))
                    .is_some_and(|pad| pad.address.ends_with("/sliderust-smoke") && pad.history.0);
                if ready && browser::current_url(view).is_some() {
                    view.focus()?;
                    ensure!(has_focus(view), "遠端 WebView 未取得原生焦點");
                    shortcut("l", 0x25)?;
                    self.stage = Stage::Address;
                }
            }
            Stage::Address if has_focus(&app.chrome) => {
                inspect(
                    app,
                    "address",
                    "document.activeElement.id === 'address' && document.activeElement.selectionStart === 0 && document.activeElement.selectionEnd === document.activeElement.value.length && document.activeElement.value.endsWith('/sliderust-smoke')",
                )?;
                self.stage = Stage::AddressResult;
            }
            Stage::AddressResult => {
                if let Some(passed) = self.address_result {
                    ensure!(passed, "⌘L 未選取完整即時網址");
                    active_view(app)?.focus()?;
                    ensure!(has_focus(active_view(app)?), "遠端焦點切換失敗");
                    shortcut("t", 0x11)?;
                    self.stage = Stage::NewPad;
                }
            }
            Stage::NewPad if app.overlay && has_focus(&app.chrome) => {
                inspect(
                    app,
                    "new_pad",
                    "document.activeElement.id === 'new-address' && !document.getElementById('overlay').hidden",
                )?;
                self.stage = Stage::NewPadResult;
            }
            Stage::NewPadResult => {
                if let Some(passed) = self.new_pad_result {
                    ensure!(passed, "⌘T 未開啟新增表單並取得焦點");
                    shortcut("w", 0x0d)?;
                    self.stage = Stage::Hidden;
                }
            }
            Stage::Hidden if !app.visible && app.animation.is_none() => {
                ensure!(!app.chrome.ns_window().isVisible(), "⌘W 尚未收合原生視窗");
                app.handle(Event::Command(Command::ShowSettings), flow)?;
                self.stage = Stage::ShortcutForm;
            }
            Stage::ShortcutForm => {
                inspect(
                    app,
                    "shortcut_form",
                    "Boolean(document.getElementById('shortcut-form')) && document.getElementById('shortcut-key').value === 'Space'",
                )?;
                self.stage = Stage::ShortcutFormResult;
            }
            Stage::ShortcutFormResult => {
                if let Some(passed) = self.shortcut_form_result {
                    ensure!(passed, "設定未顯示快捷鍵表單與預設值");
                    // 用真正的表單提交函式驗證 IPC 契約，不送出系統鍵盤事件。
                    app.chrome.evaluate_script("['control','option','shift','command'].forEach(name => document.getElementById('shortcut-' + name).checked = true); document.getElementById('shortcut-key').value = 'F18'; document.getElementById('shortcut-form').requestSubmit()")?;
                    self.stage = Stage::ShortcutApplied;
                }
            }
            Stage::ShortcutApplied if app.settings.toggle_shortcut.key == "F18" => {
                verify_shortcut_update(app, flow)?;
                inspect(
                    app,
                    "shortcut_label",
                    "document.getElementById('shortcut-key').value === 'F19' && document.getElementById('status-shortcut').textContent.endsWith('F19') && document.getElementById('home-shortcut').textContent.endsWith('F19')",
                )?;
                self.stage = Stage::ShortcutLabel;
            }
            Stage::ShortcutLabel => {
                if let Some(passed) = self.shortcut_label_result {
                    ensure!(passed, "快捷鍵變更後提示未同步");
                    verify_resize(app, flow)?;
                    self.stage = Stage::ResizeLayout;
                }
            }
            Stage::ResizeLayout => {
                // WebKit 的 viewport 在下一次畫面更新才跟上原生 frame。
                self.resize_layout_result = None;
                inspect(
                    app,
                    "resize_layout",
                    &format!(
                        "Math.abs(innerWidth - {}) < 1 && Math.abs(innerHeight - {}) < 1 && typeof document.getElementById('resize-grip').onpointerdown === 'function'",
                        app.frame.width, app.frame.height
                    ),
                )?;
                self.stage = Stage::ResizeLayoutResult;
            }
            Stage::ResizeLayoutResult => {
                if let Some(passed) = self.resize_layout_result {
                    if !passed {
                        self.stage = Stage::ResizeLayout;
                        return Ok(());
                    }
                    eprintln!(
                        "SMOKE chrome_ready=true remote_page_loaded=true spa_navigation=true native_menu_address=true native_menu_new_pad=true native_menu_hide=true shortcut_form=true native_shortcut_update=true shortcut_persistence=true shortcut_event_filter=true shortcut_labels=true native_resize=true webview_layout=true resize_persistence=true resize_rollback=true resize_grip=true"
                    );
                    self.stage = Stage::Done;
                    *flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn verify_resize(app: &mut App, flow: &mut ControlFlow) -> Result<()> {
    use sliderust::panel::{MIN_HEIGHT, MIN_WIDTH};
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
    ensure!(app.window.is_resizable(), "原生視窗未啟用滑鼠縮放");
    let initial = app.frame;
    let target = app.screen.panel(
        (initial.width + 80.0).min(900.0),
        Some((initial.height - 120.0).max(MIN_HEIGHT)),
        8.0,
        app.settings.side,
    );
    native::set_frame(&app.window, target);
    app.window_resized()?;
    ensure!(app.resize_origin.is_some(), "原生尺寸事件未開始追蹤");
    app.hide();
    ensure!(app.visible, "調整期間面板不應收合");
    app.finish_resize()?;
    ensure!(
        app.frame == target && native::window_frame(&app.window) == target,
        "原生尺寸與狀態不一致"
    );
    let saved = app.store.load()?;
    ensure!(
        saved.width == target.width && saved.height == Some(target.height),
        "視窗大小未保存"
    );
    let content = active_view(app)?.webview().frame().size;
    ensure!(
        (content.width - target.width + 80.0).abs() < 1.0
            && (content.height - target.height + 106.0).abs() < 1.0,
        "遠端 WebView 尺寸未同步"
    );

    // 只對明確指定的 smoke 暫存目錄製造保存失敗，結束前恢復原權限。
    let args: Vec<_> = std::env::args().collect();
    let root = args
        .windows(2)
        .find(|arg| arg[0] == "--data-dir")
        .map(|arg| PathBuf::from(&arg[1]))
        .context("smoke 缺少資料目錄")?;
    let permissions = fs::metadata(&root)?.permissions();
    let attempt = app.screen.panel(
        (target.width - 40.0).max(MIN_WIDTH),
        Some((target.height - 40.0).max(MIN_HEIGHT)),
        8.0,
        app.settings.side,
    );
    native::set_frame(&app.window, attempt);
    app.window_resized()?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500))?;
    let failed_save = app.finish_resize();
    fs::set_permissions(&root, permissions)?;
    ensure!(failed_save.is_err(), "唯讀目錄必須拒絕尺寸保存");
    ensure!(
        app.frame == target
            && native::window_frame(&app.window) == target
            && app.settings == saved
            && app.store.load()? == saved,
        "保存失敗未恢復原尺寸與設定"
    );
    app.handle(Event::Command(Command::FullHeight), flow)?;
    app.window_resized()?;
    ensure!(
        app.settings.height.is_none() && app.resize_origin.is_none(),
        "恢復全高不應再被當成使用者縮放"
    );
    Ok(())
}

fn verify_shortcut_update(app: &mut App, flow: &mut ControlFlow) -> Result<()> {
    let previous = app.settings.toggle_shortcut.clone();
    let previous_id = shortcuts::hotkey(&previous)?.id();
    ensure!(
        app.active_shortcut_id() == Some(previous_id),
        "新快捷鍵未啟用"
    );
    ensure!(
        app.store.load()?.toggle_shortcut == previous,
        "表單快捷鍵未保存"
    );
    let next = Shortcut {
        key: "F19".to_owned(),
        ..previous.clone()
    };
    app.handle(
        Event::Command(Command::SetShortcut {
            shortcut: next.clone(),
        }),
        flow,
    )?;
    let next_id = shortcuts::hotkey(&next)?.id();
    ensure!(
        app.active_shortcut_id() == Some(next_id),
        "第二組快捷鍵未啟用"
    );
    // 同一程序只建立一個 Carbon handler，以來回註冊驗證舊組合可重新使用。
    app.set_shortcut(previous.clone())?;
    app.set_shortcut(next.clone())?;
    let saved = app.store.load()?;
    ensure!(saved.toggle_shortcut == next, "自訂快捷鍵重新讀取不一致");
    let visible = app.visible;
    app.handle(Event::Shortcut(previous_id), flow)?;
    ensure!(app.visible == visible, "已解除的快捷鍵事件仍切換面板");
    app.handle(Event::Shortcut(next_id), flow)?;
    ensure!(app.visible != visible, "目前快捷鍵事件未切換面板");
    app.handle(Event::Command(Command::ShowSettings), flow)?;
    Ok(())
}

fn active_view(app: &App) -> Result<&WebView> {
    app.settings
        .active
        .and_then(|id| app.pads.get(&id))
        .map(|pad| &pad.view)
        .context("smoke 網站不存在")
}

fn has_focus(view: &WebView) -> bool {
    view.ns_window().firstResponder().is_some_and(|responder| {
        responder
            .downcast_ref::<NSView>()
            .is_some_and(|focused| focused.isDescendantOf(&view.webview()))
    })
}

fn inspect(app: &App, name: &'static str, script: &str) -> Result<()> {
    let proxy = app.proxy.clone();
    app.chrome
        .evaluate_script_with_callback(script, move |value| {
            let _ = proxy.send_event(Event::SmokeResult(name, value == "true"));
        })?;
    Ok(())
}

fn shortcut(key: &str, key_code: u16) -> Result<()> {
    let mtm = MainThreadMarker::new().context("必須位於主執行緒")?;
    let characters = NSString::from_str(key);
    let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        NSEventType::KeyDown, NSPoint::new(0.0, 0.0), NSEventModifierFlags::Command,
        0.0, 0, None, &characters, &characters, false, key_code,
    ).context("無法建立原生選單測試事件")?;
    let menu = NSApplication::sharedApplication(mtm)
        .mainMenu()
        .context("原生選單不存在")?;
    ensure!(menu.performKeyEquivalent(&event), "原生選單未處理 ⌘{key}");
    Ok(())
}
