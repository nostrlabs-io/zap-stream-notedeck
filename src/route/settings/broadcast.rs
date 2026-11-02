use super::{SettingsPage, ERROR};
use crate::route::{RouteServices, RouteType, SettingsSection};
use crate::theme;
use crate::widgets::Button;
use crate::zap_api::{self, Account, StreamDetails};
use egui::{Margin, RichText, Ui};
use std::sync::mpsc;

pub enum Load {
    Idle,
    Loading(mpsc::Receiver<Result<Account, String>>),
    Loaded(Account),
    Failed(String),
}

pub struct Broadcast {
    pub load: Load,
    pub form: StreamDetails,
    pub tags: String,
    pub nsfw: bool,
    pub endpoint: usize,
    pub show_key: bool,
    pub saving: Option<mpsc::Receiver<Result<(), String>>>,
    pub status: Option<Result<String, String>>,
}

impl Default for Broadcast {
    fn default() -> Self {
        Self {
            load: Load::Idle,
            form: StreamDetails::default(),
            tags: "irl".to_string(),
            nsfw: false,
            endpoint: 0,
            show_key: false,
            saving: None,
            status: None,
        }
    }
}

fn run<T: Send + 'static>(
    req: anyhow::Result<ehttp::Request>,
    ctx: egui::Context,
    parse: fn(u16, &[u8]) -> anyhow::Result<T>,
) -> mpsc::Receiver<Result<T, String>> {
    let (tx, rx) = mpsc::channel();
    match req {
        Ok(req) => ehttp::fetch(req, move |res| {
            let out = match res {
                Ok(r) => parse(r.status, &r.bytes).map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            let _ = tx.send(out);
            ctx.request_repaint();
        }),
        Err(e) => {
            let _ = tx.send(Err(e.to_string()));
        }
    }
    rx
}

fn unit(_: u16, _: &[u8]) -> anyhow::Result<()> {
    Ok(())
}

fn status_only(status: u16, body: &[u8]) -> anyhow::Result<()> {
    zap_api::check_status(status, body)?;
    unit(status, body)
}

impl Broadcast {
    fn start_load(&mut self, services: &RouteServices<'_, '_>) {
        let Some(keys) = services.current_account_keys() else {
            return;
        };
        self.load = Load::Loading(run(
            zap_api::account_request(&keys),
            services.egui.clone(),
            zap_api::parse_account,
        ));
    }

    fn poll(&mut self) {
        if let Load::Loading(rx) = &self.load {
            match rx.try_recv() {
                Ok(Ok(acc)) => {
                    if let Some(d) = &acc.details {
                        self.form = d.clone();
                        self.tags = d.tags.join(",");
                        self.nsfw = !d.content_warning.is_empty();
                    }
                    self.load = Load::Loaded(acc);
                }
                Ok(Err(e)) => self.load = Load::Failed(e),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.load = Load::Failed(crate::i18n::t("broadcast.dropped").to_string())
                }
            }
        }
        if let Some(rx) = &self.saving {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.status = Some(Ok(crate::i18n::t("broadcast.saved").to_string()));
                    self.saving = None;
                }
                Ok(Err(e)) => {
                    self.status = Some(Err(e));
                    self.saving = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.saving = None,
            }
        }
    }

    fn save(&mut self, services: &RouteServices<'_, '_>) {
        let Some(keys) = services.current_account_keys() else {
            return;
        };
        self.form.tags = zap_api::parse_tags(&self.tags);
        self.form.content_warning = if self.nsfw { "nsfw".to_string() } else { String::new() };
        let req = zap_api::update_request(&keys, None, &self.form);
        self.status = None;
        self.saving = Some(run(req, services.egui.clone(), status_only));
    }

    fn accept_tos(&mut self, services: &RouteServices<'_, '_>) {
        let Some(keys) = services.current_account_keys() else {
            return;
        };
        self.saving = Some(run(
            zap_api::accept_tos_request(&keys),
            services.egui.clone(),
            status_only,
        ));
        self.load = Load::Idle;
    }

    fn copy_row(ui: &mut Ui, label: &str, value: &str, hidden: bool) {
        SettingsPage::label(ui, label);
        ui.horizontal(|ui| {
            let shown = if hidden { "\u{2022}".repeat(24) } else { value.to_string() };
            ui.add(
                egui::Label::new(RichText::new(shown).monospace().size(14.0).color(theme::TEXT))
                    .wrap_mode(egui::TextWrapMode::Wrap),
            );
            if Button::new().text(ui, crate::i18n::t("button.copy")).clicked() {
                ui.ctx().copy_text(value.to_string());
            }
        });
    }

    pub fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        if Button::new().text(ui, crate::i18n::t("button.back")).clicked() {
            services.navigate(RouteType::SettingsPage {
                section: SettingsSection::Menu,
            });
        }
        SettingsPage::title(ui, crate::i18n::t("settings.go_live"));
        if !services.can_sign() {
            ui.label(
                theme::medium(crate::i18n::t("broadcast.login_required"), 15.0).color(theme::MUTED),
            );
            return;
        }
        self.poll();
        if matches!(self.load, Load::Idle) {
            self.start_load(services);
        }
        let account = match &self.load {
            Load::Loaded(a) => a.clone(),
            Load::Failed(e) => {
                ui.label(RichText::new(e.clone()).color(ERROR));
                if Button::new().text(ui, crate::i18n::t("button.retry")).clicked() {
                    self.load = Load::Idle;
                }
                return;
            }
            _ => {
                ui.label(theme::medium(crate::i18n::t("common.loading_dots"), 14.0).color(theme::MUTED));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                return;
            }
        };
        if !account.tos_accepted {
            SettingsPage::body(ui, crate::i18n::t("broadcast.tos"));
            if let Some(link) = &account.tos_link {
                ui.hyperlink(link);
            }
            if Button::primary().text(ui, crate::i18n::t("button.accept")).clicked() {
                self.accept_tos(services);
            }
            self.poll();
            return;
        }
        SettingsPage::label(ui, crate::i18n::t("broadcast.balance"));
        ui.label(
            theme::display(crate::i18n::tf("common.sats_amount", &[("n", &(account.balance as i64).to_string())]), 24.0).color(theme::PRIMARY),
        );
        if account.endpoints.is_empty() {
            SettingsPage::body(ui, crate::i18n::t("broadcast.no_endpoints"));
        } else {
            self.endpoint = self.endpoint.min(account.endpoints.len() - 1);
            let ep = &account.endpoints[self.endpoint];
            ui.horizontal_wrapped(|ui| {
                for (i, e) in account.endpoints.iter().enumerate() {
                    if ui.selectable_label(i == self.endpoint, &e.name).clicked() {
                        self.endpoint = i;
                    }
                }
            });
            ui.label(
                theme::medium(crate::i18n::tf("broadcast.rate", &[("rate", &ep.rate.to_string()), ("unit", &ep.unit)]), 14.0).color(theme::MUTED),
            );
            Self::copy_row(ui, crate::i18n::t("broadcast.server_url"), &ep.url, false);
            Self::copy_row(ui, crate::i18n::t("broadcast.stream_key"), &ep.key, !self.show_key);
            let toggle = if self.show_key { crate::i18n::t("broadcast.hide_key") } else { crate::i18n::t("broadcast.show_key") };
            if Button::new().text(ui, toggle).clicked() {
                self.show_key = !self.show_key;
            }
        }
        SettingsPage::label(ui, crate::i18n::t("broadcast.title"));
        SettingsPage::field(ui, &mut self.form.title, crate::i18n::t("broadcast.title_hint"), false);
        SettingsPage::label(ui, crate::i18n::t("broadcast.summary"));
        SettingsPage::field(ui, &mut self.form.summary, crate::i18n::t("broadcast.summary_hint"), false);
        SettingsPage::label(ui, crate::i18n::t("broadcast.tags"));
        SettingsPage::field(ui, &mut self.tags, "irl, music", false);
        SettingsPage::label(ui, crate::i18n::t("broadcast.goal"));
        SettingsPage::field(ui, &mut self.form.goal, "note1... or hex", true);
        ui.checkbox(&mut self.nsfw, crate::i18n::t("broadcast.nsfw"));
        match &self.status {
            Some(Ok(m)) => {
                ui.label(theme::medium(m, 14.0).color(theme::PRIMARY));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(ERROR));
            }
            None => {}
        }
        let busy = self.saving.is_some();
        if Button::primary()
            .padding(Margin::symmetric(12, 12))
            .radius(255)
            .full_width()
            .disabled(busy)
            .text(ui, crate::i18n::t("button.save"))
            .clicked()
            && !busy
        {
            self.save(services);
        }
    }
}
