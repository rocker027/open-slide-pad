//! 保存原生会话快照，释放后台 WebView；快照只留在本次进程内。
use super::{App, browser, pad::BrowserPad};
use anyhow::{Context, Result};
use objc2::{rc::Retained, runtime::AnyObject};
use sliderust::{load::LoadWatch, model::web_url, sleep::SleepTimer};
use std::time::Instant;
use wry::WebViewExtMacOS;

pub(super) struct SleepingPad {
    state: Retained<AnyObject>,
    address: String,
    title: String,
}

impl App {
    pub(super) fn live_view_mut(&mut self, view_id: u64) -> Option<&mut BrowserPad> {
        self.pads.values_mut().find(|pad| pad.view_id == view_id)
    }

    pub(super) fn wake_pad(&mut self, id: u64) -> Result<()> {
        let saved = self
            .settings
            .pads
            .iter()
            .find(|pad| pad.id == id)
            .context("網站不存在")?;
        let sleeping = self.sleeping.get(&id);
        let address = sleeping.map_or(&saved.url, |pad| &pad.address).clone();
        let title = sleeping.map_or(&saved.title, |pad| &pad.title).clone();
        let view_id = self.next_view_id;
        self.next_view_id = view_id.checked_add(1).context("網頁識別碼已用盡")?;
        let view = browser::build(
            &self.window,
            browser::Page {
                address: sleeping.is_none().then_some(address.as_str()),
                view_id,
            },
            self.proxy.clone(),
            self.frame.width,
            self.frame.height,
            self.smoke,
            &self.user_agent,
        )?;
        if let Some(sleeping) = sleeping {
            // state 只可能来自同一进程的 interactionState，未经序列化或外部输入。
            unsafe { view.webview().setInteractionState(Some(&sleeping.state)) };
        }
        self.pads.insert(
            id,
            BrowserPad {
                view,
                view_id,
                sleep: SleepTimer::default(),
                title,
                address: address.clone(),
                pending: Some(address),
                load: LoadWatch::requested(),
                progress: 0.0,
                history: (false, false),
            },
        );
        self.sleeping.remove(&id);
        Ok(())
    }

    pub(super) fn sleep_wake(&self, now: Instant) -> Option<Instant> {
        self.pads
            .iter()
            .filter(|(id, _)| self.settings.active != Some(**id))
            .filter_map(|(_, pad)| pad.sleep.next_check(self.settings.sleep_after_minutes, now))
            .min()
    }

    pub(super) fn sleep_background(&mut self, now: Instant) -> Result<bool> {
        let mut snapshots = Vec::new();
        for (id, pad) in &self.pads {
            if self.settings.active == Some(*id)
                || !pad.sleep.due(self.settings.sleep_after_minutes, now)
                || pad.load.failure().is_some()
            {
                continue;
            }
            let native = pad.view.webview();
            // 不终止正在加载的页面；没有可恢复快照时保留原 WebView。
            if unsafe { native.isLoading() } || pad.load.is_loading() {
                continue;
            }
            let Some(address) = browser::current_url(&pad.view).filter(|url| web_url(url).is_ok())
            else {
                continue;
            };
            let Some(state) = (unsafe { native.interactionState() }) else {
                continue;
            };
            snapshots.push((
                *id,
                SleepingPad {
                    state,
                    address,
                    title: pad.title.clone(),
                },
            ));
        }
        let changed = !snapshots.is_empty();
        for (id, snapshot) in snapshots {
            self.sleeping.insert(id, snapshot);
            self.pads.remove(&id);
        }
        Ok(changed)
    }
}
