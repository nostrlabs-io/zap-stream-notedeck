use nostr::{serde_json, Event, EventBuilder, JsonUtil, Keys, PublicKey, UnsignedEvent};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub const SIGN_TIMEOUT: Duration = Duration::from_secs(180);

const PERMISSION_KINDS: &[u16] = &[0, 3, 7, 1311, 1314, 9734, 10000, 10002, 24242, 27235];

#[derive(Clone, Debug, PartialEq)]
pub enum SignError {
    Rejected,
    NotInstalled,
    Unsupported,
    Timeout,
    Failed(String),
}

impl std::fmt::Display for SignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignError::Rejected => f.write_str(crate::i18n::t("signer.rejected")),
            SignError::NotInstalled => f.write_str(crate::i18n::t("signer.not_installed")),
            SignError::Unsupported => f.write_str(crate::i18n::t("signer.unsupported")),
            SignError::Timeout => f.write_str(crate::i18n::t("signer.timeout")),
            SignError::Failed(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for SignError {}

#[derive(Clone, Debug, PartialEq)]
pub enum Signer {
    Local(Keys),
    Amber { pubkey: PublicKey, package: String },
    Bunker { pubkey: PublicKey, uri: String, app_keys: Keys },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RequestKind {
    GetPublicKey,
    SignEvent,
}

impl RequestKind {
    pub fn name(&self) -> &'static str {
        match self {
            RequestKind::GetPublicKey => "get_public_key",
            RequestKind::SignEvent => "sign_event",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AmberRequest {
    pub id: String,
    pub kind: RequestKind,
    pub package: Option<String>,
    pub current_user: Option<String>,
    pub payload: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmberReply {
    pub result: Option<String>,
    pub event: Option<String>,
    pub package: Option<String>,
    pub id: Option<String>,
}

pub type Completion = Box<dyn FnOnce(Result<AmberReply, SignError>) + Send>;

pub trait Transport: Send + Sync {
    fn send(&self, request: AmberRequest, done: Completion);
}

static REPAINT: Mutex<Option<egui::Context>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static PENDING: Mutex<Option<HashMap<String, Completion>>> = Mutex::new(None);

pub fn set_repaint(ctx: &egui::Context) {
    let mut slot = REPAINT.lock().unwrap();
    if slot.is_none() {
        *slot = Some(ctx.clone());
    }
}

pub fn notify_repaint() {
    notify();
}

fn notify() {
    if let Some(ctx) = REPAINT.lock().unwrap().as_ref() {
        ctx.request_repaint();
    }
}

pub fn next_request_id() -> String {
    let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("zs-{:x}-{}", now, n)
}

pub fn register_pending(id: &str, done: Completion) {
    PENDING
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(id.to_string(), done);
}

pub fn take_pending(id: &str) -> Option<Completion> {
    PENDING.lock().unwrap().as_mut().and_then(|m| m.remove(id))
}

pub fn deliver_reply(id: &str, reply_json: &str) -> bool {
    match take_pending(id) {
        Some(done) => {
            done(parse_reply(reply_json));
            notify();
            true
        }
        None => false,
    }
}

pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

impl AmberRequest {
    pub fn get_public_key(id: String) -> Self {
        Self {
            id,
            kind: RequestKind::GetPublicKey,
            package: None,
            current_user: None,
            payload: String::new(),
        }
    }

    pub fn sign_event(id: String, package: &str, current_user: &PublicKey, event_json: String) -> Self {
        Self {
            id,
            kind: RequestKind::SignEvent,
            package: Some(package.to_string()),
            current_user: Some(current_user.to_hex()),
            payload: event_json,
        }
    }

    pub fn intent_uri(&self) -> String {
        format!("nostrsigner:{}", percent_encode(&self.payload))
    }

    pub fn intent_extras(&self) -> Vec<(String, String)> {
        let mut extras = vec![
            ("type".to_string(), self.kind.name().to_string()),
            ("id".to_string(), self.id.clone()),
        ];
        if let Some(user) = &self.current_user {
            extras.push(("current_user".to_string(), user.clone()));
        }
        if self.kind == RequestKind::GetPublicKey {
            extras.push(("permissions".to_string(), permissions_json()));
        }
        extras
    }

    pub fn provider_authority(&self) -> Option<String> {
        match (self.kind, &self.package) {
            (RequestKind::SignEvent, Some(pkg)) => {
                Some(format!("{}.{}", pkg, self.kind.name().to_uppercase()))
            }
            _ => None,
        }
    }

    pub fn provider_args(&self) -> Vec<String> {
        vec![
            self.payload.clone(),
            String::new(),
            self.current_user.clone().unwrap_or_default(),
        ]
    }

    pub fn to_json(&self) -> String {
        let extras: serde_json::Map<String, serde_json::Value> = self
            .intent_extras()
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        let provider = self.provider_authority().map(|a| {
            serde_json::json!({ "authority": a, "args": self.provider_args() })
        });
        serde_json::json!({
            "id": self.id,
            "uri": self.intent_uri(),
            "package": self.package,
            "extras": extras,
            "provider": provider,
        })
        .to_string()
    }
}

pub fn permissions_json() -> String {
    let list: Vec<serde_json::Value> = PERMISSION_KINDS
        .iter()
        .map(|k| serde_json::json!({ "type": "sign_event", "kind": k }))
        .collect();
    serde_json::Value::Array(list).to_string()
}

pub fn parse_reply(json: &str) -> Result<AmberReply, SignError> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| SignError::Failed(e.to_string()))?;
    let text = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    match text("error").as_deref() {
        Some("not_installed") => return Err(SignError::NotInstalled),
        Some(other) => return Err(SignError::Failed(other.to_string())),
        None => {}
    }
    let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    let rejected = v.get("rejected").and_then(|r| r.as_bool()).unwrap_or(false)
        || text("rejected").as_deref() == Some("true");
    if rejected || code == 0 {
        return Err(SignError::Rejected);
    }
    if code != -1 {
        return Err(SignError::Failed(format!("signer returned code {}", code)));
    }
    Ok(AmberReply {
        result: text("result"),
        event: text("event"),
        package: text("package"),
        id: text("id"),
    })
}

pub fn finish_login(reply: AmberReply) -> Result<(PublicKey, String), SignError> {
    let raw = reply
        .result
        .ok_or_else(|| SignError::Failed("signer returned no public key".into()))?;
    let pubkey = PublicKey::parse(raw.trim())
        .map_err(|e| SignError::Failed(format!("invalid public key: {}", e)))?;
    let package = reply
        .package
        .ok_or_else(|| SignError::Failed("signer did not report its package".into()))?;
    Ok((pubkey, package))
}

pub fn finish_sign(mut unsigned: UnsignedEvent, reply: AmberReply) -> Result<Event, SignError> {
    unsigned.ensure_id();
    if let Some(json) = reply.event {
        let event = Event::from_json(json).map_err(|e| SignError::Failed(e.to_string()))?;
        if event.pubkey != unsigned.pubkey {
            return Err(SignError::Failed("signed by a different key".into()));
        }
        if Some(event.id) != unsigned.id {
            return Err(SignError::Failed("signer changed the event".into()));
        }
        event
            .verify()
            .map_err(|e| SignError::Failed(e.to_string()))?;
        return Ok(event);
    }
    let sig = reply
        .result
        .ok_or_else(|| SignError::Failed("signer returned no signature".into()))?;
    let sig = sig
        .trim()
        .parse()
        .map_err(|_| SignError::Failed("invalid signature".into()))?;
    unsigned
        .add_signature(sig)
        .map_err(|e| SignError::Failed(e.to_string()))
}

#[derive(Clone)]
pub struct SignHandle(Arc<Mutex<mpsc::Receiver<Result<Event, SignError>>>>);

pub struct Completer(mpsc::Sender<Result<Event, SignError>>);

impl Completer {
    pub fn complete(self, result: Result<Event, SignError>) {
        let _ = self.0.send(result);
        notify();
    }
}

impl SignHandle {
    pub fn pending() -> (Completer, SignHandle) {
        let (tx, rx) = mpsc::channel();
        (Completer(tx), SignHandle(Arc::new(Mutex::new(rx))))
    }

    pub fn ready(result: Result<Event, SignError>) -> SignHandle {
        let (tx, handle) = Self::pending();
        tx.0.send(result).ok();
        handle
    }

    pub fn poll(&self) -> Option<Result<Event, SignError>> {
        match self.0.lock().unwrap().try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(SignError::Failed("signer dropped the request".into())))
            }
        }
    }

    pub fn wait(&self, timeout: Duration) -> Result<Event, SignError> {
        match self.0.lock().unwrap().recv_timeout(timeout) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(SignError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(SignError::Failed("signer dropped the request".into()))
            }
        }
    }
}

#[cfg(target_os = "android")]
fn default_transport() -> Option<&'static dyn Transport> {
    Some(&crate::amber_android::AndroidTransport)
}

#[cfg(not(target_os = "android"))]
fn default_transport() -> Option<&'static dyn Transport> {
    None
}

impl Signer {
    pub fn pubkey(&self) -> PublicKey {
        match self {
            Signer::Local(keys) => keys.public_key(),
            Signer::Amber { pubkey, .. } | Signer::Bunker { pubkey, .. } => *pubkey,
        }
    }

    pub fn sign(&self, builder: EventBuilder) -> SignHandle {
        match default_transport() {
            Some(t) => self.sign_with(builder, t),
            None => self.sign_with(builder, &Unavailable),
        }
    }

    pub fn sign_with(&self, builder: EventBuilder, transport: &dyn Transport) -> SignHandle {
        match self {
            Signer::Local(keys) => SignHandle::ready(
                builder
                    .sign_with_keys(keys)
                    .map_err(|e| SignError::Failed(e.to_string())),
            ),
            Signer::Bunker { pubkey, uri, app_keys } => {
                let mut unsigned = builder.build(*pubkey);
                unsigned.ensure_id();
                let (completer, handle) = SignHandle::pending();
                crate::bunker::sign(
                    *pubkey,
                    uri.clone(),
                    app_keys.clone(),
                    unsigned,
                    move |r| completer.complete(r),
                );
                handle
            }
            Signer::Amber { pubkey, package } => {
                let mut unsigned = builder.build(*pubkey);
                unsigned.ensure_id();
                let request = AmberRequest::sign_event(
                    next_request_id(),
                    package,
                    pubkey,
                    unsigned.as_json(),
                );
                let (completer, handle) = SignHandle::pending();
                transport.send(
                    request,
                    Box::new(move |reply| completer.complete(reply.and_then(|r| finish_sign(unsigned, r)))),
                );
                handle
            }
        }
    }

    pub fn sign_then(
        &self,
        builder: EventBuilder,
        then: impl FnOnce(Result<Event, SignError>) + Send + 'static,
    ) {
        let handle = self.sign(builder);
        std::thread::spawn(move || then(handle.wait(SIGN_TIMEOUT)));
    }

    pub fn sign_blocking(&self, builder: EventBuilder) -> Result<Event, SignError> {
        self.sign(builder).wait(SIGN_TIMEOUT)
    }
}

static OUTBOX: Mutex<Vec<SignHandle>> = Mutex::new(Vec::new());
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub fn submit(handle: SignHandle) {
    OUTBOX.lock().unwrap().push(handle);
}

pub fn drain_signed() -> Vec<Event> {
    let mut outbox = OUTBOX.lock().unwrap();
    let mut done = Vec::new();
    outbox.retain(|h| match h.poll() {
        None => true,
        Some(Ok(ev)) => {
            done.push(ev);
            false
        }
        Some(Err(e)) => {
            log::warn!("signing failed: {e}");
            *LAST_ERROR.lock().unwrap() = Some(e.to_string());
            false
        }
    });
    done
}

pub fn has_pending() -> bool {
    !OUTBOX.lock().unwrap().is_empty()
}

pub fn take_error() -> Option<String> {
    LAST_ERROR.lock().unwrap().take()
}

pub struct Unavailable;

impl Transport for Unavailable {
    fn send(&self, _request: AmberRequest, done: Completion) {
        done(Err(SignError::Unsupported));
    }
}

pub fn begin_login(transport: &dyn Transport) -> mpsc::Receiver<Result<(PublicKey, String), SignError>> {
    let (tx, rx) = mpsc::channel();
    transport.send(
        AmberRequest::get_public_key(next_request_id()),
        Box::new(move |reply| {
            let _ = tx.send(reply.and_then(finish_login));
            notify();
        }),
    );
    rx
}

pub fn begin_amber_login() -> mpsc::Receiver<Result<(PublicKey, String), SignError>> {
    match default_transport() {
        Some(t) => begin_login(t),
        None => begin_login(&Unavailable),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AmberAccounts {
    packages: HashMap<String, String>,
}

impl AmberAccounts {
    pub fn parse(text: &str) -> Self {
        let packages = serde_json::from_str(text).unwrap_or_default();
        Self { packages }
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string(&self.packages).unwrap_or_default()
    }

    pub fn package(&self, pubkey_hex: &str) -> Option<&str> {
        self.packages.get(pubkey_hex).map(String::as_str)
    }

    pub fn insert(&mut self, pubkey_hex: &str, package: &str) {
        self.packages.insert(pubkey_hex.to_string(), package.to_string());
    }

    pub fn remove(&mut self, pubkey_hex: &str) {
        self.packages.remove(pubkey_hex);
    }

    fn file(dir: &Path) -> PathBuf {
        dir.join("amber_accounts.json")
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(Self::file(dir))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        if let Err(e) = std::fs::write(Self::file(dir), self.to_text()) {
            log::warn!("saving amber accounts failed: {e}");
        }
    }
}

static ACCOUNTS: Mutex<Option<AmberAccounts>> = Mutex::new(None);

fn with_accounts<T>(dir: &Path, f: impl FnOnce(&mut AmberAccounts) -> T) -> T {
    let mut slot = ACCOUNTS.lock().unwrap();
    f(slot.get_or_insert_with(|| AmberAccounts::load(dir)))
}

pub fn amber_package(dir: &Path, pubkey_hex: &str) -> Option<String> {
    with_accounts(dir, |a| a.package(pubkey_hex).map(str::to_string))
}

pub fn remember_amber(dir: &Path, pubkey_hex: &str, package: &str) {
    with_accounts(dir, |a| {
        a.insert(pubkey_hex, package);
        a.save(dir);
    });
}

pub fn forget_amber(dir: &Path, pubkey_hex: &str) {
    with_accounts(dir, |a| {
        a.remove(pubkey_hex);
        a.save(dir);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Kind, Tag};

    struct FakeAmber {
        keys: Keys,
        mode: Mode,
        seen: Mutex<Vec<AmberRequest>>,
    }

    enum Mode {
        EventJson,
        SignatureOnly,
        Reject,
        OtherKey,
        Raw(&'static str),
    }

    impl FakeAmber {
        fn new(mode: Mode) -> Self {
            Self { keys: Keys::generate(), mode, seen: Mutex::new(vec![]) }
        }
    }

    impl Transport for FakeAmber {
        fn send(&self, request: AmberRequest, done: Completion) {
            self.seen.lock().unwrap().push(request.clone());
            let unsigned = UnsignedEvent::from_json(&request.payload).unwrap_or_else(|_| {
                EventBuilder::new(Kind::TextNote, "").build(self.keys.public_key())
            });
            let reply = match &self.mode {
                Mode::EventJson => {
                    let ev = unsigned.sign_with_keys(&self.keys).unwrap();
                    Ok(AmberReply { event: Some(ev.as_json()), ..Default::default() })
                }
                Mode::SignatureOnly => {
                    let ev = unsigned.sign_with_keys(&self.keys).unwrap();
                    Ok(AmberReply { result: Some(ev.sig.to_string()), ..Default::default() })
                }
                Mode::Reject => Err(SignError::Rejected),
                Mode::OtherKey => {
                    let other = Keys::generate();
                    let mut u = unsigned;
                    u.pubkey = other.public_key();
                    u.id = None;
                    let ev = u.sign_with_keys(&other).unwrap();
                    Ok(AmberReply { event: Some(ev.as_json()), ..Default::default() })
                }
                Mode::Raw(json) => parse_reply(json),
            };
            done(reply);
        }
    }

    fn amber_for(fake: &FakeAmber) -> Signer {
        Signer::Amber { pubkey: fake.keys.public_key(), package: "com.greenart7c3.nostrsigner".into() }
    }

    fn note() -> EventBuilder {
        EventBuilder::new(Kind::Custom(1311), "hi#there {\"a\":1}")
            .tag(Tag::parse(["a", "30311:abc:d"]).unwrap())
    }

    #[test]
    fn local_signer_signs_immediately() {
        let keys = Keys::generate();
        let signer = Signer::Local(keys.clone());
        let ev = signer.sign_with(note(), &Unavailable).poll().unwrap().unwrap();
        assert_eq!(ev.pubkey, keys.public_key());
        assert!(ev.verify().is_ok());
        assert_eq!(signer.pubkey(), keys.public_key());
    }

    #[test]
    fn amber_signer_accepts_returned_event() {
        let fake = FakeAmber::new(Mode::EventJson);
        let signer = amber_for(&fake);
        let ev = signer.sign_with(note(), &fake).wait(Duration::from_secs(1)).unwrap();
        assert_eq!(ev.pubkey, fake.keys.public_key());
        assert_eq!(ev.kind, Kind::Custom(1311));
        assert!(ev.verify().is_ok());
        let seen = fake.seen.lock().unwrap();
        assert_eq!(seen[0].kind, RequestKind::SignEvent);
        assert_eq!(seen[0].package.as_deref(), Some("com.greenart7c3.nostrsigner"));
        assert_eq!(seen[0].current_user.as_deref(), Some(fake.keys.public_key().to_hex().as_str()));
    }

    #[test]
    fn amber_signer_accepts_signature_only_reply() {
        let fake = FakeAmber::new(Mode::SignatureOnly);
        let ev = amber_for(&fake).sign_with(note(), &fake).poll().unwrap().unwrap();
        assert!(ev.verify().is_ok());
        assert_eq!(ev.content, "hi#there {\"a\":1}");
    }

    #[test]
    fn amber_rejection_is_reported() {
        let fake = FakeAmber::new(Mode::Reject);
        let r = amber_for(&fake).sign_with(note(), &fake).poll().unwrap();
        assert_eq!(r.unwrap_err(), SignError::Rejected);
    }

    #[test]
    fn event_from_another_key_is_refused() {
        let fake = FakeAmber::new(Mode::OtherKey);
        let r = amber_for(&fake).sign_with(note(), &fake).poll().unwrap();
        assert!(matches!(r, Err(SignError::Failed(_))));
    }

    #[test]
    fn missing_transport_is_unsupported() {
        let signer = Signer::Amber { pubkey: Keys::generate().public_key(), package: "p".into() };
        let r = signer.sign_with(note(), &Unavailable).poll().unwrap();
        assert_eq!(r.unwrap_err(), SignError::Unsupported);
    }

    #[test]
    fn pending_handle_times_out_then_completes() {
        let (completer, handle) = SignHandle::pending();
        assert!(handle.poll().is_none());
        assert_eq!(handle.wait(Duration::from_millis(5)).unwrap_err(), SignError::Timeout);
        let keys = Keys::generate();
        completer.complete(Ok(note().sign_with_keys(&keys).unwrap()));
        assert!(handle.poll().unwrap().is_ok());
    }

    #[test]
    fn dropped_completer_fails_the_handle() {
        let (completer, handle) = SignHandle::pending();
        drop(completer);
        assert!(matches!(handle.poll(), Some(Err(SignError::Failed(_)))));
    }

    #[test]
    fn intent_uri_encodes_payload() {
        let req = AmberRequest {
            id: "1".into(),
            kind: RequestKind::SignEvent,
            package: Some("pkg".into()),
            current_user: Some("aa".into()),
            payload: "{\"content\":\"a#b c\"}".into(),
        };
        let uri = req.intent_uri();
        assert!(uri.starts_with("nostrsigner:"));
        assert!(!uri.contains('#') && !uri.contains(' ') && !uri.contains('{'));
        assert!(uri.contains("%23"));
    }

    #[test]
    fn sign_event_extras_and_provider() {
        let pk = Keys::generate().public_key();
        let req = AmberRequest::sign_event("id1".into(), "com.signer", &pk, "{}".into());
        let extras = req.intent_extras();
        assert!(extras.contains(&("type".into(), "sign_event".into())));
        assert!(extras.contains(&("id".into(), "id1".into())));
        assert!(extras.contains(&("current_user".into(), pk.to_hex())));
        assert!(!extras.iter().any(|(k, _)| k == "permissions"));
        assert_eq!(req.provider_authority().as_deref(), Some("com.signer.SIGN_EVENT"));
        assert_eq!(req.provider_args(), vec!["{}".to_string(), String::new(), pk.to_hex()]);
    }

    #[test]
    fn get_public_key_has_permissions_and_no_provider() {
        let req = AmberRequest::get_public_key("x".into());
        let extras = req.intent_extras();
        assert!(extras.contains(&("type".into(), "get_public_key".into())));
        let perms = extras.iter().find(|(k, _)| k == "permissions").unwrap();
        let v: serde_json::Value = serde_json::from_str(&perms.1).unwrap();
        assert!(v.as_array().unwrap().iter().any(|p| p["kind"] == 1311 && p["type"] == "sign_event"));
        assert!(req.provider_authority().is_none());
        assert_eq!(req.intent_uri(), "nostrsigner:");
    }

    #[test]
    fn request_json_carries_everything_kotlin_needs() {
        let pk = Keys::generate().public_key();
        let req = AmberRequest::sign_event("id1".into(), "com.signer", &pk, "{\"a\":1}".into());
        let v: serde_json::Value = serde_json::from_str(&req.to_json()).unwrap();
        assert_eq!(v["id"], "id1");
        assert_eq!(v["package"], "com.signer");
        assert_eq!(v["extras"]["type"], "sign_event");
        assert_eq!(v["provider"]["authority"], "com.signer.SIGN_EVENT");
        assert_eq!(v["provider"]["args"][0], "{\"a\":1}");
        let login: serde_json::Value =
            serde_json::from_str(&AmberRequest::get_public_key("l".into()).to_json()).unwrap();
        assert!(login["provider"].is_null());
        assert!(login["package"].is_null());
    }

    #[test]
    fn parse_reply_ok() {
        let r = parse_reply(r#"{"code":-1,"result":"abc","event":"{}","package":"p","id":"7"}"#).unwrap();
        assert_eq!(r.result.as_deref(), Some("abc"));
        assert_eq!(r.event.as_deref(), Some("{}"));
        assert_eq!(r.package.as_deref(), Some("p"));
        assert_eq!(r.id.as_deref(), Some("7"));
    }

    #[test]
    fn parse_reply_rejected_cancelled_and_failed() {
        assert_eq!(parse_reply(r#"{"code":-1,"rejected":true}"#).unwrap_err(), SignError::Rejected);
        assert_eq!(parse_reply(r#"{"code":-1,"rejected":"true"}"#).unwrap_err(), SignError::Rejected);
        assert_eq!(parse_reply(r#"{"code":0}"#).unwrap_err(), SignError::Rejected);
        assert!(matches!(parse_reply(r#"{"code":2}"#), Err(SignError::Failed(_))));
        assert_eq!(parse_reply(r#"{"error":"not_installed"}"#).unwrap_err(), SignError::NotInstalled);
        assert!(matches!(parse_reply("nope"), Err(SignError::Failed(_))));
    }

    #[test]
    fn login_accepts_hex_and_npub() {
        use nostr::ToBech32;
        let pk = Keys::generate().public_key();
        let hex = AmberReply { result: Some(pk.to_hex()), package: Some("p".into()), ..Default::default() };
        assert_eq!(finish_login(hex).unwrap(), (pk, "p".to_string()));
        let npub = AmberReply { result: Some(pk.to_bech32().unwrap()), package: Some("p".into()), ..Default::default() };
        assert_eq!(finish_login(npub).unwrap().0, pk);
    }

    #[test]
    fn login_needs_key_and_package() {
        let pk = Keys::generate().public_key();
        assert!(finish_login(AmberReply { result: Some(pk.to_hex()), ..Default::default() }).is_err());
        assert!(finish_login(AmberReply { package: Some("p".into()), ..Default::default() }).is_err());
        assert!(finish_login(AmberReply { result: Some("zzz".into()), package: Some("p".into()), ..Default::default() }).is_err());
    }

    #[test]
    fn begin_login_resolves_through_transport() {
        let pk = Keys::generate().public_key();
        let json: &'static str = Box::leak(
            format!(r#"{{"code":-1,"result":"{}","package":"com.signer"}}"#, pk.to_hex()).into_boxed_str(),
        );
        let fake = FakeAmber::new(Mode::Raw(json));
        let rx = begin_login(&fake);
        assert_eq!(rx.try_recv().unwrap().unwrap(), (pk, "com.signer".to_string()));
        assert_eq!(fake.seen.lock().unwrap()[0].kind, RequestKind::GetPublicKey);
    }

    #[test]
    fn deliver_reply_routes_by_id() {
        let (tx, rx) = mpsc::channel();
        register_pending("route-test", Box::new(move |r| tx.send(r).unwrap()));
        assert!(deliver_reply("route-test", r#"{"code":-1,"result":"r"}"#));
        assert_eq!(rx.try_recv().unwrap().unwrap().result.as_deref(), Some("r"));
        assert!(!deliver_reply("route-test", r#"{"code":-1}"#));
        assert!(!deliver_reply("never-registered", "{}"));
    }

    #[test]
    fn request_ids_are_unique() {
        assert_ne!(next_request_id(), next_request_id());
    }

    #[test]
    fn registry_round_trips_on_disk() {
        let dir = std::env::temp_dir().join(format!("zs-amber-{}", next_request_id()));
        let mut a = AmberAccounts::load(&dir);
        assert_eq!(a.package("aa"), None);
        a.insert("aa", "com.signer");
        a.save(&dir);
        let b = AmberAccounts::load(&dir);
        assert_eq!(b.package("aa"), Some("com.signer"));
        let mut c = b.clone();
        c.remove("aa");
        assert_eq!(c.package("aa"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_registry_is_empty() {
        assert_eq!(AmberAccounts::parse("{{{"), AmberAccounts::default());
    }
}
