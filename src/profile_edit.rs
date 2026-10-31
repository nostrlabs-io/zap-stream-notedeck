use nostr::serde_json::{self, Map, Value};

#[derive(Clone, Default, PartialEq, Debug)]
pub struct ProfileForm {
    pub name: String,
    pub about: String,
    pub nip05: String,
    pub lud16: String,
    pub picture: String,
}

const FIELDS: [&str; 5] = ["name", "about", "nip05", "lud16", "picture"];

fn text(map: &Map<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn parse(content: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(content) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

impl ProfileForm {
    pub fn from_content(content: &str) -> Self {
        let map = parse(content);
        let name = if map.contains_key("name") {
            text(&map, "name")
        } else {
            text(&map, "display_name")
        };
        Self {
            name,
            about: text(&map, "about"),
            nip05: text(&map, "nip05"),
            lud16: text(&map, "lud16"),
            picture: text(&map, "picture"),
        }
    }

    fn value(&self, key: &str) -> &str {
        match key {
            "name" => &self.name,
            "about" => &self.about,
            "nip05" => &self.nip05,
            "lud16" => &self.lud16,
            _ => &self.picture,
        }
    }

    pub fn to_content(&self, existing: &str) -> String {
        let mut map = parse(existing);
        for key in FIELDS {
            let v = self.value(key).trim();
            if v.is_empty() {
                map.remove(key);
            } else {
                map.insert(key.to_string(), Value::String(v.to_string()));
            }
        }
        if map.contains_key("display_name") {
            let v = self.name.trim();
            if v.is_empty() {
                map.remove("display_name");
            } else {
                map.insert("display_name".to_string(), Value::String(v.to_string()));
            }
        }
        Value::Object(map).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_unknown_fields() {
        let existing = r#"{"name":"a","banner":"https://x/b.png","website":"https://w"}"#;
        let form = ProfileForm {
            name: "b".into(),
            ..ProfileForm::from_content(existing)
        };
        let out: Value = serde_json::from_str(&form.to_content(existing)).unwrap();
        assert_eq!(out["name"], "b");
        assert_eq!(out["banner"], "https://x/b.png");
        assert_eq!(out["website"], "https://w");
    }

    #[test]
    fn empty_field_is_removed() {
        let existing = r#"{"name":"a","about":"hi"}"#;
        let mut form = ProfileForm::from_content(existing);
        form.about = "  ".into();
        let out: Value = serde_json::from_str(&form.to_content(existing)).unwrap();
        assert!(out.get("about").is_none());
        assert_eq!(out["name"], "a");
    }

    #[test]
    fn display_name_follows_name() {
        let existing = r#"{"name":"a","display_name":"A"}"#;
        let mut form = ProfileForm::from_content(existing);
        form.name = "c".into();
        let out: Value = serde_json::from_str(&form.to_content(existing)).unwrap();
        assert_eq!(out["display_name"], "c");
    }

    #[test]
    fn invalid_existing_content_starts_fresh() {
        let form = ProfileForm {
            name: "n".into(),
            lud16: "a@b.c".into(),
            ..Default::default()
        };
        let out: Value = serde_json::from_str(&form.to_content("not json")).unwrap();
        assert_eq!(out["lud16"], "a@b.c");
    }

    #[test]
    fn round_trip() {
        let form = ProfileForm {
            name: "n".into(),
            about: "x".into(),
            nip05: "n@b.c".into(),
            lud16: "n@b.c".into(),
            picture: "https://p".into(),
        };
        assert_eq!(ProfileForm::from_content(&form.to_content("")), form);
    }
}
