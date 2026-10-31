use crate::chat_data::ChatData;
use crate::chat_model::toggle_mute_tags;
use crate::route::RouteServices;
use crate::theme::ON_PRIMARY;
use crate::widgets::Button;
use egui::Ui;
use nostr::prelude::hex;

pub fn show(
    ui: &mut Ui,
    services: &mut RouteServices<'_, '_>,
    data: &ChatData,
    target: &[u8; 32],
) -> bool {
    let Some(me) = services.me() else {
        return false;
    };
    if &me == target {
        return false;
    }
    let muted = data.my_mutes.contains(target);
    let label = if muted { "Unmute" } else { "Mute" };
    let mut btn = Button::new();
    if !muted {
        btn = btn.with_color(crate::theme::PRIMARY);
    }
    let _ = ON_PRIMARY;
    if btn.text(ui, label).clicked() {
        let tags = toggle_mute_tags(data.my_mute_tags.clone(), &hex::encode(target), !muted);
        services.publish_mute_list(tags, &data.my_mute_content);
        return true;
    }
    false
}
