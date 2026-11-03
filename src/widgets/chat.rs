use crate::chat_data::{self, BuildInput, ChatData, Parsed, RowKind, Want};
use crate::chat_model::{format_duration, TIMEOUT_CHOICES};
use crate::link::NostrLink;
use crate::note_ref::NoteRef;
use crate::route::RouteServices;
use crate::sub::SubRef;
use crate::stream_info::StreamInfo;
use crate::widgets::chat_zap::ChatZap;
use crate::widgets::{
    chat_badge, chat_message, chat_raid, chat_reactions, chat_timeout, emoji_picker, mute_button,
    sub_or_poll, NostrWidget,
};
use crate::zap::{format_sats, SenderTotals, Zap, SENDER_TOTALS_BATCH};
use egui::{Id, Modal, Response, ScrollArea, Sense, Ui};
use nostrdb::{Filter, NoteKey};
use std::collections::{HashMap, HashSet};

const LONG_PRESS_SECS: f64 = 0.45;

fn long_press_in(ui: &Ui, rect: egui::Rect, id: Id) -> bool {
    let ctx = ui.ctx();
    let (down, pos, started, still, now) = ctx.input(|i| {
        (
            i.pointer.primary_down(),
            i.pointer.interact_pos(),
            i.pointer.press_start_time(),
            i.pointer.is_still(),
            i.time,
        )
    });
    let inside = pos.is_some_and(|p| rect.contains(p));
    let fired_key = Id::new("chat-hold-fired");
    if !down {
        if inside {
            ctx.data_mut(|d| d.remove::<Id>(fired_key));
        }
        return false;
    }
    if !inside || !still {
        return false;
    }
    let Some(started) = started else {
        return false;
    };
    let elapsed = now - started;
    if elapsed < LONG_PRESS_SECS {
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
        return false;
    }
    let already = ctx.data(|d| d.get_temp::<Id>(fired_key));
    if already == Some(id) {
        return false;
    }
    ctx.data_mut(|d| d.insert_temp(fired_key, id));
    true
}

struct ModalState {
    id: [u8; 32],
    author: [u8; 32],
    content: String,
    picker: bool,
    timeouts: bool,
}

pub fn debug_modal_flags(v: &str) -> (bool, bool) {
    (v == "picker", v == "timeouts")
}

fn debug_modal(id: &[u8; 32], author: &[u8; 32], content: &str) -> Option<ModalState> {
    let v = std::env::var("ZAP_STREAM_DEBUG_CHAT_MODAL").ok()?;
    std::env::remove_var("ZAP_STREAM_DEBUG_CHAT_MODAL");
    let (picker, timeouts) = debug_modal_flags(&v);
    Some(ModalState { id: *id, author: *author, content: content.to_string(), picker, timeouts })
}

const RESTICK_FRAMES: u8 = 3;
const ROW_SLACK: f32 = 48.0;

fn near_bottom(offset: f32, viewport: f32, content: f32, slack: f32) -> bool {
    content <= viewport || offset + viewport >= content - slack
}

fn build_stamp(events: usize, aux: usize, now: u64, time_independent: bool) -> (usize, usize, u64) {
    (events, aux, if time_independent { 0 } else { now / 10 })
}

fn commit_build(
    slot: &mut ChatData,
    wants: &mut Vec<Want>,
    data: ChatData,
    subscribe: impl FnOnce(&[Want]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let new_wants = data.wants.clone();
    *slot = data;
    if new_wants == *wants {
        return Ok(());
    }
    subscribe(&new_wants)?;
    *wants = new_wants;
    Ok(())
}

pub struct Chat {
    link: NostrLink,
    stream: Option<NoteKey>,
    opened: std::time::Instant,
    logged: u8,
    events: HashSet<NoteRef>,
    aux: HashSet<NoteRef>,
    sub: Option<SubRef>,
    aux_sub: Option<SubRef>,
    aux_wants: Vec<Want>,
    data: ChatData,
    parsed: Parsed,
    built_for: (usize, usize, u64),
    heights: HashMap<NoteKey, f32>,
    measured_width: f32,
    at_bottom: bool,
    restick_frames: u8,
    raid_started: HashMap<NoteKey, Option<f64>>,
    modal: Option<ModalState>,
    top: Vec<([u8; 32], u64)>,
    top_len: usize,
    top_pending: bool,
    totals: SenderTotals<NoteKey>,
}

impl Chat {
    pub fn new(link: NostrLink, stream: Option<NoteKey>) -> Self {
        Self {
            link,
            stream,
            opened: std::time::Instant::now(),
            logged: 0,
            events: HashSet::new(),
            aux: HashSet::new(),
            sub: None,
            aux_sub: None,
            aux_wants: Vec::new(),
            data: ChatData::default(),
            parsed: Parsed::default(),
            built_for: (usize::MAX, usize::MAX, 0),
            heights: Default::default(),
            measured_width: 0.0,
            at_bottom: true,
            restick_frames: 0,
            raid_started: HashMap::new(),
            modal: None,
            top: Vec::new(),
            top_len: usize::MAX,
            top_pending: false,
            totals: SenderTotals::new(),
        }
    }

    pub fn set_stream(&mut self, stream: NoteKey) {
        self.stream = Some(stream);
    }

    fn host(&self, services: &RouteServices<'_, '_>) -> Option<[u8; 32]> {
        if let Some(key) = self.stream {
            if let Ok(note) = services.ctx.ndb.get_note_by_key(services.tx, key) {
                return Some(*note.host());
            }
        }
        self.link.author
    }

    fn mark(&mut self, bit: u8, what: &str) {
        if self.logged & bit == 0 {
            self.logged |= bit;
            log::info!("chat_timing {} at {:?}", what, self.opened.elapsed());
        }
    }

    pub fn disabled_until(&self) -> Option<Option<u64>> {
        self.data.disabled_until
    }

    pub fn custom_emoji(&self) -> &[(String, String)] {
        &self.data.custom_emoji
    }

    fn main_filters(&self, host: &[u8; 32], me: Option<[u8; 32]>) -> Vec<Filter> {
        let tag = self.link.to_tag_value();
        let mut moderators = vec![*host];
        moderators.extend(me);
        vec![
            Filter::new()
                .kinds([1_311, 9_735, 1_312])
                .tags([tag.as_str()], 'a')
                .limit(500)
                .build(),
            Filter::new()
                .kinds([10_000, 1_314, 10_030])
                .authors(moderators.iter())
                .build(),
            Filter::new().kinds([8]).authors([host]).build(),
        ]
    }

    fn render_row(
        &mut self,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
        idx: usize,
        host: [u8; 32],
        stream_tag: &str,
    ) {
        let row = self.data.rows[idx].clone();
        match &row.kind {
            RowKind::Message { id, author, segments, content } => {
                if services.is_muted(author) {
                    return;
                }
                if self.modal.is_none() {
                    if let Some(state) = debug_modal(id, author, content) {
                        self.modal = Some(state);
                    }
                }
                let known = self.heights.get(&row.key).copied();
                let can_open = services.me().is_some();
                let top = ui.cursor().min;
                let row_left = ui.max_rect().left();
                let row_width = ui.available_width();
                let rect = egui::Rect::from_min_size(
                    top,
                    egui::vec2(ui.available_width(), known.unwrap_or(0.0)),
                );
                let bg = if can_open && known.is_some() {
                    Some(ui.interact(rect, Id::new(("chat-row", row.key)), Sense::click()))
                } else {
                    None
                };
                chat_message::render(
                    ui,
                    services,
                    &self.data,
                    &chat_message::Message {
                        author,
                        segments,
                        is_host: *author == host,
                    },
                );
                let counts = self.data.reactions.get(id).map(|v| v.as_slice()).unwrap_or(&[]);
                let sats = self.data.zapped.get(id).copied();
                chat_reactions::show(ui, services, counts, sats);
                let full_rect = egui::Rect::from_min_max(
                    egui::pos2(row_left, top.y),
                    egui::pos2(row_left + row_width, ui.cursor().min.y),
                );
                let held = long_press_in(ui, full_rect, Id::new(("chat-row-hold", row.key)));
                if held {
                    self.modal = Some(ModalState {
                        id: *id,
                        author: *author,
                        content: content.clone(),
                        picker: false,
                        timeouts: false,
                    });
                } else if let Some(bg) = bg {
                    if bg.clicked() || bg.secondary_clicked() {
                        self.modal = Some(ModalState {
                            id: *id,
                            author: *author,
                            content: content.clone(),
                            picker: false,
                            timeouts: false,
                        });
                    }
                }
            }
            RowKind::Zap { sender, sats, message } => {
                let profile = services.profile(sender);
                let zap = Zap {
                    sender: *sender,
                    receiver: [0; 32],
                    zapper_service: &[0; 32],
                    amount: *sats,
                    message: message.clone(),
                };
                ChatZap::new(&zap, &profile).render(ui, services.ctx);
            }
            RowKind::Raid { raid, created_at } => {
                let started = self.raid_started.entry(row.key).or_insert(None);
                chat_raid::show(ui, services, &self.data, raid, *created_at, stream_tag, started);
            }
            RowKind::Timeout(t) => chat_timeout::show(ui, services, t),
            RowKind::BadgeAward { badge, users } => {
                chat_badge::award(ui, services, self.data.badge_defs.get(badge), users);
            }
        }
    }

    fn render_modal(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>, host: [u8; 32]) {
        let Some(state) = self.modal.as_mut() else {
            return;
        };
        let is_moderator = services.me() == Some(host);
        let mut close = false;
        let mut react = None;
        let mut timeout = None;
        let resp = Modal::new(Id::new("chat-modal")).show(ui.ctx(), |ui| {
            ui.set_width(320.0);
            let name = chat_message::display_name(services, &state.author);
            ui.label(crate::theme::semibold(name, 16.0));
            ui.add_space(4.0);
            ui.label(&state.content);
            ui.add_space(8.0);
            let counts = self.data.reactions.get(&state.id).map(|v| v.as_slice()).unwrap_or(&[]);
            chat_reactions::show(ui, services, counts, self.data.zapped.get(&state.id).copied());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("😀").clicked() {
                    state.picker = !state.picker;
                    state.timeouts = false;
                }
                if is_moderator && ui.button(crate::i18n::t("chat.timeout")).clicked() {
                    state.timeouts = !state.timeouts;
                    state.picker = false;
                }
            });
            if state.picker {
                react = emoji_picker::show(ui, services, &self.data.custom_emoji);
            }
            if state.timeouts {
                ui.horizontal_wrapped(|ui| {
                    for secs in TIMEOUT_CHOICES {
                        if ui.button(format_duration(secs)).clicked() {
                            timeout = Some(secs);
                        }
                    }
                });
            }
            ui.add_space(8.0);
            if mute_button::show(ui, services, &state.author) {
                close = true;
            }
        });
        if let Some(action) = std::env::var("ZAP_STREAM_DEBUG_CHAT_ACTION").ok() {
            std::env::remove_var("ZAP_STREAM_DEBUG_CHAT_ACTION");
            match action.as_str() {
                "react" => services.react(&state.id, &state.author, "🔥", None),
                "timeout" => services.timeout_user(&state.author, 600),
                "mute" => services.set_muted(&state.author, true),
                _ => {}
            }
            close = true;
        }
        if let Some(p) = react {
            services.react(&state.id, &state.author, &p.content, p.url.as_deref());
            close = true;
        }
        if let Some(secs) = timeout {
            services.timeout_user(&state.author, secs);
            close = true;
        }
        if close || resp.should_close() {
            self.modal = None;
        }
    }
}

impl Chat {
    fn top_zappers_row(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        if self.top_len != self.events.len() || self.top_pending {
            self.top_len = self.events.len();
            let ndb = &services.ctx.ndb;
            let tx = services.tx;
            self.top_pending = self.totals.absorb(
                self.events.iter().map(|r| &r.key),
                SENDER_TOTALS_BATCH,
                |key| {
                    let note = ndb.get_note_by_key(tx, key).ok()?;
                    let z = Zap::from_receipt(note).ok()?;
                    Some((z.sender, z.amount))
                },
            );
            if self.totals.take_dirty() {
                self.top = self.totals.top(10);
            }
            if self.top_pending {
                ui.ctx().request_repaint();
            }
        }
        if self.top.is_empty() {
            return;
        }
        ui.add_space(4.0);
        egui::ScrollArea::horizontal()
            .id_salt("chat-top-zappers")
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.add_space(12.0);
                    ui.set_min_height(24.0);
                    for (pk, sats) in self.top.clone() {
                        let profile = services.profile(&pk);
                        crate::widgets::Avatar::from_profile(&profile)
                            .size(20.0)
                            .render(ui, services.ctx);
                        ui.label(
                            crate::theme::semibold(format_sats(sats as f32), 13.0)
                                .color(crate::theme::ZAP),
                        );
                    }
                });
            });
        ui.add_space(8.0);
    }
}

impl NostrWidget for Chat {
    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        let host = self.host(services).unwrap_or_default();
        if !self.data.rows.is_empty() {
            self.mark(8, "first_render_with_rows");
        }
        let stream_tag = self.link.to_tag_value();

        let width = ui.available_width();
        if (self.measured_width - width).abs() > 1.0 {
            self.heights.clear();
            self.measured_width = width;
            if self.at_bottom {
                self.restick_frames = RESTICK_FRAMES;
            }
        }

        self.top_zappers_row(ui, services);

        let estimate = 32.0;
        let restick = self.restick_frames > 0;
        self.restick_frames = self.restick_frames.saturating_sub(1);
        if restick {
            ui.ctx().request_repaint();
        }
        let out = ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show_viewport(ui, |ui, viewport| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let margin = 32.0;
                let mut y = 0.0;
                let mut rendered = 0usize;
                let t0 = std::time::Instant::now();
                for idx in 0..self.data.rows.len() {
                    let key = self.data.rows[idx].key;
                    let known = self.heights.get(&key).copied();
                    let h = known.unwrap_or(estimate);
                    let offscreen = known.is_some()
                        && (y + h < viewport.min.y - margin || y > viewport.max.y + margin);
                    if offscreen {
                        ui.add_space(h);
                        y += h;
                        continue;
                    }
                    rendered += 1;
                    let rect = ui
                        .scope(|ui| self.render_row(ui, services, idx, host, &stream_tag))
                        .response
                        .rect;
                    ui.add_space(8.0);
                    let measured = rect.height() + 8.0;
                    self.heights.insert(key, measured);
                    y += measured;
                }
                if t0.elapsed().as_micros() > 1500 {
                    log::info!("chat_rows rendered={} total={} {:?}", rendered, self.data.rows.len(), t0.elapsed());
                }
                if restick {
                    ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                }
            });
        if !restick && self.restick_frames == 0 {
            self.at_bottom = near_bottom(
                out.state.offset.y,
                out.inner_rect.height(),
                out.content_size.y,
                ROW_SLACK,
            );
        }
        self.render_modal(ui, services, host);
        ui.response()
    }

    fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        let Some(host) = self.host(services) else {
            return Ok(());
        };
        let me = services.me();
        let filters = self.main_filters(&host, me);
        if self.sub.is_none() {
            self.mark(1, "sub_start");
        }
        sub_or_poll(services.ctx, services.tx, &mut self.events, &mut self.sub, filters)?;
        if !self.events.is_empty() {
            self.mark(2, "first_events");
        }

        if !self.aux_wants.is_empty() || self.aux_sub.is_some() {
            if let Some(sub) = &self.aux_sub {
                let ndb = &services.ctx.ndb;
                for k in ndb.poll_for_notes(sub.sub, 500) {
                    if let Ok(note) = ndb.get_note_by_key(services.tx, k) {
                        self.aux.insert(NoteRef::from_note(&note));
                    }
                }
            }
        }

        let now = nostr::Timestamp::now().as_u64();
        let stamp = build_stamp(self.events.len(), self.aux.len(), now, self.data.muted.is_empty());
        if stamp != self.built_for {
            self.built_for = stamp;
            let data = chat_data::build(
                &BuildInput {
                    ndb: services.ctx.ndb,
                    tx: services.tx,
                    main: &self.events,
                    aux: &self.aux,
                    host,
                    me,
                    now,
                },
                &mut self.parsed,
            );
            if !data.rows.is_empty() {
                self.mark(4, "first_build");
            }
            let aux_sub = &mut self.aux_sub;
            let aux = &mut self.aux;
            let result = commit_build(&mut self.data, &mut self.aux_wants, data, |wants| {
                *aux_sub = None;
                if wants.is_empty() {
                    return Ok(());
                }
                let filters: Vec<Filter> = wants.iter().map(|w| w.filter()).collect();
                let mut sink = HashSet::new();
                sub_or_poll(services.ctx, services.tx, &mut sink, aux_sub, filters)?;
                aux.extend(sink);
                Ok(())
            });
            if result.is_err() {
                self.built_for = (usize::MAX, usize::MAX, 0);
            }
            result?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{build_stamp, commit_build, debug_modal_flags, near_bottom};
    use crate::chat_data::{ChatData, Want};

    #[test]
    fn rows_survive_failed_aux_subscription() {
        let mut slot = ChatData::default();
        let mut wants = Vec::new();
        let mut data = ChatData::default();
        data.muted.insert([1; 32]);
        data.wants.push(Want::Event([2; 32]));
        let r = commit_build(&mut slot, &mut wants, data, |_| anyhow::bail!("too many subscriptions"));
        assert!(r.is_err());
        assert!(slot.muted.contains(&[1; 32]));
        assert!(wants.is_empty());
    }

    #[test]
    fn near_bottom_decides_anchor() {
        assert!(near_bottom(0.0, 500.0, 300.0, 48.0));
        assert!(near_bottom(980.0, 500.0, 1500.0, 48.0));
        assert!(near_bottom(960.0, 500.0, 1500.0, 48.0));
        assert!(!near_bottom(900.0, 500.0, 1500.0, 48.0));
        assert!(!near_bottom(0.0, 500.0, 1500.0, 48.0));
    }

    #[test]
    fn build_stamp_ignores_time_without_mutes() {
        assert_eq!(build_stamp(5, 2, 1000, true), build_stamp(5, 2, 5000, true));
        assert_ne!(build_stamp(5, 2, 1000, false), build_stamp(5, 2, 5000, false));
    }

    #[test]
    fn debug_modal_flags_select_panel() {
        assert_eq!(debug_modal_flags("picker"), (true, false));
        assert_eq!(debug_modal_flags("timeouts"), (false, true));
        assert_eq!(debug_modal_flags("1"), (false, false));
    }
}
