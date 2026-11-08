use crate::notify::NotifyState;
use crate::route::RouteServices;
use crate::widgets::Button;
use egui::Ui;

fn settings_dir(services: &RouteServices<'_, '_>) -> std::path::PathBuf {
    services.ctx.path.path(notedeck::DataPathType::Setting)
}

fn user_relays(services: &RouteServices<'_, '_>) -> Vec<String> {
    let Some(me) = services.me() else {
        return Vec::new();
    };
    let filter = nostrdb::Filter::new().kinds([10_002]).authors(&[me]).limit(1).build();
    services
        .ctx
        .ndb
        .query(services.tx, &[filter], 1)
        .ok()
        .and_then(|r| r.into_iter().next())
        .map(|r| crate::relay_list::relays(&crate::contacts::tags_from_note(&r.note)))
        .unwrap_or_default()
}

pub fn show(ui: &mut Ui, services: &mut RouteServices<'_, '_>, host: &[u8; 32]) {
    if services.me() == Some(*host) {
        return;
    }
    let hex = nostr::prelude::hex::encode(host);
    let id = egui::Id::new(("notify-state", hex.as_str()));
    let watched = match ui.data(|d| d.get_temp::<bool>(id)) {
        Some(w) => w,
        None => {
            let w = NotifyState::load(&settings_dir(services)).is_watched(&hex);
            ui.data_mut(|d| d.insert_temp(id, w));
            w
        }
    };
    let (label, btn) = if watched {
        (crate::i18n::t("notify.on"), Button::new())
    } else {
        (crate::i18n::t("notify.off"), Button::new().with_color(crate::theme::PRIMARY))
    };
    if btn.text(ui, &format!("🔔 {label}")).clicked() {
        let dir = settings_dir(services);
        let mut state = NotifyState::load(&dir);
        state.set_watched(&hex, !watched);
        state.set_relays(&user_relays(services));
        state.save(&dir);
        ui.data_mut(|d| d.insert_temp(id, !watched));
        crate::android_window::schedule_live_check(!state.watched.is_empty());
    }
}
