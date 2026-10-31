use crate::route::{image_from_cache, RouteServices};
use egui::{vec2, Button, RichText, ScrollArea, Ui};

const STANDARD: &[&str] = &[
    "❤️", "👍", "👎", "😂", "🤣", "😍", "😮", "😢", "😡", "🔥", "🎉", "👏", "🙏", "💯", "⚡", "🚀",
    "😀", "😃", "😄", "😁", "😆", "😅", "😊", "🙂", "😉", "😎", "🤩", "🥳", "😘", "🤗", "🤔", "🤨",
    "😐", "😴", "😭", "😱", "🥺", "😤", "🤯", "🙄", "😬", "🤝", "🙌", "👀", "💪", "🤙", "✌️", "🤞",
    "💜", "💙", "💚", "💛", "🧡", "🖤", "🤍", "💔", "✨", "⭐", "🌈", "☀️", "🌙", "🍕", "🍺", "☕",
    "🎮", "🎵", "🎸", "🏆", "💰", "🪙", "🧡", "🫡", "🫂", "🤌", "💀", "👻", "🐸", "🦄", "🐶", "🐱",
];

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
    ScrollArea::vertical()
        .max_height(220.0)
        .max_width(320.0)
        .show(ui, |ui| {
            ui.set_width(300.0);
            if !custom.is_empty() {
                ui.label(crate::theme::semibold("Custom", 13.0));
                ui.horizontal_wrapped(|ui| {
                    for (name, url) in custom {
                        let size = vec2(26.0, 26.0);
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
                ui.label(crate::theme::semibold("Emoji", 13.0));
            }
            ui.horizontal_wrapped(|ui| {
                for e in STANDARD {
                    if ui
                        .add(Button::new(RichText::new(*e).size(22.0)).frame(false))
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
