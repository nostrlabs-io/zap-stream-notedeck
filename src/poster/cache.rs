use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub const POSTER_TTL: Duration = Duration::from_secs(600);

pub fn is_fresh(age: Duration) -> bool {
    age < POSTER_TTL
}

pub struct TtlCache<V> {
    ttl: Duration,
    entries: HashMap<String, (V, Instant)>,
}

impl<V> TtlCache<V> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: &str, value: V, now: Instant) {
        self.entries.insert(key.to_string(), (value, now));
    }

    pub fn get_fresh(&self, key: &str, now: Instant) -> Option<&V> {
        self.entries
            .get(key)
            .filter(|(_, at)| now.saturating_duration_since(*at) < self.ttl)
            .map(|(v, _)| v)
    }

    pub fn get_any(&self, key: &str) -> Option<&V> {
        self.entries.get(key).map(|(v, _)| v)
    }
}

#[derive(Default)]
pub struct FailureMemo {
    urls: HashSet<String>,
    host_thumbs: HashMap<String, bool>,
}

impl FailureMemo {
    pub fn has_failed(&self, url: &str) -> bool {
        self.urls.contains(url)
    }

    pub fn mark_failed(&mut self, url: &str) {
        self.urls.insert(url.to_string());
    }

    pub fn host_thumb(&self, host: &str) -> Option<bool> {
        self.host_thumbs.get(host).copied()
    }

    pub fn set_host_thumb(&mut self, host: &str, serves: bool) {
        self.host_thumbs.insert(host.to_string(), serves);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_is_fresh_until_ttl_then_expires() {
        let mut c = TtlCache::new(POSTER_TTL);
        let t0 = Instant::now();
        c.insert("a", 1, t0);
        assert_eq!(c.get_fresh("a", t0), Some(&1));
        assert_eq!(c.get_fresh("a", t0 + POSTER_TTL - Duration::from_secs(1)), Some(&1));
        assert_eq!(c.get_fresh("a", t0 + POSTER_TTL), None);
        assert_eq!(c.get_any("a"), Some(&1));
    }

    #[test]
    fn reinsert_restarts_the_clock() {
        let mut c = TtlCache::new(POSTER_TTL);
        let t0 = Instant::now();
        c.insert("a", 1, t0);
        let t1 = t0 + POSTER_TTL;
        c.insert("a", 2, t1);
        assert_eq!(c.get_fresh("a", t1 + Duration::from_secs(5)), Some(&2));
    }

    #[test]
    fn ttl_is_ten_minutes() {
        assert_eq!(POSTER_TTL, Duration::from_secs(600));
        assert!(is_fresh(Duration::from_secs(599)));
        assert!(!is_fresh(Duration::from_secs(600)));
    }

    #[test]
    fn failure_memo_remembers_urls_and_hosts() {
        let mut m = FailureMemo::default();
        assert!(!m.has_failed("u"));
        m.mark_failed("u");
        assert!(m.has_failed("u"));
        assert_eq!(m.host_thumb("h"), None);
        m.set_host_thumb("h", false);
        assert_eq!(m.host_thumb("h"), Some(false));
    }
}
