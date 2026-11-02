pub use super::cache::FailureMemo;
use image::RgbaImage;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Image,
    Thumb,
    HostThumb,
    HlsFrame,
}

#[derive(Debug, Clone, Default)]
pub struct Candidates {
    pub image: Option<String>,
    pub thumbs: Vec<String>,
    pub streaming: Option<String>,
    pub live: bool,
}

pub trait Loader {
    fn load_image(&self, url: &str) -> Option<RgbaImage>;
    fn grab_frame(&self, streaming: &str) -> Option<RgbaImage>;
}

pub fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then_some(host)
}

pub fn host_thumb_url(streaming: &str) -> Option<(String, String)> {
    let host = host_of(streaming)?.to_string();
    let idx = streaming.find("/hls/")?;
    Some((host, format!("{}/thumb.webp", &streaming[..idx])))
}

fn try_url(loader: &dyn Loader, memo: &Mutex<FailureMemo>, url: &str) -> Option<RgbaImage> {
    if url.is_empty() || memo.lock().unwrap().has_failed(url) {
        return None;
    }
    let loaded = loader.load_image(url);
    if loaded.is_none() {
        memo.lock().unwrap().mark_failed(url);
    }
    loaded
}

pub fn resolve(
    loader: &dyn Loader,
    memo: &Mutex<FailureMemo>,
    c: &Candidates,
) -> Option<(Source, RgbaImage)> {
    if let Some(img) = c.image.as_deref().and_then(|u| try_url(loader, memo, u)) {
        return Some((Source::Image, img));
    }
    for t in &c.thumbs {
        if let Some(img) = try_url(loader, memo, t) {
            return Some((Source::Thumb, img));
        }
    }
    if let Some((host, url)) = c.streaming.as_deref().and_then(host_thumb_url) {
        let known = memo.lock().unwrap().host_thumb(&host);
        if known != Some(false) {
            let loaded = loader.load_image(&url);
            memo.lock().unwrap().set_host_thumb(&host, loaded.is_some());
            if let Some(img) = loaded {
                return Some((Source::HostThumb, img));
            }
        }
    }
    if c.live {
        if let Some(s) = c.streaming.as_deref() {
            if let Some(img) = loader.grab_frame(s) {
                return Some((Source::HlsFrame, img));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;

    struct Fake {
        ok: HashSet<String>,
        frame: bool,
        calls: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(ok: &[&str], frame: bool) -> Self {
            Self {
                ok: ok.iter().map(|s| s.to_string()).collect(),
                frame,
                calls: RefCell::new(vec![]),
            }
        }
    }

    impl Loader for Fake {
        fn load_image(&self, url: &str) -> Option<RgbaImage> {
            self.calls.borrow_mut().push(url.to_string());
            self.ok.contains(url).then(|| RgbaImage::new(2, 2))
        }

        fn grab_frame(&self, streaming: &str) -> Option<RgbaImage> {
            self.calls.borrow_mut().push(format!("grab:{streaming}"));
            self.frame.then(|| RgbaImage::new(2, 2))
        }
    }

    const STREAM: &str = "https://api-uk.zap.stream/live/abc/hls/stream.m3u8";
    const THUMB: &str = "https://api-uk.zap.stream/live/abc/thumb.webp";

    fn cands() -> Candidates {
        Candidates {
            image: Some("https://i/img.png".into()),
            thumbs: vec!["https://i/thumb.png".into()],
            streaming: Some(STREAM.into()),
            live: true,
        }
    }

    fn memo() -> Mutex<FailureMemo> {
        Mutex::new(FailureMemo::default())
    }

    #[test]
    fn image_wins_when_it_loads() {
        let f = Fake::new(&["https://i/img.png", "https://i/thumb.png", THUMB], true);
        let (src, _) = resolve(&f, &memo(), &cands()).unwrap();
        assert_eq!(src, Source::Image);
        assert_eq!(f.calls.borrow().len(), 1);
    }

    #[test]
    fn falls_back_through_thumb_host_thumb_then_frame() {
        let m = memo();
        let f = Fake::new(&["https://i/thumb.png", THUMB], true);
        assert_eq!(resolve(&f, &m, &cands()).unwrap().0, Source::Thumb);

        let f = Fake::new(&[THUMB], true);
        assert_eq!(resolve(&f, &memo(), &cands()).unwrap().0, Source::HostThumb);

        let f = Fake::new(&[], true);
        assert_eq!(resolve(&f, &memo(), &cands()).unwrap().0, Source::HlsFrame);
        assert_eq!(
            *f.calls.borrow(),
            vec![
                "https://i/img.png".to_string(),
                "https://i/thumb.png".to_string(),
                THUMB.to_string(),
                format!("grab:{STREAM}"),
            ]
        );
    }

    #[test]
    fn nothing_loads_gives_none_for_placeholder() {
        let f = Fake::new(&[], false);
        assert!(resolve(&f, &memo(), &cands()).is_none());
    }

    #[test]
    fn ended_streams_never_grab_frames() {
        let f = Fake::new(&[], true);
        let mut c = cands();
        c.live = false;
        assert!(resolve(&f, &memo(), &c).is_none());
        assert!(!f.calls.borrow().iter().any(|c| c.starts_with("grab:")));
    }

    #[test]
    fn failed_urls_are_not_retried() {
        let m = memo();
        let f = Fake::new(&[], false);
        resolve(&f, &m, &cands());
        let first = f.calls.borrow().len();
        resolve(&f, &m, &cands());
        let again: Vec<_> = f.calls.borrow()[first..].to_vec();
        assert_eq!(again, vec![format!("grab:{STREAM}")]);
    }

    #[test]
    fn host_thumb_is_probed_once_per_host() {
        let m = memo();
        let f = Fake::new(&[], false);
        let mut other = cands();
        other.streaming = Some("https://api-uk.zap.stream/live/xyz/hls/stream.m3u8".into());
        resolve(&f, &m, &cands());
        resolve(&f, &m, &other);
        let probes = f
            .calls
            .borrow()
            .iter()
            .filter(|c| c.ends_with("thumb.webp"))
            .count();
        assert_eq!(probes, 1);
    }

    #[test]
    fn host_thumb_url_needs_hls_path() {
        assert_eq!(
            host_thumb_url(STREAM),
            Some(("api-uk.zap.stream".into(), THUMB.into()))
        );
        assert_eq!(host_thumb_url("https://x.example/live/index.m3u8"), None);
    }
}
