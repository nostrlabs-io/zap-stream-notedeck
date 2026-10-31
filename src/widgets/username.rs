use egui::{Color32, Label, Response, RichText, TextWrapMode, Ui, Widget};
use nostrdb::NdbProfile;

pub struct Username<'a> {
    profile: &'a Option<NdbProfile<'a>>,
    size: f32,
}

impl<'a> Username<'a> {
    pub fn new(profile: &'a Option<NdbProfile<'a>>, size: f32) -> Self {
        Self { profile, size }
    }
}

impl Widget for Username<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let name = self
            .profile
            .map_or(crate::i18n::t("common.anon"), |f| f.name().map_or(crate::i18n::t("common.anon"), |f| f));
        let name = RichText::new(name).size(self.size).color(Color32::WHITE);
        ui.add(Label::new(name).wrap_mode(TextWrapMode::Truncate))
    }
}
