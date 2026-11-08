use crate::signer::SignError;
use nostr::nips::nip46::{Message, NostrConnectURI, Request, ResponseResult};
use nostr::{Event, JsonUtil, Keys, PublicKey, UnsignedEvent};
use nostr::serde_json;
use nostr_connect::client::NostrConnect;
use nostr_connect::prelude::NostrSigner;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(60);

pub fn parse_bunker_uri(input: &str) -> Result<NostrConnectURI, SignError> {
    let input = input.trim();
    if !input.starts_with("bunker://") {
        return Err(SignError::Failed(crate::i18n::t("login.invalid_bunker").to_string()));
    }
    match NostrConnectURI::parse(input) {
        Ok(uri) if uri.is_bunker() && !uri.relays().is_empty() => Ok(uri),
        _ => Err(SignError::Failed(crate::i18n::t("login.invalid_bunker").to_string())),
    }
}

pub fn get_public_key_request_json(id: &str) -> String {
    serde_json::json!({ "id": id, "method": "get_public_key", "params": [] }).to_string()
}

pub fn sign_event_request_json(id: &str, unsigned: &UnsignedEvent) -> String {
    serde_json::json!({
        "id": id,
        "method": "sign_event",
        "params": [unsigned.as_json()],
    })
    .to_string()
}

pub fn parse_public_key_response(json: &str) -> Result<PublicKey, SignError> {
    match Message::from_json(json).map_err(|e| SignError::Failed(e.to_string()))? {
        Message::Response { error: Some(e), result: None, .. } => Err(SignError::Failed(e)),
        Message::Response { result: Some(r), .. } => {
            r.to_get_public_key().map_err(|e| SignError::Failed(e.to_string()))
        }
        _ => Err(SignError::Failed("unexpected bunker message".to_string())),
    }
}

pub fn parse_sign_event_response(json: &str) -> Result<Event, SignError> {
    match Message::from_json(json).map_err(|e| SignError::Failed(e.to_string()))? {
        Message::Response { error: Some(e), result: None, .. } => Err(SignError::Failed(e)),
        Message::Response { result: Some(r), .. } => {
            r.to_sign_event().map_err(|e| SignError::Failed(e.to_string()))
        }
        _ => Err(SignError::Failed("unexpected bunker message".to_string())),
    }
}

pub fn request_of(json: &str) -> Option<Request> {
    match Message::from_json(json).ok()? {
        Message::Request { req, .. } => Some(req),
        _ => None,
    }
}

pub fn response_result(json: &str) -> Option<ResponseResult> {
    match Message::from_json(json).ok()? {
        Message::Response { result, .. } => result,
        _ => None,
    }
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("bunker runtime")
    })
}

static CLIENTS: Mutex<Option<HashMap<String, Arc<NostrConnect>>>> = Mutex::new(None);

fn cached(pubkey_hex: &str) -> Option<Arc<NostrConnect>> {
    CLIENTS.lock().unwrap().as_ref().and_then(|m| m.get(pubkey_hex).cloned())
}

fn cache(pubkey_hex: &str, client: Arc<NostrConnect>) {
    CLIENTS
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(pubkey_hex.to_string(), client);
}

pub fn forget_client(pubkey_hex: &str) {
    if let Some(m) = CLIENTS.lock().unwrap().as_mut() {
        m.remove(pubkey_hex);
    }
}

fn client_for(pubkey: &PublicKey, uri: &str, app_keys: &Keys) -> Result<Arc<NostrConnect>, SignError> {
    let hex = pubkey.to_hex();
    if let Some(c) = cached(&hex) {
        return Ok(c);
    }
    let uri = parse_bunker_uri(uri)?;
    let client = NostrConnect::new(uri, app_keys.clone(), CONNECT_TIMEOUT, None)
        .map_err(|e| SignError::Failed(e.to_string()))?;
    let client = Arc::new(client);
    cache(&hex, client.clone());
    Ok(client)
}

pub struct BunkerLogin {
    pub pubkey: PublicKey,
    pub uri: String,
    pub app_keys: Keys,
}

pub fn begin_login(input: &str) -> std::sync::mpsc::Receiver<Result<BunkerLogin, SignError>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let uri_text = input.trim().to_string();
    let uri = match parse_bunker_uri(&uri_text) {
        Ok(u) => u,
        Err(e) => {
            let _ = tx.send(Err(e));
            return rx;
        }
    };
    let app_keys = Keys::generate();
    runtime().spawn(async move {
        let result = async {
            let client = NostrConnect::new(uri, app_keys.clone(), CONNECT_TIMEOUT, None)
                .map_err(|e| SignError::Failed(e.to_string()))?;
            let pubkey = client
                .get_public_key()
                .await
                .map_err(|e| SignError::Failed(e.to_string()))?;
            cache(&pubkey.to_hex(), Arc::new(client));
            Ok(BunkerLogin { pubkey, uri: uri_text, app_keys })
        }
        .await;
        let _ = tx.send(result);
        crate::signer::notify_repaint();
    });
    rx
}

pub fn sign(
    pubkey: PublicKey,
    uri: String,
    app_keys: Keys,
    unsigned: UnsignedEvent,
    done: impl FnOnce(Result<Event, SignError>) + Send + 'static,
) {
    runtime().spawn(async move {
        let result = async {
            let client = client_for(&pubkey, &uri, &app_keys)?;
            let event = client
                .sign_event(unsigned)
                .await
                .map_err(|e| SignError::Failed(e.to_string()))?;
            if event.pubkey != pubkey || event.verify().is_err() {
                return Err(SignError::Failed("bunker returned an invalid event".to_string()));
            }
            Ok(event)
        }
        .await;
        done(result);
        crate::signer::notify_repaint();
    });
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BunkerEntry {
    pub uri: String,
    pub app_secret: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BunkerAccounts {
    entries: HashMap<String, BunkerEntry>,
}

impl BunkerAccounts {
    pub fn parse(text: &str) -> Self {
        Self { entries: serde_json::from_str(text).unwrap_or_default() }
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string(&self.entries).unwrap_or_default()
    }

    pub fn get(&self, pubkey_hex: &str) -> Option<&BunkerEntry> {
        self.entries.get(pubkey_hex)
    }

    pub fn insert(&mut self, pubkey_hex: &str, entry: BunkerEntry) {
        self.entries.insert(pubkey_hex.to_string(), entry);
    }

    pub fn remove(&mut self, pubkey_hex: &str) {
        self.entries.remove(pubkey_hex);
    }

    fn file(dir: &Path) -> PathBuf {
        dir.join("bunker_accounts.json")
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(Self::file(dir))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        if let Err(e) = std::fs::write(Self::file(dir), self.to_text()) {
            log::warn!("saving bunker accounts failed: {e}");
        }
    }
}

static ACCOUNTS: Mutex<Option<BunkerAccounts>> = Mutex::new(None);

fn with_accounts<T>(dir: &Path, f: impl FnOnce(&mut BunkerAccounts) -> T) -> T {
    let mut slot = ACCOUNTS.lock().unwrap();
    f(slot.get_or_insert_with(|| BunkerAccounts::load(dir)))
}

pub fn bunker_account(dir: &Path, pubkey_hex: &str) -> Option<(String, Keys)> {
    with_accounts(dir, |a| {
        let e = a.get(pubkey_hex)?;
        let keys = Keys::parse(&e.app_secret).ok()?;
        Some((e.uri.clone(), keys))
    })
}

pub fn remember_bunker(dir: &Path, login: &BunkerLogin) {
    let entry = BunkerEntry {
        uri: login.uri.clone(),
        app_secret: login.app_keys.secret_key().to_secret_hex(),
    };
    with_accounts(dir, |a| {
        a.insert(&login.pubkey.to_hex(), entry);
        a.save(dir);
    });
}

pub fn forget_bunker(dir: &Path, pubkey_hex: &str) {
    forget_client(pubkey_hex);
    with_accounts(dir, |a| {
        a.remove(pubkey_hex);
        a.save(dir);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Kind};

    fn bunker_url(remote: &Keys) -> String {
        format!(
            "bunker://{}?relay=wss%3A%2F%2Frelay.nsec.app&relay=wss://relay.damus.io&secret=abc",
            remote.public_key().to_hex()
        )
    }

    #[test]
    fn parses_bunker_url_with_relays_and_secret() {
        let remote = Keys::generate();
        let uri = parse_bunker_uri(&format!("  {}\n", bunker_url(&remote))).unwrap();
        assert_eq!(uri.remote_signer_public_key(), Some(&remote.public_key()));
        assert_eq!(uri.relays().len(), 2);
        assert_eq!(uri.relays()[0].as_str_without_trailing_slash(), "wss://relay.nsec.app");
        assert_eq!(uri.secret(), Some("abc"));
    }

    #[test]
    fn rejects_non_bunker_and_relayless_urls() {
        let remote = Keys::generate();
        assert!(parse_bunker_uri("nsec1abc").is_err());
        assert!(parse_bunker_uri("").is_err());
        assert!(parse_bunker_uri(&format!("bunker://{}", remote.public_key().to_hex())).is_err());
        assert!(parse_bunker_uri("bunker://nothex?relay=wss://r.example").is_err());
        assert!(parse_bunker_uri(&format!(
            "nostrconnect://{}?relay=wss://r.example&metadata=%7B%22name%22%3A%22x%22%7D",
            remote.public_key().to_hex()
        ))
        .is_err());
    }

    #[test]
    fn get_public_key_request_json_matches_nip46() {
        let json = get_public_key_request_json("7");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["id"], "7");
        assert_eq!(v["method"], "get_public_key");
        assert_eq!(v["params"], serde_json::json!([]));
        assert_eq!(request_of(&json), Some(Request::GetPublicKey));
    }

    #[test]
    fn sign_event_request_json_carries_unsigned_event() {
        let keys = Keys::generate();
        let mut unsigned = EventBuilder::text_note("hi").build(keys.public_key());
        unsigned.ensure_id();
        let json = sign_event_request_json("9", &unsigned);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["method"], "sign_event");
        let inner: serde_json::Value =
            serde_json::from_str(v["params"][0].as_str().unwrap()).unwrap();
        assert_eq!(inner["content"], "hi");
        assert_eq!(inner["kind"], 1);
        assert_eq!(request_of(&json), Some(Request::SignEvent(unsigned)));
    }

    #[test]
    fn public_key_response_is_parsed() {
        let user = Keys::generate().public_key();
        let json = serde_json::json!({ "id": "1", "result": user.to_hex() }).to_string();
        assert_eq!(parse_public_key_response(&json), Ok(user));
        assert!(matches!(
            response_result(&json),
            Some(ResponseResult::GetPublicKey(_))
        ));
    }

    #[test]
    fn error_response_is_reported() {
        let json = serde_json::json!({ "id": "1", "result": null, "error": "denied" }).to_string();
        assert_eq!(
            parse_public_key_response(&json),
            Err(SignError::Failed("denied".to_string()))
        );
        assert_eq!(
            parse_sign_event_response(&json),
            Err(SignError::Failed("denied".to_string()))
        );
        assert!(parse_public_key_response("not json").is_err());
    }

    #[test]
    fn sign_event_response_returns_signed_event() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "yo").sign_with_keys(&keys).unwrap();
        let json = serde_json::json!({ "id": "2", "result": event.as_json() }).to_string();
        assert_eq!(parse_sign_event_response(&json), Ok(event));
    }

    #[test]
    fn registry_round_trips_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = BunkerAccounts::default();
        a.insert("aa", BunkerEntry { uri: "bunker://x".into(), app_secret: "bb".into() });
        a.save(dir.path());
        let b = BunkerAccounts::load(dir.path());
        assert_eq!(b, a);
        assert_eq!(b.get("aa").unwrap().uri, "bunker://x");
        assert!(BunkerAccounts::parse("garbage").get("aa").is_none());
    }

    struct ApproveAll;

    impl nostr_connect::prelude::NostrConnectSignerActions for ApproveAll {
        fn approve(&self, _req: &Request) -> bool {
            true
        }
    }

    #[test]
    fn login_and_sign_through_a_relay_with_a_remote_signer() {
        use nostr_connect::prelude::{NostrConnectKeys, NostrConnectRemoteSigner};
        use std::time::Duration;

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let relay = rt.block_on(nostr_relay_builder::MockRelay::run()).unwrap();
        let user = Keys::generate();
        let keys = NostrConnectKeys { signer: Keys::generate(), user: user.clone() };
        let remote = NostrConnectRemoteSigner::new(
            keys,
            [relay.url()],
            Some("s3cret".to_string()),
            None,
        )
        .unwrap();
        let uri = remote.bunker_uri().to_string();
        rt.spawn(async move {
            let _ = remote.serve(ApproveAll).await;
        });
        std::thread::sleep(Duration::from_millis(500));

        let login = begin_login(&uri)
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(login.pubkey, user.public_key());

        let chat = EventBuilder::new(Kind::LiveEventMessage, "hello");
        let (tx, rx) = std::sync::mpsc::channel();
        let mut unsigned = chat.build(login.pubkey);
        unsigned.ensure_id();
        sign(login.pubkey, login.uri.clone(), login.app_keys.clone(), unsigned, move |r| {
            let _ = tx.send(r);
        });
        let event = rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
        assert_eq!(event.kind, Kind::LiveEventMessage);
        assert_eq!(event.pubkey, user.public_key());
        assert_eq!(event.content, "hello");
        assert!(event.verify().is_ok());
    }

    #[test]
    fn begin_login_rejects_bad_url_immediately() {
        let rx = begin_login("hello");
        assert!(rx.recv().unwrap().is_err());
    }
}
