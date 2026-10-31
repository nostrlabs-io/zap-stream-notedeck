use anyhow::{anyhow, bail, Result};
use nostr::hashes::{sha256, Hash};
use nostr::prelude::hex;
use nostr::{serde_json, Event, EventBuilder, Keys, Kind, Tag, Timestamp};

pub const DEFAULT_SERVER: &str = "https://nostr.download";

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(sha256::Hash::hash(data).to_byte_array())
}

pub fn auth_event(keys: &Keys, hash_hex: &str, now: Timestamp) -> Result<Event> {
    let expiry = (now + 3600u64).as_u64().to_string();
    let tags = [
        Tag::parse(["t", "upload"])?,
        Tag::parse(["x", hash_hex])?,
        Tag::parse(["expiration", expiry.as_str()])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(24242), "Upload avatar")
        .tags(tags)
        .custom_created_at(now)
        .sign_with_keys(keys)?)
}

pub fn auth_header(event: &Event) -> String {
    use nostr::base64::{engine::general_purpose::STANDARD, Engine};
    format!("Nostr {}", STANDARD.encode(nostr::JsonUtil::as_json(event)))
}

pub fn mime_for(file_name: &str) -> &'static str {
    match file_name
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

pub fn parse_upload_url(status: u16, body: &[u8]) -> Result<String> {
    if !(200..300).contains(&status) {
        bail!("Upload failed with status {}", status);
    }
    let v: serde_json::Value = serde_json::from_slice(body)?;
    v.get("url")
        .and_then(|u| u.as_str())
        .map(|u| u.to_string())
        .ok_or(anyhow!("Upload response had no url"))
}

pub fn upload_request(keys: &Keys, file_name: &str, data: Vec<u8>) -> Result<ehttp::Request> {
    let hash = sha256_hex(&data);
    let auth = auth_event(keys, &hash, Timestamp::now())?;
    let mut req = ehttp::Request::post(format!("{}/upload", DEFAULT_SERVER), data);
    req.method = "PUT".to_string();
    req.headers.insert("Authorization", auth_header(&auth));
    req.headers.insert("Content-Type", mime_for(file_name));
    req.headers.insert("X-SHA-256", hash);
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_empty() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn auth_event_has_upload_tags() {
        let keys = Keys::generate();
        let ev = auth_event(&keys, "abcd", Timestamp::from(1000u64)).unwrap();
        assert_eq!(ev.kind, Kind::Custom(24242));
        let tags: Vec<Vec<String>> = ev.tags.iter().map(|t| t.as_slice().to_vec()).collect();
        assert!(tags.contains(&vec!["t".into(), "upload".into()]));
        assert!(tags.contains(&vec!["x".into(), "abcd".into()]));
        assert!(tags.contains(&vec!["expiration".into(), "4600".into()]));
        assert!(ev.verify().is_ok());
    }

    #[test]
    fn auth_header_prefix() {
        let keys = Keys::generate();
        let ev = auth_event(&keys, "abcd", Timestamp::from(1000u64)).unwrap();
        assert!(auth_header(&ev).starts_with("Nostr "));
    }

    #[test]
    fn upload_url_parsed() {
        let body = br#"{"url":"https://nostr.download/abc.png","sha256":"abc"}"#;
        assert_eq!(
            parse_upload_url(200, body).unwrap(),
            "https://nostr.download/abc.png"
        );
        assert!(parse_upload_url(500, body).is_err());
        assert!(parse_upload_url(200, b"{}").is_err());
    }

    #[test]
    fn mime_by_extension() {
        assert_eq!(mime_for("a.PNG"), "image/png");
        assert_eq!(mime_for("a.jpeg"), "image/jpeg");
        assert_eq!(mime_for("a"), "application/octet-stream");
    }
}
