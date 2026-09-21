use crate::model::Settings;
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_SETTINGS_BYTES: u64 = 256 * 1024;

/// 持有整個程序生命週期的鎖；保存提交後才允許 UI 提交新狀態。
pub struct SettingsStore {
    root: PathBuf,
    _lock: File,
}

/// 兩種結果均表示設定已取代原檔；警告代表無法確認目錄資訊已同步到磁碟。
#[derive(Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Durable,
    CommittedWithWarning(String),
}

impl SettingsStore {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root).context("無法建立設定目錄")?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(root.join("instance.lock"))?;
        lock.try_lock_exclusive()
            .context("Open Slide Pad 已在執行，請使用選單列圖示或已設定的快捷鍵")?;
        Ok(Self {
            root: root.to_owned(),
            _lock: lock,
        })
    }

    pub fn load(&self) -> Result<Settings> {
        let path = self.root.join("settings.json");
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Settings::default());
            }
            Err(error) => return Err(error).context("無法讀取設定，原檔已保留"),
        };
        let mut bytes = Vec::new();
        file.take(MAX_SETTINGS_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("無法讀取設定，原檔已保留")?;
        ensure!(
            bytes.len() as u64 <= MAX_SETTINGS_BYTES,
            "設定檔過大，原檔已保留"
        );
        let settings: Settings =
            serde_json::from_slice(&bytes).context("設定檔損毀，原檔已保留")?;
        settings.validate()?;
        Ok(settings)
    }

    /// 原子提交設定；Err 保留原檔，兩種 Ok 結果都必須同步呼叫端的記憶體狀態。
    pub fn save(&self, settings: &Settings) -> Result<SaveOutcome> {
        self.save_with_directory_sync(settings, |root| File::open(root)?.sync_all())
    }

    /// 把無法載入的設定檔改名留存，讓 App 能以預設值啟動；回傳備份路徑。
    /// 只改名：不覆寫、不刪除任何檔案。以奈秒時間戳命名，避免與先前的備份同名。
    pub fn quarantine(&self) -> Result<PathBuf> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        self.quarantine_as(&stamp.to_string())
    }

    fn quarantine_as(&self, suffix: &str) -> Result<PathBuf> {
        let backup = self.root.join(format!("settings.json.broken-{suffix}"));
        // rename 會取代既有目標；程序鎖已排除第二個 App，這裡再擋同名備份。
        ensure!(!backup.exists(), "備份檔名已存在");
        fs::rename(self.root.join("settings.json"), &backup).context("無法備份設定檔")?;
        Ok(backup)
    }

    fn save_with_directory_sync(
        &self,
        settings: &Settings,
        sync_directory: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<SaveOutcome> {
        settings.validate()?;
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        file.write_all(&serde_json::to_vec_pretty(settings)?)?;
        file.as_file().sync_all()?;
        file.persist(self.root.join("settings.json"))
            .context("無法保存設定")?;
        match sync_directory(&self.root) {
            Ok(()) => Ok(SaveOutcome::Durable),
            Err(error) => Ok(SaveOutcome::CommittedWithWarning(format!(
                "設定已儲存，但無法確認磁碟同步完成：{error}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    #[test]
    fn language_defaults_to_english_and_round_trips_without_changing_existing_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::open(dir.path()).unwrap();
        assert_eq!(store.load().unwrap().language, Language::English);
        // 英文不增加欄位，因此此檔也代表尚未選過語言的 version 1 設定。
        let original = Settings::default()
            .add("example.com")
            .unwrap()
            .rename(1, "我的網站")
            .unwrap();
        store.save(&original).unwrap();
        let legacy = fs::read(&path).unwrap();
        assert!(
            serde_json::from_slice::<serde_json::Value>(&legacy)
                .unwrap()
                .get("language")
                .is_none()
        );
        assert_eq!(store.load().unwrap(), original);
        assert_eq!(fs::read(&path).unwrap(), legacy, "讀取舊設定不應改寫檔案");
        let chinese = Settings {
            language: Language::TraditionalChinese,
            ..original.clone()
        };
        store.save(&chinese).unwrap();
        drop(store);
        let reopened = SettingsStore::open(dir.path()).unwrap();
        assert_eq!(reopened.load().unwrap(), chinese);
        reopened.save(&original).unwrap();
        drop(reopened);
        assert_eq!(
            SettingsStore::open(dir.path()).unwrap().load().unwrap(),
            original
        );
        assert_eq!(fs::read(&path).unwrap(), legacy);
    }

    #[test]
    fn unknown_language_preserves_source_and_failed_save_preserves_previous_language() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::open(dir.path()).unwrap();
        let original = Settings {
            language: Language::TraditionalChinese,
            ..Settings::default()
        };
        store.save(&original).unwrap();
        let saved = fs::read(&path).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = store.save(&Settings {
            language: Language::English,
            ..original.clone()
        });
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(store.load().unwrap(), original);
        assert_eq!(fs::read(&path).unwrap(), saved);
        for invalid in [
            serde_json::json!("fr"),
            serde_json::Value::Null,
            serde_json::json!(1),
        ] {
            let mut value = serde_json::to_value(&original).unwrap();
            value["language"] = invalid;
            let bytes = serde_json::to_vec(&value).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert!(store.load().is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn fields_written_by_a_newer_version_survive_load_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::open(dir.path()).unwrap();
        let mut newer = serde_json::to_value(Settings::default().add("a.com").unwrap()).unwrap();
        newer["profiles"] = serde_json::json!([{"name": "work"}]);
        fs::write(&path, serde_json::to_vec(&newer).unwrap()).unwrap();
        let loaded = store.load().unwrap();
        store.save(&loaded.add("b.com").unwrap()).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["profiles"], newer["profiles"]);
        assert_eq!(saved["pads"].as_array().unwrap().len(), 2);
        assert_eq!(saved["version"], 1);
    }

    #[test]
    fn quarantine_renames_unreadable_settings_and_never_overwrites_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::open(dir.path()).unwrap();
        for broken in [&b"{broken"[..], br#"{"version":2}"#] {
            fs::write(&path, broken).unwrap();
            assert!(store.load().is_err());
            let backup = store.quarantine().unwrap();
            assert_eq!(backup.parent(), Some(dir.path()));
            assert!(
                backup
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.json.broken-")
            );
            assert_eq!(fs::read(&backup).unwrap(), broken, "備份須與原檔位元組相同");
            assert!(!path.exists());
            assert_eq!(store.load().unwrap(), Settings::default());
        }
        // 檔名相同的備份已存在時必須拒絕，兩個檔案都不得變動。
        fs::write(&path, b"first").unwrap();
        let first = store.quarantine_as("same").unwrap();
        fs::write(&path, b"second").unwrap();
        assert!(store.quarantine_as("same").is_err());
        assert_eq!(fs::read(&first).unwrap(), b"first");
        assert_eq!(fs::read(&path).unwrap(), b"second");
        // 沒有設定檔可備份時回報錯誤，不建立任何檔案。
        fs::remove_file(&path).unwrap();
        assert!(store.quarantine_as("missing").is_err());
        assert!(!dir.path().join("settings.json.broken-missing").exists());
    }

    #[test]
    fn resized_panel_survives_restart_and_invalid_dimensions_preserve_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path()).unwrap();
        let settings = Settings {
            width: 680.0,
            height: Some(540.0),
            top_offset: 60.0,
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        let saved = fs::read(dir.path().join("settings.json")).unwrap();
        for invalid in [
            Settings {
                height: Some(319.0),
                ..settings.clone()
            },
            Settings {
                height: Some(f64::INFINITY),
                ..settings.clone()
            },
            Settings {
                top_offset: -1.0,
                ..settings.clone()
            },
            Settings {
                top_offset: f64::NAN,
                ..settings.clone()
            },
        ] {
            assert!(store.save(&invalid).is_err());
            assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), saved);
        }
        drop(store);
        let restored = SettingsStore::open(dir.path()).unwrap().load().unwrap();
        assert_eq!(restored, settings);
        assert_eq!(restored.version, 1);
    }

    #[test]
    fn custom_shortcut_survives_restart_and_invalid_shortcut_preserves_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path()).unwrap();
        let mut settings = Settings::default();
        settings.toggle_shortcut.option = true;
        settings.toggle_shortcut.command = false;
        settings.toggle_shortcut.key = "F12".to_owned();
        store.save(&settings).unwrap();
        let saved = fs::read(dir.path().join("settings.json")).unwrap();
        let mut invalid = settings.clone();
        invalid.toggle_shortcut.key = "Enter".to_owned();
        assert!(store.save(&invalid).is_err());
        assert_eq!(fs::read(dir.path().join("settings.json")).unwrap(), saved);
        drop(store);
        assert_eq!(
            SettingsStore::open(dir.path()).unwrap().load().unwrap(),
            settings
        );
    }

    #[test]
    fn renamed_reordered_and_restored_pads_survive_restart_without_schema_changes() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path()).unwrap();
        let original = Settings::default()
            .add("a.com")
            .unwrap()
            .add("b.com")
            .unwrap()
            .add("c.com")
            .unwrap();
        let removed = original.pads[1].clone();
        let changed = original
            .rename(1, "　我的網站 🦀  ")
            .unwrap()
            .move_pad(3, 0)
            .unwrap()
            .remove(2)
            .unwrap()
            .restore_pad(removed, 1)
            .unwrap();
        assert_eq!(store.save(&changed).unwrap(), SaveOutcome::Durable);
        drop(store);
        let loaded = SettingsStore::open(dir.path()).unwrap().load().unwrap();
        assert_eq!(loaded, changed);
        assert_eq!(loaded.version, 1);
        assert_eq!(
            loaded.pads.iter().map(|pad| pad.id).collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert_eq!(loaded.active, Some(2));
        assert_eq!(loaded.next_id, 4);
    }

    #[test]
    fn precommit_io_failure_preserves_previous_settings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("settings");
        let store = SettingsStore::open(&root).unwrap();
        let original = Settings::default().add("a.com").unwrap();
        store.save(&original).unwrap();
        let bytes = fs::read(root.join("settings.json")).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
        let failure = store.save(&original.add("b.com").unwrap());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(failure.is_err(), "唯讀目錄必須在提交前拒絕寫入");
        assert_eq!(fs::read(root.join("settings.json")).unwrap(), bytes);
        assert_eq!(store.load().unwrap(), original);
    }

    #[test]
    fn directory_sync_failure_reports_committed_warning_and_keeps_new_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path()).unwrap();
        let original = Settings::default().add("a.com").unwrap();
        store.save(&original).unwrap();
        let changed = original.add("b.com").unwrap();
        let outcome = store
            .save_with_directory_sync(&changed, |_| {
                Err(std::io::Error::other("injected directory sync failure"))
            })
            .expect("rename 成功後不得回傳未提交的錯誤");
        assert!(
            matches!(outcome, SaveOutcome::CommittedWithWarning(message) if message.contains("injected directory sync failure"))
        );
        drop(store);
        assert_eq!(
            SettingsStore::open(dir.path()).unwrap().load().unwrap(),
            changed
        );
    }

    #[test]
    fn settings_size_limit_accepts_boundary_and_preserves_oversized_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path()).unwrap();
        let path = dir.path().join("settings.json");
        let original = Settings::default();
        let mut bytes = serde_json::to_vec(&original).unwrap();
        bytes.resize(MAX_SETTINGS_BYTES as usize, b' ');
        fs::write(&path, &bytes).unwrap();
        assert_eq!(store.load().unwrap(), original);
        bytes.push(b' ');
        fs::write(&path, &bytes).unwrap();
        assert!(store.load().unwrap_err().to_string().contains("設定檔過大"));
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn persists_settings_across_restart_and_rejects_second_writer() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path()).unwrap();
        let settings = Settings::default().add("rust-lang.org").unwrap();
        store.save(&settings).unwrap();
        assert!(SettingsStore::open(dir.path()).is_err());
        drop(store);
        let reopened = SettingsStore::open(dir.path()).unwrap();
        assert_eq!(reopened.load().unwrap(), settings);
    }
    #[test]
    fn corruption_is_reported_without_overwriting_user_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"{broken").unwrap();
        let store = SettingsStore::open(dir.path()).unwrap();
        assert!(store.load().is_err());
        assert_eq!(fs::read(path).unwrap(), b"{broken");
    }
    #[test]
    fn rejected_settings_preserve_the_previous_saved_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(dir.path()).unwrap();
        let original = Settings::default();
        store.save(&original).unwrap();
        let invalid = Settings {
            width: -10.0,
            ..original.clone()
        };
        assert!(store.save(&invalid).is_err());
        assert_eq!(store.load().unwrap(), original);
    }
}
