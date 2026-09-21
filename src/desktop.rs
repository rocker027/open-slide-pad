mod browser;
mod chrome;
mod menus;
mod native;
mod resize;
mod shortcuts;
mod smoke;

use anyhow::{Context, Result, ensure};
use chrome::Command;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use serde_json::json;
use sliderust::{
    model::{Pad, Settings, Side, normalize_address, web_url},
    panel::{Activity, AutoHide, EdgeTrigger, Frame, OpenedBy, POLL_TICK},
    shortcut::ShortcutBinding,
    storage::{SaveOutcome, SettingsStore},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use tao::{
    event::{Event as TaoEvent, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::macos::{ActivationPolicy, EventLoopExtMacOS},
    window::{Window, WindowBuilder},
};
use tray_icon::{TrayIcon, menu::Menu};
use wry::{WebView, WebViewExtMacOS};

#[derive(Debug)]
pub enum Event {
    Command(Command),
    Toggle,
    Shortcut(u32),
    Quit,
    Title(u64, String),
    Load(u64, bool, String),
    Popup(u64, String),
    SmokeResult(&'static str, bool),
}

struct BrowserPad {
    view: WebView,
    title: String,
    address: String,
    loading: bool,
    history: (bool, bool),
}
struct App {
    // WebViews 必須比宿主 Window 更早釋放。
    chrome: WebView,
    pads: HashMap<u64, BrowserPad>,
    window: Window,
    settings: Settings,
    store: SettingsStore,
    proxy: EventLoopProxy<Event>,
    frame: Frame,
    screen: Frame,
    visible: bool,
    home: bool,
    overlay: bool,
    auto_hide: AutoHide,
    animation: Option<(Instant, bool)>,
    edge: EdgeTrigger,
    smoke: bool,
    chrome_ready: bool,
    page_loaded: bool,
    smoke_probe: Option<smoke::Probe>,
    navigation_checked: Instant,
    last_removed: Option<(Pad, usize)>,
    _tray: TrayIcon,
    _menu: Menu,
    menu_labels: menus::Labels,
    shortcut_binding: ShortcutBinding<shortcuts::NativeShortcuts>,
    shortcut_error: Option<String>,
    resize_origin: Option<Frame>,
    pointer_resize: Option<resize::PointerResize>,
}

pub fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let smoke = arguments.iter().any(|arg| arg == "--smoke-test");
    let data_dir = arguments
        .windows(2)
        .find(|args| args[0] == "--data-dir")
        .map(|args| PathBuf::from(&args[1]));
    ensure!(
        !smoke || data_dir.is_some(),
        "smoke test 必須指定獨立 --data-dir"
    );
    // 沿用既有資料目錄識別碼，讓品牌更名後保留使用者設定。
    let root = data_dir.unwrap_or(
        directories::ProjectDirs::from("app", "sliderust", "SlideRust")
            .context("找不到使用者資料目錄")?
            .data_dir()
            .to_owned(),
    );
    let store = SettingsStore::open(&root)?;
    let mut event_loop = EventLoopBuilder::<Event>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    // tao 必須是第一個建立 NSApplication 的人，否則之後拿到的不是它的子類。
    // 載入失敗的對話框會用到 NSApplication，所以要排在事件迴圈建立之後。
    let Some(mut settings) = load_settings(&store, smoke)? else {
        return Ok(());
    };
    if smoke {
        settings = Settings::default().add("https://example.com")?;
    }
    let proxy = event_loop.create_proxy();
    let mut app: Option<App> = None;
    let mut initial_store = Some(store);
    event_loop.run(move |event, target, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + POLL_TICK);
        if let TaoEvent::NewEvents(StartCause::Init) = event {
            match App::new(
                target,
                proxy.clone(),
                initial_store.take().expect("僅初始化一次"),
                smoke,
                settings.clone(),
            ) {
                Ok(created) => app = Some(created),
                Err(error) => {
                    eprintln!("Open Slide Pad：{error:#}");
                    if !smoke {
                        report_error(&format!("{error:#}"));
                    }
                    *flow = ControlFlow::ExitWithCode(1);
                }
            }
        }
        if let Some(app) = &mut app {
            let outcome = match event {
                TaoEvent::UserEvent(Event::Quit) => {
                    *flow = ControlFlow::Exit;
                    Ok(())
                }
                TaoEvent::UserEvent(message) => app.handle(message, flow),
                TaoEvent::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    app.hide();
                    Ok(())
                }
                TaoEvent::WindowEvent {
                    event: WindowEvent::Resized(_),
                    ..
                } => app.window_resized(),
                TaoEvent::MainEventsCleared => app.tick(flow),
                _ => Ok(()),
            };
            if let Err(error) = outcome {
                eprintln!("Open Slide Pad：{error:#}");
                app.toast(&format!("{error:#}"));
            }
            // 結束中的狀態不可被覆蓋，否則程序不會退出。
            if !matches!(*flow, ControlFlow::Exit | ControlFlow::ExitWithCode(_)) {
                *flow = app.next_wake();
            }
        }
    });
}

impl App {
    fn new(
        target: &tao::event_loop::EventLoopWindowTarget<Event>,
        proxy: EventLoopProxy<Event>,
        store: SettingsStore,
        smoke: bool,
        settings: Settings,
    ) -> Result<Self> {
        let window = WindowBuilder::new()
            .with_title("Open Slide Pad")
            .with_decorations(false)
            .with_resizable(true)
            .with_visible(false)
            .with_always_on_top(true)
            .with_visible_on_all_workspaces(true)
            .build(target)?;
        let chrome = chrome::build(&window, proxy.clone())?;
        let (tray, menu, menu_labels) = menus::build(proxy.clone(), settings.language)?;
        let mut shortcut_binding =
            ShortcutBinding::new(shortcuts::NativeShortcuts(GlobalHotKeyManager::new()?));
        let shortcut_error = shortcut_binding
            .apply(&settings.toggle_shortcut, || Ok(SaveOutcome::Durable))
            .err()
            .map(|error| format!("快捷鍵尚未啟用：{error:#}"));
        let hotkey_proxy = proxy.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state == HotKeyState::Pressed {
                let _ = hotkey_proxy.send_event(Event::Shortcut(event.id));
            }
        }));
        let mut app = Self {
            chrome,
            pads: HashMap::new(),
            window,
            home: settings.pads.is_empty(),
            settings,
            store,
            proxy,
            frame: Frame::default(),
            screen: Frame::default(),
            visible: false,
            overlay: false,
            auto_hide: AutoHide::default(),
            animation: None,
            edge: EdgeTrigger::default(),
            smoke,
            chrome_ready: false,
            page_loaded: false,
            smoke_probe: smoke.then(smoke::Probe::default),
            navigation_checked: Instant::now(),
            last_removed: None,
            _tray: tray,
            _menu: menu,
            menu_labels,
            shortcut_binding,
            shortcut_error,
            resize_origin: None,
            pointer_resize: None,
        };
        app.update_shortcut_tooltip();
        app.show(OpenedBy::Request)?;
        Ok(app)
    }

    /// 隱藏且 hot edge 關閉時完全不輪詢；動畫與拖曳期間提高更新頻率。
    fn next_wake(&self) -> ControlFlow {
        let activity = Activity {
            probing: self.smoke,
            animating: self.animation.is_some()
                || self.pointer_resize.is_some()
                || self.resize_origin.is_some(),
            visible: self.visible,
            hot_edge: self.settings.hot_edge,
        };
        activity
            .wake_interval()
            .map_or(ControlFlow::Wait, |interval| {
                ControlFlow::WaitUntil(Instant::now() + interval)
            })
    }

    fn show(&mut self, by: OpenedBy) -> Result<()> {
        let screens = native::screens();
        let (_, work) = screens
            .iter()
            .find(|(full, _)| full.contains(native::mouse()))
            .or(screens.first())
            .context("找不到螢幕")?;
        self.screen = *work;
        self.reposition()?;
        self.visible = true;
        self.auto_hide.opened(by);
        self.animation = Some((Instant::now(), true));
        native::opacity(&self.window, 0.0);
        self.window.set_visible(true);
        // 碰到邊緣只顯示、不啟用 App：使用者可能正在別的 App 打字，點進面板才取得鍵盤焦點。
        if by == OpenedBy::Request {
            self.window.set_focus();
        }
        self.sync_views()?;
        self.render()
    }
    fn hide(&mut self) {
        if self.resize_origin.is_some() || native::is_live_resize(&self.window) {
            return;
        }
        self.visible = false;
        self.animation = Some((Instant::now(), false));
        self.overlay = false;
        self.edge.reset_until_leave();
        let _ = self.chrome.evaluate_script("window.closeOverlay()");
    }
    fn reposition(&mut self) -> Result<()> {
        self.resize_limits();
        self.frame = self.screen.panel(
            self.settings.width,
            self.settings.height,
            self.settings.top_offset,
            self.settings.side,
        );
        native::set_frame(&self.window, self.frame);
        self.layout()
    }
    fn layout(&self) -> Result<()> {
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        self.chrome.set_bounds(wry::Rect {
            position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
            size: wry::dpi::LogicalSize::new(size.width, size.height).into(),
        })?;
        for pad in self.pads.values() {
            pad.view
                .set_bounds(browser::bounds(size.width, size.height))?;
        }
        Ok(())
    }
    fn commit(&mut self, settings: Settings) -> Result<()> {
        let saved = self.store.save(&settings)?;
        self.settings = settings;
        if let SaveOutcome::CommittedWithWarning(warning) = saved {
            eprintln!("Open Slide Pad：{warning}");
            self.toast(&warning);
        }
        Ok(())
    }
    fn toast(&self, message: &str) {
        chrome::toast(&self.chrome, &self.settings.language.text(message));
    }
    fn sync_views(&mut self) -> Result<()> {
        if !self.home
            && !self.overlay
            && let Some(id) = self.settings.active
            && !self.pads.contains_key(&id)
        {
            let pad = self
                .settings
                .pads
                .iter()
                .find(|pad| pad.id == id)
                .context("網站不存在")?;
            let view = browser::build(
                &self.window,
                pad,
                self.proxy.clone(),
                self.frame.width,
                self.frame.height,
                self.smoke,
            )?;
            self.pads.insert(
                id,
                BrowserPad {
                    view,
                    title: pad.title.clone(),
                    address: pad.url.clone(),
                    loading: true,
                    history: (false, false),
                },
            );
        }
        for (id, pad) in &self.pads {
            pad.view
                .set_visible(!self.home && !self.overlay && self.settings.active == Some(*id))?;
        }
        self.layout()
    }
    fn render(&self) -> Result<()> {
        let active = self.settings.active.and_then(|id| self.pads.get(&id));
        let payload = json!({ "settings": self.settings.without_unknown_fields(), "shortcut_label": self.settings.toggle_shortcut.label(), "shortcut_active": self.shortcut_binding.active().is_some(), "shortcut_error": self.shortcut_error.as_ref().map(|error| self.settings.language.text(error)), "home": self.home, "undo_title": self.last_removed.as_ref().map(|(pad,_)| &pad.title), "title": active.map(|p| &p.title), "address": active.map(|p| &p.address), "loading": active.is_some_and(|p| p.loading), "back": active.is_some_and(|p| unsafe { p.view.webview().canGoBack() }), "forward": active.is_some_and(|p| unsafe { p.view.webview().canGoForward() }) });
        self.chrome
            .evaluate_script(&format!("window.render({payload})"))?;
        Ok(())
    }
    fn handle(&mut self, event: Event, flow: &mut ControlFlow) -> Result<()> {
        match event {
            Event::Shortcut(id) => {
                if self.active_shortcut_id() == Some(id) {
                    return self.handle(Event::Toggle, flow);
                }
            }
            Event::Toggle => {
                if self.visible {
                    self.hide();
                } else {
                    self.show(OpenedBy::Request)?;
                }
            }
            Event::Command(command) => self.command(command, flow)?,
            Event::Title(id, title) => {
                if let Some(pad) = self.pads.get_mut(&id) {
                    pad.title = title;
                }
            }
            Event::Load(id, finished, address) => {
                if let Some(pad) = self.pads.get_mut(&id) {
                    pad.loading = !finished;
                    if web_url(&address).is_ok() {
                        pad.address = address;
                    }
                    self.page_loaded |= finished;
                }
            }
            Event::Popup(id, address) => {
                if let Some(pad) = self.pads.get(&id) {
                    pad.view.load_url(&address)?;
                }
            }
            Event::SmokeResult(name, passed) => {
                if let Some(probe) = &mut self.smoke_probe {
                    probe.receive(name, passed);
                }
            }
            Event::Quit => *flow = ControlFlow::Exit,
        }
        self.render()
    }
    fn command(&mut self, command: Command, flow: &mut ControlFlow) -> Result<()> {
        match command {
            Command::Ready => {
                self.chrome_ready = true;
            }
            Command::SetShortcut { shortcut } => self.set_shortcut(shortcut)?,
            Command::SetLanguage { language } => {
                self.commit(Settings {
                    language,
                    ..self.settings.clone()
                })?;
                self.menu_labels.set_language(language);
                self.update_shortcut_tooltip();
            }
            Command::BeginResize => self.begin_pointer_resize(),
            Command::FullHeight => {
                self.commit(Settings {
                    height: None,
                    top_offset: 8.0,
                    ..self.settings.clone()
                })?;
                self.reposition()?;
            }
            Command::Add { address } => {
                let updated = self.settings.add(&address)?;
                self.commit(updated)?;
                self.home = false;
                self.overlay = false;
                self.chrome.evaluate_script("window.closeOverlay()")?;
            }
            Command::Remove { id } => {
                let position = self
                    .settings
                    .pads
                    .iter()
                    .position(|pad| pad.id == id)
                    .context("網站不存在")?;
                let removed = self.settings.pads[position].clone();
                self.commit(self.settings.remove(id)?)?;
                self.last_removed = Some((removed, position));
                self.pads.remove(&id);
                self.home = self.settings.pads.is_empty();
            }
            Command::Select { id } => {
                self.select(id)?;
            }
            Command::Rename { id, title } => {
                self.commit(self.settings.rename(id, &title)?)?;
                self.chrome.evaluate_script("window.showSettings()")?;
            }
            Command::Move { id, position } => self.commit(self.settings.move_pad(id, position)?)?,
            Command::UndoRemove => self.undo_remove()?,
            Command::FocusAddress => self.focus_chrome("window.focusAddress()", false)?,
            Command::NewPad => self.focus_chrome("window.showAddForm()", true)?,
            Command::ShowSettings => self.focus_chrome("window.showSettings()", true)?,
            Command::SelectIndex { index } => {
                if let Some(pad) = self.settings.pads.get(index) {
                    self.select(pad.id)?;
                }
            }
            Command::Navigate { address } => self.navigate(&address)?,
            Command::Back | Command::Forward | Command::Reload => self.navigation(command),
            Command::Home => {
                self.home = true;
                self.overlay = false;
                self.chrome.evaluate_script("window.closeOverlay()")?;
            }
            Command::Hide => self.hide(),
            Command::Pin => self.commit(Settings {
                pinned: !self.settings.pinned,
                ..self.settings.clone()
            })?,
            Command::Overlay { open } => self.overlay = open,
            Command::Side => {
                let side = if self.settings.side == Side::Right {
                    Side::Left
                } else {
                    Side::Right
                };
                self.commit(Settings {
                    side,
                    ..self.settings.clone()
                })?;
                self.reposition()?;
            }
            Command::Width { width } => {
                self.commit(Settings {
                    width,
                    ..self.settings.clone()
                })?;
                self.reposition()?;
            }
            Command::HotEdge => self.commit(Settings {
                hot_edge: !self.settings.hot_edge,
                ..self.settings.clone()
            })?,
            Command::External => self.open_external()?,
            Command::Quit => *flow = ControlFlow::Exit,
        }
        self.sync_views()
    }
    fn select(&mut self, id: u64) -> Result<()> {
        ensure!(
            self.settings.pads.iter().any(|pad| pad.id == id),
            "網站不存在"
        );
        self.home = false;
        self.overlay = false;
        self.chrome.evaluate_script("window.closeOverlay()")?;
        if self.settings.active == Some(id) {
            return Ok(());
        }
        // 選取的網站屬於 UI 狀態，是「以成功寫入作為提交點」的例外：先切換，保存失敗只提示。
        // 之後任何一次成功保存都會把目前的選取一併寫入。
        let updated = Settings {
            active: Some(id),
            ..self.settings.clone()
        };
        if let Err(error) = self.commit(updated.clone()) {
            self.settings = updated;
            eprintln!("Open Slide Pad：{error:#}");
            self.toast(&format!("已切換網站，但無法保存選取狀態：{error:#}"));
        }
        Ok(())
    }
    fn undo_remove(&mut self) -> Result<()> {
        if let Some((pad, position)) = self.last_removed.clone() {
            self.commit(self.settings.restore_pad(pad, position)?)?;
            self.last_removed = None;
            self.home = false;
        }
        Ok(())
    }
    fn focus_chrome(&mut self, script: &str, overlay: bool) -> Result<()> {
        if !self.visible {
            self.show(OpenedBy::Request)?;
        }
        self.overlay = overlay;
        self.sync_views()?;
        self.chrome.focus()?;
        self.chrome.evaluate_script(script)?;
        Ok(())
    }
    fn navigate(&mut self, address: &str) -> Result<()> {
        if self.home {
            self.commit(self.settings.add(address)?)?;
            self.home = false;
            return Ok(());
        }
        let address = normalize_address(address)?;
        if let Some(id) = self.settings.active {
            let mut updated = self.settings.clone();
            if let Some(pad) = updated.pads.iter_mut().find(|p| p.id == id) {
                pad.url = address.clone();
            }
            self.commit(updated)?;
            if let Some(pad) = self.pads.get(&id) {
                pad.view.load_url(&address)?;
            }
        }
        Ok(())
    }
    fn navigation(&self, command: Command) {
        if let Some(pad) = self.settings.active.and_then(|id| self.pads.get(&id)) {
            // 已在主執行緒，Wry 保持 WKWebView 的所有權。
            unsafe {
                match command {
                    Command::Back => {
                        pad.view.webview().goBack();
                    }
                    Command::Forward => {
                        pad.view.webview().goForward();
                    }
                    _ => {
                        pad.view.webview().reload();
                    }
                }
            }
        }
    }
    fn open_external(&self) -> Result<()> {
        if let Some(pad) = self.settings.active.and_then(|id| self.pads.get(&id)) {
            let address = browser::current_url(&pad.view).context("網頁尚未有可開啟的網址")?;
            web_url(&address)?;
            ensure!(
                std::process::Command::new("/usr/bin/open")
                    .arg(&address)
                    .status()?
                    .success(),
                "無法開啟預設瀏覽器"
            );
        }
        Ok(())
    }
    // pushState／replaceState 不會觸發載入 callback，直接讀原生導覽狀態。
    fn refresh_navigation(&mut self) -> Result<bool> {
        let Some(pad) = self.settings.active.and_then(|id| self.pads.get_mut(&id)) else {
            return Ok(false);
        };
        let Some(address) = browser::current_url(&pad.view) else {
            return Ok(false);
        };
        let history = unsafe {
            (
                pad.view.webview().canGoBack(),
                pad.view.webview().canGoForward(),
            )
        };
        let changed =
            (web_url(&address).is_ok() && pad.address != address) || pad.history != history;
        if web_url(&address).is_ok() {
            pad.address = address;
        }
        pad.history = history;
        Ok(changed)
    }
    fn tick(&mut self, flow: &mut ControlFlow) -> Result<()> {
        if self.poll_resize()? {
            return Ok(());
        }
        let now = Instant::now();
        if self.visible && now.duration_since(self.navigation_checked) >= Duration::from_millis(250)
        {
            self.navigation_checked = now;
            if self.refresh_navigation()? {
                self.render()?;
            }
        }
        if let Some(mut probe) = self.smoke_probe.take() {
            if let Err(error) = probe.poll(self, flow) {
                eprintln!("SMOKE FAILED：{error:#}");
                *flow = ControlFlow::ExitWithCode(1);
            }
            self.smoke_probe = Some(probe);
        }
        if let Some((start, opening)) = self.animation {
            let progress = (now.duration_since(start).as_secs_f64() / 0.18).min(1.0);
            native::animate_frame(
                &self.window,
                self.frame,
                if opening { progress } else { 1.0 - progress },
                self.settings.side == Side::Right,
            );
            if progress >= 1.0 {
                self.animation = None;
                if !opening {
                    self.window.set_visible(false);
                }
            }
        }
        let mouse = native::mouse();
        let at_edge = native::screens()
            .iter()
            .any(|(full, _)| full.at_edge(mouse, self.settings.side));
        if self.edge.update(at_edge && self.settings.hot_edge, now) && !self.visible {
            self.show(OpenedBy::Edge)?;
        }
        if self.visible && !self.smoke && self.animation.is_none() {
            let held = self.settings.pinned || self.overlay;
            if self
                .auto_hide
                .update(self.frame.contains(mouse), at_edge, held, now)
            {
                self.hide();
            }
        }
        Ok(())
    }
}

/// 設定無法載入時由使用者決定：結束（回傳 None，原檔不動），或把原檔改名備份後以預設值啟動。
/// smoke 不顯示對話框，直接回報錯誤。
fn load_settings(store: &SettingsStore, smoke: bool) -> Result<Option<Settings>> {
    let error = match store.load() {
        Ok(settings) => return Ok(Some(settings)),
        Err(error) => error,
    };
    if smoke {
        return Err(error);
    }
    eprintln!("Open Slide Pad：{error:#}");
    if !native::confirm_reset(&format!("{error:#}")) {
        return Ok(None);
    }
    let backup = store.quarantine()?;
    eprintln!("Open Slide Pad：原設定已備份至 {}", backup.display());
    store.load().map(Some)
}

pub fn report_error(message: &str) {
    native::alert(message);
}
