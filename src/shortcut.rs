use crate::storage::SaveOutcome;
use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shortcut {
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    pub command: bool,
    pub key: String,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            control: false,
            option: false,
            shift: true,
            command: true,
            key: "Space".to_owned(),
        }
    }
}

impl Shortcut {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.control || self.option || self.command,
            "快捷鍵須包含 ⌘ Command、⌃ Control 或 ⌥ Option"
        );
        let key = self.key.as_bytes();
        let supported = self.key.len() <= 6
            && (matches!(key, [b'K', b'e', b'y', b'A'..=b'Z'])
                || matches!(key, [b'D', b'i', b'g', b'i', b't', b'0'..=b'9'])
                || self.key == "Space"
                || self.key.strip_prefix('F').is_some_and(|number| {
                    number
                        .parse::<u8>()
                        .is_ok_and(|value| (1..=20).contains(&value) && number == value.to_string())
                }));
        ensure!(supported, "快捷鍵僅支援 A–Z、0–9、空白鍵與 F1–F20");
        let command_only = self.command && !self.control && !self.option;
        let reserved = command_only
            && ((!self.shift
                && (matches!(
                    self.key.as_str(),
                    "KeyQ"
                        | "KeyW"
                        | "KeyL"
                        | "KeyT"
                        | "KeyR"
                        | "KeyA"
                        | "KeyC"
                        | "KeyV"
                        | "KeyX"
                        | "KeyZ"
                ) || matches!(key, [b'D', b'i', b'g', b'i', b't', b'1'..=b'9'])))
                || (self.shift && self.key == "KeyZ"));
        ensure!(!reserved, "此組合已保留給 App 或文字編輯，請加入其他修飾鍵");
        Ok(())
    }

    pub fn accelerator(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        for (enabled, modifier) in [
            (self.control, "Control"),
            (self.option, "Alt"),
            (self.shift, "Shift"),
            (self.command, "Super"),
        ] {
            if enabled {
                parts.push(modifier);
            }
        }
        parts.push(&self.key);
        parts.join("+")
    }

    pub fn label(&self) -> String {
        let mut label = String::new();
        for (enabled, modifier) in [
            (self.control, '⌃'),
            (self.option, '⌥'),
            (self.shift, '⇧'),
            (self.command, '⌘'),
        ] {
            if enabled {
                label.push(modifier);
            }
        }
        label.push_str(
            self.key
                .strip_prefix("Key")
                .or_else(|| self.key.strip_prefix("Digit"))
                .unwrap_or(&self.key),
        );
        label
    }
}

pub trait ShortcutRegistry {
    fn register(&mut self, shortcut: &Shortcut) -> Result<()>;
    fn unregister(&mut self, shortcut: &Shortcut) -> Result<()>;
}

pub struct ShortcutBinding<R> {
    registry: R,
    active: Option<Shortcut>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ShortcutApplied {
    pub save: SaveOutcome,
    pub cleanup_warning: Option<String>,
}

impl<R: ShortcutRegistry> ShortcutBinding<R> {
    pub fn new(registry: R) -> Self {
        Self {
            registry,
            active: None,
        }
    }

    pub fn active(&self) -> Option<&Shortcut> {
        self.active.as_ref()
    }

    pub fn apply(
        &mut self,
        next: &Shortcut,
        persist: impl FnOnce() -> Result<SaveOutcome>,
    ) -> Result<ShortcutApplied> {
        next.validate()?;
        if self.active.as_ref() == Some(next) {
            return Ok(ShortcutApplied {
                save: persist()?,
                cleanup_warning: None,
            });
        }
        // 新組合註冊成功後才保存，避免衝突導致使用者失去原快捷鍵。
        self.registry
            .register(next)
            .with_context(|| format!("無法註冊快捷鍵 {}", next.label()))?;
        let save = match persist() {
            Ok(save) => save,
            Err(error) => {
                return match self.registry.unregister(next) {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(anyhow!(
                        "{error:#}；新快捷鍵 {} 解除失敗：{cleanup:#}",
                        next.label()
                    )),
                };
            }
        };
        // rename 後的同步警告仍代表已提交，活動快捷鍵必須跟隨磁碟設定。
        let previous = self.active.replace(next.clone());
        let cleanup_warning = previous.and_then(|previous| {
            self.registry.unregister(&previous).err().map(|error| {
                format!(
                    "快捷鍵已更新，但舊快捷鍵 {} 解除失敗：{error:#}",
                    previous.label()
                )
            })
        });
        Ok(ShortcutApplied {
            save,
            cleanup_warning,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn custom() -> Shortcut {
        Shortcut {
            control: true,
            option: true,
            shift: false,
            command: false,
            key: "KeyK".to_owned(),
        }
    }

    #[test]
    fn key_allowlist_and_modifier_requirements_are_enforced() {
        for key in (b'A'..=b'Z')
            .map(|key| format!("Key{}", key as char))
            .chain((0..=9).map(|key| format!("Digit{key}")))
            .chain((1..=20).map(|key| format!("F{key}")))
            .chain(["Space".to_owned()])
        {
            Shortcut { key, ..custom() }.validate().unwrap();
        }
        for key in [
            "",
            "keyA",
            "A",
            "Digit10",
            "F0",
            "F21",
            "F01",
            "Enter",
            "Comma",
            "BracketLeft",
            "KeyＡ",
            "KeyA\n",
        ] {
            assert!(
                Shortcut {
                    key: key.to_owned(),
                    ..custom()
                }
                .validate()
                .is_err(),
                "{key:?}"
            );
        }
        assert!(
            Shortcut {
                key: "x".repeat(8192),
                ..custom()
            }
            .validate()
            .is_err()
        );
        for shift in [false, true] {
            assert!(
                Shortcut {
                    control: false,
                    option: false,
                    shift,
                    command: false,
                    key: "Space".to_owned()
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn app_and_editing_shortcuts_are_reserved() {
        for key in [
            "KeyQ", "KeyW", "KeyL", "KeyT", "KeyR", "KeyA", "KeyC", "KeyV", "KeyX", "KeyZ",
            "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8",
            "Digit9",
        ] {
            let shortcut = Shortcut {
                control: false,
                option: false,
                shift: false,
                command: true,
                key: key.to_owned(),
            };
            assert!(shortcut.validate().is_err(), "{key}");
            Shortcut {
                option: true,
                ..shortcut
            }
            .validate()
            .unwrap();
        }
        assert!(
            Shortcut {
                key: "KeyZ".to_owned(),
                ..Shortcut::default()
            }
            .validate()
            .is_err()
        );
        Shortcut::default().validate().unwrap();
        Shortcut {
            key: "KeyT".to_owned(),
            ..Shortcut::default()
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn labels_and_accelerators_use_stable_native_modifiers() {
        assert_eq!(Shortcut::default().accelerator(), "Shift+Super+Space");
        assert_eq!(Shortcut::default().label(), "⇧⌘Space");
        assert_eq!(custom().accelerator(), "Control+Alt+KeyK");
        assert_eq!(custom().label(), "⌃⌥K");
        assert_eq!(
            Shortcut {
                key: "Digit9".to_owned(),
                ..custom()
            }
            .label(),
            "⌃⌥9"
        );
        assert_eq!(
            Shortcut {
                key: "F20".to_owned(),
                ..custom()
            }
            .label(),
            "⌃⌥F20"
        );
    }

    #[derive(Default)]
    struct RegistryState {
        events: Vec<String>,
        fail_register: bool,
        fail_unregister: bool,
    }

    struct Registry(Rc<RefCell<RegistryState>>);

    impl ShortcutRegistry for Registry {
        fn register(&mut self, shortcut: &Shortcut) -> Result<()> {
            let mut state = self.0.borrow_mut();
            state.events.push(format!("register:{}", shortcut.key));
            if state.fail_register {
                bail!("injected register conflict");
            }
            Ok(())
        }

        fn unregister(&mut self, shortcut: &Shortcut) -> Result<()> {
            let mut state = self.0.borrow_mut();
            state.events.push(format!("unregister:{}", shortcut.key));
            if state.fail_unregister {
                bail!("injected unregister failure");
            }
            Ok(())
        }
    }

    fn bound() -> (ShortcutBinding<Registry>, Rc<RefCell<RegistryState>>) {
        let state = Rc::new(RefCell::new(RegistryState::default()));
        let mut binding = ShortcutBinding::new(Registry(state.clone()));
        binding
            .apply(&Shortcut::default(), || Ok(SaveOutcome::Durable))
            .unwrap();
        state.borrow_mut().events.clear();
        (binding, state)
    }

    #[test]
    fn registration_conflict_never_persists_and_keeps_old_binding() {
        let (mut binding, state) = bound();
        state.borrow_mut().fail_register = true;
        let persisted = Cell::new(false);
        let result = binding.apply(&custom(), || {
            persisted.set(true);
            Ok(SaveOutcome::Durable)
        });
        assert!(result.is_err());
        assert!(!persisted.get());
        assert_eq!(binding.active(), Some(&Shortcut::default()));
        assert_eq!(state.borrow().events, ["register:KeyK"]);
    }

    #[test]
    fn failed_save_unregisters_new_binding_and_keeps_old_binding() {
        let (mut binding, state) = bound();
        let result = binding.apply(&custom(), || {
            state.borrow_mut().events.push("persist".to_owned());
            bail!("injected save failure")
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("injected save failure")
        );
        assert_eq!(binding.active(), Some(&Shortcut::default()));
        assert_eq!(
            state.borrow().events,
            ["register:KeyK", "persist", "unregister:KeyK"]
        );
    }

    #[test]
    fn successful_commit_switches_binding_after_save_then_releases_old() {
        let (mut binding, state) = bound();
        let applied = binding
            .apply(&custom(), || {
                state.borrow_mut().events.push("persist".to_owned());
                Ok(SaveOutcome::Durable)
            })
            .unwrap();
        assert_eq!(applied.save, SaveOutcome::Durable);
        assert!(applied.cleanup_warning.is_none());
        assert_eq!(binding.active(), Some(&custom()));
        assert_eq!(
            state.borrow().events,
            ["register:KeyK", "persist", "unregister:Space"]
        );
    }

    #[test]
    fn committed_save_warning_still_switches_to_new_binding() {
        let (mut binding, state) = bound();
        let applied = binding
            .apply(&custom(), || {
                Ok(SaveOutcome::CommittedWithWarning("sync warning".to_owned()))
            })
            .unwrap();
        assert_eq!(
            applied.save,
            SaveOutcome::CommittedWithWarning("sync warning".to_owned())
        );
        assert_eq!(binding.active(), Some(&custom()));
        assert_eq!(state.borrow().events, ["register:KeyK", "unregister:Space"]);
    }

    #[test]
    fn old_registration_cleanup_warning_does_not_revert_committed_binding() {
        let (mut binding, state) = bound();
        state.borrow_mut().fail_unregister = true;
        let applied = binding
            .apply(&custom(), || Ok(SaveOutcome::Durable))
            .unwrap();
        assert_eq!(binding.active(), Some(&custom()));
        assert!(
            applied
                .cleanup_warning
                .unwrap()
                .contains("injected unregister failure")
        );
        assert_eq!(state.borrow().events, ["register:KeyK", "unregister:Space"]);
    }

    #[test]
    fn rollback_cleanup_failure_reports_both_failures_and_keeps_old_binding() {
        let (mut binding, state) = bound();
        state.borrow_mut().fail_unregister = true;
        let error = binding
            .apply(&custom(), || bail!("injected save failure"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("injected save failure"));
        assert!(error.contains("injected unregister failure"));
        assert_eq!(binding.active(), Some(&Shortcut::default()));
    }

    #[test]
    fn same_binding_persists_without_re_registering() {
        let (mut binding, state) = bound();
        let persisted = Cell::new(false);
        binding
            .apply(&Shortcut::default(), || {
                persisted.set(true);
                Ok(SaveOutcome::Durable)
            })
            .unwrap();
        assert!(persisted.get());
        assert!(state.borrow().events.is_empty());
        assert_eq!(binding.active(), Some(&Shortcut::default()));
    }

    #[test]
    fn invalid_binding_never_registers_or_persists() {
        let (mut binding, state) = bound();
        let invalid = Shortcut {
            key: "Enter".to_owned(),
            ..custom()
        };
        let persisted = Cell::new(false);
        assert!(
            binding
                .apply(&invalid, || {
                    persisted.set(true);
                    Ok(SaveOutcome::Durable)
                })
                .is_err()
        );
        assert!(!persisted.get());
        assert!(state.borrow().events.is_empty());
        assert_eq!(binding.active(), Some(&Shortcut::default()));
    }
}
