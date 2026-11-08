use crate::note_ref::NoteRef;
use crate::note_view::NotesView;
use crate::route::RouteServices;
use crate::stream_info::{StreamInfo, StreamStatus};
use crate::sub::SubRef;
use crate::theme;
use crate::widgets;
use crate::widgets::Hero;
use crate::widgets::{sub_or_poll, NostrWidget};
use egui::{Id, Response, ScrollArea, Ui};
use nostrdb::{Filter, Note};
use std::collections::HashSet;

pub struct HomePage {
    events: HashSet<NoteRef>,
    sub: Option<SubRef>,
    zaps: HashSet<NoteRef>,
    zap_sub: Option<SubRef>,
    zap_page_subs: Vec<Option<SubRef>>,
    backfill: crate::zap::ZapBackfill,
    backfill_seen: usize,
    settle_logged: bool,
    backfill_quiet_since: std::time::Instant,
    preview: Option<(String, widgets::StreamPlayer)>,
    board: (u64, Vec<([u8; 32], u64)>),
    board_len: usize,
    window_anchor: u64,
    board_pending: bool,
    totals: crate::zap::SenderTotals<nostrdb::NoteKey>,
    render_state: Option<egui_wgpu::RenderState>,
    pull: PullToRefresh,
    scroll_at_top: bool,
}

#[derive(Default)]
struct PullToRefresh {
    distance: f32,
    armed: bool,
    was_dragging: bool,
    refreshed_at: Option<std::time::Instant>,
}

pub const PULL_THRESHOLD: f32 = 90.0;

impl PullToRefresh {
    fn update(&mut self, at_top: bool, dragging: bool, delta_y: f32) -> bool {
        let started = dragging && !self.was_dragging;
        self.was_dragging = dragging;
        if !dragging {
            let fire = self.armed && self.distance >= PULL_THRESHOLD;
            self.distance = 0.0;
            self.armed = false;
            return fire;
        }
        if started {
            self.armed = at_top;
        }
        if !at_top {
            self.armed = false;
        }
        if self.armed {
            self.distance = (self.distance + delta_y).max(0.0);
        } else {
            self.distance = 0.0;
        }
        false
    }
}

impl HomePage {
    pub fn new(render_state: Option<egui_wgpu::RenderState>) -> Self {
        Self {
            preview: None,
            pull: PullToRefresh::default(),
            scroll_at_top: true,
            board: (0, Vec::new()),
            board_len: usize::MAX,
            window_anchor: now_secs(),
            board_pending: false,
            totals: crate::zap::SenderTotals::new(),
            render_state,
            events: HashSet::new(),
            sub: None,
            zaps: HashSet::new(),
            zap_sub: None,
            zap_page_subs: Vec::new(),
            backfill: new_backfill(now_secs()),
            backfill_seen: 0,
            settle_logged: false,
            backfill_quiet_since: std::time::Instant::now(),
        }
    }

    fn refresh(&mut self) {
        self.window_anchor = now_secs();
        self.totals = crate::zap::SenderTotals::new();
        self.board_len = usize::MAX;
        self.sub = None;
        self.zap_sub = None;
        self.zap_page_subs.clear();
        self.zaps.clear();
        self.backfill = new_backfill(self.window_anchor);
        self.backfill_seen = 0;
        self.backfill_quiet_since = std::time::Instant::now();
        self.pull.refreshed_at = Some(std::time::Instant::now());
    }

    fn zap_filters(anchor: u64, until: Option<u64>) -> Vec<Filter> {
        let since = crate::zap::window_start(anchor);
        let filter = Filter::new().kinds([9_735]).since(since).limit(5_000);
        vec![match until {
            Some(u) => filter.until(u).build(),
            None => filter.build(),
        }]
    }

    fn advance_backfill(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        if self.backfill.is_done() {
            return Ok(());
        }
        if self.zaps.len() != self.backfill_seen {
            self.backfill_seen = self.zaps.len();
            self.backfill_quiet_since = std::time::Instant::now();
        }
        let quiet = self.backfill_quiet_since.elapsed();
        let settle_after = if self.zaps.is_empty() { BACKFILL_EMPTY_WAIT } else { BACKFILL_QUIET };
        if quiet < settle_after {
            services.ctx.egui.map(|c| c.request_repaint_after(BACKFILL_QUIET / 4));
            return Ok(());
        }
        let oldest = self.zaps.iter().map(|r| r.created_at).min();
        match self.backfill.settle(self.zaps.len(), oldest) {
            Some(until) => {
                let mut sub = None;
                sub_or_poll(
                    services.ctx,
                    services.tx,
                    &mut self.zaps,
                    &mut sub,
                    Self::zap_filters(self.window_anchor, Some(until)),
                )?;
                self.zap_page_subs.push(sub);
                self.backfill_quiet_since = std::time::Instant::now();
                self.backfill_seen = self.zaps.len();
            }
            None => {
                self.board_len = usize::MAX;
                self.settle_logged = false;
            }
        }
        Ok(())
    }

    fn leaderboard(&mut self, services: &mut RouteServices<'_, '_>) -> (u64, Vec<([u8; 32], u64)>) {
        if self.board_len != self.zaps.len() || self.board_pending {
            let ndb = &services.ctx.ndb;
            let tx = services.tx;
            let window_start = crate::zap::window_start(self.window_anchor);
            self.board_pending = self.totals.absorb(
                self.zaps.iter().map(|r| &r.key),
                crate::zap::SENDER_TOTALS_BATCH,
                |key| {
                    let note = ndb.get_note_by_key(tx, key).ok()?;
                    if note.created_at() < window_start {
                        return None;
                    }
                    let z = crate::zap::Zap::from_receipt(note).ok()?;
                    Some((z.sender, z.amount))
                },
            );
            self.board_len = self.zaps.len();
            if self.totals.take_dirty() {
                self.board = (self.totals.total(), self.totals.top(5));
            }
            if self.backfill.is_done() && !self.board_pending && !self.settle_logged {
                self.settle_logged = true;
                log::warn!(
                    "zap backfill settled: pages={} events={} total_sats={}",
                    self.backfill.pages_requested(),
                    self.zaps.len(),
                    self.totals.total()
                );
            }
            if self.board_pending {
                services.ctx.egui.map(|c| c.request_repaint());
            }
        }
        self.board.clone()
    }

    fn zapped_panel(
        &self,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
        total: u64,
        top: &[([u8; 32], u64)],
    ) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(theme::semibold(crate::i18n::t("home.zapped_24h"), 15.0));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let approx = if self.backfill.is_done() { "" } else { "~" };
                ui.label(theme::display(format!("{approx}{}", crate::zap::format_sats_total(total)), 46.0).color(theme::PRIMARY));
                ui.label(theme::medium(crate::i18n::t("common.sats"), 16.0).color(theme::MUTED));
            });
            for (pk, sats) in top {
                let profile = services.profile(pk);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    crate::widgets::Avatar::from_profile(&profile)
                        .size(24.0)
                        .render(ui, services.ctx);
                    let name = profile.and_then(|p| p.name()).unwrap_or(crate::i18n::t("common.anon")).to_string();
                    ui.label(theme::semibold(name, 15.0));
                    ui.with_layout(crate::bidi::end_layout(egui::Align::Center), |ui| {
                        ui.label(
                            theme::semibold(crate::zap::format_sats(*sats as f32), 15.0)
                                .color(theme::PRIMARY),
                        );
                    });
                });
            }
        });
    }

    fn tabs(ui: &mut Ui) -> Option<&'static str> {
        const TABS_TAGS: &[Option<&str>] = &[
            None,
            Some("irl"),
            Some("gaming"),
            Some("music"),
            Some("talk"),
            Some("art"),
            Some("gambling"),
            Some("science"),
        ];
        const TABS: &[&str] = &[
            "home.tab.all",
            "home.tab.irl",
            "home.tab.gaming",
            "home.tab.music",
            "home.tab.talk",
            "home.tab.art",
            "home.tab.gambling",
            "home.tab.science",
        ];
        let sel_id = Id::new("home-category");
        let selected: usize = ui.ctx().data(|d| d.get_temp(sel_id)).unwrap_or(0);
        let mut clicked = None;
        let width = ui.available_width();
        let area = egui::ScrollArea::horizontal()
            .id_salt("category-tabs")
            .max_width(width)
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 24.0;
            for (i, t) in TABS.iter().enumerate() {
                let active = i == selected;
                let text = theme::medium(crate::i18n::t(t), 16.0).color(if active { theme::TEXT } else { theme::MUTED });
                let r = ui
                    .add(egui::Label::new(text).sense(egui::Sense::click()))
                    .on_hover_and_drag_cursor(egui::CursorIcon::PointingHand);
                if r.clicked() {
                    clicked = Some(i);
                }
                if active {
                    let y = r.rect.bottom() + 6.0;
                    ui.painter().hline(
                        r.rect.x_range(),
                        y,
                        egui::Stroke::new(2.0, theme::PRIMARY),
                    );
                }
            }
        });
            });
        let overflow = area.content_size.x - area.inner_rect.width();
        if overflow > 1.0 && area.state.offset.x < overflow - 1.0 {
            let r = area.inner_rect;
            let fade = egui::Rect::from_min_max(egui::pos2(r.right() - 40.0, r.top()), r.right_bottom());
            let clear = theme::BG.gamma_multiply(0.0);
            let mut mesh = egui::epaint::Mesh::default();
            mesh.colored_vertex(fade.left_top(), clear);
            mesh.colored_vertex(fade.right_top(), theme::BG);
            mesh.colored_vertex(fade.right_bottom(), theme::BG);
            mesh.colored_vertex(fade.left_bottom(), clear);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(egui::epaint::Shape::mesh(mesh));
        }
        let y = ui.cursor().top() - 4.0;
        ui.painter().hline(
            ui.max_rect().x_range(),
            y,
            egui::Stroke::new(1.0, theme::BORDER),
        );
        let selected = match clicked {
            Some(i) => {
                ui.ctx().data_mut(|d| d.insert_temp(sel_id, i));
                i
            }
            None => selected,
        };
        TABS_TAGS.get(selected).copied().flatten()
    }

    fn get_filters() -> Vec<Filter> {
        vec![Filter::new().kinds([30_311, 30_313]).limit(100).build()]
    }
}

impl NostrWidget for HomePage {
    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        let (dragging, delta_y) = ui.input(|i| {
            (
                i.pointer.primary_down() && i.pointer.is_decidedly_dragging(),
                i.pointer.delta().y,
            )
        });
        let at_top = self.scroll_at_top;
        if self.pull.update(at_top, dragging, delta_y) {
            self.refresh();
        }
        let pulled = self.pull.distance;
        let refreshing = self
            .pull
            .refreshed_at
            .map_or(false, |t| t.elapsed() < std::time::Duration::from_millis(800));
        if refreshing {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        if pulled > 0.0 || refreshing {
            let key = if refreshing {
                "home.refreshing"
            } else {
                "home.pull_to_refresh"
            };
            ui.add_space((pulled * 0.4).min(40.0));
            ui.vertical_centered(|ui| {
                ui.label(theme::medium(crate::i18n::t(key), 13.0).color(theme::MUTED));
            });
        }
        let scroll_out = ScrollArea::vertical()
            .id_salt("home-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let wide = ui.available_width() >= theme::WIDE_BREAKPOINT;
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(if wide { 0 } else { 16 }, 12))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 16.0);
                        let events: Vec<Note> = self
                            .events
                            .iter()
                            .filter_map(|n| {
                                services.ctx.ndb.get_note_by_key(services.tx, n.key).ok()
                            })
                            .filter(|e| e.can_play())
                            .filter(|e| !services.is_muted(e.host()))
                            .collect();

                        let mut live: Vec<&Note> = events
                            .iter()
                            .filter(|r| matches!(r.status(), StreamStatus::Live))
                            .collect();
                        live.sort_by_key(|e| std::cmp::Reverse(e.viewers().unwrap_or(0)));

                        let (total, top_zappers) = {
                            let _t = crate::frame_stats::Section::start("leaderboard");
                            self.leaderboard(services)
                        };
                        let _q = crate::frame_stats::Section::start("home_body");
                        let preview_url = live.first().and_then(|t| {
                            if t.image().map(|i| !i.is_empty()).unwrap_or(false) {
                                None
                            } else {
                                t.streaming().map(|s| s.to_string())
                            }
                        });
                        match (&preview_url, &self.preview) {
                            (Some(u), Some((cur, _))) if u == cur => {}
                            (Some(u), _) => {
                                self.preview = Some((
                                    u.clone(),
                                    widgets::StreamPlayer::preview(self.render_state.as_ref(), u),
                                ));
                            }
                            (None, _) => self.preview = None,
                        }
                        if let Some(top) = live.first() {
                            let w = ui.available_width();
                            if wide && w > 900.0 {
                                let panel_w = 336.0;
                                let gap = 24.0;
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = gap;
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(w - panel_w - gap, 365.0),
                                        egui::Layout::top_down(crate::bidi::text_align()),
                                        |ui| {
                                            let p = self.preview.as_mut().map(|(_, p)| p);
                                            Hero::new(top).render(ui, services, 365.0, p)
                                        },
                                    );
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(panel_w, 365.0),
                                        egui::Layout::top_down(crate::bidi::text_align()),
                                        |ui| self.zapped_panel(ui, services, total, &top_zappers),
                                    );
                                });
                            } else {
                                let h = (w * 0.5625).clamp(300.0, 440.0);
                                let p = self.preview.as_mut().map(|(_, p)| p);
                                Hero::new(top).render(ui, services, h, p);
                                if total > 0 {
                                    egui::Frame::new()
                                        .fill(theme::SURFACE_LOW)
                                        .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                        .corner_radius(12)
                                        .inner_margin(egui::Margin::same(14))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            self.zapped_panel(ui, services, total, &top_zappers[..top_zappers.len().min(3)]);
                                        });
                                }
                            }
                        }

                        let filter = Self::tabs(ui);
                        let query: String = ui
                            .ctx()
                            .data(|d| d.get_temp(Id::new("search-query")))
                            .unwrap_or_default();
                        let query = query.to_lowercase();
                        live.retain(|e| {
                            filter.map_or(true, |f| e.hashtags().iter().any(|t| t.contains(f)))
                                && (query.is_empty()
                                    || e.title().unwrap_or("").to_lowercase().contains(&query))
                        });

                        let events_live = NotesView::from_vec(live.iter().copied().collect());
                        if events_live.len() > 0 {
                            widgets::StreamList::new(
                                Id::new("live-streams"),
                                events_live,
                                None::<egui::RichText>,
                            )
                            .render(ui, services);
                        }
                        let events_planned = NotesView::from_vec(
                            events
                                .iter()
                                .filter(|r| matches!(r.status(), StreamStatus::Planned))
                                .collect(),
                        );
                        if events_planned.len() > 0 {
                            widgets::StreamList::new(
                                Id::new("planned-streams"),
                                events_planned,
                                Some(theme::semibold(crate::i18n::t("list.planned"), 15.0)),
                            )
                            .render(ui, services);
                        }
                        let events_ended = NotesView::from_vec(
                            events
                                .iter()
                                .filter(|r| {
                                    matches!(r.status(), StreamStatus::Ended)
                                        && r.recording().map(|r| !r.is_empty()).unwrap_or(false)
                                })
                                .collect(),
                        );
                        if events_ended.len() > 0 {
                            widgets::StreamList::new(
                                Id::new("ended-streams"),
                                events_ended,
                                Some(theme::semibold(crate::i18n::t("list.ended"), 15.0)),
                            )
                            .render(ui, services);
                        }
                    });
                ui.response()
            });
        self.scroll_at_top = scroll_out.state.offset.y <= 0.5;
        scroll_out.inner
    }

    fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.zaps,
            &mut self.zap_sub,
            Self::zap_filters(self.window_anchor, None),
        )?;
        for sub in self.zap_page_subs.iter_mut() {
            sub_or_poll(services.ctx, services.tx, &mut self.zaps, sub, Vec::new())?;
        }
        self.advance_backfill(services)?;
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.events,
            &mut self.sub,
            Self::get_filters(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_past_threshold_at_top_refreshes_on_release() {
        let mut p = PullToRefresh::default();
        assert!(!p.update(true, true, 60.0));
        assert!(!p.update(true, true, 40.0));
        assert!(p.update(true, false, 0.0));
        assert_eq!(p.distance, 0.0);
    }

    #[test]
    fn short_pull_does_not_refresh() {
        let mut p = PullToRefresh::default();
        p.update(true, true, 30.0);
        assert!(!p.update(true, false, 0.0));
    }

    #[test]
    fn drag_away_from_top_resets() {
        let mut p = PullToRefresh::default();
        p.update(true, true, 80.0);
        p.update(false, true, 20.0);
        assert!(!p.update(false, false, 0.0));
    }

    #[test]
    fn scroll_down_never_shows_indicator() {
        let mut p = PullToRefresh::default();
        for _ in 0..20 {
            assert!(!p.update(false, true, -15.0));
            assert_eq!(p.distance, 0.0);
        }
        assert!(!p.update(false, false, 0.0));
    }

    #[test]
    fn scrolling_up_to_top_does_not_trigger() {
        let mut p = PullToRefresh::default();
        p.update(false, true, 40.0);
        p.update(false, true, 40.0);
        p.update(true, true, 60.0);
        p.update(true, true, 60.0);
        assert_eq!(p.distance, 0.0);
        assert!(!p.update(true, false, 0.0));
    }

    #[test]
    fn upward_drag_at_top_does_not_accumulate() {
        let mut p = PullToRefresh::default();
        p.update(true, true, -50.0);
        assert_eq!(p.distance, 0.0);
    }
}

const BACKFILL_QUIET: std::time::Duration = std::time::Duration::from_millis(2_500);
const BACKFILL_EMPTY_WAIT: std::time::Duration = std::time::Duration::from_secs(8);
const BACKFILL_MAX_PAGES: usize = 10;

fn new_backfill(anchor: u64) -> crate::zap::ZapBackfill {
    crate::zap::ZapBackfill::new(crate::zap::window_start(anchor), BACKFILL_MAX_PAGES)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
