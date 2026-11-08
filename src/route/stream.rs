use crate::link::NostrLink;
use crate::route::RouteServices;
use crate::theme::{MARGIN_DEFAULT, NEUTRAL_800, ROUNDING_DEFAULT};
use crate::widgets::{
    sub_or_poll, Chat, Goal, NostrWidget, PlaceholderRect, StreamCards, StreamPlayer, StreamTitle, WriteChat,
};
use egui::{vec2, Align, Frame, Layout, Response, ScrollArea, Stroke, Ui, Vec2, Widget};
use nostrdb::{Filter, Note};

use crate::note_ref::NoteRef;
use crate::player_health::{format_countdown, playback_for, Playback};
use crate::stream_info::{is_moq_url, StreamInfo, StreamStatus};
use crate::widgets::stream_player::{paint_state_card, paint_state_overlay, PlayerInfo};
use crate::sub::SubRef;
use std::borrow::Borrow;
use std::collections::HashSet;

pub struct StreamPage {
    link: NostrLink,
    render_state: Option<egui_wgpu::RenderState>,
    player: Option<StreamPlayer>,
    chat: Option<Chat>,
    goal: Option<Goal>,
    cards: Option<StreamCards>,
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
            goal: None,
            cards: None,
            player: None,
            events: HashSet::new(),
            sub: None,
        }
    }

    fn video(
        &mut self,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
        rect: egui::Rect,
        event: &Note<'_>,
    ) {
        let need_poster = self.player.is_none() || event.status() != StreamStatus::Live;
        let poster = if need_poster {
            crate::poster::paint::poster_for(ui, services, event)
        } else {
            None
        };
        let backdrop = |ui: &Ui| {
            crate::poster::paint::paint_poster(ui.painter(), rect, 16, poster.as_ref(), "");
        };
        let forced = std::env::var("ZAP_STREAM_DEBUG_STATE").ok();
        let playback = match forced.as_deref() {
            Some("planned") => Playback::NotStarted,
            Some("offline") | Some("ended") => Playback::Offline,
            _ => playback_for(
                &event.status(),
                event.streaming().is_some(),
                event.recording().is_some(),
            ),
        };
        let moq_only = event.streaming().map_or(false, is_moq_url)
            && event.recording().map_or(true, str::is_empty);
        if moq_only && playback == Playback::Play {
            self.player = None;
            paint_state_card(
                ui,
                rect,
                crate::i18n::t("stream.unsupported"),
                crate::i18n::t("stream.unsupported_detail"),
            );
            return;
        }
        match playback {
            Playback::Play => {
                if self.player.is_none() {
                    if let Some(u) = event.streaming().or(event.recording()) {
                        self.player = Some(StreamPlayer::new(self.render_state.as_ref(), u));
                        let host = crate::widgets::chat_message::display_name(services, event.host());
                        crate::android_window::set_now_playing(
                            Some(event.title().unwrap_or(crate::i18n::t("common.untitled"))),
                            &host,
                        );
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
                    None => backdrop(ui),
                }
            }
            Playback::NotStarted => {
                self.player = None;
                let now = now_secs();
                let detail = match event.starts().checked_sub(now) {
                    Some(left) if left > 0 => crate::i18n::tf("stream.starts_in", &[("time", &format_countdown(left))]),
                    _ => crate::i18n::t("stream.starting_soon").to_string(),
                };
                backdrop(ui);
                paint_state_overlay(ui, rect, crate::i18n::t("stream.not_started"), &detail);
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            }
            Playback::Offline => {
                self.player = None;
                let title = if event.status() == StreamStatus::Ended
                    || forced.as_deref() == Some("ended")
                {
                    crate::i18n::t("stream.ended")
                } else {
                    crate::i18n::t("stream.offline")
                };
                backdrop(ui);
                paint_state_overlay(ui, rect, title, crate::i18n::t("stream.check_back"));
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
                let video_h = ((w / 16.0) * 9.0).min(h * 0.42);
                let (rect, _) = ui.allocate_exact_size(vec2(w, video_h), egui::Sense::hover());
                let _t = crate::frame_stats::Section::start("player");
                self.video(ui, services, rect, event);

                let info_h = ((h - video_h) * 0.5).max(150.0);
                ScrollArea::vertical()
                    .id_salt("stream-info")
                    .max_height(info_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(egui::Margin::symmetric(margin as i8, 8))
                            .show(ui, |ui| {
                                ui.set_width(w - margin * 2.0);
                                StreamTitle::new(event).render_compact(ui, services);
                                if let Some(c) = &self.cards {
                                    c.render(ui, services);
                                }
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
                        self.goal_bar(ui, services);
                        if let Some(c) = self.chat.as_mut() {
                            let chat_rect = egui::Rect::from_min_size(
                                ui.cursor().min + vec2(8.0, 0.0),
                                vec2(w - 16.0, (ui.available_height() - chat_h).max(0.0)),
                            );
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(chat_rect),
                                |ui| c.render(ui, services),
                            );
                            ui.advance_cursor_after_rect(chat_rect);
                        } else {
                            ui.label(crate::i18n::t("common.loading"));
                        }
                        {
                            let (emoji, disabled) = self
                                .chat
                                .as_ref()
                                .map(|c| (c.custom_emoji().to_vec(), c.disabled_until()))
                                .unwrap_or_default();
                            self.new_msg.render(ui, services, &emoji, disabled);
                        }
                    },
                );
            },
        );
        ui.response()
    }

    fn goal_bar(&self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        if let Some((title, total, target)) = crate::widgets::goal::debug_goal() {
            Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 8))
                .show(ui, |ui| crate::widgets::goal::paint_goal(ui, &title, total, target));
        } else if let Some(g) = &self.goal {
            Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 8))
                .show(ui, |ui| g.render(ui, services));
        }
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
            self.goal_bar(ui, services);
            if let Some(c) = self.chat.as_mut() {
                ui.allocate_ui(
                    vec2(ui.available_width(), (ui.available_height() - chat_h).max(0.0)),
                    |ui| {
                        c.render(ui, services);
                    },
                );
            } else {
                ui.label(crate::i18n::t("common.loading"));
            }
            {
                            let (emoji, disabled) = self
                                .chat
                                .as_ref()
                                .map(|c| (c.custom_emoji().to_vec(), c.disabled_until()))
                                .unwrap_or_default();
                            self.new_msg.render(ui, services, &emoji, disabled);
                        }
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
                        self.video(ui, services, rect, event);
                        ui.add_space(14.);
                        StreamTitle::new(event).render(ui, services);
                        if let Some(c) = &self.cards {
                            c.render(ui, services);
                        }
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
    fn exit_fullscreen(&mut self) -> bool {
        match self.player.as_mut() {
            Some(p) if p.is_fullscreen() => {
                p.exit_fullscreen();
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        let events: Vec<Note> = self
            .events
            .iter()
            .map_while(|e| services.ctx.ndb.get_note_by_key(services.tx, e.key).ok())
            .collect();

        if let Some(event) = events.first() {
            let goal_id = event.goal();
            if self.goal.as_ref().map(|g| *g.id()) != goal_id {
                self.goal = goal_id.map(Goal::new);
            }

            let host = *event.host();
            if self.cards.as_ref().map(|c| *c.host()) != Some(host) {
                self.cards = Some(StreamCards::new(host));
            }

            if let (Some(c), Some(k)) = (self.chat.as_mut(), event.key()) {
                c.set_stream(k);
            }

            if ui.available_width() < 720.0 {
                self.render_mobile(event, ui, services)
            } else {
                self.render_desktop(event, ui, services)
            }
        } else {
            ui.label(crate::i18n::t("common.loading"))
        }
    }

    fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        if self.chat.is_none() {
            self.chat = Some(Chat::new(self.link.clone(), None));
        }
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
        if let Some(g) = self.goal.as_mut() {
            g.update(services)?;
        }
        if let Some(c) = self.cards.as_mut() {
            c.update(services)?;
        }
        Ok(())
    }
}

impl Drop for StreamPage {
    fn drop(&mut self) {
        if self.player.is_some() {
            crate::android_window::set_now_playing(None, "");
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
