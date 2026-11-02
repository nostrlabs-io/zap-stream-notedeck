use crate::chat_data::ChatData;
use crate::chat_model::{is_auto_raid, split_coordinate, Raid};
use crate::link::{IdOrStr, NostrLink, NostrLinkType};
use crate::route::{RouteServices, RouteType};
use crate::theme::{ON_PRIMARY, PRIMARY};
use egui::{Align, Frame, Layout, RichText, Ui};

pub const AUTO_RAID_DELAY: f64 = 5.0;

pub fn target_link(coord: &str) -> Option<NostrLink> {
    let (kind, author, d) = split_coordinate(coord)?;
    Some(NostrLink::new(
        NostrLinkType::Coordinate,
        IdOrStr::Str(d),
        Some(kind),
        Some(author),
        vec![],
    ))
}

pub fn show(
    ui: &mut Ui,
    services: &mut RouteServices<'_, '_>,
    data: &ChatData,
    raid: &Raid,
    created_at: u64,
    stream_tag: &str,
    started: &mut Option<f64>,
) {
    let raiding = raid.from == stream_tag;
    let other = if raiding { &raid.to } else { &raid.from };
    let Some(info) = data.embeds.get(other) else {
        return;
    };
    let name = super::chat_message::display_name(services, &info.author).to_uppercase();
    let now_unix = nostr::Timestamp::now().as_u64();
    let auto = raiding && is_auto_raid(created_at, now_unix);
    let now = ui.input(|i| i.time);
    if auto && started.is_none() {
        *started = Some(now);
    }
    let link = target_link(other);
    Frame::new()
        .fill(PRIMARY)
        .corner_radius(crate::theme::ROUNDING_DEFAULT)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                let label = if raiding {
                    crate::i18n::tf("chat.raiding", &[("name", &name)])
                } else {
                    crate::i18n::tf("chat.raid_from", &[("name", &name)])
                };
                ui.label(RichText::new(label).strong().color(ON_PRIMARY));
                if let Some(start) = *started {
                    let left = AUTO_RAID_DELAY - (now - start);
                    if left > 0.0 {
                        ui.label(
                            RichText::new(crate::i18n::tf("chat.raiding_in", &[("n", &(left.ceil() as u32).to_string())]))
                                .color(ON_PRIMARY),
                        );
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                    } else if let Some(l) = &link {
                        *started = None;
                        services.navigate(RouteType::EventPage { link: l.clone(), event: None });
                    }
                }
                if let Some(l) = link {
                    if ui.button(crate::i18n::t("chat.open_stream")).clicked() {
                        *started = None;
                        services.navigate(RouteType::EventPage { link: l, event: None });
                    }
                }
            });
        });
}
