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
        crate::i18n::t("button.unblock")
    } else {
        crate::i18n::t("button.block")
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
