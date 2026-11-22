use serde_json::Value;

/// Add missing rest/exit defaults in memory without overwriting a user's dialogue file.
pub fn with_rest_dialogues(mut data: Value) -> Value {
    let Some(entries) = data.get_mut("dialogues").and_then(Value::as_array_mut) else {
        return data;
    };
    for entry in entries.iter_mut() {
        if entry["event"] == "finish" {
            entry["event"] = "break_start".into();
        }
    }
    let defaults: Value = serde_json::from_str(include_str!("../../public/dialogues.json"))
        .expect("Bundled dialogues must be valid");
    for event in ["break_start", "break_touch", "break_skipped", "app_quit"] {
        if entries.iter().any(|d| d["event"] == event) {
            continue;
        }
        if let Some(default) = defaults["dialogues"]
            .as_array()
            .and_then(|ds| ds.iter().find(|d| d["event"] == event))
        {
            let mut entry = default.clone();
            let original = entry["id"].as_str().unwrap_or(event).to_string();
            let mut id = original.clone();
            let mut suffix = 0;
            while entries.iter().any(|d| d["id"].as_str() == Some(&id)) {
                suffix += 1;
                id = format!("{original}-{suffix}");
            }
            entry["id"] = id.into();
            entries.push(entry);
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserve_custom_lines_and_add_missing_rest_events() {
        let custom = serde_json::json!({"version":1,"dialogues":[{"id":"mine","event":"finish","lines":[{"ru":"Моя реплика","en":"My line","voice":"my_voice"}]}]});
        let result = with_rest_dialogues(custom.clone());
        let entries = result["dialogues"].as_array().unwrap();
        assert_eq!(entries[0]["event"], "break_start");
        assert_eq!(entries[0]["lines"], custom["dialogues"][0]["lines"]);
        for event in ["break_start", "break_touch", "break_skipped", "app_quit"] {
            assert_eq!(entries.iter().filter(|d| d["event"] == event).count(), 1);
        }
        assert_eq!(with_rest_dialogues(result.clone()), result);
    }
    #[test]
    fn preserve_custom_farewell_and_avoid_colliding_with_an_existing_id() {
        let custom = serde_json::json!({"version":1,"dialogues":[{"id":"my-exit","event":"app_quit","lines":[{"ru":"Моё прощание","en":"My goodbye","voice":"my_exit"}]}]});
        let result = with_rest_dialogues(custom.clone());
        assert_eq!(result["dialogues"][0], custom["dialogues"][0]);
        assert_eq!(
            result["dialogues"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|d| d["event"] == "app_quit")
                .count(),
            1
        );
        let collision = serde_json::json!({"version":1,"dialogues":[{"id":"goodbye","event":"touch","lines":[{"ru":"Моя строка","en":"My line"}]}]});
        let result = with_rest_dialogues(collision.clone());
        assert_eq!(result["dialogues"][0], collision["dialogues"][0]);
        let added = result["dialogues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["event"] == "app_quit")
            .unwrap();
        assert_eq!(added["id"], "goodbye-1");
        assert_eq!(added["lines"][0]["voice"], "goodbye");
    }
}
