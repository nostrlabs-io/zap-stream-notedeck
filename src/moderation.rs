use nostr::prelude::hex;
use nostr::{EventBuilder, Kind, Metadata, Tag, Timestamp};

const REPORT_KIND: u16 = 1984;
const DELETION_KIND: u16 = 5;
const REPLACEABLE_KINDS: [u32; 3] = [0, 3, 10_000];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReportType {
    Nudity,
    Profanity,
    Illegal,
    Spam,
    Impersonation,
    Other,
}

impl ReportType {
    pub const ALL: [ReportType; 6] = [
        ReportType::Nudity,
        ReportType::Profanity,
        ReportType::Illegal,
        ReportType::Spam,
        ReportType::Impersonation,
        ReportType::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ReportType::Nudity => "nudity",
            ReportType::Profanity => "profanity",
            ReportType::Illegal => "illegal",
            ReportType::Spam => "spam",
            ReportType::Impersonation => "impersonation",
            ReportType::Other => "other",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            ReportType::Nudity => "report.type.nudity",
            ReportType::Profanity => "report.type.profanity",
            ReportType::Illegal => "report.type.illegal",
            ReportType::Spam => "report.type.spam",
            ReportType::Impersonation => "report.type.impersonation",
            ReportType::Other => "report.type.other",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum ReportTarget {
    Profile {
        pubkey: [u8; 32],
    },
    Message {
        id: [u8; 32],
        author: [u8; 32],
        relays: Vec<String>,
    },
    Stream {
        coordinate: String,
        host: [u8; 32],
        relays: Vec<String>,
    },
}

impl ReportTarget {
    pub fn author(&self) -> [u8; 32] {
        match self {
            ReportTarget::Profile { pubkey } => *pubkey,
            ReportTarget::Message { author, .. } => *author,
            ReportTarget::Stream { host, .. } => *host,
        }
    }

    pub fn relays(&self) -> &[String] {
        match self {
            ReportTarget::Profile { .. } => &[],
            ReportTarget::Message { relays, .. } | ReportTarget::Stream { relays, .. } => relays,
        }
    }
}

fn tag(parts: &[&str]) -> Option<Tag> {
    Tag::parse(parts.iter().map(|p| p.to_string()).collect::<Vec<_>>()).ok()
}

pub fn report_builder(target: &ReportTarget, kind: ReportType) -> EventBuilder {
    let reason = kind.as_str();
    let author = hex::encode(target.author());
    let mut tags = Vec::new();
    match target {
        ReportTarget::Profile { .. } => {}
        ReportTarget::Message { id, .. } => tags.push(tag(&["e", &hex::encode(id), reason])),
        ReportTarget::Stream { coordinate, .. } => tags.push(tag(&["a", coordinate, reason])),
    }
    tags.push(tag(&["p", &author, reason]));
    EventBuilder::new(Kind::from(REPORT_KIND), "").tags(tags.into_iter().flatten())
}

pub fn relays_from_tags(tags: &[Vec<String>]) -> Vec<String> {
    tags.iter()
        .find(|t| t.first().map(String::as_str) == Some("relays"))
        .map(|t| t.iter().skip(1).cloned().collect())
        .unwrap_or_default()
}

pub fn retain_unblocked<T>(
    items: &mut Vec<T>,
    host: impl Fn(&T) -> [u8; 32],
    is_blocked: impl Fn(&[u8; 32]) -> bool,
) {
    items.retain(|i| !is_blocked(&host(i)));
}

#[derive(Clone, PartialEq, Debug)]
pub struct OwnEvent {
    pub kind: u32,
    pub id: [u8; 32],
    pub d_tag: Option<String>,
}

pub struct DeleteAccountEvents {
    pub deletion_request: EventBuilder,
    pub blank_profile: EventBuilder,
}

fn is_addressable(kind: u32) -> bool {
    (30_000..40_000).contains(&kind)
}

pub fn delete_account_events(me: &[u8; 32], events: &[OwnEvent], now: u64) -> DeleteAccountEvents {
    let me_hex = hex::encode(me);
    let mut tags: Vec<Vec<String>> = Vec::new();
    let mut kinds: Vec<u32> = Vec::new();
    let mut push_unique = |tags: &mut Vec<Vec<String>>, t: Vec<String>| {
        if !tags.contains(&t) {
            tags.push(t);
        }
    };
    for e in events {
        if !kinds.contains(&e.kind) {
            kinds.push(e.kind);
        }
        let t = if is_addressable(e.kind) {
            vec![
                "a".to_string(),
                format!("{}:{}:{}", e.kind, me_hex, e.d_tag.clone().unwrap_or_default()),
            ]
        } else {
            vec!["e".to_string(), hex::encode(e.id)]
        };
        push_unique(&mut tags, t);
    }
    for kind in REPLACEABLE_KINDS {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        push_unique(&mut tags, vec!["a".to_string(), format!("{kind}:{me_hex}:")]);
    }
    for kind in kinds {
        push_unique(&mut tags, vec!["k".to_string(), kind.to_string()]);
    }
    let deletion_request = EventBuilder::new(Kind::from(DELETION_KIND), "")
        .tags(tags.into_iter().filter_map(|t| Tag::parse(t).ok()))
        .custom_created_at(Timestamp::from(now));
    let blank_profile = EventBuilder::metadata(&Metadata::new().name("deleted"))
        .custom_created_at(Timestamp::from(now + 1));
    DeleteAccountEvents {
        deletion_request,
        blank_profile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Event, Keys};

    fn sign(b: EventBuilder) -> Event {
        b.sign_with_keys(&Keys::generate()).unwrap()
    }

    fn tags_of(e: &Event) -> Vec<Vec<String>> {
        e.tags.iter().map(|t| t.as_slice().to_vec()).collect()
    }

    fn strs(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn profile_report_has_only_p_tag_with_type() {
        let pk = [7u8; 32];
        let ev = sign(report_builder(&ReportTarget::Profile { pubkey: pk }, ReportType::Spam));
        assert_eq!(ev.kind.as_u16(), 1984);
        assert_eq!(tags_of(&ev), vec![strs(&["p", &hex::encode(pk), "spam"])]);
    }

    #[test]
    fn message_report_has_e_and_p_tags() {
        let target = ReportTarget::Message {
            id: [1u8; 32],
            author: [2u8; 32],
            relays: vec![],
        };
        let ev = sign(report_builder(&target, ReportType::Profanity));
        assert_eq!(ev.kind.as_u16(), 1984);
        assert_eq!(
            tags_of(&ev),
            vec![
                strs(&["e", &hex::encode([1u8; 32]), "profanity"]),
                strs(&["p", &hex::encode([2u8; 32]), "profanity"]),
            ]
        );
    }

    #[test]
    fn stream_report_has_a_and_p_tags() {
        let host = [3u8; 32];
        let coordinate = format!("30311:{}:abc", hex::encode(host));
        let target = ReportTarget::Stream {
            coordinate: coordinate.clone(),
            host,
            relays: vec![],
        };
        let ev = sign(report_builder(&target, ReportType::Illegal));
        assert_eq!(ev.kind.as_u16(), 1984);
        assert_eq!(
            tags_of(&ev),
            vec![
                strs(&["a", &coordinate, "illegal"]),
                strs(&["p", &hex::encode(host), "illegal"]),
            ]
        );
    }

    #[test]
    fn every_report_type_uses_a_nip56_name() {
        let names: Vec<&str> = ReportType::ALL.iter().map(|t| t.as_str()).collect();
        assert_eq!(
            names,
            ["nudity", "profanity", "illegal", "spam", "impersonation", "other"]
        );
    }

    #[test]
    fn relays_tag_is_read_from_stream_tags() {
        let tags = vec![strs(&["d", "x"]), strs(&["relays", "wss://a", "wss://b"])];
        assert_eq!(relays_from_tags(&tags), strs(&["wss://a", "wss://b"]));
        assert!(relays_from_tags(&[]).is_empty());
    }

    fn own_events() -> Vec<OwnEvent> {
        vec![
            OwnEvent { kind: 0, id: [10u8; 32], d_tag: None },
            OwnEvent { kind: 3, id: [11u8; 32], d_tag: None },
            OwnEvent { kind: 10_000, id: [12u8; 32], d_tag: None },
            OwnEvent { kind: 30_311, id: [13u8; 32], d_tag: Some("stream-1".to_string()) },
            OwnEvent { kind: 1311, id: [14u8; 32], d_tag: None },
        ]
    }

    #[test]
    fn blank_profile_is_kind_0_named_deleted_without_other_fields() {
        let me = [9u8; 32];
        let out = delete_account_events(&me, &own_events(), 1000);
        let ev = sign(out.blank_profile);
        assert_eq!(ev.kind.as_u16(), 0);
        let meta: nostr::serde_json::Value = nostr::serde_json::from_str(&ev.content).unwrap();
        assert_eq!(meta["name"], "deleted");
        for field in ["picture", "about", "lud16", "banner", "nip05", "website", "display_name"] {
            assert!(meta.get(field).is_none(), "{field} present");
        }
    }

    #[test]
    fn deletion_request_targets_every_own_event() {
        let me = [9u8; 32];
        let me_hex = hex::encode(me);
        let out = delete_account_events(&me, &own_events(), 1000);
        let ev = sign(out.deletion_request);
        assert_eq!(ev.kind.as_u16(), 5);
        let tags = tags_of(&ev);
        for id in [10u8, 11, 12, 14] {
            assert!(tags.contains(&strs(&["e", &hex::encode([id; 32])])), "missing e {id}");
        }
        assert!(tags.contains(&strs(&["a", &format!("30311:{me_hex}:stream-1")])));
        assert!(!tags.contains(&strs(&["e", &hex::encode([13u8; 32])])));
        for kind in ["0", "3", "10000", "30311", "1311"] {
            assert!(tags.contains(&strs(&["k", kind])), "missing k {kind}");
        }
    }

    #[test]
    fn deletion_covers_replaceable_lists_even_when_not_found_locally() {
        let me = [9u8; 32];
        let me_hex = hex::encode(me);
        let out = delete_account_events(&me, &[], 1000);
        let tags = tags_of(&sign(out.deletion_request));
        for kind in [0, 3, 10_000] {
            assert!(tags.contains(&strs(&["a", &format!("{kind}:{me_hex}:")])));
        }
    }

    #[test]
    fn blank_profile_is_newer_than_the_deletion_request() {
        let out = delete_account_events(&[9u8; 32], &own_events(), 1000);
        let deletion = sign(out.deletion_request);
        let blank = sign(out.blank_profile);
        assert!(blank.created_at > deletion.created_at);
    }

    #[test]
    fn blocked_hosts_are_removed_from_stream_lists() {
        let mut streams = vec![("a", [1u8; 32]), ("b", [2u8; 32]), ("c", [1u8; 32])];
        let blocked = [1u8; 32];
        retain_unblocked(&mut streams, |s| s.1, |h| *h == blocked);
        assert_eq!(streams, vec![("b", [2u8; 32])]);
    }
}
