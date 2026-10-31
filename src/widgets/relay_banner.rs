use crate::i18n::t;
use crate::relay_health::Health;
use crate::theme;
use egui::{Frame, Margin, Ui};

pub fn relay_banner(ui: &mut Ui, health: Health) {
    let key = match health {
        Health::Online => return,
        Health::Connecting => "relays.connecting",
        Health::Offline => "relays.offline",
    };
    Frame::new()
        .fill(theme::SURFACE_LOW)
        .inner_margin(Margin::symmetric(16, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(theme::medium(t(key), 13.0).color(theme::MUTED));
        });
}
