use crate::chat_data::{BadgeDef, ChatData};
use crate::route::{image_from_cache, RouteServices};
use egui::{vec2, Ui};

pub fn inline(ui: &mut Ui, services: &mut RouteServices<'_, '_>, def: Option<&BadgeDef>) {
    let Some(url) = def.and_then(|d| d.image.as_deref()) else {
        return;
    };
    let size = vec2(18.0, 18.0);
    match image_from_cache(
        services.ctx.img_cache,
        services.ctx.media_jobs.sender(),
        ui,
        url,
        Some(size),
    ) {
        Some(img) => {
            ui.add(img.fit_to_exact_size(size));
        }
        None => {
            ui.allocate_space(size);
        }
    }
}

pub fn badges_for<'a>(data: &'a ChatData, author: &[u8; 32]) -> Vec<&'a BadgeDef> {
    data.badges_of
        .get(author)
        .map(|v| v.iter().filter_map(|a| data.badge_defs.get(a)).collect())
        .unwrap_or_default()
}

pub fn award(
    ui: &mut Ui,
    services: &mut RouteServices<'_, '_>,
    def: Option<&BadgeDef>,
    users: &[[u8; 32]],
) {
    egui::Frame::new()
        .fill(crate::theme::SURFACE)
        .corner_radius(crate::theme::ROUNDING_DEFAULT)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                if let Some(url) = def.and_then(|d| d.image.as_deref()) {
                    let size = vec2(64.0, 64.0);
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
                }
                if let Some(n) = def.and_then(|d| d.name.as_deref()) {
                    ui.label(crate::theme::semibold(n, 17.0));
                }
                if let Some(d) = def.and_then(|d| d.description.as_deref()) {
                    ui.label(egui::RichText::new(d).color(crate::theme::MUTED));
                }
                let names: Vec<String> = users
                    .iter()
                    .map(|u| super::chat_message::display_name(services, u))
                    .collect();
                ui.label(format!("Awarded to {}", names.join(", ")));
            });
        });
}
