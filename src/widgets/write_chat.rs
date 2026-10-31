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
            picker_open: false,
        }
    }

    pub fn render(
        &mut self,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
        custom_emoji: &[(String, String)],
        disabled_until: Option<Option<u64>>,
    ) -> Response {
        let logo_bytes = include_bytes!("../resources/send-03.svg");
        Frame::new()
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
                            format!("Chat disabled, timeout ends in {}", format_duration(left))
                        }
                        None => "Chat disabled".to_string(),
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
                        if let Some(ev) = services.write_live_chat_msg(&self.link, &text, tags) {
                            info!("Sending: {:?}", ev);
                            services.broadcast_event(ev);
                        }
                        self.msg.clear();
                    }
                    let toggle = ui.add(egui::Button::new(RichText::new("😀").size(18.0)).frame(false));
                    if toggle.clicked() {
                        self.picker_open = !self.picker_open;
                    }
                    ui.add(NativeTextInput::new(&mut self.msg).with_hint_text("Message.."));
                });
            });
        if self.picker_open && disabled_until.is_none() {
            let mut keep_open = true;
            egui::Window::new("emoji-picker")
                .title_bar(false)
                .resizable(false)
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -80.0))
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
