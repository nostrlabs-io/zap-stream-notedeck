use nwc::prelude::{
    Event, Filter, Keys, Kind, PublicKey, RelayOptions, RelayPool, ReqExitPolicy, Timestamp,
};
use std::time::Duration;

pub const NWC_RELAYS: [&str; 1] = ["wss://relay.getalby.com/v1"];
pub const RETURN_SCHEME: &str = "zswc";
pub const RETURN_URL: &str = "zswc://handler";
pub const WALLET_INFO_KIND: u16 = 13194;
pub const POLL: Duration = Duration::from_secs(15);
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
pub const TOTAL_WAIT: Duration = Duration::from_secs(120);

pub fn walletauth_url(client: &PublicKey, relays: &[&str]) -> String {
    let mut url = format!("nostr+walletauth://{}", client.to_hex());
    let mut sep = '?';
    let mut push = |k: &str, v: &str| {
        url.push(sep);
        sep = '&';
        url.push_str(k);
        url.push('=');
        url.push_str(&encode(v));
    };
    for r in relays {
        push("relay", r);
    }
    push("name", "zap.stream");
    push("request_methods", "pay_invoice get_info get_balance");
    push("icon", "https://zap.stream/logo.png");
    push("return_to", RETURN_URL);
    url
}

fn encode(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn latest_wallet_info(events: impl IntoIterator<Item = Event>) -> Option<Event> {
    events
        .into_iter()
        .filter(|e| e.kind == Kind::Custom(WALLET_INFO_KIND))
        .max_by_key(|e| e.created_at)
}

pub fn nwc_url(info: &Event, secret_hex: &str, relays: &[&str]) -> String {
    let mut url = format!("nostr+walletconnect://{}", info.pubkey.to_hex());
    let mut sep = '?';
    for r in relays {
        url.push(sep);
        sep = '&';
        url.push_str("relay=");
        url.push_str(&encode(r));
    }
    url.push(sep);
    url.push_str("secret=");
    url.push_str(secret_hex);
    url
}

static RETURNED: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub fn signal_return() {
    RETURNED.notify_one();
}

pub trait WalletInfoSource {
    async fn fetch(&self) -> Result<Vec<Event>, String>;
}

pub async fn wait_for_wallet(
    source: &impl WalletInfoSource,
    returned: &tokio::sync::Notify,
    secret_hex: &str,
    poll: Duration,
    total: Duration,
) -> Result<String, String> {
    let started = tokio::time::Instant::now();
    let mut last_err = None;
    loop {
        match source.fetch().await {
            Ok(events) => {
                if let Some(info) = latest_wallet_info(events) {
                    return Ok(nwc_url(&info, secret_hex, &NWC_RELAYS));
                }
                last_err = None;
            }
            Err(e) => last_err = Some(e),
        }
        if started.elapsed() + poll > total {
            return Err(last_err
                .unwrap_or_else(|| crate::i18n::t("settings.wallet_1tap_timeout").to_string()));
        }
        tokio::select! {
            _ = tokio::time::sleep(poll) => {}
            _ = returned.notified() => {}
        }
    }
}

struct RelaySource {
    pool: RelayPool,
    filter: Filter,
}

impl WalletInfoSource for RelaySource {
    async fn fetch(&self) -> Result<Vec<Event>, String> {
        self.pool
            .fetch_events_from(
                NWC_RELAYS,
                self.filter.clone(),
                FETCH_TIMEOUT,
                ReqExitPolicy::ExitOnEOSE,
            )
            .await
            .map(|events| events.to_vec())
            .map_err(|e| e.to_string())
    }
}

pub struct Pending {
    pub keys: Keys,
}

impl Pending {
    pub fn start() -> (Self, String) {
        let keys = Keys::generate();
        let url = walletauth_url(&keys.public_key(), &NWC_RELAYS);
        (Self { keys }, url)
    }

    pub async fn wait(&self) -> Result<String, String> {
        let pool = RelayPool::default();
        for r in NWC_RELAYS {
            pool.add_relay(r, RelayOptions::default())
                .await
                .map_err(|e| e.to_string())?;
        }
        pool.connect().await;
        let filter = Filter::new()
            .kind(Kind::Custom(WALLET_INFO_KIND))
            .pubkey(self.keys.public_key())
            .since(Timestamp::now() - Duration::from_secs(600))
            .limit(5);
        let source = RelaySource { pool, filter };
        let secret = self.keys.secret_key().to_secret_hex();
        let result = wait_for_wallet(&source, &RETURNED, &secret, POLL, TOTAL_WAIT).await;
        source.pool.shutdown().await;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nwc::prelude::EventBuilder;

    fn info_event(wallet: &Keys, client: &PublicKey, at: u64) -> Event {
        EventBuilder::new(Kind::Custom(WALLET_INFO_KIND), "pay_invoice get_info")
            .tags([nwc::prelude::Tag::public_key(*client)])
            .custom_created_at(Timestamp::from(at))
            .sign_with_keys(wallet)
            .unwrap()
    }

    #[test]
    fn builds_nwc_url_from_wallet_info_event() {
        let wallet = Keys::generate();
        let client = Keys::generate();
        let ev = info_event(&wallet, &client.public_key(), 100);
        let secret = client.secret_key().to_secret_hex();
        let url = nwc_url(&ev, &secret, &NWC_RELAYS);
        assert_eq!(
            url,
            format!(
                "nostr+walletconnect://{}?relay=wss%3A%2F%2Frelay.getalby.com%2Fv1&secret={}",
                wallet.public_key().to_hex(),
                secret
            )
        );
        let parsed = nwc::prelude::NostrWalletConnectURI::parse(&url).unwrap();
        assert_eq!(parsed.public_key, wallet.public_key());
        assert_eq!(parsed.secret.to_secret_hex(), secret);
    }

    #[test]
    fn picks_newest_wallet_info() {
        let old = Keys::generate();
        let new = Keys::generate();
        let client = Keys::generate().public_key();
        let events = vec![info_event(&old, &client, 10), info_event(&new, &client, 20)];
        let picked = latest_wallet_info(events).unwrap();
        assert_eq!(picked.pubkey, new.public_key());
    }

    struct Fake {
        calls: std::sync::Mutex<usize>,
        script: Vec<Result<Vec<Event>, String>>,
    }

    impl WalletInfoSource for Fake {
        async fn fetch(&self) -> Result<Vec<Event>, String> {
            let mut n = self.calls.lock().unwrap();
            let i = (*n).min(self.script.len() - 1);
            *n += 1;
            self.script[i].clone()
        }
    }

    #[tokio::test(start_paused = true)]
    async fn event_on_third_poll_builds_url() {
        let wallet = Keys::generate();
        let client = Keys::generate();
        let ev = info_event(&wallet, &client.public_key(), 5);
        let secret = client.secret_key().to_secret_hex();
        let fake = Fake {
            calls: Default::default(),
            script: vec![Ok(vec![]), Ok(vec![]), Ok(vec![ev.clone()])],
        };
        let start = tokio::time::Instant::now();
        let url = wait_for_wallet(&fake, &Default::default(), &secret, POLL, TOTAL_WAIT).await.unwrap();
        assert_eq!(url, nwc_url(&ev, &secret, &NWC_RELAYS));
        assert_eq!(*fake.calls.lock().unwrap(), 3);
        assert_eq!(start.elapsed(), POLL * 2);
        assert!(nwc::prelude::NostrWalletConnectURI::parse(&url).is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_before_deadline_gives_timeout_text() {
        let fake = Fake { calls: Default::default(), script: vec![Ok(vec![])] };
        let start = tokio::time::Instant::now();
        let err = wait_for_wallet(&fake, &Default::default(), "00", POLL, TOTAL_WAIT).await.unwrap_err();
        assert_eq!(err, crate::i18n::t("settings.wallet_1tap_timeout"));
        assert!(start.elapsed() <= TOTAL_WAIT);
        assert!(start.elapsed() >= TOTAL_WAIT - POLL);
        assert_eq!(*fake.calls.lock().unwrap(), 9);
    }

    #[tokio::test(start_paused = true)]
    async fn relay_error_is_returned_at_deadline() {
        let fake = Fake { calls: Default::default(), script: vec![Err("relay down".into())] };
        let err = wait_for_wallet(&fake, &Default::default(), "00", POLL, TOTAL_WAIT).await.unwrap_err();
        assert_eq!(err, "relay down");
    }

    #[tokio::test(start_paused = true)]
    async fn return_signal_fetches_without_waiting_the_poll() {
        let wallet = Keys::generate();
        let client = Keys::generate();
        let ev = info_event(&wallet, &client.public_key(), 5);
        let fake = Fake {
            calls: Default::default(),
            script: vec![Ok(vec![]), Ok(vec![ev])],
        };
        let returned = tokio::sync::Notify::new();
        returned.notify_one();
        let start = tokio::time::Instant::now();
        wait_for_wallet(&fake, &returned, "00", POLL, TOTAL_WAIT).await.unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[test]
    fn no_wallet_info_yields_none() {
        assert!(latest_wallet_info(Vec::new()).is_none());
    }

    #[test]
    fn walletauth_url_carries_client_key_and_params() {
        let client = Keys::generate().public_key();
        let url = walletauth_url(&client, &NWC_RELAYS);
        assert!(url.starts_with(&format!("nostr+walletauth://{}?relay=wss%3A%2F%2Frelay.getalby.com%2Fv1&", client.to_hex())));
        assert!(url.contains("request_methods=pay_invoice%20get_info%20get_balance"));
        assert!(url.contains("return_to=zswc%3A%2F%2Fhandler"));
    }
}
