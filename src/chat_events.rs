use nostr::prelude::hex;
use nostr::{EventBuilder, Kind, Tag};

pub fn reaction(
    target: &[u8; 32],
    author: &[u8; 32],
    content: &str,
    emoji_url: Option<&str>,
) -> EventBuilder {
    let mut builder = EventBuilder::new(Kind::Reaction, content)
        .tag(Tag::parse(["e".to_string(), hex::encode(target)]).unwrap())
        .tag(Tag::parse(["p".to_string(), hex::encode(author)]).unwrap());
    if let (Some(url), Some(name)) = (
        emoji_url,
        content.strip_prefix(':').and_then(|c| c.strip_suffix(':')),
    ) {
        builder = builder.tag(Tag::parse(["emoji".to_string(), name.to_string(), url.to_string()]).unwrap());
    }
    builder
}

pub fn timeout(user: &[u8; 32], secs: u64, now: u64) -> EventBuilder {
    EventBuilder::new(Kind::Custom(1314), "")
        .tag(Tag::parse(["p".to_string(), hex::encode(user)]).unwrap())
        .tag(Tag::parse(["expiration".to_string(), (now + secs).to_string()]).unwrap())
}

pub fn mute_list(tags: Vec<Vec<String>>, target: &[u8; 32], mute: bool) -> EventBuilder {
    let tags = crate::contacts::with_mute(tags, &hex::encode(target), mute);
    EventBuilder::new(Kind::MuteList, "").tags(tags.into_iter().filter_map(|t| Tag::parse(t).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags_of(b: EventBuilder) -> Vec<Vec<String>> {
        let ev = b.sign_with_keys(&nostr::Keys::generate()).unwrap();
        ev.tags.iter().map(|t| t.as_slice().to_vec()).collect()
    }

    #[test]
    fn reaction_tags_target_and_author() {
        let t = tags_of(reaction(&[1; 32], &[2; 32], "+", None));
        assert_eq!(t[0], vec!["e".to_string(), hex::encode([1u8; 32])]);
        assert_eq!(t[1], vec!["p".to_string(), hex::encode([2u8; 32])]);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn custom_emoji_reaction_carries_emoji_tag() {
        let t = tags_of(reaction(&[1; 32], &[2; 32], ":cat:", Some("https://x/cat.png")));
        assert_eq!(t[2], vec!["emoji", "cat", "https://x/cat.png"]);
    }

    #[test]
    fn timeout_expires_after_duration() {
        let t = tags_of(timeout(&[3; 32], 600, 1000));
        assert_eq!(t[1], vec!["expiration".to_string(), "1600".to_string()]);
    }
}
