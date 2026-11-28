//! SettingsStore: durable atomic replacement; malformed user files are never overwritten.
use crate::timer::Database;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub struct SettingsStore {
    pub root: PathBuf,
    _lock: File,
}
impl SettingsStore {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(root.join("assets")).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("instance.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|_| {
            "Miku Focus уже запущен (хранилище занято). Откройте приложение через значок в области уведомлений.".to_string()
        })?;
        Ok(Self { root, _lock: lock })
    }
    pub fn load(&self) -> Result<Database, String> {
        let path = self.root.join("state.json");
        if !path.exists() {
            return Ok(Database::default());
        }
        let mut data: Database =
            serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| {
                format!(
                    "{} повреждён: {e}. Оригинал сохранён; восстановите state.backup.json вручную.",
                    path.display()
                )
            })?;
        if data.schema_version != 1 {
            return Err("Неподдерживаемая версия хранилища; файл не изменён".into());
        }
        data.settings.migrate_wallpapers();
        data.settings.validate()?;
        data.settings.migrate_sound_defaults();
        data.localize_builtin_categories();
        Ok(data)
    }
    pub fn save(&self, data: &Database) -> Result<(), String> {
        let bytes = serde_json::to_vec(data).map_err(|e| e.to_string())?;
        let path = self.root.join("state.json");
        if path.exists() {
            fs::copy(&path, self.root.join("state.backup.json")).map_err(|e| e.to_string())?;
        }
        atomic_write(&path, &bytes)
    }
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    // std::fs::rename atomically replaces an existing destination on Unix and Windows.
    fs::rename(&temp, path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    File::open(path.parent().ok_or("No parent")?)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_roundtrip_and_corruption_is_preserved() {
        let root = std::env::temp_dir().join(format!("miku-store-test-{}", std::process::id()));
        let store = SettingsStore::new(root.clone()).unwrap();
        store.save(&Database::default()).unwrap();
        store.save(&Database::default()).unwrap();
        assert!(root.join("state.backup.json").exists());
        assert_eq!(store.load().unwrap().schema_version, 1);
        fs::write(root.join("state.json"), b"not json").unwrap();
        assert!(store.load().is_err());
        assert_eq!(fs::read(root.join("state.json")).unwrap(), b"not json");
        drop(store);
        // Only the uniquely named directory created by this test is removed.
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_background_migration_and_theme_persist_without_touching_assets_or_history() {
        let root =
            std::env::temp_dir().join(format!("miku-appearance-store-{}", std::process::id()));
        let store = SettingsStore::new(root.clone()).unwrap();
        let image = root.join("assets/old.png");
        fs::write(&image, b"original image bytes").unwrap();
        let mut db = Database::default();
        db.action("start", 1000).unwrap();
        db.action("reset", 2000).unwrap();
        let mut legacy = serde_json::to_value(&db).unwrap();
        legacy["settings"]
            .as_object_mut()
            .unwrap()
            .remove("wallpapers");
        legacy["settings"].as_object_mut().unwrap().remove("theme");
        legacy["settings"]["background"] = serde_json::json!(image);
        fs::write(
            root.join("state.json"),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let mut loaded = store.load().unwrap();
        assert_eq!(loaded.settings.wallpapers.len(), 1);
        assert_eq!(loaded.settings.theme, crate::timer::Theme::Light);
        assert_eq!(loaded.sessions, db.sessions);
        loaded.settings.theme = crate::timer::Theme::Dark;
        store.save(&loaded).unwrap();
        let restored = store.load().unwrap();
        assert_eq!(restored.settings.theme, crate::timer::Theme::Dark);
        assert_eq!(restored.settings.wallpapers, loaded.settings.wallpapers);
        assert_eq!(fs::read(&image).unwrap(), b"original image bytes");
        assert!(root.join("state.backup.json").exists());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
