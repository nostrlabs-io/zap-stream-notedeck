use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Healthy,
    Stalled,
    Failed,
}

const STALL_LIMIT: Duration = Duration::from_secs(15);
const RETRY_STEP: Duration = Duration::from_secs(2);
const RETRY_CAP: u32 = 5;

pub fn should_retry(health: Health, unhealthy_for: Duration, attempts: u32) -> bool {
    match health {
        Health::Healthy => false,
        Health::Stalled => unhealthy_for >= STALL_LIMIT,
        Health::Failed => unhealthy_for >= RETRY_STEP * (attempts + 1).min(RETRY_CAP),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Play,
    NotStarted,
    Offline,
}

pub fn playback_for(
    status: &crate::stream_info::StreamStatus,
    has_streaming: bool,
    has_recording: bool,
) -> Playback {
    use crate::stream_info::StreamStatus::*;
    match status {
        Planned => Playback::NotStarted,
        Live if has_streaming || has_recording => Playback::Play,
        Ended if has_recording => Playback::Play,
        _ => Playback::Offline,
    }
}

pub fn format_elapsed(total_secs: u64) -> String {
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn format_countdown(total_secs: u64) -> String {
    let d = total_secs / 86_400;
    let h = (total_secs % 86_400) / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m {s}s")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_never_retries() {
        assert!(!should_retry(Health::Healthy, Duration::from_secs(999), 0));
    }

    #[test]
    fn stall_retries_only_after_limit() {
        assert!(!should_retry(Health::Stalled, Duration::from_secs(14), 0));
        assert!(should_retry(Health::Stalled, Duration::from_secs(15), 0));
    }

    #[test]
    fn failure_backs_off_then_caps() {
        assert!(!should_retry(Health::Failed, Duration::from_secs(1), 0));
        assert!(should_retry(Health::Failed, Duration::from_secs(2), 0));
        assert!(!should_retry(Health::Failed, Duration::from_secs(5), 2));
        assert!(should_retry(Health::Failed, Duration::from_secs(10), 50));
        assert!(!should_retry(Health::Failed, Duration::from_secs(9), 50));
    }

    #[test]
    fn picks_playback_from_status() {
        use crate::stream_info::StreamStatus::*;
        assert_eq!(playback_for(&Planned, true, true), Playback::NotStarted);
        assert_eq!(playback_for(&Live, true, false), Playback::Play);
        assert_eq!(playback_for(&Ended, true, false), Playback::Offline);
        assert_eq!(playback_for(&Ended, false, true), Playback::Play);
        assert_eq!(playback_for(&Live, false, false), Playback::Offline);
    }

    #[test]
    fn formats_elapsed_and_countdown() {
        assert_eq!(format_elapsed(75), "1:15");
        assert_eq!(format_elapsed(3725), "1:02:05");
        assert_eq!(format_countdown(45), "45s");
        assert_eq!(format_countdown(125), "2m 5s");
        assert_eq!(format_countdown(3725), "1h 2m 5s");
        assert_eq!(format_countdown(90_000), "1d 1h 0m");
    }
}
