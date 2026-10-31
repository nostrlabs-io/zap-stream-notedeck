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
        .unwrap_or("Nostrich")
        .to_string()
}

pub struct Message<'a> {
    pub author: &'a [u8; 32],
    pub segments: &'a [Segment],
    pub is_host: bool,
}

fn text_format(color: Color32) -> TextFormat {
    let mut format = TextFormat::default();
    format.line_height = Some(24.0);
    format.valign = Align::Center;
    format.color = color;
    format
}

fn new_job() -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.break_anywhere = true;
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
    ui.horizontal_wrapped(|ui| {
        Avatar::from_profile(&profile).size(24.).render(ui, services.ctx);
        let name = profile
            .and_then(|p| p.name().or(p.display_name()))
            .unwrap_or("Nostrich");
        let color = if msg.is_host { PRIMARY } else { NEUTRAL_500 };
        ui.add(Label::new(RichText::new(name).color(color)).wrap_mode(TextWrapMode::Wrap));
        for def in chat_badge::badges_for(data, msg.author) {
            chat_badge::inline(ui, services, Some(def));
        }
        let mut job = new_job();
        let mut first = true;
        for seg in msg.segments {
            let lead = if first { 4.0 } else { 0.0 };
            first = false;
            match seg {
                Segment::Text(t) => job.append(t, lead, text_format(Color32::WHITE)),
                Segment::Hashtag(t) => job.append(t, lead, text_format(PRIMARY)),
                Segment::Url(u) => {
                    flush(ui, &mut job);
                    ui.hyperlink_to(RichText::new(u).color(PRIMARY), u);
                }
                Segment::Mention(pk) => {
                    flush(ui, &mut job);
                    let name = format!("@{}", display_name(services, pk));
                    if ui
                        .add(Label::new(RichText::new(name).color(PRIMARY)).sense(Sense::click()))
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
}
