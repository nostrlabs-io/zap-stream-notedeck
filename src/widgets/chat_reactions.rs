use crate::chat_model::ReactionCount;
use crate::route::{image_from_cache, RouteServices};
use crate::theme::{FONT_SIZE_SM, SURFACE, ZAP};
use crate::zap::format_sats;
use egui::{vec2, Frame, Margin, RichText, Ui};

pub fn show(
    ui: &mut Ui,
    services: &mut RouteServices<'_, '_>,
    counts: &[ReactionCount],
    sats: Option<u64>,
) {
    if counts.is_empty() && sats.is_none() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        if let Some(sats) = sats.filter(|s| *s > 0) {
            chip(ui, |ui| {
                ui.label(RichText::new("⚡").size(FONT_SIZE_SM).color(ZAP));
                ui.label(RichText::new(format_sats(sats as f32)).size(FONT_SIZE_SM));
            });
        }
        for c in counts {
            chip(ui, |ui| {
                let size = vec2(16.0, 16.0);
                let img = c.emoji_url.as_deref().and_then(|u| {
                    image_from_cache(
                        services.ctx.img_cache,
                        services.ctx.media_jobs.sender(),
                        ui,
                        u,
                        Some(size),
                    )
                });
                match img {
                    Some(i) => {
                        ui.add(i.fit_to_exact_size(size));
                    }
                    None if c.emoji_url.is_some() => {
                        ui.allocate_space(size);
                    }
                    None => {
                        let text = if c.content == "+" { "👍" } else if c.content == "-" { "👎" } else { c.content.as_str() };
                        ui.label(RichText::new(text).size(FONT_SIZE_SM));
                    }
                }
                if c.count > 1 {
                    ui.label(RichText::new(c.count.to_string()).size(FONT_SIZE_SM));
                }
            });
        }
    });
}

fn chip(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(SURFACE)
        .corner_radius(8.0)
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                add(ui);
            });
        });
}
