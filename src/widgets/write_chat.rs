use crate::chat_model::{active_token, filter_emoji, format_duration, message_tags, replace_token, Token, Trigger};
use crate::link::NostrLink;
use crate::route::{image_from_cache, RouteServices};
use crate::theme::{MUTED, NEUTRAL_900, ROUNDING_DEFAULT};
use crate::widgets::{emoji_picker, Avatar, NativeTextInput};
use egui::emath::Align;
use egui::{Frame, Image, Layout, Response, RichText, Sense, Ui, Widget};
use log::info;
use nostr::nips::nip19::ToBech32;
use std::collections::HashMap;

pub struct WriteChat {
    link: NostrLink,
    msg: String,
    picked: HashMap<String, String>,
    mentions: HashMap<String, [u8; 32]>,
    token: Option<Token>,
    popup_hovered: bool,
    focus_pending: bool,
    debug_write: bool,
    input_id: Option<egui::Id>,
    picker_open: bool,
}

impl WriteChat {
    pub fn new(link: NostrLink) -> Self {
        Self {
            link,
            msg: if cfg!(debug_assertions) {
                std::env::var("ZAP_STREAM_DEBUG_WRITE").unwrap_or_default()
            } else {
                String::new()
            },
            focus_pending: cfg!(debug_assertions) && std::env::var_os("ZAP_STREAM_DEBUG_WRITE").is_some(),
            debug_write: cfg!(debug_assertions) && std::env::var_os("ZAP_STREAM_DEBUG_WRITE").is_some(),
            picked: HashMap::new(),
            mentions: HashMap::new(),
            token: None,
            popup_hovered: false,
            input_id: None,
            picker_open: std::env::var_os("ZAP_STREAM_DEBUG_EMOJI").is_some(),
        }
    }

    pub fn render(
        &mut self,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
        custom_emoji: &[(String, String)],
        disabled_until: Option<Option<u64>>,
    ) -> Response {
        let debug_emoji = [("fire".to_string(), String::new()), ("goat".to_string(), String::new())];
        let custom_emoji: &[(String, String)] =
            if cfg!(debug_assertions) && custom_emoji.is_empty() && self.debug_write {
                &debug_emoji
            } else {
                custom_emoji
            };
        if !services.can_sign() {
            let resp = Frame::new()
                .inner_margin(egui::Margin::symmetric(12, 12))
                .outer_margin(egui::Margin::same(12))
                .fill(NEUTRAL_900)
                .stroke(egui::Stroke::new(1.0, crate::theme::BORDER))
                .corner_radius(ROUNDING_DEFAULT)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        crate::theme::medium(crate::i18n::t("chat.login_required"), 14.0)
                            .color(crate::theme::MUTED),
                    );
                })
                .response
                .interact(Sense::click())
                .on_hover_and_drag_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                services.navigate(crate::route::RouteType::LoginPage);
            }
            return resp;
        }
        let logo_bytes = include_bytes!("../resources/send-03.svg");
        let input_box = Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 8))
            .outer_margin(egui::Margin::same(12))
            .fill(NEUTRAL_900)
            .stroke(egui::Stroke::new(1.0, crate::theme::BORDER))
            .corner_radius(ROUNDING_DEFAULT)
            .show(ui, |ui| {
                if let Some(until) = disabled_until {
                    let text = match until {
                        Some(t) => {
                            let left = t.saturating_sub(nostr::Timestamp::now().as_u64());
                            crate::i18n::tf("chat.disabled_timeout", &[("left", &format_duration(left))])
                        }
                        None => crate::i18n::t("chat.disabled").to_string(),
                    };
                    ui.label(RichText::new(text).color(MUTED));
                    return;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Image::from_bytes("send-03.svg", logo_bytes)
                        .sense(Sense::click())
                        .ui(ui)
                        .clicked()
                        || self.msg.ends_with('\n')
                    {
                        let text = self.msg.trim().to_string();
                        let tags = message_tags(&text, &self.picked, &self.mentions);
                        if services.write_live_chat_msg(&self.link, &text, tags) {
                            info!("Sending: {}", text);
                        }
                        self.msg.clear();
                        self.token = None;
                        self.picked.clear();
                        self.mentions.clear();
                    }
                    let toggle = ui.add(egui::Button::new(RichText::new("😀").size(18.0)).frame(false));
                    if toggle.clicked() {
                        self.picker_open = !self.picker_open;
                    }
                    let input = ui.add(NativeTextInput::new(&mut self.msg).with_hint_text(crate::i18n::t("chat.message_hint")));
                    self.input_id = Some(input.id);
                    if self.focus_pending {
                        self.focus_pending = false;
                        input.request_focus();
                        let end = egui::text::CCursor::new(self.msg.chars().count());
                        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), input.id) {
                            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                            egui::TextEdit::store_state(ui.ctx(), input.id, state);
                        }
                        ui.ctx().request_repaint();
                    }
                    let tracking = input.has_focus() || self.debug_write;
                    if tracking || self.popup_hovered {
                        if tracking {
                            let cursor = egui::TextEdit::load_state(ui.ctx(), input.id)
                                .and_then(|s| s.cursor.char_range())
                                .map(|r| r.primary.index.0)
                                .unwrap_or_else(|| self.msg.chars().count());
                            self.token = active_token(&self.msg, cursor);
                        }
                    } else {
                        self.token = None;
                    }
                });
            })
            .response
            .rect;
        if self.picker_open && disabled_until.is_none() {
            let mut keep_open = true;
            egui::Window::new("emoji-picker")
                .title_bar(false)
                .resizable(false)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(input_box.right_top() - egui::vec2(0.0, 6.0))
                .show(ui.ctx(), |ui| {
                    if let Some(p) = emoji_picker::show(ui, services, custom_emoji) {
                        self.msg.push_str(&p.content);
                        if let (Some(n), Some(u)) = (p.name, p.url) {
                            self.picked.insert(n, u);
                        }
                        keep_open = false;
                    }
                });
            self.picker_open = keep_open;
        }
        if disabled_until.is_none() {
            self.show_suggestions(ui, services, custom_emoji, input_box);
        }
        ui.response()
    }

    fn show_suggestions(
        &mut self,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
        custom_emoji: &[(String, String)],
        input_box: egui::Rect,
    ) {
        self.popup_hovered = false;
        let Some(token) = self.token.clone() else {
            return;
        };
        let mut chosen: Option<(String, Option<(String, String)>, Option<(String, [u8; 32])>)> = None;
        let mut any = false;
        let resp = egui::Window::new("write-suggest")
            .title_bar(false)
            .resizable(false)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(input_box.left_top() - egui::vec2(0.0, 6.0))
            .show(ui.ctx(), |ui| {
                ui.set_max_width(input_box.width().min(320.0));
                match token.trigger {
                    Trigger::Mention => {
                        let found: Vec<[u8; 32]> = services
                            .ctx
                            .ndb
                            .search_profile(services.tx, &token.query, 6)
                            .map(|v| v.into_iter().copied().collect())
                            .unwrap_or_default();
                        for pk in found {
                            any = true;
                            let profile = services.profile(&pk);
                            let name = profile
                                .and_then(|p| p.display_name().or(p.name()))
                                .unwrap_or("Nostrich")
                                .to_string();
                            let r = ui
                                .horizontal(|ui| {
                                    Avatar::from_profile(&profile).size(24.0).render(ui, services.ctx);
                                    ui.label(name);
                                })
                                .response
                                .interact(Sense::click())
                                .on_hover_and_drag_cursor(egui::CursorIcon::PointingHand);
                            if r.clicked() {
                                if let Ok(npub) = nostr::PublicKey::from_slice(&pk).map(|p| p.to_bech32()) {
                                    if let Ok(npub) = npub {
                                        chosen = Some((format!("nostr:{} ", npub), None, Some((npub, pk))));
                                    }
                                }
                            }
                        }
                    }
                    Trigger::Emoji => {
                        for (name, url) in filter_emoji(custom_emoji, &token.query, 8) {
                            any = true;
                            let size = egui::vec2(20.0, 20.0);
                            let r = ui
                                .horizontal(|ui| {
                                    if let Some(img) = image_from_cache(
                                        services.ctx.img_cache,
                                        services.ctx.media_jobs.sender(),
                                        ui,
                                        url,
                                        Some(size),
                                    ) {
                                        ui.add(img.fit_to_exact_size(size));
                                    } else {
                                        ui.allocate_space(size);
                                    }
                                    ui.label(format!(":{}:", name));
                                })
                                .response
                                .interact(Sense::click())
                                .on_hover_and_drag_cursor(egui::CursorIcon::PointingHand);
                            if r.clicked() {
                                chosen = Some((format!(":{}: ", name), Some((name.clone(), url.clone())), None));
                            }
                        }
                    }
                }
            });
        if !any {
            return;
        }
        if let Some(r) = resp {
            self.popup_hovered = r.response.contains_pointer();
        }
        if let Some((text, emoji, mention)) = chosen {
            let (msg, cursor) = replace_token(&self.msg, &token, &text);
            self.msg = msg;
            if let Some((n, u)) = emoji {
                self.picked.insert(n, u);
            }
            if let Some((npub, pk)) = mention {
                self.mentions.insert(npub, pk);
            }
            self.token = None;
            self.popup_hovered = false;
            if let Some(id) = self.input_id {
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(cursor))));
                    egui::TextEdit::store_state(ui.ctx(), id, state);
                }
                ui.ctx().memory_mut(|m| m.request_focus(id));
            }
        }
    }
}
