use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Write, path::Path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skin {
    pub value: String,
    pub signature: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub username: Option<String>,
    pub skin: Skin,
    pub moderated: Option<String>,
}
pub type Choices = BTreeMap<String, Choice>;
pub fn name(s: &str) -> Result<String, String> {
    if s.is_empty() || s.len() > 16 || !s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
        return Err("Use a Minecraft username (1-16 letters, numbers or underscores). Image URLs are not supported.".into());
    }
    Ok(s.to_ascii_lowercase())
}
pub fn validate(s: &Skin) -> Result<(), String> {
    if s.value.len() > 16384 || s.signature.is_empty() || s.signature.len() > 4096 {
        return Err("Invalid signed texture".into());
    }
    let decoded = STANDARD
        .decode(&s.value)
        .map_err(|_| "Invalid texture encoding")?;
    let body: serde_json::Value =
        serde_json::from_slice(&decoded).map_err(|_| "Invalid texture JSON")?;
    let url = body["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("Missing texture URL")?;
    let hash = url
        .strip_prefix("https://textures.minecraft.net/texture/")
        .or_else(|| url.strip_prefix("http://textures.minecraft.net/texture/"))
        .ok_or("Invalid texture host")?;
    if !(32..=64).contains(&hash.len())
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid texture path".into());
    }
    Ok(())
}
pub fn default_skin(model: &str) -> Result<Skin, String> {
    let mut defaults: BTreeMap<String, Skin> =
        serde_json::from_str(include_str!("../resources/defaults.json"))
            .map_err(|e| e.to_string())?;
    let skin = defaults.remove(model).ok_or("Choose steve or alex")?;
    validate(&skin)?;
    Ok(skin)
}
pub fn load(path: &Path) -> Result<Choices, String> {
    match fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Choices::new()),
        Err(e) => return Err(e.to_string()),
        Ok(m) if m.len() > 16 * 1024 * 1024 => return Err("Skin store is too large".into()),
        _ => {}
    }
    let choices: Choices = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Invalid skins.json; file preserved: {e}"))?;
    if choices.len() > 4096 {
        return Err("Skin storage limit reached".into());
    }
    for (key, choice) in &choices {
        if name(key)? != *key {
            return Err("Skin store keys must be lowercase".into());
        }
        validate(&choice.skin)?;
        if let Some(account) = &choice.username {
            name(account)?;
        }
        if let Some(model) = &choice.moderated {
            default_skin(model)?;
        }
    }
    Ok(choices)
}
pub fn save(path: &Path, choices: &Choices) -> Result<(), String> {
    if choices.len() > 4096 {
        return Err("Skin storage limit reached".into());
    }
    let parent = path.parent().ok_or("Invalid skin data path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(choices).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}
pub fn profile_id(bytes: &[u8]) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let id = v["id"].as_str().ok_or("Missing profile ID")?;
    if id.len() != 32 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid profile ID".into());
    }
    Ok(id.to_owned())
}
pub fn session_skin(bytes: &[u8]) -> Result<Option<Skin>, String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let props = v["properties"]
        .as_array()
        .ok_or("Missing profile properties")?;
    for prop in props {
        if prop["name"] == "textures" {
            let skin = Skin {
                value: prop["value"].as_str().ok_or("Missing texture")?.into(),
                signature: prop["signature"]
                    .as_str()
                    .ok_or("Missing signature")?
                    .into(),
            };
            validate(&skin)?;
            return Ok(Some(skin));
        }
    }
    Ok(None)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_urls() {
        assert_eq!(name("ExamplePlayer").unwrap(), "exampleplayer");
        for n in [
            "",
            "../ops",
            "a b",
            "https://example.com",
            "abcdefghijklmnopq",
        ] {
            assert!(name(n).is_err());
        }
    }
    #[test]
    fn defaults_and_texture_validation() {
        let a = default_skin("alex").unwrap();
        let b = default_skin("steve").unwrap();
        assert_ne!(a, b);
        assert!(default_skin("other").is_err());
        let mut evil = a;
        evil.value=STANDARD.encode(r#"{"textures":{"SKIN":{"url":"https://textures.minecraft.net.evil.example/texture/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}"#);
        assert!(validate(&evil).is_err());
    }
    #[test]
    fn official_response_validation() {
        assert!(profile_id(br#"{"id":"../profile"}"#).is_err());
        let skin = default_skin("steve").unwrap();
        let bytes=serde_json::to_vec(&serde_json::json!({"properties":[{"name":"textures","value":skin.value,"signature":skin.signature}]})).unwrap();
        assert_eq!(session_skin(&bytes).unwrap(), Some(skin));
        assert!(session_skin(br#"{"properties":[{"name":"textures","value":"bad"}]}"#).is_err());
    }
    #[test]
    fn persistence_and_corrupt_preservation() {
        let dir = std::env::temp_dir().join(format!("native-skins-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("skins.json");
        let mut choices = Choices::new();
        choices.insert(
            "example".into(),
            Choice {
                username: None,
                skin: default_skin("alex").unwrap(),
                moderated: Some("alex".into()),
            },
        );
        save(&path, &choices).unwrap();
        assert_eq!(load(&path).unwrap(), choices);
        fs::write(&path, "broken").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
        fs::remove_dir_all(dir).unwrap();
    }
}
