//! 本机 HTTP fixture 验证实际 WebView 释放与会话恢复，不操作用户网站。
use super::{App, Event, browser, chrome::Command};
use anyhow::{Context, Result, ensure};
use objc2::rc::Weak;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tao::event_loop::ControlFlow;
use wry::WebViewExtMacOS;

const PAGE: &str = "<!doctype html><title>Sleep fixture</title><form id='compose'><textarea id='draft' name='draft'></textarea></form><div style='height:4000px'>Sleep fixture</div>";

struct Fixture {
    address: String,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Fixture {
    fn new() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = format!("http://{}/", listener.local_addr()?);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut request = [0; 2048];
                    let _ = stream.read(&mut request);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
                        PAGE.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });
        Ok(Self {
            address,
            stopped,
            worker: Some(worker),
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Debug)]
enum Stage {
    Ready,
    Settings,
    Marked,
    Background,
    Released,
    Restored,
    Result,
}

pub(super) struct Probe {
    fixture: Fixture,
    stage: Stage,
    started: Instant,
    marked_at: Option<Instant>,
    source: u64,
    old_view_id: u64,
    weak: Option<Weak<wry::WryWebView>>,
    marked: bool,
    restored: bool,
}

impl Probe {
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            fixture: Fixture::new()?,
            stage: Stage::Ready,
            started: Instant::now(),
            marked_at: None,
            source: 0,
            old_view_id: 0,
            weak: None,
            marked: false,
            restored: false,
        })
    }
    pub(super) fn address(&self) -> &str {
        &self.fixture.address
    }
    pub(super) fn receive(&mut self, name: &str, passed: bool) {
        match name {
            "sleep_marked" => self.marked = passed,
            "sleep_restored" => self.restored = passed,
            _ => {}
        }
    }
    pub(super) fn poll(&mut self, app: &mut App, flow: &mut ControlFlow) -> Result<()> {
        ensure!(
            self.started.elapsed() < Duration::from_secs(25),
            "休眠 smoke 逾時：{:?} marked={} restored={} live={} sleeping={}",
            self.stage,
            self.marked,
            self.restored,
            app.pads.len(),
            app.sleeping.len()
        );
        match self.stage {
            Stage::Ready if app.chrome_ready && app.page_loaded => {
                self.source = app.settings.active.context("smoke 網站不存在")?;
                app.handle(Event::Command(Command::ShowSettings), flow)?;
                app.chrome.evaluate_script("(() => { const select = document.getElementById('sleep-select'); select.value = '30'; select.dispatchEvent(new Event('change')); })()")?;
                self.stage = Stage::Settings;
            }
            Stage::Settings if app.settings.sleep_after_minutes == 30 => {
                ensure!(
                    app.store.load()?.sleep_after_minutes == 30,
                    "休眠時間未保存"
                );
                verify_failed_save(app, flow)?;
                app.handle(Event::Command(Command::Select { id: self.source }), flow)?;
                let view = &app.pads[&self.source].view;
                inspect_view(
                    app,
                    view,
                    "sleep_marked",
                    "(() => { const field = document.getElementById('draft'); field.focus(); document.execCommand('insertText', false, 'kept draft'); field.blur(); window.scrollTo(0,600); history.pushState({}, '', '/restored'); return field.value === 'kept draft'; })()",
                )?;
                self.marked_at = Some(Instant::now());
                self.stage = Stage::Marked;
            }
            Stage::Marked
                if self.marked
                    && self.marked_at.unwrap().elapsed() > Duration::from_millis(500) =>
            {
                app.handle(
                    Event::Command(Command::Add {
                        address: format!("{}background", self.address()),
                    }),
                    flow,
                )?;
                self.stage = Stage::Background;
            }
            Stage::Background
                if app.page_loaded && app.pads.values().all(|pad| !pad.load.is_loading()) =>
            {
                self.old_view_id = app.pads[&self.source].view_id;
                self.weak = Some(Weak::from_retained(&app.pads[&self.source].view.webview()));
                let deadline = app.sleep_wake(Instant::now()).context("缺少休眠排程")?;
                let original = (
                    app.visible,
                    app.settings.hot_edge,
                    app.smoke,
                    app.animation.take(),
                );
                app.visible = false;
                app.settings.hot_edge = false;
                app.smoke = false;
                ensure!(
                    matches!(app.next_wake(), ControlFlow::WaitUntil(at) if at == deadline),
                    "收起且停用 hot edge 時未保留低頻休眠排程"
                );
                (app.visible, app.settings.hot_edge, app.smoke, app.animation) = original;
                let future = Instant::now() + Duration::from_secs(1801);
                ensure!(app.sleep_background(future)?, "背景分頁未休眠");
                ensure!(
                    !app.pads.contains_key(&self.source) && app.sleeping.contains_key(&self.source),
                    "休眠仍保留 live WebView"
                );
                ensure!(app.pads.len() == 1, "目前分頁不應休眠");
                let wake = app.sleep_wake(Instant::now());
                ensure!(wake.is_none(), "已休眠分頁仍排程喚醒");
                self.stage = Stage::Released;
            }
            Stage::Released if self.weak.as_ref().unwrap().load().is_none() => {
                app.handle(Event::Command(Command::Select { id: self.source }), flow)?;
                ensure!(
                    app.pads[&self.source].view_id != self.old_view_id,
                    "恢復未建立新世代"
                );
                app.handle(Event::Title(self.old_view_id, "stale".into()), flow)?;
                app.handle(
                    Event::Load(self.old_view_id, true, "https://example.com/stale".into()),
                    flow,
                )?;
                app.handle(Event::Crashed(self.old_view_id), flow)?;
                app.handle(
                    Event::Popup(self.old_view_id, "https://example.com/stale".into()),
                    flow,
                )?;
                ensure!(
                    app.settings.pads.len() == 2
                        && app.pads[&self.source].title != "stale"
                        && app.pads[&self.source].load.failure().is_none(),
                    "過期回呼干擾恢復頁面"
                );
                self.stage = Stage::Restored;
            }
            Stage::Restored if !app.pads[&self.source].load.is_loading() => {
                let view = &app.pads[&self.source].view;
                ensure!(
                    browser::current_url(view).is_some_and(|url| url.ends_with("/restored")),
                    "恢復網址錯誤"
                );
                ensure!(unsafe { view.webview().canGoBack() }, "恢復遺失歷史");
                inspect_view(app, view, "sleep_restored", "window.scrollY >= 300")?;
                view.evaluate_script_with_callback(
                    "document.getElementById('draft').value === 'kept draft'",
                    |value| {
                        eprintln!(
                            "SLEEP OBSERVATION current_form_preserved={value} (not guaranteed)"
                        )
                    },
                )?;
                self.stage = Stage::Result;
            }
            Stage::Result if self.restored => {
                app.handle(Event::Command(Command::SetSleep { minutes: 0 }), flow)?;
                ensure!(
                    !app.sleep_background(Instant::now() + Duration::from_secs(10000))?,
                    "停用後仍休眠"
                );
                ensure!(
                    app.handle(Event::Command(Command::SetSleep { minutes: 1 }), flow)
                        .is_err(),
                    "非法休眠時間未拒絕"
                );
                ensure!(
                    app.settings.sleep_after_minutes == 0
                        && app.store.load()?.sleep_after_minutes == 0,
                    "非法設定破壞原設定"
                );
                app.handle(Event::Command(Command::SetSleep { minutes: 5 }), flow)?;
                let background = app
                    .settings
                    .pads
                    .iter()
                    .find(|pad| pad.id != self.source)
                    .unwrap()
                    .id;
                ensure!(
                    app.sleep_background(Instant::now() + Duration::from_secs(301))?,
                    "再次休眠失敗"
                );
                app.handle(Event::Command(Command::Remove { id: background }), flow)?;
                ensure!(
                    !app.sleeping.contains_key(&background) && !app.pads.contains_key(&background),
                    "關閉休眠分頁未釋放快照"
                );
                eprintln!(
                    "SLEEP SMOKE settings_ipc=true settings_persistence=true failed_save_preserved=true hidden_sleep_deadline=true background_sleep=true active_preserved=true native_webview_released=true fresh_generation=true stale_callbacks_ignored=true restored_url=true restored_history=true restored_scroll=true disable_sleep=true invalid_setting_preserved=true sleeping_close=true"
                );
                *flow = ControlFlow::Exit;
            }
            _ => {}
        }
        Ok(())
    }
}

fn verify_failed_save(app: &mut App, flow: &mut ControlFlow) -> Result<()> {
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
    let args: Vec<_> = std::env::args().collect();
    let root = args
        .windows(2)
        .find(|pair| pair[0] == "--data-dir")
        .map(|pair| PathBuf::from(&pair[1]))
        .context("smoke 缺少資料目錄")?;
    let saved = app.settings.clone();
    let bytes = fs::read(root.join("settings.json"))?;
    let permissions = fs::metadata(&root)?.permissions();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500))?;
    let result = app.handle(Event::Command(Command::SetSleep { minutes: 60 }), flow);
    fs::set_permissions(&root, permissions)?;
    ensure!(result.is_err(), "唯讀目錄必須拒絕休眠設定保存");
    ensure!(
        app.settings == saved
            && app.store.load()? == saved
            && fs::read(root.join("settings.json"))? == bytes,
        "休眠保存失敗破壞原設定"
    );
    Ok(())
}

fn inspect_view(app: &App, view: &wry::WebView, name: &'static str, script: &str) -> Result<()> {
    let proxy = app.proxy.clone();
    view.evaluate_script_with_callback(script, move |answer| {
        let _ = proxy.send_event(Event::SmokeResult(name, answer == "true"));
    })?;
    Ok(())
}
