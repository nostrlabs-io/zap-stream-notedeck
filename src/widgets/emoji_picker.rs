use crate::route::{image_from_cache, RouteServices};
use egui::{vec2, Button, RichText, ScrollArea, Ui};

const STANDARD: &[&str] = &[
    "❤️", "👍", "👎", "😂", "🤣", "😍", "😮", "😢", "😡", "🔥", "🎉", "👏", "🙏", "💯", "⚡", "🚀",
    "😀", "😃", "😄", "😁", "😆", "😅", "😊", "🙂", "😉", "😎", "🤩", "🥳", "😘", "🤗", "🤔", "🤨",
    "😐", "😴", "😭", "😱", "🥺", "😤", "🤯", "🙄", "😬", "🤝", "🙌", "👀", "💪", "🤙", "✌️", "🤞",
    "💜", "💙", "💚", "💛", "🧡", "🖤", "🤍", "💔", "✨", "⭐", "🌈", "☀️", "🌙", "🍕", "🍺", "☕",
    "🎮", "🎵", "🎸", "🏆", "💰", "🪙", "🧡", "🫡", "🫂", "🤌", "💀", "👻", "🐸", "🦄", "🐶", "🐱",
];

const CELL: f32 = 36.0;
const VISIBLE_ROWS: f32 = 6.0;
const MAX_WIDTH: f32 = 324.0;

pub fn picker_width(screen_width: f32) -> f32 {
    let usable = (screen_width - 24.0).clamp(CELL, MAX_WIDTH);
    (usable / CELL).floor() * CELL
}

pub struct Picked {
    pub content: String,
    pub name: Option<String>,
    pub url: Option<String>,
}

pub fn show(
    ui: &mut Ui,
    services: &mut RouteServices<'_, '_>,
    custom: &[(String, String)],
) -> Option<Picked> {
    let mut picked = None;
    let width = picker_width(ui.ctx().content_rect().width());
    ScrollArea::vertical()
        .max_height(CELL * VISIBLE_ROWS)
        .max_width(width + 12.0)
        .show(ui, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            if !custom.is_empty() {
                ui.label(crate::theme::semibold(crate::i18n::t("emoji.custom"), 13.0));
                ui.horizontal_wrapped(|ui| {
                    for (name, url) in custom {
                        let size = vec2(CELL - 8.0, CELL - 8.0);
                        let Some(img) = image_from_cache(
                            services.ctx.img_cache,
                            services.ctx.media_jobs.sender(),
                            ui,
                            url,
                            Some(size),
                        ) else {
                            ui.allocate_space(size);
                            continue;
                        };
                        let r = ui.add(Button::image(img.fit_to_exact_size(size)).frame(false));
                        if r.on_hover_text(format!(":{}:", name)).clicked() {
                            picked = Some(Picked {
                                content: format!(":{}:", name),
                                name: Some(name.clone()),
                                url: Some(url.clone()),
                            });
                        }
                    }
                });
                ui.add_space(6.0);
                ui.label(crate::theme::semibold(crate::i18n::t("emoji.emoji"), 13.0));
            }
            ui.horizontal_wrapped(|ui| {
                for e in STANDARD {
                    if ui
                        .add(
                            Button::new(RichText::new(*e).size(22.0))
                                .frame(false)
                                .min_size(vec2(CELL, CELL)),
                        )
                        .clicked()
                    {
                        picked = Some(Picked {
                            content: e.to_string(),
                            name: None,
                            url: None,
                        });
                    }
                }
            });
        });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_is_whole_columns() {
        assert_eq!(picker_width(400.0) % CELL, 0.0);
        assert_eq!(picker_width(1400.0), 324.0);
    }

    #[test]
    fn width_fits_narrow_screens() {
        assert!(picker_width(400.0) <= 400.0 - 24.0);
        assert!(picker_width(100.0) >= CELL);
    }
}
