use crate::chat_model::*;
use crate::chat_text::{self, embed_target, EmbedTarget, Segment};
use crate::link::NostrLink;
use crate::note_ref::NoteRef;
use crate::note_util::NoteUtil;
use crate::stream_info::StreamInfo;
use crate::zap::Zap;
use nostr::prelude::hex;
use nostrdb::{Filter, Ndb, NoteKey, Transaction};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub enum RowKind {
    Message {
        id: [u8; 32],
        author: Pubkey,
        segments: Vec<Segment>,
        content: String,
    },
    Zap {
        sender: Pubkey,
        sats: u64,
        message: String,
    },
    Raid {
        raid: Raid,
        created_at: u64,
    },
    Timeout(Timeout),
    BadgeAward {
        badge: String,
        users: Vec<Pubkey>,
    },
}

#[derive(Clone)]
pub struct Row {
    pub key: NoteKey,
    pub created_at: u64,
    pub id: [u8; 32],
    pub kind: RowKind,
}

#[derive(Clone, Default)]
pub struct BadgeDef {
    pub image: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone)]
pub struct EmbedInfo {
    pub link: NostrLink,
    pub kind: u32,
    pub author: Pubkey,
    pub content: String,
}

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Want {
    Reactions(Vec<[u8; 32]>),
    Coordinate(u32, Pubkey, String),
    Event([u8; 32]),
}

impl Want {
    pub fn filter(&self) -> Filter {
        match self {
            Want::Reactions(ids) => Filter::new()
                .kinds([7, 9735])
                .events(ids.iter())
                .limit(500)
                .build(),
            Want::Coordinate(kind, pk, d) => Filter::new()
                .kinds([*kind as u64])
                .authors([pk])
                .tags([d.as_str()], 'd')
                .limit(5)
                .build(),
            Want::Event(id) => Filter::new().ids([id]).limit(1).build(),
        }
    }
}

#[derive(Default)]
pub struct ChatData {
    pub rows: Vec<std::sync::Arc<Row>>,
    pub reactions: HashMap<[u8; 32], Vec<ReactionCount>>,
    pub zapped: HashMap<[u8; 32], u64>,
    pub badges_of: HashMap<Pubkey, Vec<String>>,
    pub badge_defs: HashMap<String, BadgeDef>,
    pub embeds: HashMap<String, EmbedInfo>,
    pub custom_emoji: Vec<(String, String)>,
    pub muted: HashSet<Pubkey>,
    pub my_mutes: HashSet<Pubkey>,
    pub my_mute_tags: Vec<Vec<String>>,
    pub my_mute_content: String,
    pub disabled_until: Option<Option<u64>>,
    pub wants: Vec<Want>,
}

#[derive(Default)]
pub struct Parsed {
    segments: HashMap<NoteKey, Vec<Segment>>,
}

pub struct BuildInput<'a> {
    pub ndb: &'a Ndb,
    pub tx: &'a Transaction,
    pub main: &'a HashSet<NoteRef>,
    pub aux: &'a HashSet<NoteRef>,
    pub host: Pubkey,
    pub me: Option<Pubkey>,
    pub now: u64,
}

fn expiration(tags: &[Vec<String>]) -> Option<u64> {
    tags.iter()
        .find(|t| t.len() > 1 && t[0] == "expiration")
        .and_then(|t| t[1].parse().ok())
}

fn pubkeys_of(tags: &[Vec<String>]) -> Vec<Pubkey> {
    tags.iter()
        .filter(|t| t.len() > 1 && t[0] == "p")
        .filter_map(|t| hex_pk(&t[1]))
        .collect()
}

fn first_tag<'a>(tags: &'a [Vec<String>], name: &str) -> Option<&'a str> {
    tags.iter()
        .find(|t| t.len() > 1 && t[0] == name)
        .map(|t| t[1].as_str())
}

fn last_e_target(tags: &[Vec<String>]) -> Option<[u8; 32]> {
    tags.iter()
        .rev()
        .find(|t| t.len() > 1 && t[0] == "e")
        .and_then(|t| hex_pk(&t[1]))
}

pub fn build(input: &BuildInput, parsed: &mut Parsed) -> ChatData {
    let mut data = ChatData::default();
    let mut moderators = vec![input.host];
    if let Some(me) = input.me {
        moderators.push(me);
    }

    let mut main: Vec<NoteRef> = input.main.iter().copied().collect();
    main.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.key.cmp(&b.key)));

    let mut lists = Vec::new();
    let mut timeouts: Vec<(NoteKey, [u8; 32], Timeout)> = Vec::new();
    let mut set_refs: Vec<String> = Vec::new();
    let mut my_list: Option<(u64, Vec<Vec<String>>, String)> = None;
    let mut candidates: Vec<(NoteKey, u32, u64, [u8; 32], Pubkey, Vec<Vec<String>>)> = Vec::new();
    let mut awards: Vec<(NoteKey, u64, [u8; 32], String, Vec<Pubkey>)> = Vec::new();

    for r in &main {
        let Ok(note) = input.ndb.get_note_by_key(input.tx, r.key) else {
            continue;
        };
        let kind = note.kind();
        let author = *note.pubkey();
        match kind {
            1311 | 9735 | 1312 => {
                candidates.push((r.key, kind, r.created_at, *note.id(), author, note.tags_vec()))
            }
            1314 => {
                let tags = note.tags_vec();
                let t = Timeout {
                    author,
                    targets: pubkeys_of(&tags),
                    created_at: r.created_at,
                    expiration: expiration(&tags),
                };
                timeouts.push((r.key, *note.id(), t));
            }
            10000 => {
                let tags = note.tags_vec();
                if Some(author) == input.me
                    && my_list.as_ref().is_none_or(|(c, _, _)| *c < r.created_at)
                {
                    my_list = Some((r.created_at, tags.clone(), note.content().to_string()));
                }
                lists.push(MuteList {
                    author,
                    created_at: r.created_at,
                    muted: pubkeys_of(&tags),
                });
            }
            8 if author == input.host => {
                let tags = note.tags_vec();
                if let Some(a) = first_tag(&tags, "a") {
                    awards.push((r.key, r.created_at, *note.id(), a.to_string(), pubkeys_of(&tags)));
                }
            }
            10030 => set_refs.extend(emoji_set_refs(&note.tags_vec())),
            30030 => collect_set(&mut data, &note.tags_vec()),
            _ => {}
        }
    }

    let mut reactions: Vec<Reaction> = Vec::new();
    let mut zap_events: Vec<([u8; 32], Pubkey, u64)> = Vec::new();
    for r in input.aux.iter() {
        let Ok(note) = input.ndb.get_note_by_key(input.tx, r.key) else {
            continue;
        };
        let kind = note.kind();
        let tags = note.tags_vec();
        match kind {
            7 => {
                if let Some(target) = last_e_target(&tags) {
                    reactions.push(Reaction {
                        target,
                        author: *note.pubkey(),
                        content: note.content().to_string(),
                        emoji_url: reaction_emoji_url(note.content(), &tags),
                    });
                }
            }
            9735 => {
                if let (Some(target), Ok(zap)) = (last_e_target(&tags), Zap::from_receipt(note)) {
                    zap_events.push((target, zap.sender, zap.amount / 1000));
                }
            }
            30030 => collect_set(&mut data, &tags),
            30009 => collect_badge(&mut data, &note),
            _ => {
                let link = NostrLink::from_note(&note);
                let author = if kind == 30_311 { *note.host() } else { *note.pubkey() };
                data.embeds.insert(
                    link.to_tag_value(),
                    EmbedInfo {
                        link,
                        kind,
                        author,
                        content: note.content().to_string(),
                    },
                );
            }
        }
    }

    let valid_timeouts: Vec<Timeout> = timeouts.iter().map(|(_, _, t)| t.clone()).collect();
    data.muted = muted_set(&moderators, &lists, &valid_timeouts, input.now);
    if let Some(me) = input.me {
        if let Some((_, tags, content)) = my_list {
            data.my_mutes = pubkeys_of(&tags).into_iter().collect();
            data.my_mute_tags = tags;
            data.my_mute_content = content;
        }
        if data.muted.contains(&me) {
            data.disabled_until =
                Some(latest_timeout_for(&valid_timeouts, &me, input.now).and_then(|t| t.expiration));
        }
    }

    for (key, kind, created_at, id, author, tags) in candidates {
        match kind {
            1311 => {
                if !is_visible(&author, &moderators, &data.muted) {
                    continue;
                }
                let content = input
                    .ndb
                    .get_note_by_key(input.tx, key)
                    .map(|n| n.content().to_string())
                    .unwrap_or_default();
                let segments = parsed
                    .segments
                    .entry(key)
                    .or_insert_with(|| {
                        let emoji: HashMap<String, String> =
                            emoji_from_tags(&tags).into_iter().collect();
                        chat_text::parse(&content, &emoji)
                    })
                    .clone();
                for s in &segments {
                    match s {
                        Segment::Embed(b) => match embed_target(b) {
                            Some(EmbedTarget::Event(id)) => data.wants.push(Want::Event(id)),
                            Some(EmbedTarget::Coordinate(k, pk, d)) => {
                                data.wants.push(Want::Coordinate(k, pk, d))
                            }
                            None => {}
                        },
                        _ => {}
                    }
                }
                data.rows.push(std::sync::Arc::new(Row {
                    key,
                    created_at,
                    id,
                    kind: RowKind::Message {
                        id,
                        author,
                        segments,
                        content,
                    },
                }));
            }
            9735 => {
                let Ok(note) = input.ndb.get_note_by_key(input.tx, key) else {
                    continue;
                };
                let Ok(zap) = Zap::from_receipt(note) else {
                    continue;
                };
                if !is_visible(&zap.sender, &moderators, &data.muted) {
                    continue;
                }
                data.rows.push(std::sync::Arc::new(Row { key, created_at, id, kind: RowKind::Zap { sender: zap.sender, sats: zap.amount, message: zap.message.clone() } }));
            }
            1312 => {
                if let Some(raid) = parse_raid(&tags) {
                    for c in [&raid.from, &raid.to] {
                        if let Some((k, pk, d)) = split_coordinate(c) {
                            data.wants.push(Want::Coordinate(k, pk, d));
                        }
                    }
                    data.rows.push(std::sync::Arc::new(Row {
                        key,
                        created_at,
                        id,
                        kind: RowKind::Raid { raid, created_at },
                    }));
                }
            }
            _ => {}
        }
    }

    for (key, id, t) in timeouts {
        if moderators.contains(&t.author) && t.active(input.now) {
            data.rows.push(std::sync::Arc::new(Row { key, created_at: t.created_at, id, kind: RowKind::Timeout(t) }));
        }
    }

    for (key, created_at, id, badge, users) in awards {
        for u in &users {
            let list = data.badges_of.entry(*u).or_default();
            if !list.contains(&badge) {
                list.push(badge.clone());
            }
        }
        data.rows.push(std::sync::Arc::new(Row {
            key,
            created_at,
            id,
            kind: RowKind::BadgeAward { badge, users },
        }));
    }

    data.rows
        .sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));

    reactions.retain(|r| !data.muted.contains(&r.author));
    data.reactions = aggregate_reactions(&reactions);
    let mut zapped: HashMap<[u8; 32], u64> = HashMap::new();
    for (target, sender, sats) in zap_events {
        if !data.muted.contains(&sender) {
            *zapped.entry(target).or_default() += sats;
        }
    }
    data.zapped = zapped;

    let recent_ids: Vec<[u8; 32]> = data
        .rows
        .iter()
        .rev()
        .filter_map(|r| match &r.kind {
            RowKind::Message { id, .. } => Some(*id),
            _ => None,
        })
        .take(150)
        .collect();
    if !recent_ids.is_empty() {
        let mut ids = recent_ids;
        ids.sort();
        data.wants.push(Want::Reactions(ids));
    }
    for r in set_refs {
        if let Some((k, pk, d)) = split_coordinate(&r) {
            data.wants.push(Want::Coordinate(k, pk, d));
        }
    }
    for b in data.badges_of.values().flatten() {
        if let Some((k, pk, d)) = split_coordinate(b) {
            data.wants.push(Want::Coordinate(k, pk, d));
        }
    }
    data.wants.sort();
    data.wants.dedup();
    data
}

fn collect_badge(data: &mut ChatData, note: &nostrdb::Note<'_>) {
    let tags = note.tags_vec();
    let d = first_tag(&tags, "d").unwrap_or("");
    let coord = format!("30009:{}:{}", hex::encode(note.pubkey()), d);
    data.badge_defs.insert(
        coord,
        BadgeDef {
            image: first_tag(&tags, "image").map(String::from),
            name: first_tag(&tags, "name").map(String::from),
            description: first_tag(&tags, "description").map(String::from),
        },
    );
}

fn collect_set(data: &mut ChatData, tags: &[Vec<String>]) {
    for (name, url) in emoji_from_tags(tags) {
        if !data.custom_emoji.iter().any(|(n, _)| *n == name) {
            data.custom_emoji.push((name, url));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_events;
    use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
    use nostrdb::Config;

    const NOW: u64 = 2_000_000_000;

    fn tag(parts: &[&str]) -> Tag {
        Tag::parse(parts.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    fn ingest(ndb: &Ndb, keys: &Keys, builder: EventBuilder, at: u64) {
        let ev = builder
            .custom_created_at(nostr::Timestamp::from(at))
            .sign_with_keys(keys)
            .unwrap();
        ndb.process_event(&ev.as_json()).unwrap();
    }

    fn collect(ndb: &Ndb, kinds: &[u64]) -> HashSet<NoteRef> {
        let tx = Transaction::new(ndb).unwrap();
        let filter = Filter::new().kinds(kinds.iter().copied()).limit(100).build();
        ndb.query(&tx, &[filter], 100)
            .unwrap()
            .into_iter()
            .map(NoteRef::from_query_result)
            .collect()
    }

    fn setup() -> (tempfile::TempDir, Ndb) {
        let dir = tempfile::tempdir().unwrap();
        let ndb = Ndb::new(dir.path().to_str().unwrap(), &Config::new()).unwrap();
        (dir, ndb)
    }

    fn settle() {
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    #[test]
    fn reaction_sent_by_picker_shows_on_the_message() {
        let (_d, ndb) = setup();
        let host = Keys::generate();
        let chatter = Keys::generate();
        let me = Keys::generate();
        let a = format!("30311:{}:live", host.public_key());
        ingest(&ndb, &chatter, EventBuilder::new(Kind::Custom(1311), "hi").tag(tag(&["a", &a])), NOW - 100);
        settle();
        let main = collect(&ndb, &[1311]);
        let msg_id = {
            let tx = Transaction::new(&ndb).unwrap();
            let k = main.iter().next().unwrap().key;
            let id = *ndb.get_note_by_key(&tx, k).unwrap().id();
            id
        };
        let author = chatter.public_key().to_bytes();
        ingest(&ndb, &me, chat_events::reaction(&msg_id, &author, "🔥", None), NOW - 50);
        ingest(&ndb, &host, chat_events::reaction(&msg_id, &author, "🔥", None), NOW - 40);
        settle();
        let aux = collect(&ndb, &[7]);
        let tx = Transaction::new(&ndb).unwrap();
        let data = build(
            &BuildInput { ndb: &ndb, tx: &tx, main: &main, aux: &aux, host: host.public_key().to_bytes(), me: Some(me.public_key().to_bytes()), now: NOW },
            &mut Parsed::default(),
        );
        let counts = data.reactions.get(&msg_id).unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].count, 2);
    }

    #[test]
    fn host_timeout_hides_target_and_shows_notice() {
        let (_d, ndb) = setup();
        let host = Keys::generate();
        let bad = Keys::generate();
        let a = format!("30311:{}:live", host.public_key());
        ingest(&ndb, &bad, EventBuilder::new(Kind::Custom(1311), "spam").tag(tag(&["a", &a])), NOW - 100);
        ingest(&ndb, &host, chat_events::timeout(&bad.public_key().to_bytes(), 600, NOW - 60), NOW - 60);
        settle();
        let main = collect(&ndb, &[1311, 1314]);
        let tx = Transaction::new(&ndb).unwrap();
        let build_at = |now| {
            build(
                &BuildInput { ndb: &ndb, tx: &tx, main: &main, aux: &HashSet::new(), host: host.public_key().to_bytes(), me: None, now },
                &mut Parsed::default(),
            )
        };
        let data = build_at(NOW);
        assert!(data.muted.contains(&bad.public_key().to_bytes()));
        assert!(!data.rows.iter().any(|r| matches!(r.kind, RowKind::Message { .. })));
        let timeout_row = data.rows.iter().find_map(|r| match &r.kind {
            RowKind::Timeout(t) => Some(t.clone()),
            _ => None,
        });
        assert_eq!(timeout_row.unwrap().duration(), 600);
        let later = build_at(NOW + 1000);
        assert!(later.muted.is_empty());
        assert!(later.rows.iter().any(|r| matches!(r.kind, RowKind::Message { .. })));
    }

    #[test]
    fn timeout_from_a_stranger_is_ignored() {
        let (_d, ndb) = setup();
        let host = Keys::generate();
        let stranger = Keys::generate();
        let victim = Keys::generate();
        let a = format!("30311:{}:live", host.public_key());
        ingest(&ndb, &victim, EventBuilder::new(Kind::Custom(1311), "hello").tag(tag(&["a", &a])), NOW - 100);
        ingest(&ndb, &stranger, chat_events::timeout(&victim.public_key().to_bytes(), 600, NOW - 60), NOW - 60);
        settle();
        let main = collect(&ndb, &[1311, 1314]);
        let tx = Transaction::new(&ndb).unwrap();
        let data = build(
            &BuildInput { ndb: &ndb, tx: &tx, main: &main, aux: &HashSet::new(), host: host.public_key().to_bytes(), me: None, now: NOW },
            &mut Parsed::default(),
        );
        assert!(data.muted.is_empty());
    }

    #[test]
    fn own_mute_list_hides_user_and_unmute_clears_it() {
        let (_d, ndb) = setup();
        let host = Keys::generate();
        let me = Keys::generate();
        let noisy = Keys::generate();
        let noisy_pk = noisy.public_key().to_bytes();
        let a = format!("30311:{}:live", host.public_key());
        ingest(&ndb, &noisy, EventBuilder::new(Kind::Custom(1311), "noise").tag(tag(&["a", &a])), NOW - 100);
        ingest(&ndb, &me, chat_events::mute_list(vec![], &noisy_pk, true), NOW - 50);
        settle();
        let main = collect(&ndb, &[1311, 10000]);
        let (muted_hides, my_tags) = {
            let tx = Transaction::new(&ndb).unwrap();
            let data = build(
            &BuildInput { ndb: &ndb, tx: &tx, main: &main, aux: &HashSet::new(), host: host.public_key().to_bytes(), me: Some(me.public_key().to_bytes()), now: NOW },
            &mut Parsed::default(),
        );
            (
                data.my_mutes.contains(&noisy_pk)
                    && !data.rows.iter().any(|r| matches!(r.kind, RowKind::Message { .. })),
                data.my_mute_tags.clone(),
            )
        };
        assert!(muted_hides);

        ingest(&ndb, &me, chat_events::mute_list(my_tags, &noisy_pk, false), NOW - 10);
        settle();
        let main = collect(&ndb, &[1311, 10000]);
        let tx = Transaction::new(&ndb).unwrap();
        let data = build(
            &BuildInput { ndb: &ndb, tx: &tx, main: &main, aux: &HashSet::new(), host: host.public_key().to_bytes(), me: Some(me.public_key().to_bytes()), now: NOW },
            &mut Parsed::default(),
        );
        assert!(data.my_mutes.is_empty());
        assert!(data.rows.iter().any(|r| matches!(r.kind, RowKind::Message { .. })));
    }

    #[test]
    fn raid_event_becomes_a_row_and_wants_both_streams() {
        let (_d, ndb) = setup();
        let host = Keys::generate();
        let other = Keys::generate();
        let from = format!("30311:{}:live", host.public_key());
        let to = format!("30311:{}:other", other.public_key());
        ingest(
            &ndb,
            &host,
            EventBuilder::new(Kind::Custom(1312), "").tag(tag(&["a", &from, "", "from"])).tag(tag(&["a", &to, "", "to"])),
            NOW - 5,
        );
        settle();
        let main = collect(&ndb, &[1312]);
        let tx = Transaction::new(&ndb).unwrap();
        let data = build(
            &BuildInput { ndb: &ndb, tx: &tx, main: &main, aux: &HashSet::new(), host: host.public_key().to_bytes(), me: None, now: NOW },
            &mut Parsed::default(),
        );
        let raid = data.rows.iter().find_map(|r| match &r.kind {
            RowKind::Raid { raid, .. } => Some(raid.clone()),
            _ => None,
        });
        let raid = raid.unwrap();
        assert_eq!((raid.from, raid.to), (from, to));
        assert_eq!(data.wants.iter().filter(|w| matches!(w, Want::Coordinate(..))).count(), 2);
    }

    #[test]
    fn badge_award_by_host_attaches_badge_to_user() {
        let (_d, ndb) = setup();
        let host = Keys::generate();
        let fan = Keys::generate();
        let coord = format!("30009:{}:vip", host.public_key());
        ingest(&ndb, &host, EventBuilder::new(Kind::Custom(8), "").tag(tag(&["a", &coord])).tag(tag(&["p", &fan.public_key().to_hex()])), NOW - 20);
        ingest(&ndb, &fan, EventBuilder::new(Kind::Custom(8), "").tag(tag(&["a", &coord])).tag(tag(&["p", &fan.public_key().to_hex()])), NOW - 10);
        ingest(
            &ndb,
            &host,
            EventBuilder::new(Kind::Custom(30009), "").tag(tag(&["d", "vip"])).tag(tag(&["name", "VIP"])).tag(tag(&["image", "https://x/y.png"])),
            NOW - 30,
        );
        settle();
        let main = collect(&ndb, &[8]);
        let aux = collect(&ndb, &[30009]);
        let tx = Transaction::new(&ndb).unwrap();
        let data = build(
            &BuildInput { ndb: &ndb, tx: &tx, main: &main, aux: &aux, host: host.public_key().to_bytes(), me: None, now: NOW },
            &mut Parsed::default(),
        );
        let fan_pk = fan.public_key().to_bytes();
        assert_eq!(data.badges_of.get(&fan_pk).unwrap(), &vec![coord.clone()]);
        assert_eq!(data.rows.iter().filter(|r| matches!(r.kind, RowKind::BadgeAward { .. })).count(), 1);
        assert_eq!(data.badge_defs.get(&coord).unwrap().name.as_deref(), Some("VIP"));
    }
}
