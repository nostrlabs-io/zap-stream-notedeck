use crate::link::NostrLink;
use crate::route::RouteServices;
use crate::theme::{MARGIN_DEFAULT, NEUTRAL_800, ROUNDING_DEFAULT};
use crate::widgets::{
    sub_or_poll, Chat, NostrWidget, PlaceholderRect, StreamPlayer, StreamTitle, WriteChat,
};
use egui::{vec2, Align, Frame, Layout, Response, ScrollArea, Stroke, Ui, Vec2, Widget};
use nostrdb::{Filter, Note};

use crate::note_ref::NoteRef;
use crate::player_health::{format_countdown, playback_for, Playback};
use crate::stream_info::{is_moq_url, StreamInfo, StreamStatus};
use crate::widgets::stream_player::{paint_state_card, PlayerInfo};
use crate::sub::SubRef;
use std::borrow::Borrow;
use std::collections::HashSet;

pub struct StreamPage {
    link: NostrLink,
    render_state: Option<egui_wgpu::RenderState>,
    player: Option<StreamPlayer>,
    chat: Option<Chat>,
    new_msg: WriteChat,

    events: HashSet<NoteRef>,
    sub: Option<SubRef>,
}

impl StreamPage {
    pub fn new_from_link(link: NostrLink, render_state: Option<egui_wgpu::RenderState>) -> Self {
        Self {
            render_state,
            new_msg: WriteChat::new(link.clone()),
            link,
            chat: None,
            player: None,
            events: HashSet::new(),
            sub: None,
        }
    }

    fn video(&mut self, ui: &mut Ui, rect: egui::Rect, event: &Note<'_>) {
        let playback = playback_for(
            &event.status(),
            event.streaming().is_some(),
            event.recording().is_some(),
        );
        let moq_only = event.streaming().map_or(false, is_moq_url)
            && event.recording().map_or(true, str::is_empty);
        if moq_only && playback == Playback::Play {
            self.player = None;
            paint_state_card(
                ui,
                rect,
                "Unsupported stream",
                "This stream uses MoQ, which this app cannot play yet",
            );
            return;
        }
        match playback {
            Playback::Play => {
                if self.player.is_none() {
                    if let Some(u) = event.streaming().or(event.recording()) {
                        self.player = Some(StreamPlayer::new(self.render_state.as_ref(), u));
                    }
                }
                let live = event.status() == StreamStatus::Live;
                let info = PlayerInfo {
                    live,
                    started: Some(event.starts()),
                    viewers: event.viewers(),
                };
                match &mut self.player {
                    Some(player) => player.show_controlled(ui, rect, &info),
                    None => {
                        ui.put(rect, PlaceholderRect);
                    }
                }
            }
            Playback::NotStarted => {
                self.player = None;
                let now = now_secs();
                let detail = match event.starts().checked_sub(now) {
                    Some(left) if left > 0 => format!("Starts in {}", format_countdown(left)),
                    _ => "Starting soon".to_string(),
                };
                paint_state_card(ui, rect, "Stream not started", &detail);
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            }
            Playback::Offline => {
                self.player = None;
                let title = if event.status() == StreamStatus::Ended {
                    "Stream ended"
                } else {
                    "Stream offline"
                };
                paint_state_card(ui, rect, title, "Check back later");
            }
        }
    }

    fn get_filters(&self) -> Vec<Filter> {
        let f: Filter = self.link.borrow().try_into().unwrap();
        vec![f.limit_mut(1)]
    }

    fn render_mobile(
        &mut self,
        event: &Note<'_>,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
    ) -> Response {
        let chat_h = 64.0;
        let w = ui.available_width();
        let h = ui.available_height();
        let margin = 16.0;
        ui.allocate_ui_with_layout(
            Vec2::new(w, h),
            Layout::top_down(Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                let video_h = (w / 16.0) * 9.0;
                let (rect, _) = ui.allocate_exact_size(vec2(w, video_h), egui::Sense::hover());
                let _t = crate::frame_stats::Section::start("player");
                self.video(ui, rect, event);

                let info_h = (h - video_h) * 0.38;
                ScrollArea::vertical()
                    .id_salt("stream-info")
                    .max_height(info_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(egui::Margin::symmetric(margin as i8, 8))
                            .show(ui, |ui| {
                                ui.set_width(w - margin * 2.0);
                                StreamTitle::new(event).render(ui, services);
                            });
                    });

                ui.painter().hline(
                    ui.max_rect().x_range(),
                    ui.cursor().top(),
                    Stroke::new(1.0, crate::theme::BORDER),
                );
                let remaining = ui.available_height();
                ui.allocate_ui_with_layout(
                    vec2(w, remaining),
                    Layout::top_down(Align::Min),
                    |ui| {
                        let _t = crate::frame_stats::Section::start("chat");
                        if let Some(c) = self.chat.as_mut() {
                            ui.allocate_ui(
                                vec2(w, (ui.available_height() - chat_h).max(0.0)),
                                |ui| c.render(ui, services),
                            );
                        } else {
                            ui.label("Loading..");
                        }
                        self.new_msg.render(ui, services);
                    },
                );
            },
        );
        ui.response()
    }

    fn chat_panel(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        let chat_h = 64.0;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(0., 0.);
            Frame::new()
                .inner_margin(egui::Margin {
                    left: 16,
                    right: 16,
                    top: 16,
                    bottom: 4,
                })
                .show(ui, |ui| {
                    ui.label(crate::theme::semibold("Stream chat", 15.0));
                });
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().top(),
                Stroke::new(1.0, crate::theme::BORDER),
            );
            if let Some(c) = self.chat.as_mut() {
                ui.allocate_ui(
                    vec2(ui.available_width(), (ui.available_height() - chat_h).max(0.0)),
                    |ui| {
                        c.render(ui, services);
                    },
                );
            } else {
                ui.label("Loading..");
            }
            self.new_msg.render(ui, services);
        });
    }

    fn render_desktop(
        &mut self,
        event: &Note<'_>,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
    ) -> Response {
        let max_h = ui.available_height();
        let chat_w = 398.0;
        let gap = 24.0;
        let video_width = ui.available_width() - chat_w - gap;
        let video_height = max_h.min((video_width / 16.0) * 9.0);

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                vec2(video_width, max_h),
                Layout::top_down_justified(Align::Min),
                |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(video_width, video_height),
                            egui::Sense::hover(),
                        );
                        self.video(ui, rect, event);
                        ui.add_space(14.);
                        StreamTitle::new(event).render(ui, services);
                    });
                },
            );
            ui.allocate_ui_with_layout(
                vec2(chat_w, max_h),
                Layout::top_down_justified(Align::Min),
                |ui| {
                    self.chat_panel(ui, services);
                },
            );
        });

        ui.response()
    }
}

impl NostrWidget for StreamPage {
    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        let events: Vec<Note> = self
            .events
            .iter()
            .map_while(|e| services.ctx.ndb.get_note_by_key(services.tx, e.key).ok())
            .collect();

        if let Some(event) = events.first() {
            if self.chat.is_none() {
                let ok = event.key().unwrap();
                let chat = Chat::new(self.link.clone(), ok);
                self.chat = Some(chat);
            }

            if ui.available_width() < 720.0 {
                self.render_mobile(event, ui, services)
            } else {
                self.render_desktop(event, ui, services)
            }
        } else {
            ui.label("Loading..")
        }
    }

    fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        let filters = self.get_filters();
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.events,
            &mut self.sub,
            filters,
        )?;
        if let Some(c) = self.chat.as_mut() {
            c.update(services)?;
        }
        Ok(())
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
