use super::App;
use anyhow::{Context, Result};
use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};
use sliderust::{
    shortcut::{Shortcut, ShortcutRegistry},
    storage::SaveOutcome,
};

pub struct NativeShortcuts(pub GlobalHotKeyManager);

pub fn hotkey(shortcut: &Shortcut) -> Result<HotKey> {
    shortcut.validate()?;
    shortcut.accelerator().parse().context("快捷鍵格式不支援")
}

impl ShortcutRegistry for NativeShortcuts {
    fn register(&mut self, shortcut: &Shortcut) -> Result<()> {
        self.0
            .register(hotkey(shortcut)?)
            .context("無法註冊快捷鍵，可能已被其他 App 或系統使用，請換一組")
    }

    fn unregister(&mut self, shortcut: &Shortcut) -> Result<()> {
        self.0
            .unregister(hotkey(shortcut)?)
            .context("無法釋放舊快捷鍵，請重新啟動 Open Slide Pad")
    }
}

impl App {
    pub(super) fn active_shortcut_id(&self) -> Option<u32> {
        self.shortcut_binding
            .active()
            .and_then(|shortcut| hotkey(shortcut).ok())
            .map(|key| key.id())
    }

    pub(super) fn set_shortcut(&mut self, shortcut: Shortcut) -> Result<()> {
        let mut updated = self.settings.clone();
        updated.toggle_shortcut = shortcut.clone();
        let applied = self
            .shortcut_binding
            .apply(&shortcut, || self.store.save(&updated));
        match applied {
            Ok(applied) => {
                self.settings = updated;
                let mut warnings = Vec::new();
                if let SaveOutcome::CommittedWithWarning(warning) = applied.save {
                    warnings.push(warning);
                }
                if let Some(warning) = applied.cleanup_warning {
                    warnings.push(warning);
                }
                self.shortcut_error = (!warnings.is_empty()).then(|| warnings.join("；"));
                self.update_shortcut_tooltip();
                self.toast(
                    self.shortcut_error
                        .as_deref()
                        .unwrap_or("快捷鍵已套用，重新啟動後也會保留"),
                );
                Ok(())
            }
            Err(error) => {
                let message = format!("未變更快捷鍵：{error:#}");
                self.shortcut_error = Some(message.clone());
                self.render()?;
                Err(anyhow::anyhow!(message))
            }
        }
    }

    pub(super) fn update_shortcut_tooltip(&self) {
        let label = if self.shortcut_binding.active().is_some() {
            format!("Open Slide Pad · {}", self.settings.toggle_shortcut.label())
        } else {
            self.settings
                .language
                .text("Open Slide Pad · 快捷鍵未啟用，可從設定重新套用")
        };
        if let Err(error) = self._tray.set_tooltip(Some(label)) {
            eprintln!("無法更新選單列提示：{error}");
        }
    }
}
