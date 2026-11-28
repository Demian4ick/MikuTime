//! Bounded binary wallpaper imports. Original files and retained copies are never removed.
use serde::Deserialize;

pub const MAX_IMAGE: usize = 20 * 1024 * 1024;
pub const MAX_PREVIEW: usize = 256 * 1024;

pub fn builtins() -> &'static [crate::timer::Wallpaper] {
    static CATALOG: std::sync::OnceLock<Vec<crate::timer::Wallpaper>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/lib/wallpaper-catalog.json"))
            .expect("invalid bundled wallpaper catalogue")
    })
}

pub fn is_builtin_path(path: &str) -> bool {
    builtins().iter().any(|item| item.path == path)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Metadata {
    name: String,
    preview_length: usize,
}

pub struct Import<'a> {
    pub name: String,
    pub image: &'a [u8],
    pub preview: &'a [u8],
    pub extension: &'static str,
}

pub fn dimensions(bytes: &[u8]) -> Result<(u32, u32, &'static str), String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && bytes.get(12..16) == Some(b"IHDR")
        && bytes.len() >= 24
    {
        return Ok((
            u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
            u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
            "png",
        ));
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        let mut offset = 2;
        while offset < bytes.len() {
            if bytes[offset] != 0xff {
                break;
            }
            while bytes.get(offset) == Some(&0xff) {
                offset += 1;
            }
            let marker = *bytes.get(offset).ok_or("Повреждённый JPEG")?;
            offset += 1;
            if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
                continue;
            }
            if marker == 0xda || marker == 0xd9 {
                break;
            }
            let length = bytes.get(offset..offset + 2).ok_or("Повреждённый JPEG")?;
            let length = u16::from_be_bytes(length.try_into().unwrap()) as usize;
            if length < 2 || offset + length > bytes.len() {
                break;
            }
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                if length < 8 {
                    break;
                }
                let height =
                    u16::from_be_bytes(bytes[offset + 3..offset + 5].try_into().unwrap()) as u32;
                let width =
                    u16::from_be_bytes(bytes[offset + 5..offset + 7].try_into().unwrap()) as u32;
                return Ok((width, height, "jpg"));
            }
            offset += length;
        }
    }
    Err("Не удалось прочитать PNG/JPEG".into())
}

pub fn parse(bytes: &[u8]) -> Result<Import<'_>, String> {
    if bytes.len() > MAX_IMAGE + MAX_PREVIEW + 4096 + 4 {
        return Err("Файл обоев больше 20 МиБ".into());
    }
    let length = u32::from_le_bytes(
        bytes
            .get(..4)
            .ok_or("Нет данных обоев")?
            .try_into()
            .unwrap(),
    ) as usize;
    if length > 4096 {
        return Err("Слишком длинное описание обоев".into());
    }
    let metadata: Metadata =
        serde_json::from_slice(bytes.get(4..4 + length).ok_or("Нет описания обоев")?)
            .map_err(|_| "Неверное описание обоев")?;
    let name = metadata.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("Неверное название обоев".into());
    }
    if metadata.preview_length == 0 || metadata.preview_length > MAX_PREVIEW {
        return Err("Неверная миниатюра обоев".into());
    }
    let start = 4 + length;
    let end = start + metadata.preview_length;
    let preview = bytes.get(start..end).ok_or("Нет миниатюры обоев")?;
    let image = bytes.get(end..).ok_or("Нет изображения обоев")?;
    if image.is_empty() || image.len() > MAX_IMAGE {
        return Err("Файл обоев больше 20 МиБ или пуст".into());
    }
    let (width, height, extension) = dimensions(image)?;
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err("Изображение должно быть не больше 4096×4096".into());
    }
    let (width, height, ext) = dimensions(preview)?;
    if ext != "jpg" || width == 0 || height == 0 || width > 320 || height > 180 {
        return Err("Неверный размер миниатюры".into());
    }
    Ok(Import {
        name,
        image,
        preview,
        extension,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::{Database, Settings, SettingsPatch, Theme, Wallpaper};
    fn jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xc0, 0, 8, 8];
        bytes.extend(height.to_be_bytes());
        bytes.extend(width.to_be_bytes());
        bytes.push(1);
        bytes
    }
    fn payload(image: &[u8], preview: &[u8], name: &str) -> Vec<u8> {
        let metadata = serde_json::to_vec(
            &serde_json::json!({ "name": name, "previewLength": preview.len() }),
        )
        .unwrap();
        let mut bytes = (metadata.len() as u32).to_le_bytes().to_vec();
        bytes.extend(metadata);
        bytes.extend(preview);
        bytes.extend(image);
        bytes
    }
    fn wallpaper(id: &str) -> Wallpaper {
        Wallpaper {
            id: id.into(),
            name: "Город".into(),
            path: format!("/assets/{id}.jpg"),
            thumbnail: Some(format!("/assets/{id}-preview.jpg")),
        }
    }
    #[test]
    fn imports_are_bounded_and_keep_the_original_bytes() {
        let image = jpeg(1920, 1080);
        let preview = jpeg(320, 180);
        let bytes = payload(&image, &preview, "  Город  ");
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.name, "Город");
        assert_eq!(parsed.image, image);
        assert_eq!(parsed.preview, preview);
        assert_eq!(parsed.extension, "jpg");
        for bytes in [
            vec![],
            vec![0xff; 16],
            payload(&image, &preview, "\n"),
            payload(&image, &preview, "bad\nname"),
            payload(&jpeg(4097, 10), &preview, "Город"),
            payload(&image, &jpeg(321, 180), "Город"),
            payload(&image, &vec![0; MAX_PREVIEW + 1], "Город"),
            payload(&vec![0; MAX_IMAGE + 1], &preview, "Город"),
        ] {
            assert!(parse(&bytes).is_err());
        }
    }
    #[test]
    fn malformed_image_headers_cannot_panic_or_escape_dimension_limits() {
        for length in 0..30 {
            assert!(dimensions(&vec![0xff; length]).is_err());
        }
        assert!(dimensions(b"\xff\xd8\xff\xe0\x00\x01").is_err());
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend(1920u32.to_be_bytes());
        png.extend(1080u32.to_be_bytes());
        assert_eq!(dimensions(&png).unwrap(), (1920, 1080, "png"));
        for end in 0..png.len() {
            assert!(dimensions(&png[..end]).is_err());
        }
    }
    #[test]
    fn legacy_background_is_retained_once_and_theme_defaults_to_light() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"background":"/old/background.png","voices":{}}"#).unwrap();
        assert_eq!(settings.theme, Theme::Light);
        settings.migrate_wallpapers();
        settings.migrate_wallpapers();
        settings.validate().unwrap();
        assert_eq!(settings.wallpapers.len(), 1);
        assert_eq!(settings.wallpapers[0].path, "/old/background.png");
        assert_eq!(settings.background.as_deref(), Some("/old/background.png"));
    }
    #[test]
    fn selecting_and_removing_wallpapers_preserves_the_timer_history_and_sound_maps() {
        let mut db = Database::default();
        db.action("start", 1000).unwrap();
        db.action("pause", 2000).unwrap();
        let before = serde_json::to_value(&db).unwrap();
        db.settings.add_wallpaper(wallpaper("one")).unwrap();
        db.settings.add_wallpaper(wallpaper("two")).unwrap();
        db.settings.select_wallpaper(Some("one")).unwrap();
        assert_eq!(db.settings.background.as_deref(), Some("/assets/one.jpg"));
        db.settings.remove_wallpaper("two").unwrap();
        assert_eq!(db.settings.background.as_deref(), Some("/assets/one.jpg"));
        db.update_settings(SettingsPatch {
            theme: Some(Theme::Dark),
            ..Default::default()
        })
        .unwrap();
        db.settings.remove_wallpaper("one").unwrap();
        assert!(db.settings.background.is_none());
        let after = serde_json::to_value(&db).unwrap();
        for field in ["timer", "sessions", "breaks", "categories"] {
            assert_eq!(before[field], after[field]);
        }
        for field in ["voices", "sfx", "sfxBindings", "volume", "sfxVolume"] {
            assert_eq!(before["settings"][field], after["settings"][field]);
        }
        let restored: Database = serde_json::from_value(after).unwrap();
        assert_eq!(restored.settings.theme, Theme::Dark);
    }
    #[test]
    fn unknown_selection_duplicate_ids_and_capacity_fail_without_mutation() {
        let mut settings = Settings::default();
        settings.add_wallpaper(wallpaper("one")).unwrap();
        let before = serde_json::to_value(&settings).unwrap();
        assert!(settings.select_wallpaper(Some("../other")).is_err());
        assert!(settings.remove_wallpaper("unknown").is_err());
        assert!(settings.add_wallpaper(wallpaper("one")).is_err());
        assert_eq!(serde_json::to_value(&settings).unwrap(), before);
        for index in 1..100 {
            settings
                .add_wallpaper(wallpaper(&format!("wall-{index}")))
                .unwrap();
        }
        assert!(settings.add_wallpaper(wallpaper("extra")).is_err());
        settings.remove_wallpaper("one").unwrap();
        settings.add_wallpaper(wallpaper("extra")).unwrap();
    }

    #[test]
    fn builtins_are_available_to_fresh_and_existing_profiles_without_import_or_migration() {
        assert_eq!(builtins().len(), 9);
        let mut settings = Settings::default();
        for builtin in builtins() {
            assert!(crate::audio::valid_id(&builtin.id));
            assert!(builtin.path.starts_with("/wallpapers/"));
            assert!(builtin
                .thumbnail
                .as_ref()
                .unwrap()
                .starts_with("/wallpapers/previews/"));
            settings.select_wallpaper(Some(&builtin.id)).unwrap();
            assert_eq!(settings.background.as_ref(), Some(&builtin.path));
            let mut restored: Settings =
                serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
            restored.migrate_wallpapers();
            restored.validate().unwrap();
            assert_eq!(restored.background.as_ref(), Some(&builtin.path));
            assert!(restored.wallpapers.is_empty());
            assert!(settings.remove_wallpaper(&builtin.id).is_err());
            assert!(settings.add_wallpaper(builtin.clone()).is_err());
        }
        settings.add_wallpaper(wallpaper("personal")).unwrap();
        settings.select_wallpaper(Some(&builtins()[0].id)).unwrap();
        settings.remove_wallpaper("personal").unwrap();
        assert_eq!(settings.background.as_ref(), Some(&builtins()[0].path));
        let before = serde_json::to_value(&settings).unwrap();
        for id in [
            "builtin-unknown",
            "/wallpapers/night-stars.jpg",
            "../wallpapers/night-stars.jpg",
        ] {
            assert!(settings.select_wallpaper(Some(id)).is_err());
            assert_eq!(serde_json::to_value(&settings).unwrap(), before);
        }
        for index in 0..100 {
            settings
                .add_wallpaper(wallpaper(&format!("personal-{index}")))
                .unwrap();
        }
        settings.select_wallpaper(Some(&builtins()[0].id)).unwrap();
        assert_eq!(settings.wallpapers.len(), 100);
        assert_eq!(settings.background.as_ref(), Some(&builtins()[0].path));
        settings.select_wallpaper(None).unwrap();
        assert!(settings.background.is_none());
    }
}
