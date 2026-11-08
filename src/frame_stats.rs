use std::time::{Duration, Instant};

pub static PROCESS_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn mark_start() {
    let _ = PROCESS_START.set(Instant::now());
}

pub fn debug_scroll_step() -> Option<(f32, [f32; 2])> {
    static STEP: std::sync::OnceLock<Option<(f32, f32, [f32; 2])>> = std::sync::OnceLock::new();
    let (px, period, at) = (*STEP.get_or_init(|| {
        let v = std::env::var("ZAP_STREAM_DEBUG_SCROLL").ok()?;
        let p: Vec<f32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
        Some((*p.first()?, *p.get(1).unwrap_or(&4.0), [*p.get(2).unwrap_or(&200.0), *p.get(3).unwrap_or(&600.0)]))
    }))?;
    let t = PROCESS_START.get()?.elapsed().as_secs_f32();
    Some((scroll_delta(px, period, t), at))
}

pub fn scroll_delta(px: f32, period: f32, t: f32) -> f32 {
    if (t / period) as u64 % 2 == 0 {
        -px
    } else {
        px
    }
}

pub struct FrameStats {
    last: Instant,
    window_start: Instant,
    frames: Vec<f32>,
    work: Vec<f32>,
}

impl FrameStats {
    pub fn enabled() -> Option<Self> {
        if cfg!(target_os = "android") || std::env::var_os("ZAP_STREAM_FRAME_STATS").is_some() {
            Some(Self::new())
        } else {
            None
        }
    }

    fn new() -> Self {
        let now = Instant::now();
        Self {
            last: now,
            window_start: now,
            frames: Vec::new(),
            work: Vec::new(),
        }
    }

    pub fn end(&mut self) {
        self.work.push(self.last.elapsed().as_secs_f32() * 1000.0);
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        if self.frames.is_empty() && self.work.is_empty() && !FIRST.swap(true, std::sync::atomic::Ordering::Relaxed) {
            if let Some(start) = PROCESS_START.get() {
                log::info!("first_frame {:.0}ms after start", start.elapsed().as_secs_f32() * 1000.0);
            }
        }
        self.frames
            .push(now.duration_since(self.last).as_secs_f32() * 1000.0);
        self.last = now;
        if now.duration_since(self.window_start) >= Duration::from_secs(5) {
            self.report();
            self.window_start = now;
        }
    }

    fn report(&mut self) {
        let work = summary(std::mem::take(&mut self.work));
        let Some(frames) = summary(std::mem::take(&mut self.frames)) else {
            return;
        };
        log::info!(
            "frame_stats frames={} mean={:.2}ms p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms",
            frames.count, frames.mean, frames.p50, frames.p95, frames.p99, frames.max
        );
        if let Some(w) = work {
            log::info!(
                "work_stats frames={} mean={:.2}ms p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms",
                w.count, w.mean, w.p50, w.p95, w.p99, w.max
            );
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub mean: f32,
    pub p50: f32,
    pub p95: f32,
    pub p99: f32,
    pub max: f32,
}

pub fn summary(mut t: Vec<f32>) -> Option<Summary> {
    if t.is_empty() {
        return None;
    }
    t.sort_by(|a, b| a.total_cmp(b));
    let q = |p: f32| t[((t.len() - 1) as f32 * p) as usize];
    Some(Summary {
        count: t.len(),
        mean: t.iter().sum::<f32>() / t.len() as f32,
        p50: q(0.5),
        p95: q(0.95),
        p99: q(0.99),
        max: q(1.0),
    })
}

pub struct Section {
    name: &'static str,
    start: Instant,
}

impl Section {
    pub fn start(name: &'static str) -> Self {
        Self {
            name,
            start: Instant::now(),
        }
    }
}

impl Drop for Section {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_secs_f32() * 1000.0;
        if ms > 8.0 {
            log::info!("slow_section {} {:.1}ms", self.name, ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_scroll_alternates_direction_each_period() {
        assert_eq!(scroll_delta(3.0, 4.0, 1.0), -3.0);
        assert_eq!(scroll_delta(3.0, 4.0, 5.0), 3.0);
        assert_eq!(scroll_delta(3.0, 4.0, 9.0), -3.0);
    }

    #[test]
    fn summary_picks_percentiles() {
        let s = summary((1..=100).map(|v| v as f32).collect()).unwrap();
        assert_eq!(s.count, 100);
        assert_eq!(s.p50, 50.0);
        assert_eq!(s.p95, 95.0);
        assert_eq!(s.p99, 99.0);
        assert_eq!(s.max, 100.0);
        assert!(summary(Vec::new()).is_none());
    }
}
