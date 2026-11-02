use super::chain::Loader;
use super::frame;
use crate::hls;
use image::RgbaImage;
use std::io::Read;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub const IMAGE_TIMEOUT: Duration = Duration::from_secs(4);
pub const MAX_BODY: u64 = 12 * 1024 * 1024;
pub const MAX_IMAGE_WIDTH: u32 = 1280;
pub const MAX_GRABS: usize = 2;

struct Gate {
    active: Mutex<usize>,
    cv: Condvar,
}

static GRABS: Gate = Gate {
    active: Mutex::new(0),
    cv: Condvar::new(),
};

struct GrabPermit;

impl GrabPermit {
    fn acquire() -> Self {
        let mut n = GRABS.active.lock().unwrap();
        while *n >= MAX_GRABS {
            n = GRABS.cv.wait(n).unwrap();
        }
        *n += 1;
        GrabPermit
    }
}

impl Drop for GrabPermit {
    fn drop(&mut self) {
        *GRABS.active.lock().unwrap() -= 1;
        GRABS.cv.notify_one();
    }
}

pub struct HttpLoader {
    agent: ureq::Agent,
}

impl Default for HttpLoader {
    fn default() -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(IMAGE_TIMEOUT)
                .redirects(4)
                .user_agent("zap-stream-app")
                .build(),
        }
    }
}

impl HttpLoader {
    fn get(&self, url: &str) -> Option<Vec<u8>> {
        let resp = self.agent.get(url).call().ok()?;
        let mut body = Vec::new();
        resp.into_reader().take(MAX_BODY).read_to_end(&mut body).ok()?;
        (!body.is_empty()).then_some(body)
    }

    fn get_text(&self, url: &str) -> Option<String> {
        String::from_utf8(self.get(url)?).ok()
    }

    fn media_playlist(&self, url: &str) -> Option<(String, String)> {
        let text = self.get_text(url)?;
        if !hls::is_master(&text) {
            return Some((url.to_string(), text));
        }
        let top = hls::parse_master(&text, url).into_iter().next()?;
        let media = self.get_text(&top.url)?;
        Some((top.url, media))
    }
}

pub struct MediaPlaylist {
    pub init: Option<String>,
    pub segments: Vec<String>,
}

pub fn parse_media_playlist(text: &str, base: &str) -> MediaPlaylist {
    let mut init = None;
    let mut segments = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with("#EXT-X-MAP") {
            init = hls::attribute(line, "URI").map(|u| hls::resolve_url(base, u));
        } else if !line.is_empty() && !line.starts_with('#') {
            segments.push(hls::resolve_url(base, line));
        }
    }
    MediaPlaylist { init, segments }
}

impl Loader for HttpLoader {
    fn load_image(&self, url: &str) -> Option<RgbaImage> {
        let body = self.get(url)?;
        let img = image::load_from_memory(&body).ok()?.to_rgba8();
        Some(frame::downscale(img, MAX_IMAGE_WIDTH))
    }

    fn grab_frame(&self, streaming: &str) -> Option<RgbaImage> {
        let _permit = GrabPermit::acquire();
        let (base, text) = self.media_playlist(streaming)?;
        let playlist = parse_media_playlist(&text, &base);
        let init = match &playlist.init {
            Some(u) => Some(self.get(u)?),
            None => None,
        };
        playlist.segments.iter().rev().take(2).find_map(|seg| {
            let data = self.get(seg)?;
            match &init {
                Some(init) => frame::frame_from_fmp4(init, &data),
                None => frame::frame_from_ts(&data),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fmp4_playlist_with_map() {
        let t = "#EXTM3U\n#EXT-X-MAP:URI=\"scene/init.mp4\"\n#EXTINF:6,\nscene/a.m4s\n#EXTINF:6,\nscene/b.m4s\n";
        let p = parse_media_playlist(t, "https://h.example/live/stream.m3u8");
        assert_eq!(p.init.as_deref(), Some("https://h.example/live/scene/init.mp4"));
        assert_eq!(
            p.segments,
            vec![
                "https://h.example/live/scene/a.m4s",
                "https://h.example/live/scene/b.m4s"
            ]
        );
    }

    #[test]
    fn parses_ts_playlist_without_map() {
        let t = "#EXTM3U\n#EXTINF:1,\n0-00994.ts\n#EXTINF:1,\n0-00995.ts\n";
        let p = parse_media_playlist(t, "https://v3.example/live/live.m3u8?x=1");
        assert!(p.init.is_none());
        assert_eq!(p.segments.last().unwrap(), "https://v3.example/live/0-00995.ts");
    }
}

#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn grabs_frames_from_live_hls_hosts() {
        let l = HttpLoader::default();
        for u in [
            "https://esports.einundzwanzig.space/live/stream.m3u8",
            "https://v3.coinos.io/live/5240ef07857b2812/live.m3u8",
            "https://video.noderunnersradio.com/hls/stream.m3u8",
            "https://api-uk.zap.stream/537a365c-f1ec-44ac-af10-22d14a7319fb/hls/live.m3u8",
        ] {
            let r = l.grab_frame(u);
            eprintln!("{u} -> {:?}", r.as_ref().map(|i| (i.width(), i.height())));
        }
    }
}
