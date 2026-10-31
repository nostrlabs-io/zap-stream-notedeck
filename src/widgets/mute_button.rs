use crate::route::RouteServices;
use crate::widgets::Button;
use egui::Ui;

pub fn show(ui: &mut Ui, services: &mut RouteServices<'_, '_>, target: &[u8; 32]) -> bool {
    let Some(me) = services.me() else {
        return false;
    };
    if &me == target {
        return false;
    }
    let muted = services.is_muted(target);
    let label = if muted {
        crate::i18n::t("button.unmute")
    } else {
        crate::i18n::t("button.mute")
    };
    let mut btn = Button::new();
    if !muted {
        btn = btn.with_color(crate::theme::PRIMARY);
    }
    if btn.text(ui, label).clicked() {
        services.set_muted(target, !muted);
        return true;
    }
    false
}
