use crate::theme;
use crate::widgets::Button;
use egui::{Context, Frame, Id, Margin};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const TERMS_URL: &str = "https://zap.stream/tos";
pub const CONTACT_EMAIL: &str = "info@zap.stream";

const TERMS_FILE: &str = "terms_accepted";
const SENSITIVE_FILE: &str = "show_sensitive";

static LOADED: AtomicBool = AtomicBool::new(false);
static TERMS_ACCEPTED: AtomicBool = AtomicBool::new(false);
static SHOW_SENSITIVE: AtomicBool = AtomicBool::new(false);

pub fn init(dir: &Path) {
    if LOADED.swap(true, Ordering::Relaxed) {
        return;
    }
    TERMS_ACCEPTED.store(dir.join(TERMS_FILE).exists(), Ordering::Relaxed);
    SHOW_SENSITIVE.store(dir.join(SENSITIVE_FILE).exists(), Ordering::Relaxed);
}

pub fn terms_accepted() -> bool {
    TERMS_ACCEPTED.load(Ordering::Relaxed)
}

pub fn accept_terms(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(TERMS_FILE), "1");
    TERMS_ACCEPTED.store(true, Ordering::Relaxed);
}

pub fn show_sensitive() -> bool {
    SHOW_SENSITIVE.load(Ordering::Relaxed)
}

pub fn set_show_sensitive(dir: &Path, on: bool) {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join(SENSITIVE_FILE);
    if on {
        let _ = std::fs::write(path, "1");
    } else {
        let _ = std::fs::remove_file(path);
    }
    SHOW_SENSITIVE.store(on, Ordering::Relaxed);
}

pub fn hidden_by_filter(sensitive: bool) -> bool {
    sensitive && !show_sensitive()
}

pub fn show_terms(ctx: &Context, dir: &Path) {
    if terms_accepted() {
        return;
    }
    let width = 380.0_f32.min((ctx.content_rect().width() - 48.0).max(220.0));
    let modal = Modal::frame();
    let mut accepted = false;
    modal.show(ctx, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.label(theme::display_bold_text(crate::i18n::t("terms.title"), 22.0));
        crate::bidi::wrapped_label(
            ui,
            crate::i18n::tl("terms.body"),
            egui::FontId::proportional(15.0),
            theme::TEXT_SOFT,
        );
        crate::bidi::wrapped_label(
            ui,
            crate::i18n::tl("terms.contact"),
            egui::FontId::proportional(14.0),
            theme::MUTED,
        );
        if Button::new().full_width().text(ui, crate::i18n::t("terms.read")).clicked() {
            crate::android_window::open_url(ui.ctx(), TERMS_URL);
        }
        if Button::primary().full_width().text(ui, crate::i18n::t("terms.accept")).clicked() {
            accepted = true;
        }
    });
    if accepted {
        accept_terms(dir);
    }
}

struct Modal;

impl Modal {
    fn frame() -> egui::Modal {
        egui::Modal::new(Id::new("terms-modal")).frame(
            Frame::new()
                .fill(theme::SURFACE_LOW)
                .stroke(egui::Stroke::new(1.0, theme::SURFACE))
                .corner_radius(16)
                .inner_margin(Margin::same(24)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_acceptance_is_stored_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join(TERMS_FILE).exists());
        accept_terms(dir.path());
        assert!(dir.path().join(TERMS_FILE).exists());
        assert!(terms_accepted());
    }

    #[test]
    fn sensitive_streams_are_hidden_until_enabled() {
        let dir = tempfile::tempdir().unwrap();
        set_show_sensitive(dir.path(), false);
        assert!(hidden_by_filter(true));
        assert!(!hidden_by_filter(false));
        set_show_sensitive(dir.path(), true);
        assert!(dir.path().join(SENSITIVE_FILE).exists());
        assert!(!hidden_by_filter(true));
        set_show_sensitive(dir.path(), false);
        assert!(!dir.path().join(SENSITIVE_FILE).exists());
    }
}
