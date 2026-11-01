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
use crate::zap::{format_sats, top_senders, Zap};
use egui::{Id, Modal, Response, ScrollArea, Sense, Ui};
use nostrdb::{Filter, NoteKey};
use std::collections::{HashMap, HashSet};

struct ModalState {
    id: [u8; 32],
    author: [u8; 32],
    content: String,
    picker: bool,
    timeouts: bool,
}

pub struct Chat {
    link: NostrLink,
    stream: NoteKey,
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
    raid_started: HashMap<NoteKey, Option<f64>>,
    modal: Option<ModalState>,
    top: Vec<([u8; 32], u64)>,
    top_len: usize,
}

impl Chat {
    pub fn new<'a>(link: NostrLink, stream: NoteKey) -> Self {
        Self {
            link,
            stream,
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
            raid_started: HashMap::new(),
            modal: None,
            top: Vec::new(),
            top_len: usize::MAX,
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
                let known = self.heights.get(&row.key).copied();
                let can_open = services.me().is_some();
                let top = ui.cursor().min;
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
                if let Some(bg) = bg {
                    if bg.clicked() || bg.long_touched() || bg.secondary_clicked() {
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
            RowKind::Zap => {
                let Ok(ev) = services.ctx.ndb.get_note_by_key(services.tx, row.key) else {
                    return;
                };
                if let Ok(zap) = Zap::from_receipt(ev) {
                    let profile = services.profile(&zap.sender);
                    ChatZap::new(&zap, &profile).render(ui, services.ctx);
                }
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
                if is_moderator && ui.button("Timeout").clicked() {
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
        if self.top_len != self.events.len() {
            self.top_len = self.events.len();
            let mut zaps = Vec::new();
            for r in &self.events {
                if let Ok(note) = services.ctx.ndb.get_note_by_key(services.tx, r.key) {
                    if note.kind() == 9735 {
                        if let Ok(z) = Zap::from_receipt(note) {
                            zaps.push(z);
                        }
                    }
                }
            }
            self.top = top_senders(&zaps, 10);
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
        let stream = services
            .ctx
            .ndb
            .get_note_by_key(services.tx, self.stream)
            .unwrap();
        let host = *stream.host();
        let stream_tag = self.link.to_tag_value();

        let width = ui.available_width();
        if (self.measured_width - width).abs() > 1.0 {
            self.heights.clear();
            self.measured_width = width;
        }

        self.top_zappers_row(ui, services);

        let estimate = 32.0;
        ScrollArea::vertical()
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
            });
        self.render_modal(ui, services, host);
        ui.response()
    }

    fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        let stream = services
            .ctx
            .ndb
            .get_note_by_key(services.tx, self.stream)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let host = *stream.host();
        let me = services.me();
        let filters = self.main_filters(&host, me);
        sub_or_poll(services.ctx, services.tx, &mut self.events, &mut self.sub, filters)?;

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
        let stamp = (self.events.len(), self.aux.len(), now / 10);
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
            if data.wants != self.aux_wants {
                self.aux_wants = data.wants.clone();
                self.aux_sub = None;
                if !self.aux_wants.is_empty() {
                    let filters: Vec<Filter> = self.aux_wants.iter().map(|w| w.filter()).collect();
                    let mut sink = HashSet::new();
                    sub_or_poll(services.ctx, services.tx, &mut sink, &mut self.aux_sub, filters)?;
                    self.aux.extend(sink);
                }
            }
            self.data = data;
        }
        Ok(())
    }
}
