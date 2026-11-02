#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendition {
    pub label: String,
    pub url: String,
    pub bandwidth: u64,
    pub height: Option<u32>,
}

pub fn resolve_url(base: &str, reference: &str) -> String {
    if reference.starts_with("http://") || reference.starts_with("https://") {
        return reference.to_string();
    }
    let without_query = base.split(['?', '#']).next().unwrap_or(base);
    if let Some(path) = reference.strip_prefix('/') {
        let scheme_end = without_query.find("://").map(|i| i + 3).unwrap_or(0);
        let host_end = without_query[scheme_end..]
            .find('/')
            .map(|i| i + scheme_end)
            .unwrap_or(without_query.len());
        return format!("{}/{}", &without_query[..host_end], path);
    }
    match without_query.rfind('/') {
        Some(i) => format!("{}/{}", &without_query[..i], reference),
        None => reference.to_string(),
    }
}

pub fn attribute<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let list = line.split_once(':')?.1;
    let mut rest = list;
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=')?;
        let (value, next) = if let Some(quoted) = after.strip_prefix('"') {
            let end = quoted.find('"')?;
            (&quoted[..end], quoted[end + 1..].trim_start_matches(','))
        } else {
            match after.split_once(',') {
                Some((v, n)) => (v, n),
                None => (after, ""),
            }
        };
        if key.trim() == name {
            return Some(value);
        }
        rest = next;
    }
    None
}

pub fn parse_master(text: &str, base_url: &str) -> Vec<Rendition> {
    let mut out: Vec<Rendition> = Vec::new();
    let mut pending: Option<(u64, Option<u32>, Option<String>)> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with("#EXT-X-STREAM-INF") {
            let bandwidth = attribute(line, "BANDWIDTH")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let height = attribute(line, "RESOLUTION")
                .and_then(|v| v.split_once('x'))
                .and_then(|(_, h)| h.parse().ok());
            let name = attribute(line, "NAME").map(str::to_string);
            pending = Some((bandwidth, height, name));
        } else if !line.is_empty() && !line.starts_with('#') {
            if let Some((bandwidth, height, name)) = pending.take() {
                let label = match (height, name) {
                    (Some(h), _) => format!("{h}p"),
                    (None, Some(n)) => n,
                    (None, None) => format!("{} kbps", bandwidth / 1000),
                };
                out.push(Rendition {
                    label,
                    url: resolve_url(base_url, line),
                    bandwidth,
                    height,
                });
            }
        }
    }
    out.sort_by(|a, b| b.bandwidth.cmp(&a.bandwidth));
    out.dedup_by(|a, b| a.url == b.url);
    out
}

pub fn is_master(text: &str) -> bool {
    text.contains("#EXT-X-STREAM-INF")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &str = "#EXTM3U\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360,CODECS=\"avc1.4d401f,mp4a.40.2\"\n\
360p/stream.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=3000000,RESOLUTION=1920x1080,CODECS=\"avc1.640028,mp4a.40.2\"\n\
https://cdn.example.com/1080p/stream.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=1500000,NAME=\"mid\"\n\
/abs/mid.m3u8\n";

    #[test]
    fn parses_renditions_highest_first() {
        let r = parse_master(MASTER, "https://ex.com/live/abc/stream.m3u8?x=1");
        let labels: Vec<_> = r.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, vec!["1080p", "mid", "360p"]);
        assert_eq!(r[0].url, "https://cdn.example.com/1080p/stream.m3u8");
        assert_eq!(r[1].url, "https://ex.com/abs/mid.m3u8");
        assert_eq!(r[2].url, "https://ex.com/live/abc/360p/stream.m3u8");
        assert_eq!(r[2].height, Some(360));
    }

    #[test]
    fn media_playlist_is_not_master() {
        assert!(!is_master("#EXTM3U\n#EXTINF:4,\nseg.ts\n"));
        assert!(is_master(MASTER));
        assert!(parse_master("#EXTM3U\n#EXTINF:4,\nseg.ts\n", "https://a/b.m3u8").is_empty());
    }
}
