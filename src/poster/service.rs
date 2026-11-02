use super::cache::{TtlCache, POSTER_TTL};
use super::chain::{resolve, Candidates, FailureMemo, Loader};
use super::http::HttpLoader;
use egui::{Color32, Context, TextureHandle, TextureOptions};
use image::RgbaImage;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime};

pub const MAX_WORKERS: usize = 4;

#[derive(Clone)]
pub struct Poster {
    pub texture: Option<TextureHandle>,
    pub tint: Option<Color32>,
}

#[derive(Clone)]
pub struct Request {
    pub key: String,
    pub candidates: Candidates,
    pub avatar: Option<String>,
}

struct State {
    cache: TtlCache<Poster>,
    inflight: HashSet<String>,
    queue: VecDeque<(Context, Request)>,
    workers: usize,
}

pub struct PosterService {
    state: Mutex<State>,
    memo: Mutex<FailureMemo>,
    dir: OnceLock<PathBuf>,
    loader: HttpLoader,
}

static SERVICE: OnceLock<PosterService> = OnceLock::new();

pub fn global() -> &'static PosterService {
    SERVICE.get_or_init(PosterService::new)
}

pub fn disk_name(key: &str) -> String {
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{safe}.png")
}

pub fn average_color(img: &RgbaImage) -> Color32 {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for p in img.pixels().step_by(7) {
        if p.0[3] < 128 {
            continue;
        }
        r += p.0[0] as u64;
        g += p.0[1] as u64;
        b += p.0[2] as u64;
        n += 1;
    }
    if n == 0 {
        return Color32::from_rgb(32, 29, 38);
    }
    Color32::from_rgb((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

impl PosterService {
    fn new() -> Self {
        Self {
            state: Mutex::new(State {
                cache: TtlCache::new(POSTER_TTL),
                inflight: HashSet::new(),
                queue: VecDeque::new(),
                workers: 0,
            }),
            memo: Mutex::new(FailureMemo::default()),
            dir: OnceLock::new(),
            loader: HttpLoader::default(),
        }
    }

    pub fn set_dir(&self, base: &Path) {
        let dir = base.join("posters");
        let _ = std::fs::create_dir_all(&dir);
        let _ = self.dir.set(dir);
    }

    pub fn has_dir(&self) -> bool {
        self.dir.get().is_some()
    }

    pub fn lookup(
        &'static self,
        ctx: &Context,
        key: &str,
        live: bool,
        build: impl FnOnce() -> Request,
    ) -> Option<Poster> {
        let now = Instant::now();
        let mut st = self.state.lock().unwrap();
        let fresh = if live {
            st.cache.get_fresh(key, now)
        } else {
            st.cache.get_any(key)
        };
        if let Some(p) = fresh {
            return Some(p.clone());
        }
        let stale = st.cache.get_any(key).cloned();
        if st.inflight.insert(key.to_string()) {
            st.queue.push_back((ctx.clone(), build()));
            if st.workers < MAX_WORKERS {
                st.workers += 1;
                std::thread::spawn(move || self.work());
            }
        }
        stale
    }

    fn work(&'static self) {
        loop {
            let job = {
                let mut st = self.state.lock().unwrap();
                match st.queue.pop_front() {
                    Some(j) => j,
                    None => {
                        st.workers -= 1;
                        return;
                    }
                }
            };
            let (ctx, req) = job;
            let poster = self.build(&ctx, &req);
            let mut st = self.state.lock().unwrap();
            st.cache.insert(&req.key, poster, Instant::now());
            st.inflight.remove(&req.key);
            drop(st);
            ctx.request_repaint();
        }
    }

    fn disk_path(&self, key: &str) -> Option<PathBuf> {
        self.dir.get().map(|d| d.join(disk_name(key)))
    }

    fn read_disk(&self, req: &Request) -> Option<RgbaImage> {
        let path = self.disk_path(&req.key)?;
        if req.candidates.live {
            let age = SystemTime::now()
                .duration_since(std::fs::metadata(&path).ok()?.modified().ok()?)
                .ok()?;
            if !super::cache::is_fresh(age) {
                return None;
            }
        }
        image::open(path).ok().map(|i| i.to_rgba8())
    }

    fn write_disk(&self, key: &str, img: &RgbaImage) {
        if let Some(path) = self.disk_path(key) {
            let _ = img.save_with_format(path, image::ImageFormat::Png);
        }
    }

    fn build(&self, ctx: &Context, req: &Request) -> Poster {
        let img = match self.read_disk(req) {
            Some(i) => Some(i),
            None => resolve(&self.loader, &self.memo, &req.candidates).map(|(_, i)| {
                self.write_disk(&req.key, &i);
                i
            }),
        };
        match img {
            Some(i) => Poster {
                tint: Some(average_color(&i)),
                texture: Some(upload(ctx, &req.key, i)),
            },
            None => Poster {
                texture: None,
                tint: req
                    .avatar
                    .as_deref()
                    .and_then(|u| self.loader.load_image(u))
                    .map(|i| average_color(&i)),
            },
        }
    }
}

fn upload(ctx: &Context, key: &str, img: RgbaImage) -> TextureHandle {
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    ctx.load_texture(format!("poster:{key}"), color, TextureOptions::LINEAR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_names_are_filesystem_safe() {
        assert_eq!(disk_name("ab12:my stream/../x"), "ab12_my_stream____x.png");
    }

    #[test]
    fn average_color_ignores_transparent_pixels() {
        let mut img = RgbaImage::new(4, 4);
        for p in img.pixels_mut() {
            *p = image::Rgba([200, 100, 50, 255]);
        }
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        assert_eq!(average_color(&img), Color32::from_rgb(200, 100, 50));
        assert_eq!(average_color(&RgbaImage::new(2, 2)), Color32::from_rgb(32, 29, 38));
    }
}
