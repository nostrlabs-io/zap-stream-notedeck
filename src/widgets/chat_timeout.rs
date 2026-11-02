use crate::chat_model::{format_duration, Timeout};
use crate::route::RouteServices;
use crate::theme::MUTED;
use egui::{RichText, Ui};

pub fn show(ui: &mut Ui, services: &mut RouteServices<'_, '_>, t: &Timeout) {
    let moderator = super::chat_message::display_name(services, &t.author);
    let users: Vec<String> = t
        .targets
        .iter()
        .map(|u| super::chat_message::display_name(services, u))
        .collect();
    ui.add(
        egui::Label::new(
            RichText::new(crate::i18n::tf("chat.timed_out", &[("moderator", &moderator), ("users", &users.join(", ")), ("duration", &format_duration(t.duration()))]))
            .color(MUTED),
        )
        .wrap(),
    );
}
