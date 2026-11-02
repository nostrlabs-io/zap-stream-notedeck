use crate::chat_model::{format_duration, message_emoji_tags};
use crate::link::NostrLink;
use crate::route::RouteServices;
use crate::theme::{MUTED, NEUTRAL_900, ROUNDING_DEFAULT};
use crate::widgets::{emoji_picker, NativeTextInput};
use egui::emath::Align;
use egui::{Frame, Image, Layout, Response, RichText, Sense, Ui, Widget};
use log::info;
use std::collections::HashMap;

pub struct WriteChat {
    link: NostrLink,
    msg: String,
    picked: HashMap<String, String>,
    picker_open: bool,
}

impl WriteChat {
    pub fn new(link: NostrLink) -> Self {
        Self {
            link,
            msg: String::new(),
            picked: HashMap::new(),
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
                        let tags = message_emoji_tags(&text, &self.picked);
                        if services.write_live_chat_msg(&self.link, &text, tags) {
                            info!("Sending: {}", text);
                        }
                        self.msg.clear();
                    }
                    let toggle = ui.add(egui::Button::new(RichText::new("😀").size(18.0)).frame(false));
                    if toggle.clicked() {
                        self.picker_open = !self.picker_open;
                    }
                    ui.add(NativeTextInput::new(&mut self.msg).with_hint_text(crate::i18n::t("chat.message_hint")));
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
        ui.response()
    }
}
