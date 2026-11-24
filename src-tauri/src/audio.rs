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
pub fn migration_bindings_v2() -> BTreeMap<String, String> {
    serde_json::from_value(catalog()["migrationDefaultsV2"].clone())
        .expect("Invalid legacy sound bindings")
}
pub fn default_voices() -> BTreeMap<String, String> {
    static VOICES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    VOICES
        .get_or_init(|| {
            let catalog: Value =
                serde_json::from_str(include_str!("../../src/lib/voice-catalog.json"))
                    .expect("Invalid bundled voice catalogue");
            catalog["voices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|voice| {
                    (
                        voice["id"].as_str().unwrap().into(),
                        voice["file"].as_str().unwrap().into(),
                    )
                })
                .collect()
        })
        .clone()
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
        assert_eq!(default_bindings().len(), 20);
        assert_eq!(migration_bindings_v2().len(), 6);
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
        assert_eq!(restored.sound_defaults_version, 2);
    }
    #[test]
    fn upgrading_v1_adds_only_inner_menu_sound_and_keeps_existing_disabled_sounds_disabled() {
        let mut settings: crate::timer::Settings = serde_json::from_str(r#"{"soundDefaultsVersion":1,"sfxBindings":{"duration_up":"soft"},"quietMode":true,"sfxVolume":0.2}"#).unwrap();
        settings.migrate_sound_defaults();
        assert_eq!(settings.sfx_bindings.len(), 2);
        assert_eq!(settings.sfx_bindings["duration_up"], "soft");
        assert_eq!(settings.sfx_bindings["menu_choice"], "choice-hover");
        assert!(!settings.sfx_bindings.contains_key("choice_hover"));
        assert!(!settings.sfx_bindings.contains_key("menu_open"));
        assert!(settings.quiet_mode);
        assert_eq!(settings.sfx_volume, 0.2);
        settings.bind_sfx("menu_choice", None).unwrap();
        let mut restored: crate::timer::Settings =
            serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        restored.migrate_sound_defaults();
        assert!(!restored.sfx_bindings.contains_key("menu_choice"));
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
        assert_eq!(settings.voices, default_voices());
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
    #[test]
    fn fresh_install_contains_all_voices_and_laptop_effect_assignments() {
        let settings = crate::timer::Settings::default();
        assert_eq!(settings.voices.len(), 21);
        assert_eq!(settings.voices["goodbye"], "/voices/goodbye.wav");
        assert_eq!(settings.sfx_bindings.len(), 20);
        assert_eq!(settings.sfx_bindings["launch"], "game-menu-open");
        assert_eq!(settings.sfx_bindings["start"], "game-menu-close");
        assert_eq!(settings.volume, 0.7);
        assert_eq!(settings.sfx_volume, 0.5);
        assert!(settings
            .voices
            .iter()
            .all(|(id, path)| valid_id(id) && path == &format!("/voices/{id}.wav")));
        assert!(settings.validate().is_ok());
    }
    #[test]
    fn saved_empty_voices_and_disabled_events_stay_empty_after_upgrade_and_reload() {
        let mut settings: crate::timer::Settings = serde_json::from_str(
            r#"{"voices":{},"sfxBindings":{},"soundDefaultsVersion":2,"quietMode":true}"#,
        )
        .unwrap();
        settings.migrate_sound_defaults();
        assert!(settings.voices.is_empty() && settings.sfx_bindings.is_empty());
        assert!(settings.quiet_mode);
        let mut fresh = crate::timer::Settings::default();
        fresh.clear_sound("voice", "welcome").unwrap();
        fresh.bind_sfx("launch", None).unwrap();
        let restored: crate::timer::Settings =
            serde_json::from_value(serde_json::to_value(&fresh).unwrap()).unwrap();
        assert!(!restored.voices.contains_key("welcome"));
        assert!(!restored.sfx_bindings.contains_key("launch"));
    }
    #[test]
    fn explicit_builtin_set_preserves_imports_custom_assignments_and_other_settings() {
        let mut settings: crate::timer::Settings = serde_json::from_str(r#"{"voices":{"welcome":"/local/my-voice.wav"},"sfxBindings":{"start":"soft"},"soundDefaultsVersion":2,"quietMode":true,"volume":0.2,"sfxVolume":0.1,"focusMinutes":37}"#).unwrap();
        settings.use_builtin_audio(None).unwrap();
        assert_eq!(settings.voices.len(), 21);
        assert_eq!(settings.voices["welcome"], "/local/my-voice.wav");
        assert_eq!(settings.sfx_bindings["start"], "soft");
        assert_eq!(settings.sfx_bindings.len(), 20);
        assert!(settings.quiet_mode);
        assert_eq!(
            (settings.volume, settings.sfx_volume, settings.focus_minutes),
            (0.2, 0.1, 37)
        );
        settings.use_builtin_audio(Some("welcome")).unwrap();
        assert_eq!(settings.voices["welcome"], "/voices/welcome.wav");
        let before = serde_json::to_value(&settings).unwrap();
        assert!(settings.use_builtin_audio(Some("../private")).is_err());
        assert_eq!(serde_json::to_value(settings).unwrap(), before);
    }
    #[test]
    fn legacy_migration_does_not_enable_new_defaults_over_previous_choices() {
        let mut settings: crate::timer::Settings = serde_json::from_str(
            r#"{"voices":{},"sfxBindings":{"start":"soft"},"soundDefaultsVersion":0}"#,
        )
        .unwrap();
        settings.migrate_sound_defaults();
        assert_eq!(settings.sfx_bindings["start"], "soft");
        assert_eq!(settings.sfx_bindings.len(), 7);
        assert!(!settings.sfx_bindings.contains_key("launch"));
        assert!(settings.voices.is_empty());
    }
}
