use crate::chat_data::{ChatData, EmbedInfo};
use crate::chat_text::embed_target;
use crate::route::{RouteServices, RouteType};
use crate::theme::{FONT_SIZE_SM, PRIMARY, SURFACE};
use egui::{Frame, Id, Margin, RichText, Sense, Ui};

fn heading(kind: u32, name: &str) -> String {
    match kind {
        30_023 => crate::i18n::tf("embed.article", &[("name", &name)]),
        30_311 => crate::i18n::tf("embed.live", &[("name", &name)]),
        _ => crate::i18n::tf("embed.note", &[("name", &name)]),
    }
}

pub fn show(ui: &mut Ui, services: &mut RouteServices<'_, '_>, data: &ChatData, bech: &str) {
    let Some(target) = embed_target(bech) else {
        ui.label(RichText::new(format!("nostr:{}", bech)).color(PRIMARY));
        return;
    };
    let Some(info) = data.embeds.get(&target.key()) else {
        return;
    };
    let name = super::chat_message::display_name(services, &info.author);
    let open_key = format!("embed:{}", target.key());
    let r = Frame::new()
        .fill(SURFACE)
        .corner_radius(8.0)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("🔗 {}", heading(info.kind, &name))).size(FONT_SIZE_SM));
        })
        .response
        .interact(Sense::click());
    if r.clicked() {
        if info.kind == 30_311 {
            if let Some(link) = super::chat_raid::target_link(&target.key()) {
                services.navigate(RouteType::EventPage { link, event: None });
            }
        } else {
            services.set(&open_key, true);
        }
    }
    if services.get::<bool>(&open_key).unwrap_or(false) {
        preview(ui, services, info, &name, &open_key);
    }
}

fn preview(
    ui: &Ui,
    services: &mut RouteServices<'_, '_>,
    info: &EmbedInfo,
    name: &str,
    open_key: &str,
) {
    let modal = egui::Modal::new(Id::new(open_key)).show(ui.ctx(), |ui| {
        ui.set_width(320.0);
        ui.label(crate::theme::semibold(name, 15.0));
        ui.add_space(6.0);
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            ui.label(&info.content);
        });
    });
    if modal.should_close() {
        services.set(open_key, false);
    }
}
