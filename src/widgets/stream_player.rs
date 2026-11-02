use crate::hls::{self, Rendition};
use crate::player_health::{self, Health};
use crate::theme::{self, BG, TEXT, TEXT_SOFT};
use crate::widgets::player_controls::{self, ControlsInput};
use crate::widgets::PlaceholderRect;
use egui::epaint::{Mesh, Shape};
use egui::{pos2, vec2, Align2, Color32, FontId, Id, Order, Pos2, Rect, Response, Sense, Ui, Widget};
use lumina_video::{VideoPlayer, VideoState};
use poll_promise::Promise;
use std::time::{Duration, Instant};

const CONTROLS_HIDE_AFTER: Duration = Duration::from_secs(3);

pub struct PlayerInfo {
    pub live: bool,
    pub started: Option<u64>,
    pub viewers: Option<u32>,
}

pub struct StreamPlayer {
    player: Option<VideoPlayer>,
    render_state: Option<egui_wgpu::RenderState>,
    url: String,
    active_url: String,
    preview: bool,
    master: Option<Promise<ehttp::Result<ehttp::Response>>>,
    renditions: Vec<Rendition>,
    selected_quality: Option<usize>,
    menu_open: bool,
    fullscreen: bool,
    fullscreen_sent: bool,
    last_activity: Instant,
    unhealthy_since: Option<Instant>,
    attempts: u32,
}

impl StreamPlayer {
    pub fn new(render_state: Option<&egui_wgpu::RenderState>, url: &str) -> Self {
        let mut s = Self::bare(render_state, url, false);
        s.start_master_fetch();
        s.player = s.build_player(url);
        s
    }

    pub fn preview(render_state: Option<&egui_wgpu::RenderState>, url: &str) -> Self {
        let mut s = Self::bare(render_state, url, true);
        s.player = s.build_player(url);
        s
    }

    fn bare(render_state: Option<&egui_wgpu::RenderState>, url: &str, preview: bool) -> Self {
        Self {
            player: None,
            render_state: render_state.cloned(),
            url: url.to_string(),
            active_url: url.to_string(),
            preview,
            master: None,
            renditions: Vec::new(),
            selected_quality: None,
            menu_open: false,
            fullscreen: std::env::var_os("ZAP_STREAM_DEBUG_FULLSCREEN").is_some(),
            fullscreen_sent: false,
            last_activity: Instant::now(),
            unhealthy_since: None,
            attempts: 0,
        }
    }

    fn start_master_fetch(&mut self) {
        if !self.url.contains(".m3u8") {
            return;
        }
        let (sender, promise) = Promise::new();
        ehttp::fetch(ehttp::Request::get(&self.url), move |r| sender.send(r));
        self.master = Some(promise);
    }

    fn build_player(&self, url: &str) -> Option<VideoPlayer> {
        let rs = self.render_state.as_ref()?;
        let p = VideoPlayer::with_wgpu(url, rs).with_autoplay(true);
        Some(if self.preview {
            p.with_muted(true).with_controls(false)
        } else {
            p.with_controls(false)
        })
    }

    fn poll_master(&mut self) {
        let Some(promise) = &self.master else { return };
        let Some(result) = promise.ready() else { return };
        if let Ok(resp) = result {
            if let Some(text) = resp.text() {
                if hls::is_master(text) {
                    self.renditions = hls::parse_master(text, &self.url);
                }
            }
        }
        self.master = None;
    }

    fn set_quality(&mut self, choice: Option<usize>) {
        self.selected_quality = choice;
        let target = match choice {
            Some(i) => self.renditions[i].url.clone(),
            None => self.url.clone(),
        };
        self.reload(&target);
    }

    fn reload(&mut self, url: &str) {
        let (volume, muted) = self
            .player
            .as_ref()
            .map(|p| (p.volume(), p.is_muted()))
            .unwrap_or((100, false));
        self.active_url = url.to_string();
        self.player = self.build_player(url);
        if let Some(p) = self.player.as_mut() {
            p.set_volume(volume);
            if muted {
                p.toggle_mute();
            }
        }
        self.unhealthy_since = None;
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn exit_fullscreen(&mut self) {
        self.fullscreen = false;
    }

    fn health(&self) -> Health {
        match self.player.as_ref().map(|p| p.state()) {
            Some(VideoState::Error(_)) | Some(VideoState::Ended) => Health::Failed,
            Some(VideoState::Buffering { .. }) => Health::Stalled,
            Some(VideoState::Loading) => Health::Stalled,
            _ => Health::Healthy,
        }
    }

    fn watch_health(&mut self, ctx: &egui::Context) {
        let health = self.health();
        if health == Health::Healthy {
            self.unhealthy_since = None;
            self.attempts = 0;
            return;
        }
        let since = *self.unhealthy_since.get_or_insert_with(Instant::now);
        if player_health::should_retry(health, since.elapsed(), self.attempts) {
            self.attempts += 1;
            let url = self.active_url.clone();
            self.reload(&url);
            self.unhealthy_since = Some(Instant::now());
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn reconnect_banner(&self, ui: &Ui, rect: Rect) {
        if self.unhealthy_since.is_none() || self.attempts == 0 {
            return;
        }
        let failed = matches!(
            self.player.as_ref().map(|p| p.state()),
            Some(VideoState::Error(_)) | Some(VideoState::Ended)
        );
        let text = if failed && self.attempts >= 3 {
            crate::i18n::t("player.unavailable")
        } else {
            crate::i18n::t("player.reconnecting")
        };
        let g = ui.painter().layout_no_wrap(
            text.into(),
            FontId::new(14.0, theme::medium_family()),
            TEXT,
        );
        let r = Rect::from_center_size(
            rect.center_top() + vec2(0.0, 44.0),
            g.size() + vec2(24.0, 14.0),
        );
        ui.painter()
            .rect_filled(r, egui::CornerRadius::same(8), Color32::from_black_alpha(190));
        ui.painter()
            .galley(r.min + vec2(12.0, 7.0), g, Color32::PLACEHOLDER);
    }

    fn controls_visible(&mut self, ui: &Ui, rect: Rect) -> bool {
        let now = ui.input(|i| {
            i.pointer
                .hover_pos()
                .map(|p| rect.contains(p) && (i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_down()))
                .unwrap_or(false)
        });
        if now || self.menu_open {
            self.last_activity = Instant::now();
        }
        let paused = self.player.as_ref().map(|p| !p.is_playing()).unwrap_or(true);
        let visible = paused
            || std::env::var_os("ZAP_STREAM_DEBUG_CONTROLS").is_some()
            || self.last_activity.elapsed() < CONTROLS_HIDE_AFTER;
        if visible && !paused {
            ui.ctx().request_repaint_after(CONTROLS_HIDE_AFTER);
        }
        visible
    }

    pub fn show_in(&mut self, ui: &mut Ui, rect: Rect) -> bool {
        self.poll_master();
        match self.player.as_mut() {
            Some(p) => {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                p.show(&mut child, rect.size());
                true
            }
            None => false,
        }
    }

    pub fn show_controlled(&mut self, ui: &mut Ui, rect: Rect, info: &PlayerInfo) {
        self.poll_master();
        self.watch_health(ui.ctx());
        self.sync_os_fullscreen(ui.ctx());
        if self.fullscreen {
            ui.painter().rect_filled(rect, 16.0, BG);
            let screen = ui.ctx().content_rect();
            let mut inner = None;
            egui::Area::new(Id::new("player-fullscreen"))
                .order(Order::Foreground)
                .fixed_pos(screen.min)
                .show(ui.ctx(), |ui| {
                    ui.set_clip_rect(screen);
                    ui.painter().rect_filled(screen, 0.0, Color32::BLACK);
                    inner = Some(self.draw(ui, screen, info, true));
                });
            let _ = inner;
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.fullscreen = false;
            }
        } else {
            self.draw(ui, rect, info, false);
            mask_corners(ui, rect, 16.0);
        }
    }

    fn sync_os_fullscreen(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_os = "android"))]
        if self.fullscreen != self.fullscreen_sent {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            self.fullscreen_sent = self.fullscreen;
        }
        #[cfg(target_os = "android")]
        {
            let _ = ctx;
            if self.fullscreen != self.fullscreen_sent {
                crate::android_window::set_landscape_fullscreen(self.fullscreen);
                self.fullscreen_sent = self.fullscreen;
            }
        }
    }

    fn draw(&mut self, ui: &mut Ui, rect: Rect, info: &PlayerInfo, fullscreen: bool) {
        let video_rect = fit_16_9(rect);
        let Some(p) = self.player.as_mut() else {
            ui.put(rect, PlaceholderRect);
            return;
        };
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(video_rect));
        child.set_clip_rect(video_rect.intersect(ui.clip_rect()));
        p.show(&mut child, video_rect.size());
        let _ = fullscreen;

        self.reconnect_banner(ui, video_rect);

        if !self.controls_visible(ui, video_rect) {
            return;
        }
        let labels: Vec<String> = self.renditions.iter().map(|r| r.label.clone()).collect();
        let quality_label = self
            .selected_quality
            .and_then(|i| labels.get(i).cloned())
            .unwrap_or_else(|| "Auto".to_string());
        let elapsed = info.started.filter(|_| info.live).map(|s| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(s);
            player_health::format_elapsed(now.saturating_sub(s))
        });
        if elapsed.is_some() {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
        let p = self.player.as_ref().unwrap();
        let input = ControlsInput {
            playing: p.is_playing(),
            muted: p.is_muted(),
            volume: p.volume(),
            fullscreen: self.fullscreen,
            quality_label: &quality_label,
            qualities: &labels,
            selected_quality: self.selected_quality,
            menu_open: self.menu_open,
            live: info.live,
            elapsed,
            viewers: info.viewers,
        };
        let out = player_controls::show(ui, video_rect, &input);
        let p = self.player.as_mut().unwrap();
        if out.toggle_play {
            p.toggle_playback();
        }
        if out.toggle_mute {
            p.toggle_mute();
        }
        if let Some(v) = out.volume {
            if p.is_muted() && v > 0 {
                p.toggle_mute();
            }
            p.set_volume(v);
        }
        if out.toggle_fullscreen {
            self.fullscreen = !self.fullscreen;
        }
        if out.enter_pip {
            crate::android_window::enter_pip();
        }
        if out.toggle_menu {
            self.menu_open = !self.menu_open;
        }
        if let Some(i) = out.pick_quality {
            self.menu_open = false;
            let choice = if i == 0 { None } else { Some(i - 1) };
            if choice != self.selected_quality {
                self.set_quality(choice);
            }
        }
    }
}

fn fit_16_9(rect: Rect) -> Rect {
    let target = 16.0 / 9.0;
    if rect.width() / rect.height() > target {
        Rect::from_center_size(rect.center(), vec2(rect.height() * target, rect.height()))
    } else {
        Rect::from_center_size(rect.center(), vec2(rect.width(), rect.width() / target))
    }
}

impl Widget for &mut StreamPlayer {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let info = PlayerInfo {
            live: false,
            started: None,
            viewers: None,
        };
        self.show_controlled(ui, rect, &info);
        response
    }
}

pub fn paint_state_card(ui: &mut Ui, rect: Rect, title: &str, detail: &str) {
    ui.painter().rect_filled(rect, 16.0, theme::NEUTRAL_800);
    let center = rect.center();
    ui.painter().text(
        center - vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        title,
        FontId::new(22.0, theme::semibold_family()),
        TEXT,
    );
    ui.painter().text(
        center + vec2(0.0, 16.0),
        Align2::CENTER_CENTER,
        detail,
        FontId::new(15.0, theme::medium_family()),
        TEXT_SOFT,
    );
}

pub fn mask_corners(ui: &Ui, rect: Rect, r: f32) {
    let painter = ui.painter();
    let steps = 8;
    let corners: [(Pos2, f32, f32); 4] = [
        (rect.left_top(), 1.0, 1.0),
        (rect.right_top(), -1.0, 1.0),
        (rect.right_bottom(), -1.0, -1.0),
        (rect.left_bottom(), 1.0, -1.0),
    ];
    for (corner, sx, sy) in corners {
        let center = pos2(corner.x + sx * r, corner.y + sy * r);
        let mut mesh = Mesh::default();
        mesh.colored_vertex(corner, BG);
        for i in 0..=steps {
            let a = std::f32::consts::FRAC_PI_2 * i as f32 / steps as f32;
            let p = pos2(center.x - sx * r * a.sin(), center.y - sy * r * a.cos());
            mesh.colored_vertex(p, BG);
        }
        for i in 0..steps {
            mesh.add_triangle(0, 1 + i as u32, 2 + i as u32);
        }
        painter.add(Shape::mesh(mesh));
    }
}
