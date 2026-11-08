use nostr::{Event, Filter, Kind, PublicKey, Timestamp};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const CHANNEL_ID: &str = "go_live";
pub const CHANNEL_NAME: &str = "Go live alerts";
pub const STREAM_KIND: u16 = 30_311;
pub const MAX_STARTED_AGE: u64 = 2 * 60 * 60;
const SEEN_KEEP: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NotifyState {
    pub watched: BTreeSet<String>,
    pub relays: Vec<String>,
    pub seen: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub coordinate: String,
    pub host: String,
    pub title: String,
    pub url: String,
}

impl NotifyState {
    pub fn file(dir: &Path) -> PathBuf {
        dir.join("notify.txt")
    }

    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            match (parts.next(), parts.next(), parts.next()) {
                (Some("watch"), Some(pk), None) if is_hex_key(pk) => {
                    s.watched.insert(pk.to_string());
                }
                (Some("relay"), Some(url), None) => s.relays.push(url.to_string()),
                (Some("seen"), Some(coord), Some(at)) => {
                    if let Ok(at) = at.parse() {
                        s.seen.push((coord.to_string(), at));
                    }
                }
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for w in &self.watched {
            out.push_str(&format!("watch {w}\n"));
        }
        for r in &self.relays {
            out.push_str(&format!("relay {r}\n"));
        }
        for (c, at) in &self.seen {
            out.push_str(&format!("seen {c} {at}\n"));
        }
        out
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(Self::file(dir))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(Self::file(dir), self.to_text());
    }

    pub fn is_watched(&self, host_hex: &str) -> bool {
        self.watched.contains(host_hex)
    }

    pub fn set_watched(&mut self, host_hex: &str, on: bool) {
        if on {
            self.watched.insert(host_hex.to_string());
        } else {
            self.watched.remove(host_hex);
        }
    }

    pub fn set_relays(&mut self, relays: &[String]) {
        self.relays = relays.to_vec();
    }

    fn was_seen(&self, coordinate: &str) -> bool {
        self.seen.iter().any(|(c, _)| c == coordinate)
    }

    pub fn alerts(&mut self, events: &[Event], now: u64) -> Vec<Alert> {
        let mut newest: Vec<&Event> = Vec::new();
        for ev in events.iter().filter(|e| e.kind == Kind::Custom(STREAM_KIND)) {
            let coord = coordinate(ev);
            match newest.iter_mut().find(|e| coordinate(e) == coord) {
                Some(slot) if slot.created_at < ev.created_at => *slot = ev,
                Some(_) => {}
                None => newest.push(ev),
            }
        }
        let mut out = Vec::new();
        for ev in newest {
            let host = host_of(ev);
            let coord = coordinate(ev);
            if !self.is_watched(&host) || self.was_seen(&coord) || !is_fresh_live(ev, now) {
                continue;
            }
            self.seen.push((coord.clone(), now));
            out.push(Alert {
                url: stream_url(ev),
                title: tag(ev, "title").unwrap_or_default(),
                coordinate: coord,
                host,
            });
        }
        self.seen.retain(|(_, at)| now.saturating_sub(*at) < SEEN_KEEP);
        out
    }

    pub fn filters(&self) -> Vec<Filter> {
        let hosts: Vec<PublicKey> = self
            .watched
            .iter()
            .filter_map(|h| PublicKey::from_hex(h).ok())
            .collect();
        if hosts.is_empty() {
            return Vec::new();
        }
        let since = Timestamp::from(Timestamp::now().as_u64().saturating_sub(MAX_STARTED_AGE));
        let base = Filter::new().kind(Kind::Custom(STREAM_KIND)).since(since).limit(100);
        vec![base.clone().authors(hosts.clone()), base.pubkeys(hosts)]
    }
}

fn is_hex_key(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn tag(ev: &Event, name: &str) -> Option<String> {
    ev.tags
        .iter()
        .map(|t| t.as_slice())
        .find(|t| t.first().map(String::as_str) == Some(name))
        .and_then(|t| t.get(1).cloned())
}

fn host_of(ev: &Event) -> String {
    ev.tags
        .iter()
        .map(|t| t.as_slice())
        .find(|t| t.len() >= 4 && t[0] == "p" && t[3] == "host")
        .map(|t| t[1].to_lowercase())
        .unwrap_or_else(|| ev.pubkey.to_hex())
}

fn coordinate(ev: &Event) -> String {
    format!(
        "{}:{}:{}",
        STREAM_KIND,
        ev.pubkey.to_hex(),
        tag(ev, "d").unwrap_or_default()
    )
}

fn is_fresh_live(ev: &Event, now: u64) -> bool {
    if tag(ev, "status").as_deref() != Some("live") {
        return false;
    }
    let started = tag(ev, "starts")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(ev.created_at.as_u64());
    now.saturating_sub(started) <= MAX_STARTED_AGE
}

pub fn stream_url(ev: &Event) -> String {
    use nostr::nips::nip01::Coordinate;
    use nostr::nips::nip19::ToBech32;
    let c = Coordinate::new(Kind::Custom(STREAM_KIND), ev.pubkey).identifier(tag(ev, "d").unwrap_or_default());
    format!("https://zap.stream/{}", c.to_bech32().unwrap_or_default())
}

pub fn alert_title(name: &str) -> String {
    crate::i18n::tf("notify.went_live", &[("name", name)])
}

pub fn alert_json(alerts: &[(Alert, String)]) -> String {
    let list: Vec<nostr::serde_json::Value> = alerts
        .iter()
        .map(|(a, name)| {
            nostr::serde_json::json!({
                "id": a.coordinate,
                "title": alert_title(name),
                "body": a.title,
                "url": a.url,
            })
        })
        .collect();
    nostr::serde_json::Value::Array(list).to_string()
}

pub fn fetch(filters: Vec<Filter>, relays: &[String], timeout: Duration) -> Vec<Event> {
    use nostr_relay_pool::{FilterOptions, RelayOptions, RelayPool};
    enostr::install_crypto();
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(_) => return Vec::new(),
    };
    rt.block_on(async {
        let pool = RelayPool::default();
        for r in relays {
            let _ = pool.add_relay(r.as_str(), RelayOptions::default()).await;
        }
        pool.connect(Some(timeout)).await;
        let events = pool
            .fetch_events(filters, timeout, FilterOptions::ExitOnEOSE)
            .await
            .map(|e| e.into_iter().collect())
            .unwrap_or_default();
        pool.shutdown().await.ok();
        events
    })
}

pub fn names(events: &[Event]) -> std::collections::HashMap<String, String> {
    events
        .iter()
        .filter(|e| e.kind == Kind::Metadata)
        .filter_map(|e| {
            let v: nostr::serde_json::Value = nostr::serde_json::from_str(&e.content).ok()?;
            let name = v
                .get("display_name")
                .and_then(|n| n.as_str())
                .filter(|n| !n.is_empty())
                .or_else(|| v.get("name").and_then(|n| n.as_str()))?;
            Some((e.pubkey.to_hex(), name.to_string()))
        })
        .collect()
}

static CHECK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn check(dir: &Path, now: u64) -> String {
    check_with(dir, now, fetch)
}

fn check_with(dir: &Path, now: u64, fetch: impl Fn(Vec<Filter>, &[String], Duration) -> Vec<Event>) -> String {
    let _guard = CHECK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(code) = crate::i18n::load_saved(dir).or_else(|| crate::i18n::detect_system_locale().map(str::to_string)) {
        crate::i18n::set_locale(&code);
    }
    let mut state = NotifyState::load(dir);
    let filters = state.filters();
    if filters.is_empty() {
        return "[]".to_string();
    }
    let relays = if state.relays.is_empty() {
        crate::relays::DEFAULT_RELAYS.iter().map(|r| r.to_string()).collect()
    } else {
        state.relays.clone()
    };
    let events = fetch(filters, &relays, Duration::from_secs(10));
    let alerts = state.alerts(&events, now);
    state.save(dir);
    if alerts.is_empty() {
        return "[]".to_string();
    }
    let hosts: Vec<PublicKey> = alerts.iter().filter_map(|a| PublicKey::from_hex(&a.host).ok()).collect();
    let profiles = fetch(
        vec![Filter::new().kind(Kind::Metadata).authors(hosts)],
        &relays,
        Duration::from_secs(5),
    );
    let names = names(&profiles);
    let named: Vec<(Alert, String)> = alerts
        .into_iter()
        .map(|a| {
            let name = names.get(&a.host).cloned().unwrap_or_else(|| a.host[..8].to_string());
            (a, name)
        })
        .collect();
    alert_json(&named)
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_stream_zap_app_LiveCheckWorker_nativeCheck<'l>(
    mut env: jni::JNIEnv<'l>,
    _this: jni::objects::JObject<'l>,
    dir: jni::objects::JString<'l>,
) -> jni::sys::jstring {
    let dir: String = env.get_string(&dir).map(Into::into).unwrap_or_default();
    let out = std::panic::catch_unwind(|| check(Path::new(&dir), Timestamp::now().as_u64()))
        .unwrap_or_else(|_| "[]".to_string());
    env.new_string(out).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Tag};

    fn stream(keys: &Keys, d: &str, status: &str, starts: u64, at: u64) -> Event {
        EventBuilder::new(Kind::Custom(STREAM_KIND), "")
            .tags([
                Tag::parse(["d", d]).unwrap(),
                Tag::parse(["title", "Morning show"]).unwrap(),
                Tag::parse(["status", status]).unwrap(),
                Tag::parse(["starts", &starts.to_string()]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(at))
            .sign_with_keys(keys)
            .unwrap()
    }

    fn watching(keys: &Keys) -> NotifyState {
        let mut s = NotifyState::default();
        s.set_watched(&keys.public_key().to_hex(), true);
        s
    }

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn alerts_once_for_a_watched_live_stream() {
        let k = Keys::generate();
        let mut s = watching(&k);
        let ev = stream(&k, "a", "live", NOW - 60, NOW - 30);
        let first = s.alerts(&[ev.clone()], NOW);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].title, "Morning show");
        assert!(first[0].url.starts_with("https://zap.stream/naddr1"));
        assert!(s.alerts(&[ev], NOW + 900).is_empty());
    }

    #[test]
    fn ignores_unwatched_ended_and_old_streams() {
        let watched = Keys::generate();
        let other = Keys::generate();
        let mut s = watching(&watched);
        let events = [
            stream(&other, "x", "live", NOW - 60, NOW - 30),
            stream(&watched, "ended", "ended", NOW - 60, NOW - 30),
            stream(&watched, "old", "live", NOW - MAX_STARTED_AGE - 10, NOW - 30),
        ];
        assert!(s.alerts(&events, NOW).is_empty());
    }

    #[test]
    fn newest_version_of_a_stream_decides() {
        let k = Keys::generate();
        let mut s = watching(&k);
        let planned = stream(&k, "a", "planned", NOW + 600, NOW - 100);
        let live = stream(&k, "a", "live", NOW - 60, NOW - 30);
        assert_eq!(s.alerts(&[live, planned], NOW).len(), 1);
    }

    #[test]
    fn two_streams_of_one_host_alert_separately() {
        let k = Keys::generate();
        let mut s = watching(&k);
        let events = [stream(&k, "a", "live", NOW - 60, NOW - 30), stream(&k, "b", "live", NOW - 60, NOW - 30)];
        assert_eq!(s.alerts(&events, NOW).len(), 2);
    }

    #[test]
    fn seen_entries_expire_after_a_week() {
        let k = Keys::generate();
        let mut s = watching(&k);
        s.alerts(&[stream(&k, "a", "live", NOW - 60, NOW - 30)], NOW);
        assert_eq!(s.seen.len(), 1);
        s.alerts(&[], NOW + SEEN_KEEP + 1);
        assert!(s.seen.is_empty());
    }

    #[test]
    fn host_tag_overrides_the_author() {
        let author = Keys::generate();
        let host = Keys::generate();
        let mut s = watching(&host);
        let ev = EventBuilder::new(Kind::Custom(STREAM_KIND), "")
            .tags([
                Tag::parse(["d", "a"]).unwrap(),
                Tag::parse(["status", "live"]).unwrap(),
                Tag::parse(["starts", &(NOW - 60).to_string()]).unwrap(),
                Tag::parse(["p", &host.public_key().to_hex(), "", "host"]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(NOW - 30))
            .sign_with_keys(&author)
            .unwrap();
        let a = s.alerts(&[ev], NOW);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].host, host.public_key().to_hex());
    }

    #[test]
    fn state_round_trips_and_drops_bad_lines() {
        let mut s = NotifyState::default();
        s.set_watched(&"ab".repeat(32), true);
        s.set_relays(&["wss://relay.example".to_string()]);
        s.seen.push(("30311:x:y".into(), 5));
        let text = s.to_text() + "watch nothex\ngarbage\n";
        assert_eq!(NotifyState::parse(&text), s);
    }

    #[test]
    fn no_filter_without_watched_hosts() {
        assert!(NotifyState::default().filters().is_empty());
        let k = Keys::generate();
        assert_eq!(watching(&k).filters().len(), 2);
    }

    #[test]
    #[ignore]
    fn check_reaches_live_relays() {
        let dir = tempfile::tempdir().unwrap();
        let live = fetch(
            vec![Filter::new().kind(Kind::Custom(STREAM_KIND)).limit(50)],
            &crate::relays::DEFAULT_RELAYS.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
            Duration::from_secs(10),
        );
        let now = Timestamp::now().as_u64();
        let host = live
            .iter()
            .find(|e| is_fresh_live(e, now))
            .map(host_of)
            .expect("a live stream on the default relays");
        let mut s = NotifyState::default();
        s.set_watched(&host, true);
        s.save(dir.path());
        let out = check(dir.path(), now);
        assert!(out.contains("https://zap.stream/naddr1"), "{out}");
        assert_eq!(check(dir.path(), now), "[]");
    }

    #[test]
    fn concurrent_checks_alert_once() {
        let k = Keys::generate();
        let dir = tempfile::tempdir().unwrap();
        watching(&k).save(dir.path());
        let ev = stream(&k, "a", "live", NOW - 60, NOW - 30);
        let alerts: Vec<String> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let ev = ev.clone();
                    let dir = dir.path();
                    s.spawn(move || {
                        check_with(dir, NOW, move |f, _, _| {
                            std::thread::sleep(Duration::from_millis(20));
                            if f.iter().any(|f| f.kinds.as_ref().map_or(false, |k| k.contains(&Kind::Metadata))) {
                                Vec::new()
                            } else {
                                vec![ev.clone()]
                            }
                        })
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert_eq!(alerts.iter().filter(|a| a.as_str() != "[]").count(), 1, "{alerts:?}");
    }

    #[test]
    fn channel_id_is_stable() {
        assert_eq!(CHANNEL_ID, "go_live");
    }

    #[test]
    fn alert_json_carries_deep_link() {
        let a = Alert {
            coordinate: "30311:h:d".into(),
            host: "h".into(),
            title: "Show".into(),
            url: "https://zap.stream/naddr1x".into(),
        };
        let v: nostr::serde_json::Value = nostr::serde_json::from_str(&alert_json(&[(a, "Alice".into())])).unwrap();
        assert_eq!(v[0]["url"], "https://zap.stream/naddr1x");
        assert_eq!(v[0]["body"], "Show");
        assert!(v[0]["title"].as_str().unwrap().contains("Alice"));
    }
}
