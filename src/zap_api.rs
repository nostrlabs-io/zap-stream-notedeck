use anyhow::{anyhow, bail, Result};
use nostr::hashes::{sha256, Hash};
use nostr::prelude::hex;
use nostr::serde_json::{self, json, Value};
use nostr::{Event, EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};

pub const API_URL: &str = "https://api-core.zap.stream/api/v1";

#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    pub name: String,
    pub url: String,
    pub key: String,
    pub rate: f64,
    pub unit: String,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamDetails {
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub content_warning: String,
    pub goal: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub balance: f64,
    pub endpoints: Vec<Endpoint>,
    pub tos_accepted: bool,
    pub tos_link: Option<String>,
    pub details: Option<StreamDetails>,
}

fn string(v: &Value, key: &str) -> String {
    v.get(key).and_then(|s| s.as_str()).unwrap_or_default().to_string()
}

pub fn parse_tags(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|t| t.trim().trim_start_matches('#').to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

pub fn parse_account(status: u16, body: &[u8]) -> Result<Account> {
    if !(200..300).contains(&status) {
        bail!("API error {}: {}", status, String::from_utf8_lossy(body));
    }
    let v: Value = serde_json::from_slice(body)?;
    let endpoints = v
        .get("endpoints")
        .and_then(|e| e.as_array())
        .ok_or(anyhow!("Account had no endpoints"))?
        .iter()
        .map(|e| Endpoint {
            name: string(e, "name"),
            url: string(e, "url"),
            key: string(e, "key"),
            rate: e.pointer("/cost/rate").and_then(|r| r.as_f64()).unwrap_or_default(),
            unit: e
                .pointer("/cost/unit")
                .and_then(|r| r.as_str())
                .unwrap_or_default()
                .to_string(),
            capabilities: e
                .get("capabilities")
                .and_then(|c| c.as_array())
                .map(|c| c.iter().filter_map(|s| s.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        })
        .collect();
    let details = v.get("details").filter(|d| d.is_object()).map(|d| StreamDetails {
        title: string(d, "title"),
        summary: string(d, "summary"),
        tags: d
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|t| t.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        content_warning: string(d, "content_warning"),
        goal: string(d, "goal"),
    });
    Ok(Account {
        balance: v.get("balance").and_then(|b| b.as_f64()).unwrap_or_default(),
        endpoints,
        tos_accepted: v.pointer("/tos/accepted").and_then(|a| a.as_bool()).unwrap_or(true),
        tos_link: v.pointer("/tos/link").and_then(|l| l.as_str()).map(String::from),
        details,
    })
}

pub fn auth_event(
    keys: &Keys,
    url: &str,
    method: &str,
    body: Option<&[u8]>,
    now: Timestamp,
) -> Result<Event> {
    let mut tags = vec![Tag::parse(["u", url])?, Tag::parse(["method", method])?];
    if let Some(b) = body {
        let hash = hex::encode(sha256::Hash::hash(b).to_byte_array());
        tags.push(Tag::parse(["payload", hash.as_str()])?);
    }
    Ok(EventBuilder::new(Kind::HttpAuth, "")
        .tags(tags)
        .custom_created_at(now)
        .sign_with_keys(keys)?)
}

fn signed(keys: &Keys, mut req: ehttp::Request, method: &str) -> Result<ehttp::Request> {
    let ev = auth_event(keys, &req.url, method, Some(&req.body).filter(|b| !b.is_empty()).map(|b| b.as_slice()), Timestamp::now())?;
    use nostr::base64::{engine::general_purpose::STANDARD, Engine};
    req.headers.insert("Authorization", format!("Nostr {}", STANDARD.encode(ev.as_json())));
    req.headers.insert("Accept", "application/json");
    Ok(req)
}

pub fn account_request(keys: &Keys) -> Result<ehttp::Request> {
    signed(keys, ehttp::Request::get(format!("{}/account", API_URL)), "GET")
}

pub fn accept_tos_request(keys: &Keys) -> Result<ehttp::Request> {
    let body = json!({ "accept_tos": true }).to_string().into_bytes();
    let mut req = ehttp::Request::post(format!("{}/account", API_URL), body);
    req.method = "PATCH".to_string();
    req.headers.insert("Content-Type", "application/json");
    signed(keys, req, "PATCH")
}

pub fn update_body(id: Option<&str>, d: &StreamDetails) -> Vec<u8> {
    json!({
        "id": id,
        "title": d.title,
        "summary": d.summary,
        "content_warning": if d.content_warning.is_empty() { Value::Null } else { Value::String(d.content_warning.clone()) },
        "goal": if d.goal.is_empty() { Value::Null } else { Value::String(d.goal.clone()) },
        "tags": d.tags,
    })
    .to_string()
    .into_bytes()
}

pub fn update_request(keys: &Keys, id: Option<&str>, d: &StreamDetails) -> Result<ehttp::Request> {
    let mut req = ehttp::Request::post(format!("{}/event", API_URL), update_body(id, d));
    req.method = "PATCH".to_string();
    req.headers.insert("Content-Type", "application/json");
    signed(keys, req, "PATCH")
}

pub fn check_status(status: u16, body: &[u8]) -> Result<()> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        bail!("API error {}: {}", status, String::from_utf8_lossy(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT: &[u8] = br#"{"balance":1500,"endpoints":[{"name":"Basic","url":"rtmp://in.zap.stream:1935/Basic","key":"abc123","cost":{"unit":"min","rate":10},"capabilities":["variant:720h","dvr:source"]}],"tos":{"accepted":false,"link":"https://zap.stream/tos"},"details":{"title":"Hi","summary":"S","tags":["irl","art"],"content_warning":"nsfw","goal":"g1"}}"#;

    #[test]
    fn account_parsed_with_int_rate() {
        let a = parse_account(200, ACCOUNT).unwrap();
        assert_eq!(a.balance, 1500.0);
        assert_eq!(a.endpoints[0].key, "abc123");
        assert_eq!(a.endpoints[0].rate, 10.0);
        assert!(!a.tos_accepted);
        let d = a.details.unwrap();
        assert_eq!(d.tags, vec!["irl", "art"]);
        assert_eq!(d.content_warning, "nsfw");
    }

    #[test]
    fn account_without_details() {
        let a = parse_account(200, br#"{"balance":0,"endpoints":[],"tos":{"accepted":true}}"#).unwrap();
        assert!(a.details.is_none());
        assert!(a.tos_accepted);
    }

    #[test]
    fn error_status_rejected() {
        assert!(parse_account(401, b"nope").is_err());
        assert!(check_status(500, b"x").is_err());
        assert!(check_status(200, b"").is_ok());
    }

    #[test]
    fn auth_event_binds_url_method_and_payload() {
        let keys = Keys::generate();
        let ev = auth_event(&keys, "https://x/api", "PATCH", Some(b"{}"), Timestamp::from(5u64)).unwrap();
        assert_eq!(ev.kind, Kind::HttpAuth);
        let tags: Vec<Vec<String>> = ev.tags.iter().map(|t| t.as_slice().to_vec()).collect();
        assert!(tags.contains(&vec!["u".into(), "https://x/api".into()]));
        assert!(tags.contains(&vec!["method".into(), "PATCH".into()]));
        assert!(tags.contains(&vec![
            "payload".into(),
            "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a".into()
        ]));
        assert!(ev.verify().is_ok());
    }

    #[test]
    fn get_request_has_signed_header_and_no_payload() {
        let keys = Keys::generate();
        let req = account_request(&keys).unwrap();
        assert!(req.headers.get("Authorization").unwrap().starts_with("Nostr "));
        assert_eq!(req.method, "GET");
    }

    #[test]
    fn update_body_nulls_empty_optionals() {
        let d = StreamDetails { title: "T".into(), tags: vec!["irl".into()], ..Default::default() };
        let v: Value = serde_json::from_slice(&update_body(Some("d1"), &d)).unwrap();
        assert_eq!(v["id"], "d1");
        assert!(v["content_warning"].is_null());
        assert!(v["goal"].is_null());
        assert_eq!(v["tags"][0], "irl");
    }

    #[test]
    fn tags_normalised() {
        assert_eq!(parse_tags(" #IRL, art ,,"), vec!["irl", "art"]);
    }
}
