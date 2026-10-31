use nostr::nips::nip19::{FromBech32, Nip19};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Emoji { name: String, url: String },
    Mention([u8; 32]),
    Embed(String),
    Url(String),
    Hashtag(String),
}

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)nostr:(?:nprofile|npub|note|nevent|naddr)[a-z0-9]+|#\w+|:\w+:|https?://[-a-z0-9@:%._+~#=]{1,256}\.[a-z0-9()]{1,10}\b[-a-z0-9()@:%_+.~#?&/=]*",
    )
    .unwrap()
});

pub fn decode_pubkey(bech: &str) -> Option<[u8; 32]> {
    match Nip19::from_bech32(bech).ok()? {
        Nip19::Pubkey(pk) => Some(pk.to_bytes()),
        Nip19::Profile(p) => Some(p.public_key.to_bytes()),
        _ => None,
    }
}

pub fn parse(content: &str, emoji: &HashMap<String, String>) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut last = 0;
    let mut push_text = |out: &mut Vec<Segment>, text: &str| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(Segment::Text(prev)) => prev.push_str(text),
            _ => out.push(Segment::Text(text.to_string())),
        }
    };
    for m in TOKEN.find_iter(content) {
        push_text(&mut out, &content[last..m.start()]);
        last = m.end();
        let tok = m.as_str();
        if let Some(bech) = tok.strip_prefix("nostr:") {
            let lower = bech.to_ascii_lowercase();
            if lower.starts_with("npub") || lower.starts_with("nprofile") {
                match decode_pubkey(&lower) {
                    Some(pk) => out.push(Segment::Mention(pk)),
                    None => push_text(&mut out, tok),
                }
            } else if Nip19::from_bech32(&lower).is_ok() {
                out.push(Segment::Embed(lower));
            } else {
                push_text(&mut out, tok);
            }
        } else if tok.starts_with('#') {
            out.push(Segment::Hashtag(tok.to_string()));
        } else if tok.starts_with(':') {
            let name = &tok[1..tok.len() - 1];
            match emoji.get(name) {
                Some(url) => out.push(Segment::Emoji {
                    name: name.to_string(),
                    url: url.clone(),
                }),
                None => push_text(&mut out, tok),
            }
        } else {
            out.push(Segment::Url(tok.to_string()));
        }
    }
    push_text(&mut out, &content[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Keys, ToBech32};

    fn emoji() -> HashMap<String, String> {
        HashMap::from([("soapbox".to_string(), "https://x/s.png".to_string())])
    }

    #[test]
    fn custom_emoji_only_when_tagged() {
        let s = parse("hi :soapbox: and :other:", &emoji());
        assert_eq!(
            s,
            vec![
                Segment::Text("hi ".into()),
                Segment::Emoji {
                    name: "soapbox".into(),
                    url: "https://x/s.png".into()
                },
                Segment::Text(" and :other:".into()),
            ]
        );
    }

    #[test]
    fn npub_mention_resolves_to_pubkey() {
        let k = Keys::generate();
        let npub = k.public_key().to_bech32().unwrap();
        let s = parse(&format!("hey nostr:{npub}!"), &HashMap::new());
        assert_eq!(s[1], Segment::Mention(k.public_key().to_bytes()));
    }

    #[test]
    fn bad_npub_stays_text() {
        let s = parse("nostr:npub1abc", &HashMap::new());
        assert_eq!(s, vec![Segment::Text("nostr:npub1abc".into())]);
    }

    #[test]
    fn note_reference_becomes_embed() {
        let id = nostr::EventId::all_zeros().to_bech32().unwrap();
        let s = parse(&format!("see nostr:{id}"), &HashMap::new());
        assert_eq!(s[1], Segment::Embed(id));
    }

    #[test]
    fn urls_and_hashtags() {
        let s = parse("go https://example.com/a.png #nostr", &HashMap::new());
        assert_eq!(s[1], Segment::Url("https://example.com/a.png".into()));
        assert_eq!(s[3], Segment::Hashtag("#nostr".into()));
    }
}
