use crate::moderation::{ReportTarget, ReportType};
use crate::route::{RouteServices, RouteType};
use crate::theme;
use crate::widgets::Button;
use egui::{Context, Frame, Id, Margin, Ui};

#[derive(Clone)]
struct ReportState {
    target: ReportTarget,
    sent: Option<bool>,
}

fn state_id() -> Id {
    Id::new("report-state")
}

pub fn open(ctx: &Context, target: ReportTarget) {
    ctx.data_mut(|d| d.insert_temp(state_id(), Some(ReportState { target, sent: None })));
}

fn current(ctx: &Context) -> Option<ReportState> {
    ctx.data(|d| d.get_temp::<Option<ReportState>>(state_id())).flatten()
}

fn store(ctx: &Context, state: Option<ReportState>) {
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
}

pub fn button(ui: &mut Ui, services: &mut RouteServices<'_, '_>, target: ReportTarget) -> bool {
    if services.me() == Some(target.author()) {
        return false;
    }
    if !services.can_sign() {
        if Button::new()
            .text(ui, crate::i18n::t("button.login_to_report"))
            .clicked()
        {
            services.navigate(RouteType::LoginPage);
            return true;
        }
        return false;
    }
    if Button::new().text(ui, crate::i18n::t("button.report")).clicked() {
        open(ui.ctx(), target);
        return true;
    }
    false
}

fn title_key(target: &ReportTarget) -> &'static str {
    match target {
        ReportTarget::Profile { .. } => "report.title_profile",
        ReportTarget::Message { .. } => "report.title_message",
        ReportTarget::Stream { .. } => "report.title_stream",
    }
}

pub fn show(ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
    let Some(mut state) = current(ui.ctx()) else {
        return;
    };
    let mut close = false;
    let width = 360.0_f32.min((ui.ctx().content_rect().width() - 64.0).max(200.0));
    let modal = egui::Modal::new(Id::new("report-modal")).frame(
        Frame::new()
            .fill(theme::SURFACE_LOW)
            .stroke(egui::Stroke::new(1.0, theme::SURFACE))
            .corner_radius(16)
            .inner_margin(Margin::same(24)),
    );
    let resp = modal.show(ui.ctx(), |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        ui.label(theme::semibold(crate::i18n::t(title_key(&state.target)), 18.0));
        match state.sent {
            None | Some(false) => {
                ui.label(theme::medium(crate::i18n::t("report.choose"), 14.0).color(theme::MUTED));
                if state.sent == Some(false) {
                    ui.label(
                        theme::medium(crate::i18n::t("report.failed"), 14.0)
                            .color(egui::Color32::from_rgb(255, 99, 99)),
                    );
                }
                for kind in ReportType::ALL {
                    if Button::new()
                        .full_width()
                        .text(ui, crate::i18n::t(kind.label_key()))
                        .clicked()
                    {
                        state.sent = Some(services.send_report(&state.target, kind));
                    }
                }
                if Button::new().text(ui, crate::i18n::t("button.cancel")).clicked() {
                    close = true;
                }
            }
            Some(true) => {
                ui.label(theme::semibold(crate::i18n::t("report.sent"), 16.0).color(theme::PRIMARY));
                ui.add(
                    egui::Label::new(
                        theme::medium(crate::i18n::t("report.sent_detail"), 14.0).color(theme::TEXT_SOFT),
                    )
                    .wrap_mode(egui::TextWrapMode::Wrap),
                );
                let author = state.target.author();
                if services.me() != Some(author) && !services.is_muted(&author) {
                    ui.add(
                        egui::Label::new(
                            theme::medium(crate::i18n::t("report.block_offer"), 14.0).color(theme::MUTED),
                        )
                        .wrap_mode(egui::TextWrapMode::Wrap),
                    );
                    if Button::primary()
                        .full_width()
                        .text(ui, crate::i18n::t("button.block"))
                        .clicked()
                    {
                        services.set_muted(&author, true);
                    }
                }
                if Button::new().text(ui, crate::i18n::t("button.done")).clicked() {
                    close = true;
                }
            }
        }
    });
    if close || resp.should_close() {
        store(ui.ctx(), None);
    } else {
        store(ui.ctx(), Some(state));
    }
}
