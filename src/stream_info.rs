use crate::note_util::NoteUtil;
use nostrdb::{NdbStrVariant, Note};
use std::fmt::{Display, Formatter};

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub enum StreamStatus {
    Live,
    Ended,
    Planned,
}

impl Display for StreamStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamStatus::Live => write!(f, "Live"),
            StreamStatus::Ended => write!(f, "Ended"),
            StreamStatus::Planned => write!(f, "Planned"),
        }
    }
}

pub trait StreamInfo {
    fn title(&self) -> Option<&str>;

    fn summary(&self) -> Option<&str>;

    fn host(&self) -> &[u8; 32];

    fn streaming(&self) -> Option<&str>;

    fn recording(&self) -> Option<&str>;

    /// Is the stream playable by this app
    fn can_play(&self) -> bool;

    fn starts(&self) -> u64;

    fn image(&self) -> Option<&str>;

    fn thumbs(&self) -> Vec<String>;

    fn stream_key(&self) -> String;

    fn status(&self) -> StreamStatus;

    fn viewers(&self) -> Option<u32>;

    fn hashtags(&self) -> Vec<String>;

    fn game_id(&self) -> Option<String>;

    fn share_url(&self) -> Option<String>;

    fn goal(&self) -> Option<[u8; 32]>;

    fn sensitive(&self) -> bool;
}

pub fn is_game_tag(tag: &str) -> bool {
    match tag.split_once(':') {
        Some((a, b)) => {
            !a.is_empty()
                && !b.is_empty()
                && a.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
                && b.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        }
        None => false,
    }
}

pub fn game_label(game_id: &str) -> String {
    let name = game_id.split_once(':').map_or(game_id, |(_, n)| n);
    crate::route::listing::category_name(name)
        .map(str::to_string)
        .unwrap_or_else(|| name.replace('-', " "))
}

pub fn is_moq_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.starts_with("moq://") || u.starts_with("moqt://") || u.contains("/moq")
}

impl StreamInfo for Note<'_> {
    fn title(&self) -> Option<&str> {
        if let Some(s) = self.get_tag_value("title") {
            s.variant().str()
        } else {
            None
        }
    }

    fn summary(&self) -> Option<&str> {
        if let Some(s) = self.get_tag_value("summary") {
            s.variant().str()
        } else {
            None
        }
    }

    fn host(&self) -> &[u8; 32] {
        match self.find_tag_value(|t| {
            t[0].variant().str() == Some("p") && t[3].variant().str() == Some("host")
        }) {
            Some(t) => match t.variant() {
                NdbStrVariant::Id(i) => i,
                NdbStrVariant::Str(s) => self.pubkey(),
            },
            None => self.pubkey(),
        }
    }

    fn streaming(&self) -> Option<&str> {
        if let Some(s) = self.get_tag_value("streaming") {
            s.variant().str()
        } else {
            None
        }
    }

    fn recording(&self) -> Option<&str> {
        if let Some(s) = self.get_tag_value("recording") {
            s.variant().str()
        } else {
            None
        }
    }

    /// Is the stream playable by this app
    fn can_play(&self) -> bool {
        if self.kind() == 30_313 {
            return true; // n94-stream can always be played
        }
        if let Some(stream) = self.streaming() {
            stream.contains(".m3u8") && !is_moq_url(stream)
        } else {
            false
        }
    }

    fn starts(&self) -> u64 {
        if let Some(s) = self.get_tag_value("starts") {
            s.variant().str().map_or(self.created_at(), |v| {
                v.parse::<u64>().unwrap_or(self.created_at())
            })
        } else {
            self.created_at()
        }
    }

    fn image(&self) -> Option<&str> {
        if let Some(s) = self.get_tag_value("image") {
            s.variant().str()
        } else {
            None
        }
    }

    fn thumbs(&self) -> Vec<String> {
        ["thumb", "thumbnail"]
            .iter()
            .filter_map(|k| self.get_tag_value(k).and_then(|s| s.variant().str().map(str::to_string)))
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn stream_key(&self) -> String {
        let d = self
            .get_tag_value("d")
            .and_then(|t| t.variant().str().map(str::to_string))
            .unwrap_or_default();
        format!("{}:{}", nostr::prelude::hex::encode(self.pubkey()), d)
    }

    fn status(&self) -> StreamStatus {
        if let Some(s) = self.get_tag_value("status") {
            match s.variant().str() {
                Some("live") => StreamStatus::Live,
                Some("planned") => StreamStatus::Planned,
                _ => StreamStatus::Ended,
            }
        } else {
            StreamStatus::Ended
        }
    }

    fn viewers(&self) -> Option<u32> {
        if let Some(s) = self.get_tag_value("current_participants") {
            s.variant().str().map(|v| v.parse::<u32>().unwrap_or(0))
        } else {
            None
        }
    }

    fn hashtags(&self) -> Vec<String> {
        let mut out = Vec::new();
        for tag in self.tags() {
            if tag.get_unchecked(0).variant().str() == Some("t") {
                if let Some(v) = tag.get_unchecked(1).variant().str() {
                    if !v.is_empty() {
                        out.push(v.to_lowercase());
                    }
                }
            }
        }
        out
    }

    fn game_id(&self) -> Option<String> {
        self.tags().iter().find_map(|tag| {
            if tag.get_unchecked(0).variant().str() != Some("t") {
                return None;
            }
            tag.get_unchecked(1)
                .variant()
                .str()
                .filter(|v| is_game_tag(v))
                .map(|v| v.to_lowercase())
        })
    }

    fn share_url(&self) -> Option<String> {
        use nostr::nips::nip19::ToBech32;
        let coord: nostr::prelude::Coordinate = crate::link::NostrLink::from_note(self)
            .try_into()
            .ok()?;
        coord
            .to_bech32()
            .ok()
            .map(|n| format!("https://zap.stream/{}", n))
    }

    fn sensitive(&self) -> bool {
        self.get_tag_value("content_warning").is_some()
            || self.get_tag_value("content-warning").is_some()
    }

    fn goal(&self) -> Option<[u8; 32]> {
        let v = self.get_tag_value("goal")?;
        match v.variant() {
            NdbStrVariant::Id(i) => Some(*i),
            NdbStrVariant::Str(s) => {
                let mut out = [0u8; 32];
                nostr::prelude::hex::decode_to_slice(s, &mut out).ok()?;
                Some(out)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_game_tags() {
        assert!(is_game_tag("internal:gaming"));
        assert!(is_game_tag("igdb:the-witcher-3"));
        assert!(!is_game_tag("gaming"));
        assert!(!is_game_tag("a:b:c"));
        assert!(!is_game_tag(":x"));
    }

    #[test]
    fn labels_games() {
        assert_eq!(game_label("internal:gaming"), "Gaming");
        assert_eq!(game_label("igdb:the-witcher-3"), "the witcher 3");
    }

    #[test]
    fn detects_moq_urls() {
        assert!(is_moq_url("moq://relay.example/stream"));
        assert!(is_moq_url("https://x.example/moq/stream"));
        assert!(!is_moq_url("https://x.example/live/index.m3u8"));
    }
}
