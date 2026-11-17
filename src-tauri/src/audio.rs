//! Shared audio catalogue; paths can only be changed through bounded local asset import.
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

pub fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/lib/sound-catalog.json"))
            .expect("Invalid bundled sound catalogue")
    })
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub fn builtin(id: &str) -> bool {
    catalog()["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"].as_str() == Some(id))
}
pub fn event(id: &str) -> bool {
    catalog()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"].as_str() == Some(id))
}
pub fn valid_bindings(
    bindings: &BTreeMap<String, String>,
    sounds: &BTreeMap<String, String>,
) -> bool {
    bindings
        .iter()
        .all(|(key, value)| event(key) && (builtin(value) || sounds.contains_key(value)))
}
pub fn default_bindings() -> BTreeMap<String, String> {
    serde_json::from_value(catalog()["defaults"].clone()).expect("Invalid default sound bindings")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogue_has_nine_distinct_local_effects_and_approved_defaults() {
        let effects = catalog()["effects"].as_array().unwrap();
        assert_eq!(effects.len(), 9);
        let mut ids = std::collections::BTreeSet::new();
        for effect in effects {
            let id = effect["id"].as_str().unwrap();
            assert!(valid_id(id) && ids.insert(id));
            assert!(effect["file"].as_str().unwrap().starts_with("/sfx/"));
        }
        assert!(event("focus_complete"));
        assert!(!event("arbitrary_command"));
        assert!(!valid_id("../bad"));
        assert!(valid_bindings(&default_bindings(), &BTreeMap::new()));
        assert_eq!(default_bindings().len(), 5);
    }
    #[test]
    fn legacy_settings_add_approved_audio_defaults_and_keep_existing_voices() {
        let mut settings: crate::timer::Settings =
            serde_json::from_str(r#"{"focusMinutes":30,"voices":{"hello":"/local/hello.wav"}}"#)
                .unwrap();
        settings.migrate_sound_defaults();
        assert!(settings.sfx.is_empty());
        assert_eq!(settings.sfx_bindings, default_bindings());
        assert_eq!(settings.sfx_volume, 0.5);
        assert_eq!(settings.voices["hello"], "/local/hello.wav");
        assert!(settings.validate().is_ok());
    }
    #[test]
    fn approved_migration_is_once_only_and_preserves_unrelated_choices_and_later_silence() {
        let mut settings: crate::timer::Settings =
            serde_json::from_str(r#"{"sfxBindings":{"menu_open":"soft","start":"soft"}}"#).unwrap();
        assert_eq!(settings.sound_defaults_version, 0);
        settings.migrate_sound_defaults();
        assert_eq!(settings.sfx_bindings["menu_open"], "menu-open");
        assert_eq!(settings.sfx_bindings["start"], "soft");
        assert_eq!(settings.sfx_bindings["choice_hover"], "menu-hover");
        settings.bind_sfx("choice_hover", None).unwrap();
        let mut restored: crate::timer::Settings =
            serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        restored.migrate_sound_defaults();
        assert!(!restored.sfx_bindings.contains_key("choice_hover"));
        assert_eq!(restored.sound_defaults_version, 1);
    }
    #[test]
    fn individual_bindings_are_atomic_and_unknown_events_or_sounds_do_not_mutate_settings() {
        let mut settings = crate::timer::Settings::default();
        settings.bind_sfx("start", Some("high".into())).unwrap();
        settings.bind_sfx("pause", Some("low".into())).unwrap();
        let before = settings.sfx_bindings.clone();
        assert!(settings.bind_sfx("unknown", Some("high".into())).is_err());
        assert!(settings.bind_sfx("start", Some("unknown".into())).is_err());
        assert_eq!(settings.sfx_bindings, before);
        settings.bind_sfx("pause", None).unwrap();
        assert_eq!(settings.sfx_bindings["start"], "high");
        assert!(!settings.sfx_bindings.contains_key("pause"));
    }
    #[test]
    fn removing_custom_sound_clears_dangling_bindings_but_builtin_reset_keeps_fallback() {
        let mut settings = crate::timer::Settings::default();
        settings
            .sfx
            .insert("custom".into(), "/not-deleted.ogg".into());
        settings
            .sfx
            .insert("high".into(), "/replacement.wav".into());
        settings.voices.insert("hello".into(), "/voice.wav".into());
        settings.bind_sfx("start", Some("custom".into())).unwrap();
        settings.bind_sfx("pause", Some("high".into())).unwrap();
        settings.clear_sound("sfx", "custom").unwrap();
        assert!(!settings.sfx_bindings.contains_key("start"));
        settings.clear_sound("sfx", "high").unwrap();
        assert_eq!(settings.sfx_bindings["pause"], "high");
        assert!(settings.sfx.is_empty());
        settings.clear_sound("voice", "hello").unwrap();
        assert!(settings.voices.is_empty());
        assert!(settings.clear_sound("bad", "high").is_err());
        assert!(settings.validate().is_ok());
    }
    #[test]
    fn sound_volume_patch_preserves_bindings_and_current_timer_accounting() {
        let mut db = crate::timer::Database::default();
        db.action("start", 1_000).unwrap();
        db.settings.bind_sfx("start", Some("high".into())).unwrap();
        let timer = serde_json::to_value(&db.timer).unwrap();
        db.update_settings(crate::timer::SettingsPatch {
            sfx_volume: Some(0.0),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(db.settings.volume, 0.7);
        assert_eq!(db.settings.sfx_volume, 0.0);
        assert_eq!(db.settings.sfx_bindings["start"], "high");
        assert_eq!(serde_json::to_value(&db.timer).unwrap(), timer);
        assert!(db
            .update_settings(crate::timer::SettingsPatch {
                sfx_volume: Some(f64::NAN),
                ..Default::default()
            })
            .is_err());
        assert_eq!(db.settings.sfx_volume, 0.0);
    }
}
