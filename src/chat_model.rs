use std::collections::{HashMap, HashSet};

pub type Pubkey = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeout {
    pub author: Pubkey,
    pub targets: Vec<Pubkey>,
    pub created_at: u64,
    pub expiration: Option<u64>,
}

impl Timeout {
    pub fn active(&self, now: u64) -> bool {
        self.created_at <= now && self.expiration.is_some_and(|e| e > now)
    }

    pub fn duration(&self) -> u64 {
        self.expiration
            .map(|e| e.saturating_sub(self.created_at))
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuteList {
    pub author: Pubkey,
    pub created_at: u64,
    pub muted: Vec<Pubkey>,
}

pub fn muted_set(
    moderators: &[Pubkey],
    lists: &[MuteList],
    timeouts: &[Timeout],
    now: u64,
) -> HashSet<Pubkey> {
    let mut newest: HashMap<Pubkey, &MuteList> = HashMap::new();
    for l in lists.iter().filter(|l| moderators.contains(&l.author)) {
        match newest.get(&l.author) {
            Some(prev) if prev.created_at >= l.created_at => {}
            _ => {
                newest.insert(l.author, l);
            }
        }
    }
    let mut out: HashSet<Pubkey> = newest.values().flat_map(|l| l.muted.iter().copied()).collect();
    for t in timeouts
        .iter()
        .filter(|t| moderators.contains(&t.author) && t.active(now))
    {
        out.extend(t.targets.iter().copied());
    }
    out
}

pub fn is_visible(author: &Pubkey, moderators: &[Pubkey], muted: &HashSet<Pubkey>) -> bool {
    moderators.contains(author) || !muted.contains(author)
}

pub fn latest_timeout_for(timeouts: &[Timeout], who: &Pubkey, now: u64) -> Option<Timeout> {
    timeouts
        .iter()
        .filter(|t| t.targets.contains(who) && t.active(now))
        .max_by_key(|t| t.expiration)
        .cloned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reaction {
    pub target: [u8; 32],
    pub author: Pubkey,
    pub content: String,
    pub emoji_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionCount {
    pub content: String,
    pub emoji_url: Option<String>,
    pub count: usize,
}

pub fn aggregate_reactions(reactions: &[Reaction]) -> HashMap<[u8; 32], Vec<ReactionCount>> {
    let mut seen: HashSet<(&[u8; 32], &Pubkey, &str)> = HashSet::new();
    let mut by_target: HashMap<[u8; 32], Vec<ReactionCount>> = HashMap::new();
    for r in reactions {
        if r.content.is_empty() || !seen.insert((&r.target, &r.author, r.content.as_str())) {
            continue;
        }
        let list = by_target.entry(r.target).or_default();
        match list.iter_mut().find(|c| c.content == r.content) {
            Some(c) => c.count += 1,
            None => list.push(ReactionCount {
                content: r.content.clone(),
                emoji_url: r.emoji_url.clone(),
                count: 1,
            }),
        }
    }
    for list in by_target.values_mut() {
        list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.content.cmp(&b.content)));
    }
    by_target
}

pub fn reaction_emoji_url(content: &str, tags: &[Vec<String>]) -> Option<String> {
    let name = content.strip_prefix(':')?.strip_suffix(':')?;
    tags.iter()
        .find(|t| t.len() > 2 && t[0] == "emoji" && t[1] == name)
        .map(|t| t[2].clone())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raid {
    pub from: String,
    pub to: String,
}

pub fn parse_raid(tags: &[Vec<String>]) -> Option<Raid> {
    let find = |markers: &[&str]| {
        tags.iter()
            .find(|t| t.len() > 3 && t[0] == "a" && markers.contains(&t[3].as_str()))
            .map(|t| t[1].clone())
    };
    Some(Raid {
        from: find(&["from", "root"])?,
        to: find(&["to", "mention"])?,
    })
}

pub fn toggle_mute_tags(mut tags: Vec<Vec<String>>, who: &str, mute: bool) -> Vec<Vec<String>> {
    tags.retain(|t| !(t.len() > 1 && t[0] == "p" && t[1] == who));
    if mute {
        tags.push(vec!["p".to_string(), who.to_string()]);
    }
    tags
}

pub fn is_auto_raid(created_at: u64, now: u64) -> bool {
    created_at.abs_diff(now) < 60
}

pub fn split_coordinate(coord: &str) -> Option<(u32, Pubkey, String)> {
    let mut it = coord.splitn(3, ':');
    let kind: u32 = it.next()?.parse().ok()?;
    let pk = it.next()?;
    let d = it.next()?.to_string();
    if pk.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&pk[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some((kind, out, d))
}

pub fn format_duration(secs: u64) -> String {
    const UNITS: [(u64, &str); 4] = [(86_400, "d"), (3_600, "h"), (60, "m"), (1, "s")];
    let mut left = secs;
    let mut parts = Vec::new();
    for (size, label) in UNITS {
        if left >= size {
            parts.push(format!("{}{}", left / size, label));
            left %= size;
        }
        if parts.len() == 2 {
            break;
        }
    }
    if parts.is_empty() {
        "0s".to_string()
    } else {
        parts.join(" ")
    }
}

pub const TIMEOUT_CHOICES: [u64; 15] = [
    10, 30, 60, 300, 600, 1_800, 3_600, 21_600, 43_200, 86_400, 172_800, 604_800, 1_209_600,
    1_814_400, 2_419_200,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(n: u8) -> Pubkey {
        [n; 32]
    }

    fn timeout(author: u8, target: u8, created: u64, exp: Option<u64>) -> Timeout {
        Timeout {
            author: pk(author),
            targets: vec![pk(target)],
            created_at: created,
            expiration: exp,
        }
    }

    #[test]
    fn timeout_without_expiration_never_mutes() {
        let t = timeout(1, 2, 10, None);
        assert!(muted_set(&[pk(1)], &[], &[t], 20).is_empty());
    }

    #[test]
    fn expired_timeout_does_not_mute() {
        let t = timeout(1, 2, 10, Some(15));
        assert!(muted_set(&[pk(1)], &[], &[t], 20).is_empty());
    }

    #[test]
    fn timeout_from_non_moderator_is_ignored() {
        let t = timeout(9, 2, 10, Some(100));
        assert!(muted_set(&[pk(1)], &[], &[t], 20).is_empty());
    }

    #[test]
    fn active_moderator_timeout_mutes_target() {
        let t = timeout(1, 2, 10, Some(100));
        let set = muted_set(&[pk(1)], &[], &[t], 20);
        assert!(set.contains(&pk(2)));
    }

    #[test]
    fn only_newest_mute_list_per_author_counts() {
        let old = MuteList { author: pk(1), created_at: 1, muted: vec![pk(5)] };
        let new = MuteList { author: pk(1), created_at: 2, muted: vec![pk(6)] };
        let set = muted_set(&[pk(1)], &[old, new], &[], 20);
        assert!(set.contains(&pk(6)) && !set.contains(&pk(5)));
    }

    #[test]
    fn moderators_stay_visible_when_muted() {
        let muted = HashSet::from([pk(1), pk(2)]);
        assert!(is_visible(&pk(1), &[pk(1)], &muted));
        assert!(!is_visible(&pk(2), &[pk(1)], &muted));
    }

    #[test]
    fn reactions_dedupe_per_author_and_sort_by_count() {
        let t = [7u8; 32];
        let r = |a: u8, c: &str| Reaction {
            target: t,
            author: pk(a),
            content: c.into(),
            emoji_url: None,
        };
        let agg = aggregate_reactions(&[r(1, "+"), r(1, "+"), r(2, "+"), r(3, "x"), r(4, "")]);
        let list = &agg[&t];
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].content, "+");
        assert_eq!(list[0].count, 2);
    }

    #[test]
    fn reaction_emoji_resolves_from_tags() {
        let tags = vec![vec!["emoji".into(), "fire".into(), "https://x/f.png".into()]];
        assert_eq!(reaction_emoji_url(":fire:", &tags).as_deref(), Some("https://x/f.png"));
        assert_eq!(reaction_emoji_url("+", &tags), None);
    }

    #[test]
    fn raid_tags_parse_both_marker_styles() {
        let tags = vec![
            vec!["a".into(), "30311:aa:one".into(), "".into(), "root".into()],
            vec!["a".into(), "30311:bb:two".into(), "".into(), "mention".into()],
        ];
        let raid = parse_raid(&tags).unwrap();
        assert_eq!(raid.from, "30311:aa:one");
        assert_eq!(raid.to, "30311:bb:two");
        assert!(parse_raid(&tags[..1]).is_none());
    }

    #[test]
    fn toggling_mute_adds_once_and_removes() {
        let base = vec![vec!["t".to_string(), "x".to_string()]];
        let muted = toggle_mute_tags(base.clone(), "ab", true);
        let muted = toggle_mute_tags(muted, "ab", true);
        assert_eq!(muted.iter().filter(|t| t[0] == "p").count(), 1);
        assert_eq!(toggle_mute_tags(muted, "ab", false), base);
    }

    #[test]
    fn coordinate_splits_and_rejects_short_keys() {
        let c = format!("30311:{}:my:stream", "ab".repeat(32));
        let (k, p, d) = split_coordinate(&c).unwrap();
        assert_eq!((k, p[0], d.as_str()), (30311, 0xab, "my:stream"));
        assert!(split_coordinate("30311:ab:x").is_none());
    }

    #[test]
    fn durations_format_two_units() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(86_400 + 3_600 + 5), "1d 1h");
    }

    #[test]
    fn latest_timeout_picks_longest_active() {
        let a = timeout(1, 2, 10, Some(100));
        let b = timeout(1, 2, 10, Some(200));
        let c = timeout(1, 3, 10, Some(300));
        let got = latest_timeout_for(&[a, b, c], &pk(2), 20).unwrap();
        assert_eq!(got.expiration, Some(200));
    }
}
