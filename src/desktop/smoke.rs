//! 以原生選單與 responder 契約驗證整合，不送出系統鍵盤輸入。
use super::{App, Event, browser, chrome::Command, menus, native, shortcuts};
use anyhow::{Context, Result, ensure};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSView};
use objc2_foundation::{NSPoint, NSString};
use sliderust::{i18n::Language, load::LoadFailure, shortcut::Shortcut};
use std::time::{Duration, Instant};
use tao::event_loop::ControlFlow;
use wry::{WebView, WebViewExtMacOS};

#[derive(Debug, Default)]
enum Stage {
    #[default]
    Page,
    PopupLink,
    PopupScript,
    Frames,
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
    LanguageDefault,
    LanguageChanged,
    LanguageChinese,
    LanguageRestored,
    LanguageEnglish,
    LoadFailure,
    LoadFailureResult,
    LoadRetry,
    LoadDismiss,
    LoadDismissRetried,
    LoadDismissed,
    ImportShown,
    ImportShownResult,
    Imported,
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
    language_result: Option<bool>,
    language_settings: Option<sliderust::model::Settings>,
    frames_result: Option<bool>,
    user_agent_result: Option<bool>,
    load_failure_result: Option<bool>,
    import_result: Option<bool>,
    frames_started: Option<Instant>,
    frames_polled: Option<Instant>,
    frames_loaded: Option<Instant>,
    started: Option<Instant>,
    popup_source: Option<(u64, String)>,
    popup_link: Option<u64>,
}

// 本機沒有服務監聽的埠：連線立刻被拒，用來重現「載入失敗」。
const UNREACHABLE: &str = "http://127.0.0.1:1/";
// fixture 裡唯一會被勾選匯入的書籤；匯入只寫設定，不載入，所以不需要真的連上。
const IMPORTED: &str = "https://www.iana.org/domains/reserved";

// 三個 iframe 各自向上層回報載入：srcdoc 與 blob 應放行，data: 應被導覽規則擋下。
const EMBED_FRAMES: &str = r#"(() => {
  window.__smokeFrames = [];
  addEventListener('message', event => window.__smokeFrames.push(String(event.data)));
  const page = name => `<script>parent.postMessage('${name}', '*')</script>`;
  const embed = configure => { const frame = document.createElement('iframe'); configure(frame); document.body.append(frame); };
  embed(frame => { frame.srcdoc = page('srcdoc'); });
  embed(frame => { frame.src = URL.createObjectURL(new Blob([page('blob')], {type: 'text/html'})); });
  embed(frame => { frame.src = 'data:text/html,' + encodeURIComponent(page('data')); });
})()"#;

impl Probe {
    pub fn receive(&mut self, name: &str, passed: bool) {
        match name {
            "address" => self.address_result = Some(passed),
            "new_pad" => self.new_pad_result = Some(passed),
            "shortcut_form" => self.shortcut_form_result = Some(passed),
            "shortcut_label" => self.shortcut_label_result = Some(passed),
            "resize_layout" => self.resize_layout_result = Some(passed),
            "language" => self.language_result = Some(passed),
            "user_agent" => self.user_agent_result = Some(passed),
            "load_failure" => self.load_failure_result = Some(passed),
            "import" => self.import_result = Some(passed),
            // 一旦看過 data: 文件載入就維持失敗，不被之後的回報蓋掉。
            "frames" => self.frames_result = Some(passed && self.frames_result != Some(false)),
            _ => {}
        }
    }
    pub fn poll(&mut self, app: &mut App, flow: &mut ControlFlow) -> Result<()> {
        let start = *self.started.get_or_insert_with(Instant::now);
        ensure!(
            start.elapsed() < Duration::from_secs(25),
            "原生 smoke 逾時，停在 {:?}",
            self.stage
        );
        match self.stage {
            Stage::Page if app.chrome_ready && app.page_loaded => {
                let id = app.settings.active.context("smoke 網站不存在")?;
                let address =
                    browser::current_url(active_view(app)?).context("smoke 網站不存在")?;
                self.popup_source = Some((id, address));
                active_view(app)?.evaluate_script(
                    "(() => { const link = document.createElement('a'); link.href = 'https://example.com/?sliderust-tab=link'; link.target = '_blank'; document.body.append(link); link.click(); })()",
                )?;
                self.stage = Stage::PopupLink;
            }
            Stage::PopupLink if app.settings.pads.len() == 2 => {
                self.check_popup_source(app)?;
                let id = app.settings.active.context("smoke 網站不存在")?;
                ensure!(
                    active_pad(app)?.address == "https://example.com/?sliderust-tab=link",
                    "新分頁連結未載入獨立頁面"
                );
                self.popup_link = Some(id);
                let source = self.popup_source.as_ref().unwrap().0;
                app.pads[&source].view.evaluate_script(
                    "window.open('https://example.com/?sliderust-tab=script', '_blank')",
                )?;
                self.stage = Stage::PopupScript;
            }
            Stage::PopupScript if app.settings.pads.len() == 3 => {
                self.check_popup_source(app)?;
                ensure!(
                    active_pad(app)?.address == "https://example.com/?sliderust-tab=script",
                    "window.open 未載入獨立頁面"
                );
                let active = app.settings.active.context("smoke 網站不存在")?;
                let background = self.popup_link.unwrap();
                close_popup(app, background, flow)?;
                ensure!(
                    app.settings.active == Some(active) && !app.pads.contains_key(&background),
                    "關閉背景分頁改變目前分頁"
                );
                close_popup(app, active, flow)?;
                let source = self.popup_source.as_ref().unwrap().0;
                ensure!(
                    app.settings.active == Some(source) && app.settings.pads.len() == 1,
                    "關閉目前分頁未返回來源分頁"
                );
                app.handle(Event::Command(Command::UndoRemove), flow)?;
                ensure!(
                    app.settings.active == Some(active) && app.pads.contains_key(&active),
                    "關閉分頁無法復原"
                );
                close_popup(app, active, flow)?;
                self.check_popup_source(app)?;
                eprintln!(
                    "SMOKE target_blank_new_pad=true window_open_new_pad=true popup_source_preserved=true close_background_pad=true close_active_pad=true undo_close_pad=true"
                );
                verify_user_agent(app)?;
                active_view(app)?.evaluate_script(EMBED_FRAMES)?;
                self.frames_started = Some(Instant::now());
                self.stage = Stage::Frames;
            }
            Stage::Frames => {
                if self.frames_settled(app)? {
                    ensure!(
                        self.user_agent_result == Some(true),
                        "遠端網頁看到的 User-Agent 與 App 設定的不一致"
                    );
                    let view = active_view(app)?;
                    view.evaluate_script("history.pushState({}, '', '/sliderust-smoke')")?;
                    self.stage = Stage::Spa;
                }
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
                    verify_menu_language(Language::English)?;
                    self.language_settings = Some(app.settings.clone());
                    inspect(
                        app,
                        "language",
                        "document.documentElement.lang === 'en' && document.getElementById('overlay-title').textContent === 'Settings' && document.getElementById('language-select').value === 'en'",
                    )?;
                    self.stage = Stage::LanguageDefault;
                }
            }
            Stage::LanguageDefault => {
                if let Some(passed) = self.language_result.take() {
                    ensure!(passed, "預設英文介面不正確");
                    select_language(app, "zh-TW")?;
                    self.stage = Stage::LanguageChanged;
                }
            }
            Stage::LanguageChanged if app.settings.language == Language::TraditionalChinese => {
                let mut expected = self
                    .language_settings
                    .clone()
                    .context("缺少語言切換前狀態")?;
                expected.language = Language::TraditionalChinese;
                ensure!(
                    app.settings == expected && app.store.load()? == expected,
                    "語言切換未保存或改變其他偏好"
                );
                ensure!(
                    browser::current_url(active_view(app)?)
                        .is_some_and(|url| url.ends_with("/sliderust-smoke")),
                    "語言切換不應重載網站"
                );
                verify_menu_language(Language::TraditionalChinese)?;
                verify_language_rollback(app, flow)?;
                inspect(
                    app,
                    "language",
                    "document.documentElement.lang === 'zh-TW' && document.getElementById('overlay-title').textContent === '設定' && document.getElementById('language-select').value === 'zh-TW' && document.getElementById('settings').title === '設定'",
                )?;
                self.stage = Stage::LanguageChinese;
            }
            Stage::LanguageChinese => {
                if let Some(passed) = self.language_result.take() {
                    ensure!(passed, "繁中介面或保存失敗後狀態不正確");
                    select_language(app, "en")?;
                    self.stage = Stage::LanguageRestored;
                }
            }
            Stage::LanguageRestored if app.settings.language == Language::English => {
                ensure!(
                    Some(&app.settings) == self.language_settings.as_ref()
                        && app.store.load()? == app.settings,
                    "切回英文不應改變其他偏好"
                );
                verify_menu_language(Language::English)?;
                inspect(
                    app,
                    "language",
                    "document.documentElement.lang === 'en' && document.getElementById('overlay-title').textContent === 'Settings' && document.getElementById('settings').title === 'Settings' && document.getElementById('language-select').value === 'en'",
                )?;
                self.stage = Stage::LanguageEnglish;
            }
            Stage::LanguageEnglish => {
                if let Some(passed) = self.language_result.take() {
                    ensure!(passed, "切回英文後介面未同步");
                    // 翻譯後的選單仍使用原本的原生快捷鍵。
                    shortcut("l", 0x25)?;
                    verify_select_policy(app, flow)?;
                    app.handle(
                        Event::Command(Command::Add {
                            address: UNREACHABLE.to_owned(),
                        }),
                        flow,
                    )?;
                    ensure!(active_pad(app)?.load.is_loading(), "新網站未進入載入狀態");
                    self.stage = Stage::LoadFailure;
                }
            }
            Stage::LoadFailure if load_failed(app)? => {
                inspect(
                    app,
                    "load_failure",
                    &format!(
                        "!document.getElementById('load-error').hidden && document.getElementById('load-error-address').textContent === '{UNREACHABLE}' && document.getElementById('progress').hidden && document.getElementById('reload').dataset.action === 'reload'"
                    ),
                )?;
                self.stage = Stage::LoadFailureResult;
            }
            Stage::LoadFailureResult => {
                if let Some(passed) = self.load_failure_result.take() {
                    ensure!(passed, "載入失敗時未顯示錯誤畫面與原本要求的網址");
                    // 錯誤畫面上沒有載入可停、首次載入失敗也沒有頁面可回：兩個命令都不可改變狀態。
                    app.handle(Event::Command(Command::Stop), flow)?;
                    app.handle(Event::Command(Command::DismissFailure), flow)?;
                    let pad = active_pad(app)?;
                    ensure!(
                        pad.pending.as_deref() == Some(UNREACHABLE)
                            && pad.load.failure() == Some(LoadFailure::Unreachable),
                        "錯誤畫面上的停止／返回改掉了重試要用的網址或失敗狀態"
                    );
                    app.handle(Event::Command(Command::Reload), flow)?;
                    ensure!(
                        active_pad(app)?.load.is_loading(),
                        "失敗畫面上的重試未重新開始載入"
                    );
                    self.stage = Stage::LoadRetry;
                }
            }
            Stage::LoadRetry if load_failed(app)? => {
                // 回到可用的網站：失敗狀態只屬於那個分頁，原生網頁視圖要回來。
                let working = app.settings.pads[0].id;
                app.handle(Event::Command(Command::Select { id: working }), flow)?;
                let pad = active_pad(app)?;
                ensure!(
                    pad.load.failure().is_none() && !pad.view.webview().isHidden(),
                    "切回正常網站後仍停在失敗狀態"
                );
                // 在已有內容的分頁用網址列導覽到連不上的網址：原頁面還在，錯誤畫面要能關掉。
                app.handle(
                    Event::Command(Command::Navigate {
                        address: UNREACHABLE.to_owned(),
                    }),
                    flow,
                )?;
                self.stage = Stage::LoadDismiss;
            }
            Stage::LoadDismiss if load_failed(app)? => {
                // 先重試一次再回去：重試失敗不可讓原頁面離得更遠。
                app.handle(Event::Command(Command::Reload), flow)?;
                self.stage = Stage::LoadDismissRetried;
            }
            Stage::LoadDismissRetried if load_failed(app)? => {
                ensure!(
                    browser::has_previous_page(&active_pad(app)?.view),
                    "網址列導覽失敗後歷史中找不到原頁面"
                );
                app.handle(Event::Command(Command::DismissFailure), flow)?;
                self.stage = Stage::LoadDismissed;
            }
            Stage::LoadDismissed if active_pad(app)?.load.failure().is_none() => {
                let pad = active_pad(app)?;
                ensure!(
                    !pad.view.webview().isHidden()
                        && browser::current_url(&pad.view)
                            .is_some_and(|url| url.starts_with("https://example.com/")),
                    "「回到原本的頁面」後未回到原頁面"
                );
                write_bookmark_fixture(app)?;
                app.handle(Event::Command(Command::ShowImport), flow)?;
                self.stage = Stage::ImportShown;
            }
            Stage::ImportShown if app.overlay => {
                // 候選只該有三個：127.0.0.1:1（此時兩個分頁都停在這個網址，視為已加入而停用）、iana、rfc-editor；
                // javascript: 與重複的都不該出現。
                inspect(
                    app,
                    "import",
                    "(() => { const boxes = [...document.querySelectorAll('#import-list input')]; return boxes.length === 3 && boxes[0].disabled && !boxes[1].disabled && !boxes[2].disabled && document.getElementById('import-submit').disabled; })()",
                )?;
                self.stage = Stage::ImportShownResult;
            }
            Stage::ImportShownResult => {
                if let Some(passed) = self.import_result {
                    ensure!(passed, "匯入畫面的候選清單與 fixture 不符");
                    ensure!(app.import_candidates.len() == 3, "Rust 端候選數量不符");
                    // 走真正的核取方塊與表單提交，驗證控制面板送回索引的契約。
                    app.chrome.evaluate_script("(() => { const box = document.querySelectorAll('#import-list input')[1]; box.checked = true; box.dispatchEvent(new Event('change')); document.getElementById('import-form').requestSubmit(); })()")?;
                    self.stage = Stage::Imported;
                }
            }
            Stage::Imported if app.settings.pads.iter().any(|pad| pad.url == IMPORTED) => {
                let saved = app.store.load()?;
                ensure!(
                    saved
                        .pads
                        .last()
                        .is_some_and(|pad| pad.url == IMPORTED && pad.title == "IANA"),
                    "匯入的網站未寫入設定檔"
                );
                ensure!(
                    app.home && !app.overlay && app.import_candidates.is_empty(),
                    "匯入後應回到首頁並清空候選"
                );
                eprintln!(
                    "SMOKE chrome_ready=true remote_page_loaded=true user_agent=true embedded_frames=true spa_navigation=true native_menu_address=true native_menu_new_pad=true native_menu_hide=true shortcut_form=true native_shortcut_update=true shortcut_persistence=true shortcut_event_filter=true shortcut_labels=true native_resize=true webview_layout=true resize_persistence=true resize_rollback=true resize_grip=true english_default=true language_ipc=true language_persistence=true language_rollback=true language_menus=true language_preserves_sites=true select_policy=true load_failure_screen=true load_retry=true load_dismiss=true bookmark_import=true"
                );
                self.stage = Stage::Done;
                *flow = ControlFlow::Exit;
            }
            _ => {}
        }
        Ok(())
    }

    fn check_popup_source(&self, app: &App) -> Result<()> {
        let (id, address) = self.popup_source.as_ref().context("smoke 網站不存在")?;
        ensure!(
            browser::current_url(&app.pads[id].view).as_ref() == Some(address),
            "新分頁開啟改變來源頁面"
        );
        ensure!(
            app.pads.len() == app.settings.pads.len(),
            "分頁未建立獨立 WebView"
        );
        Ok(())
    }

    /// 等 srcdoc 與 blob 文件回報載入，再多觀察半秒，確認 data: 文件始終被擋下。
    fn frames_settled(&mut self, app: &App) -> Result<bool> {
        ensure!(
            self.frames_result != Some(false),
            "遠端 WebView 放行了 data: 文件"
        );
        if self.frames_result == Some(true) {
            let loaded = *self.frames_loaded.get_or_insert_with(Instant::now);
            if loaded.elapsed() >= Duration::from_millis(500) {
                return Ok(true);
            }
        }
        ensure!(
            self.frames_result.is_some()
                || self
                    .frames_started
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(5)),
            "遠端 WebView 未載入 srcdoc／blob 文件"
        );
        if self
            .frames_polled
            .is_none_or(|at| at.elapsed() >= Duration::from_millis(200))
        {
            self.frames_polled = Some(Instant::now());
            let proxy = app.proxy.clone();
            active_view(app)?.evaluate_script_with_callback(
                "(window.__smokeFrames || []).join(',')",
                move |frames| {
                    if frames.contains("data") {
                        let _ = proxy.send_event(Event::SmokeResult("frames", false));
                    } else if frames.contains("srcdoc") && frames.contains("blob") {
                        let _ = proxy.send_event(Event::SmokeResult("frames", true));
                    }
                },
            )?;
        }
        Ok(false)
    }
}

fn close_popup(app: &mut App, id: u64, flow: &mut ControlFlow) -> Result<()> {
    let command =
        menus::close_pad_command(&format!("close_pad_{id}")).context("smoke 關閉分頁命令不存在")?;
    app.handle(Event::Command(command), flow)
}

/// 網站靠 UA 判斷瀏覽器：遠端網頁回報的字串必須就是 App 組出的 Safari 相容 UA。
fn verify_user_agent(app: &App) -> Result<()> {
    ensure!(
        app.user_agent.contains(" Version/") && app.user_agent.ends_with(" Safari/605.1.15"),
        "User-Agent 缺少 Safari 識別：{}",
        app.user_agent
    );
    let expected = app.user_agent.clone();
    let proxy = app.proxy.clone();
    active_view(app)?.evaluate_script_with_callback("navigator.userAgent", move |reported| {
        let matches =
            serde_json::from_str::<String>(&reported).is_ok_and(|agent| agent == expected);
        let _ = proxy.send_event(Event::SmokeResult("user_agent", matches));
    })?;
    Ok(())
}

fn select_language(app: &App, language: &str) -> Result<()> {
    app.chrome.evaluate_script(&format!("document.getElementById('language-select').value = '{language}'; document.getElementById('language-select').onchange()"))?;
    Ok(())
}

fn verify_menu_language(language: Language) -> Result<()> {
    let mtm = MainThreadMarker::new().context("必須位於主執行緒")?;
    let menu = NSApplication::sharedApplication(mtm)
        .mainMenu()
        .context("原生選單不存在")?;
    for (index, source) in [(1, "檔案"), (2, "編輯"), (3, "瀏覽"), (4, "網站")] {
        let item = menu.itemAtIndex(index).context("缺少原生選單項目")?;
        ensure!(
            item.title().to_string() == language.text(source),
            "原生選單語言不一致：{source}"
        );
    }
    Ok(())
}

/// 選取的網站屬於 UI 狀態：已選同一站不寫檔；保存失敗仍要切換，之後的成功保存會一併寫入。
fn verify_select_policy(app: &mut App, flow: &mut ControlFlow) -> Result<()> {
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        path::PathBuf,
    };
    let args: Vec<_> = std::env::args().collect();
    let root = args
        .windows(2)
        .find(|arg| arg[0] == "--data-dir")
        .map(|arg| PathBuf::from(&arg[1]))
        .context("smoke 缺少資料目錄")?;
    let file = root.join("settings.json");
    let first = app.settings.active.context("smoke 網站不存在")?;
    app.handle(
        Event::Command(Command::Add {
            address: "https://example.com/second".to_owned(),
        }),
        flow,
    )?;
    let second = app.settings.active.context("smoke 網站不存在")?;
    ensure!(second != first, "新增網站後未選取新網站");
    // 原子保存會換掉 inode，可用來分辨「沒寫檔」與「寫了相同內容」。
    let inode = fs::metadata(&file)?.ino();
    app.handle(Event::Command(Command::Select { id: second }), flow)?;
    ensure!(
        fs::metadata(&file)?.ino() == inode,
        "選取已開啟的網站不應寫檔"
    );
    let permissions = fs::metadata(&root)?.permissions();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500))?;
    let switched = app.handle(Event::Command(Command::Select { id: first }), flow);
    fs::set_permissions(&root, permissions)?;
    ensure!(switched.is_ok(), "保存失敗不應阻止切換網站");
    ensure!(
        app.settings.active == Some(first) && !app.home,
        "保存失敗後畫面未切換網站"
    );
    ensure!(
        app.store.load()?.active == Some(second),
        "保存失敗卻改動了磁碟設定"
    );
    app.handle(Event::Command(Command::Pin), flow)?;
    ensure!(
        app.store.load()? == app.settings,
        "後續保存未寫入目前選取的網站"
    );
    Ok(())
}

fn verify_language_rollback(app: &mut App, flow: &mut ControlFlow) -> Result<()> {
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
    let args: Vec<_> = std::env::args().collect();
    let root = args
        .windows(2)
        .find(|arg| arg[0] == "--data-dir")
        .map(|arg| PathBuf::from(&arg[1]))
        .context("smoke 缺少資料目錄")?;
    let saved = app.settings.clone();
    let bytes = fs::read(root.join("settings.json"))?;
    let permissions = fs::metadata(&root)?.permissions();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500))?;
    let failed = app.handle(
        Event::Command(Command::SetLanguage {
            language: Language::English,
        }),
        flow,
    );
    fs::set_permissions(&root, permissions)?;
    ensure!(failed.is_err(), "唯讀目錄必須拒絕語言保存");
    ensure!(
        app.settings == saved
            && app.store.load()? == saved
            && fs::read(root.join("settings.json"))? == bytes,
        "語言保存失敗改變原設定"
    );
    verify_menu_language(Language::TraditionalChinese)
}

fn verify_resize(app: &mut App, flow: &mut ControlFlow) -> Result<()> {
    use sliderust::panel::{MIN_HEIGHT, MIN_WIDTH};
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
    ensure!(app.window.is_resizable(), "原生視窗未啟用滑鼠縮放");
    let initial = app.frame;
    let target = app.screen.panel(
        (initial.width + browser::CHROME_WIDTH).min(900.0),
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
        (content.width - target.width + browser::CHROME_WIDTH).abs() < 1.0
            && (content.height - target.height + browser::CHROME_HEIGHT).abs() < 1.0,
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

/// 兩個 profile、兩種檔名，混入已加入的、危險的與重複的書籤，驗證過濾與去重走的是真實檔案路徑。
fn write_bookmark_fixture(app: &App) -> Result<()> {
    let node = |name: &str, url: &str| serde_json::json!({"type": "url", "name": name, "url": url, "id": "1", "guid": "g"});
    let document = |children: Vec<serde_json::Value>| {
        serde_json::json!({"roots": {"bookmark_bar": {"type": "folder", "children": children}}})
            .to_string()
    };
    let root = app
        .chrome_root
        .as_ref()
        .context("smoke 未設定 Chrome 根目錄")?;
    let default = root.join("Default");
    let profile = root.join("Profile 1");
    std::fs::create_dir_all(&default)?;
    std::fs::create_dir_all(&profile)?;
    std::fs::write(
        default.join("Bookmarks"),
        document(vec![
            node("Unreachable", UNREACHABLE),
            node("Script", "javascript:alert(1)"),
            node("IANA", IMPORTED),
            node("IANA again", IMPORTED),
        ]),
    )?;
    std::fs::write(
        profile.join("AccountBookmarks"),
        document(vec![node("RFC", "https://www.rfc-editor.org/")]),
    )?;
    Ok(())
}

fn active_pad(app: &App) -> Result<&super::BrowserPad> {
    app.settings
        .active
        .and_then(|id| app.pads.get(&id))
        .context("smoke 網站不存在")
}

/// 連不上的網站：由輪詢推斷出失敗，且原生網頁視圖已讓位給控制面板的錯誤畫面。
fn load_failed(app: &App) -> Result<bool> {
    let pad = active_pad(app)?;
    if pad.load.failure() != Some(LoadFailure::Unreachable) {
        return Ok(false);
    }
    ensure!(
        pad.view.webview().isHidden(),
        "載入失敗後原生網頁視圖仍蓋住錯誤畫面"
    );
    Ok(true)
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
