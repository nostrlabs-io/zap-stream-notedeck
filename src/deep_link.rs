use crate::link::{IdOrStr, NostrLink};
use nostr::nips::nip19::Nip19;
use nostr::FromBech32;

#[derive(Debug, Eq, PartialEq)]
pub enum DeepLink {
    Stream(NostrLink),
    Profile([u8; 32]),
}

const STREAM_KIND: u32 = 30_311;

fn identifier(input: &str) -> &str {
    let trimmed = input.trim();
    let without_scheme = trimmed
        .strip_prefix("nostr://")
        .or_else(|| trimmed.strip_prefix("nostr:"))
        .unwrap_or(trimmed);
    let without_web = ["https://", "http://"]
        .iter()
        .find_map(|p| without_scheme.strip_prefix(p))
        .map(|rest| rest.split_once('/').map_or("", |(_, path)| path))
        .unwrap_or(without_scheme);
    let end = without_web
        .find(|c| c == '?' || c == '#' || c == '/')
        .unwrap_or(without_web.len());
    &without_web[..end]
}

pub fn parse(input: &str) -> Option<DeepLink> {
    match Nip19::from_bech32(identifier(input)).ok()? {
        Nip19::Coordinate(c) if c.kind.as_u16() as u32 == STREAM_KIND => {
            Some(DeepLink::Stream(NostrLink::new(
                crate::link::NostrLinkType::Coordinate,
                IdOrStr::Str(c.identifier.clone()),
                Some(STREAM_KIND),
                Some(c.public_key.to_bytes()),
                c.relays.iter().map(|r| r.to_string()).collect(),
            )))
        }
        Nip19::Pubkey(pk) => Some(DeepLink::Profile(pk.to_bytes())),
        Nip19::Profile(p) => Some(DeepLink::Profile(p.public_key.to_bytes())),
        _ => None,
    }
}

pub fn pending_file(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("pending_link")
}

pub fn take_pending(dir: &std::path::Path) -> Option<DeepLink> {
    let path = pending_file(dir);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::nips::nip19::ToBech32;
    use nostr::prelude::Coordinate;
    use nostr::{Keys, Kind};

    fn naddr(kind: u16) -> (String, [u8; 32]) {
        let keys = Keys::generate();
        let pk = keys.public_key().to_bytes();
        let coord = Coordinate::new(Kind::from_u16(kind), keys.public_key()).identifier("abc-123");
        (coord.to_bech32().unwrap(), pk)
    }

    #[test]
    fn parses_web_url() {
        let (n, pk) = naddr(30_311);
        let Some(DeepLink::Stream(l)) = parse(&format!("https://zap.stream/{n}")) else {
            panic!("not a stream")
        };
        assert_eq!(l.author, Some(pk));
        assert_eq!(l.id.to_string(), "abc-123");
    }

    #[test]
    fn parses_nostr_uri_with_and_without_slashes() {
        let (n, _) = naddr(30_311);
        assert!(matches!(parse(&format!("nostr:{n}")), Some(DeepLink::Stream(_))));
        assert!(matches!(parse(&format!("nostr://{n}")), Some(DeepLink::Stream(_))));
        assert!(matches!(parse(&n), Some(DeepLink::Stream(_))));
    }

    #[test]
    fn parses_npub_and_nprofile() {
        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let expected = keys.public_key().to_bytes();
        assert_eq!(parse(&format!("nostr:{npub}")), Some(DeepLink::Profile(expected)));
        assert_eq!(
            parse(&format!("https://zap.stream/{npub}?x=1")),
            Some(DeepLink::Profile(expected))
        );
    }

    #[test]
    fn rejects_other_kinds_and_junk() {
        let (n, _) = naddr(30_023);
        assert_eq!(parse(&format!("https://zap.stream/{n}")), None);
        assert_eq!(parse("https://zap.stream/"), None);
        assert_eq!(parse("hello"), None);
    }
}
