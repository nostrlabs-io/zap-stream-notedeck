pub fn is_following(tags: &[Vec<String>], pubkey_hex: &str) -> bool {
    tags.iter()
        .any(|t| t.len() > 1 && t[0] == "p" && t[1] == pubkey_hex)
}

pub fn with_follow(mut tags: Vec<Vec<String>>, pubkey_hex: &str, follow: bool) -> Vec<Vec<String>> {
    tags.retain(|t| !(t.len() > 1 && t[0] == "p" && t[1] == pubkey_hex));
    if follow {
        tags.push(vec!["p".to_string(), pubkey_hex.to_string()]);
    }
    tags
}

pub fn tags_from_note(note: &nostrdb::Note<'_>) -> Vec<Vec<String>> {
    note.tags()
        .iter()
        .map(|t| {
            (0..t.count())
                .filter_map(|i| t.get(i))
                .map(|s| match s.variant() {
                    nostrdb::NdbStrVariant::Str(s) => s.to_string(),
                    nostrdb::NdbStrVariant::Id(id) => nostr::prelude::hex::encode(id),
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(hex: &str) -> Vec<String> {
        vec!["p".to_string(), hex.to_string()]
    }

    #[test]
    fn follow_appends_and_keeps_other_tags() {
        let tags = vec![p("a"), vec!["t".to_string(), "x".to_string()]];
        let out = with_follow(tags, "b", true);
        assert_eq!(out.len(), 3);
        assert!(is_following(&out, "b"));
        assert!(is_following(&out, "a"));
        assert!(out.contains(&vec!["t".to_string(), "x".to_string()]));
    }

    #[test]
    fn follow_twice_does_not_duplicate() {
        let out = with_follow(with_follow(vec![], "b", true), "b", true);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn unfollow_removes_only_target() {
        let out = with_follow(vec![p("a"), p("b")], "b", false);
        assert_eq!(out, vec![p("a")]);
        assert!(!is_following(&out, "b"));
    }
}
