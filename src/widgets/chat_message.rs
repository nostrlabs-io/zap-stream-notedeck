use crate::link::NostrLink;
use crate::route::{RouteServices, RouteType};
use crate::stream_info::StreamInfo;
use crate::theme::{NEUTRAL_500, PRIMARY};
use crate::widgets::Avatar;
use egui::epaint::text::TextWrapMode;
use egui::text::LayoutJob;
use egui::{Align, Color32, Label, Response, TextFormat, Ui};
use nostrdb::{NdbProfile, Note};

pub struct ChatMessage<'a> {
    stream: &'a Note<'a>,
    ev: &'a Note<'a>,
    profile: &'a Option<NdbProfile<'a>>,
}

impl<'a> ChatMessage<'a> {
    pub fn new(
        stream: &'a Note<'a>,
        ev: &'a Note<'a>,
        profile: &'a Option<NdbProfile<'a>>,
    ) -> ChatMessage<'a> {
        ChatMessage {
            stream,
            ev,
            profile,
        }
    }

    pub fn render(self, ui: &mut Ui, services: &mut RouteServices) -> Response {
        ui.horizontal_wrapped(|ui| {
            let mut job = LayoutJob::default();
            // TODO: avoid this somehow
            job.wrap.break_anywhere = true;

            let is_host = self.stream.host().eq(self.ev.pubkey());
            let name = self
                .profile
                .map_or(crate::i18n::t("common.anon"), |f| f.name().map_or(crate::i18n::t("common.anon"), |f| f));

            let name_color = if is_host { PRIMARY } else { NEUTRAL_500 };

            let mut format = TextFormat::default();
            format.line_height = Some(24.0);
            format.valign = Align::Center;

            format.color = name_color;
            job.append(name, 0.0, format.clone());
            format.color = Color32::WHITE;
            job.append(self.ev.content(), 5.0, format.clone());

            let avatar = Avatar::from_profile(self.profile)
                .size(24.)
                .render(ui, services.ctx);
            if avatar.clicked() {
                services.navigate(RouteType::ProfilePage {
                    link: NostrLink::profile(self.ev.pubkey()),
                })
            }
            if services.can_sign() && self.ev.pubkey() != services.ctx.accounts.get_selected_account().key.pubkey.bytes() {
                let author = *self.ev.pubkey();
                let muted = services.is_muted(&author);
                avatar.context_menu(|ui| {
                    let label = if muted { crate::i18n::t("button.unmute") } else { crate::i18n::t("button.mute") };
                    if ui.button(label).clicked() {
                        services.set_muted(&author, !muted);
                        ui.close();
                    }
                });
            }
            ui.add(Label::new(job).wrap_mode(TextWrapMode::Wrap));

            // consume reset of space
            ui.add_space(ui.available_size_before_wrap().x);
        })
        .response
    }
}
