use crate::chat_data::ChatData;
use crate::chat_text::Segment;
use crate::link::NostrLink;
use crate::route::{image_from_cache, RouteServices, RouteType};
use crate::theme::{NEUTRAL_500, PRIMARY};
use crate::widgets::{chat_badge, note_embed, Avatar};
use egui::epaint::text::TextWrapMode;
use egui::text::LayoutJob;
use egui::{vec2, Align, Color32, Label, RichText, Sense, TextFormat, Ui};

pub fn display_name(services: &RouteServices<'_, '_>, pk: &[u8; 32]) -> String {
    services
        .profile(pk)
        .and_then(|p| p.name().or(p.display_name()))
        .unwrap_or(crate::i18n::t("common.anon"))
        .to_string()
}

pub struct Message<'a> {
    pub author: &'a [u8; 32],
    pub segments: &'a [Segment],
    pub is_host: bool,
}

const LINE_HEIGHT: f32 = 24.0;

fn text_format(ui: &Ui, color: Color32) -> TextFormat {
    let mut format = TextFormat::default();
    format.font_id = egui::TextStyle::Body.resolve(ui.style());
    format.line_height = Some(LINE_HEIGHT);
    format.valign = Align::Center;
    format.color = color;
    format
}

fn new_job() -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.break_anywhere = false;
    job
}

fn flush(ui: &mut Ui, job: &mut LayoutJob) {
    if job.text.is_empty() {
        return;
    }
    let done = std::mem::replace(job, new_job());
    ui.add(Label::new(done).wrap_mode(TextWrapMode::Wrap));
}

pub fn render(ui: &mut Ui, services: &mut RouteServices<'_, '_>, data: &ChatData, msg: &Message) {
    let profile = services.profile(msg.author);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.add_space(12.0);
        let avatar = Avatar::from_profile(&profile)
            .size(24.)
            .render(ui, services.ctx);
        if avatar.clicked() {
            services.navigate(RouteType::ProfilePage {
                link: NostrLink::profile(msg.author),
            });
        }
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
        let name = profile
                    .and_then(|p| p.name().or(p.display_name()))
                    .unwrap_or(crate::i18n::t("common.anon"))
                    .to_string();
                let color = if msg.is_host { PRIMARY } else { NEUTRAL_500 };
                let mut job = new_job();
                job.append(&name, 0.0, text_format(ui, color));
                let badges = chat_badge::badges_for(data, msg.author);
                if !badges.is_empty() {
                    flush(ui, &mut job);
                    for def in badges {
                        chat_badge::inline(ui, services, Some(def));
                    }
                }
                let mut lead = 6.0;
                for seg in msg.segments {
                    match seg {
                        Segment::Text(t) => {
                            job.append(&crate::bidi::visual(t), lead, text_format(ui, Color32::WHITE));
                            lead = 0.0;
                        }
                        Segment::Hashtag(t) => {
                            job.append(&crate::bidi::visual(t), lead, text_format(ui, PRIMARY));
                            lead = 0.0;
                        }
                        Segment::Url(u) => {
                            flush(ui, &mut job);
                            lead = 0.0;
                            let link = ui
                                .add(
                                    Label::new(
                                        RichText::new(u)
                                            .color(PRIMARY)
                                            .line_height(Some(LINE_HEIGHT)),
                                    )
                                    .sense(Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if link.clicked() {
                                crate::android_window::open_url(ui.ctx(), u);
                            }
                        }
                        Segment::Mention(pk) => {
                            flush(ui, &mut job);
                            let name = format!("@{}", display_name(services, pk));
                            if ui
                                .add(
                                    Label::new(
                                        RichText::new(name)
                                            .color(PRIMARY)
                                            .line_height(Some(LINE_HEIGHT)),
                                    )
                                    .sense(Sense::click()),
                                )
                                .clicked()
                            {
                                services.navigate(RouteType::ProfilePage {
                                    link: NostrLink::profile(pk),
                                });
                            }
                        }
                        Segment::Emoji { name, url } => {
                            flush(ui, &mut job);
                            let size = vec2(22.0, 22.0);
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
                                    ui.label(format!(":{}:", name));
                                }
                            }
                        }
                        Segment::Embed(bech) => {
                            flush(ui, &mut job);
                            note_embed::show(ui, services, data, bech);
                        }
                    }
                }
                flush(ui, &mut job);
                ui.add_space(ui.available_size_before_wrap().x);
            });
        });
    });
}
