pub fn relays(tags: &[Vec<String>]) -> Vec<String> {
    tags.iter()
        .filter(|t| t.len() > 1 && t[0] == "r")
        .map(|t| t[1].clone())
        .collect()
}

pub fn normalize(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let with_scheme = if url.contains("://") {
        url.to_string()
    } else {
        format!("wss://{}", url)
    };
    if !(with_scheme.starts_with("wss://") || with_scheme.starts_with("ws://")) {
        return None;
    }
    let host = with_scheme.split("://").nth(1)?.trim_end_matches('/');
    if host.is_empty() {
        return None;
    }
    Some(format!("{}://{}", with_scheme.split("://").next()?, host))
}

pub fn with_relay(mut tags: Vec<Vec<String>>, url: &str) -> Vec<Vec<String>> {
    if let Some(url) = normalize(url) {
        if !relays(&tags).contains(&url) {
            tags.push(vec!["r".to_string(), url]);
        }
    }
    tags
}

pub fn without_relay(mut tags: Vec<Vec<String>>, url: &str) -> Vec<Vec<String>> {
    tags.retain(|t| !(t.len() > 1 && t[0] == "r" && t[1] == url));
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(u: &str) -> Vec<String> {
        vec!["r".to_string(), u.to_string()]
    }

    #[test]
    fn bare_host_gets_wss() {
        assert_eq!(normalize("relay.damus.io/"), Some("wss://relay.damus.io".into()));
    }

    #[test]
    fn http_is_rejected() {
        assert_eq!(normalize("https://example.com"), None);
        assert_eq!(normalize("  "), None);
    }

    #[test]
    fn add_does_not_duplicate() {
        let tags = with_relay(vec![r("wss://a.io")], "a.io");
        assert_eq!(tags.len(), 1);
        let tags = with_relay(tags, "wss://b.io");
        assert_eq!(relays(&tags), vec!["wss://a.io", "wss://b.io"]);
    }

    #[test]
    fn remove_keeps_other_tags() {
        let tags = vec![r("wss://a.io"), vec!["alt".into(), "x".into()], r("wss://b.io")];
        let tags = without_relay(tags, "wss://a.io");
        assert_eq!(tags.len(), 2);
        assert_eq!(relays(&tags), vec!["wss://b.io"]);
    }
}
