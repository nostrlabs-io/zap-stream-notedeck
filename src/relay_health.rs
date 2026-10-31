use std::time::{Duration, Instant};

pub const GRACE: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Health {
    Online,
    Connecting,
    Offline,
}

#[derive(Default)]
pub struct RelayHealth {
    down_since: Option<Instant>,
}

impl RelayHealth {
    pub fn observe(&mut self, connected: usize, connecting: usize, now: Instant) -> Health {
        if connected > 0 {
            self.down_since = None;
            return Health::Online;
        }
        let since = *self.down_since.get_or_insert(now);
        if connecting > 0 || now.duration_since(since) < GRACE {
            Health::Connecting
        } else {
            Health::Offline
        }
    }
}

pub fn count(ctx: &notedeck::AppContext<'_>) -> (usize, usize) {
    let mut connected = 0;
    let mut connecting = 0;
    for r in ctx.remote.relay_inspect().known_relay_infos() {
        match r.status {
            enostr::RelayStatus::Connected => connected += 1,
            enostr::RelayStatus::Connecting => connecting += 1,
            enostr::RelayStatus::Disconnected => {}
        }
    }
    (connected, connecting)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_connecting_within_grace_then_offline() {
        let mut h = RelayHealth::default();
        let t0 = Instant::now();
        assert_eq!(h.observe(0, 0, t0), Health::Connecting);
        assert_eq!(h.observe(0, 0, t0 + Duration::from_secs(2)), Health::Connecting);
        assert_eq!(h.observe(0, 0, t0 + GRACE + Duration::from_secs(1)), Health::Offline);
    }

    #[test]
    fn connected_relay_clears_offline() {
        let mut h = RelayHealth::default();
        let t0 = Instant::now();
        h.observe(0, 0, t0);
        assert_eq!(h.observe(0, 0, t0 + GRACE * 2), Health::Offline);
        assert_eq!(h.observe(1, 0, t0 + GRACE * 3), Health::Online);
        assert_eq!(h.observe(0, 0, t0 + GRACE * 3), Health::Connecting);
    }

    #[test]
    fn pending_connection_is_never_offline() {
        let mut h = RelayHealth::default();
        let t0 = Instant::now();
        h.observe(0, 1, t0);
        assert_eq!(h.observe(0, 1, t0 + GRACE * 5), Health::Connecting);
    }
}
