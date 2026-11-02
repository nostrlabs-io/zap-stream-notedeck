use crate::profile_edit::ProfileForm;
use crate::route::{RouteAction, RouteServices, RouteType, SettingsSection};
use crate::theme;
use crate::widgets::{Avatar, Button, NostrWidget};
use egui::{Color32, Frame, Margin, Response, RichText, Stroke, TextEdit, Ui};
use nostr::{EventBuilder, Kind, Tag};
use notedeck::{Wallet, ZapWallet};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod broadcast;

const PROFILE_WAIT: Duration = Duration::from_secs(3);
const ERROR: Color32 = Color32::from_rgb(255, 99, 99);

pub struct SettingsPage {
    section: SettingsSection,
    form: ProfileForm,
    existing: String,
    seeded: bool,
    started: Instant,
    status: Option<Result<String, String>>,
    upload: Option<mpsc::Receiver<Result<String, String>>>,
    wallet_uri: String,
    relay_input: String,
    broadcast: broadcast::Broadcast,
}

impl SettingsPage {
    pub fn new(section: SettingsSection) -> Self {
        Self {
            section,
            form: ProfileForm::default(),
            existing: String::new(),
            seeded: false,
            started: Instant::now(),
            status: None,
            upload: None,
            wallet_uri: String::new(),
            relay_input: String::new(),
            broadcast: broadcast::Broadcast::default(),
        }
    }

    fn me(services: &RouteServices<'_, '_>) -> [u8; 32] {
        *services.ctx.accounts.get_selected_account().key.pubkey.bytes()
    }

    fn own_note_tags(services: &RouteServices<'_, '_>, kind: u64) -> Vec<Vec<String>> {
        let me = Self::me(services);
        let filter = nostrdb::Filter::new().kinds([kind]).authors(&[me]).limit(1).build();
        services
            .ctx
            .ndb
            .query(services.tx, &[filter], 1)
            .ok()
            .and_then(|r| r.into_iter().next())
            .map(|r| crate::contacts::tags_from_note(&r.note))
            .unwrap_or_default()
    }

    fn title(ui: &mut Ui, text: &str) {
        ui.add(egui::Label::new(theme::display_bold_text(text, 26.0)));
    }

    fn label(ui: &mut Ui, text: &str) {
        ui.label(theme::medium(text, 14.0).color(theme::MUTED));
    }

    fn field(ui: &mut Ui, value: &mut String, hint: &str, mono: bool) {
        Frame::new()
            .fill(theme::SURFACE)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(12)
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                let mut edit = TextEdit::singleline(value)
                    .frame(Frame::NONE)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY);
                if mono {
                    edit = edit.font(egui::TextStyle::Monospace);
                }
                ui.add(edit);
            });
    }

    fn status_line(&self, ui: &mut Ui) {
        match &self.status {
            Some(Ok(m)) => {
                ui.label(theme::medium(m, 14.0).color(theme::PRIMARY));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(ERROR));
            }
            None => {}
        }
    }

    fn signed_in(services: &RouteServices<'_, '_>) -> bool {
        services.can_sign()
    }

    fn row(ui: &mut Ui, title: &str, subtitle: &str) -> bool {
        Frame::new()
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(12)
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(theme::semibold(title, 16.0).color(theme::TEXT));
                    ui.label(theme::medium(subtitle, 14.0).color(theme::MUTED));
                });
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_and_drag_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    }

    fn menu(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        Self::title(ui, crate::i18n::t("button.settings"));
        if Self::row(ui, crate::i18n::t("button.profile"), crate::i18n::t("settings.profile_desc")) {
            services.navigate(RouteType::SettingsPage {
                section: SettingsSection::Profile,
            });
        }
        if Self::row(ui, crate::i18n::t("settings.wallet"), crate::i18n::t("settings.wallet_desc")) {
            services.navigate(RouteType::SettingsPage {
                section: SettingsSection::Wallet,
            });
        }
        if Self::row(ui, crate::i18n::t("settings.relays"), crate::i18n::t("settings.relays_desc")) {
            services.navigate(RouteType::SettingsPage {
                section: SettingsSection::Relays,
            });
        }
        if Self::row(ui, crate::i18n::t("settings.go_live"), crate::i18n::t("settings.go_live_desc")) {
            services.navigate(RouteType::SettingsPage {
                section: SettingsSection::Broadcast,
            });
        }
        if Self::row(ui, crate::i18n::t("button.logout"), crate::i18n::t("settings.logout_desc")) {
            let me = services.ctx.accounts.get_selected_account().key.pubkey;
            services.action(RouteAction::Logout(me));
        }
    }

    fn back(ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        if Button::new().text(ui, crate::i18n::t("button.back")).clicked() {
            services.navigate(RouteType::SettingsPage {
                section: SettingsSection::Menu,
            });
        }
    }

    fn poll_upload(&mut self) {
        let Some(rx) = &self.upload else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(url)) => {
                self.form.picture = url;
                self.status = Some(Ok(crate::i18n::t("settings.picture_uploaded").to_string()));
                self.upload = None;
            }
            Ok(Err(e)) => {
                self.status = Some(Err(e));
                self.upload = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.upload = None,
        }
    }

    #[cfg(not(target_os = "android"))]
    fn start_upload(&mut self, services: &RouteServices<'_, '_>) {
        let Some(keys) = services.current_account_keys() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let ctx = services.egui.clone();
        self.upload = Some(rx);
        self.status = None;
        tokio::spawn(async move {
            let picked = rfd::AsyncFileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                .pick_file()
                .await;
            let Some(file) = picked else {
                let _ = tx.send(Err(crate::i18n::t("settings.no_file").to_string()));
                ctx.request_repaint();
                return;
            };
            let name = file.file_name();
            let data = file.read().await;
            match crate::blossom::upload_request(&keys, &name, data) {
                Ok(req) => {
                    let tx2 = tx.clone();
                    let ctx2 = ctx.clone();
                    ehttp::fetch(req, move |res| {
                        let out = match res {
                            Ok(r) => crate::blossom::parse_upload_url(r.status, &r.bytes)
                                .map_err(|e| e.to_string()),
                            Err(e) => Err(e),
                        };
                        let _ = tx2.send(out);
                        ctx2.request_repaint();
                    });
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    ctx.request_repaint();
                }
            }
        });
    }

    #[cfg(target_os = "android")]
    fn start_upload(&mut self, _services: &RouteServices<'_, '_>) {
        self.status = Some(Err(crate::i18n::t("settings.upload_unavailable").to_string()));
    }

    fn profile(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        Self::back(ui, services);
        Self::title(ui, crate::i18n::t("settings.edit_profile"));
        if !Self::signed_in(services) {
            ui.label(theme::medium("Log in with a private key to edit your profile", 15.0).color(theme::MUTED));
            return;
        }
        self.poll_upload();
        let me = Self::me(services);
        ui.horizontal(|ui| {
            Avatar::new_optional(Some(self.form.picture.as_str()).filter(|p| !p.is_empty()))
                .size(96.)
                .render(ui, services.ctx);
            ui.vertical(|ui| {
                let uploading = self.upload.is_some();
                let label = if uploading { crate::i18n::t("settings.uploading") } else { crate::i18n::t("settings.upload_picture") };
                if Button::new().disabled(uploading).text(ui, label).clicked() && !uploading {
                    self.start_upload(services);
                }
            });
        });
        Self::label(ui, crate::i18n::t("profile.picture_url"));
        Self::field(ui, &mut self.form.picture, "https://", true);
        Self::label(ui, crate::i18n::t("profile.display_name"));
        Self::field(ui, &mut self.form.name, "Satoshi", false);
        Self::label(ui, crate::i18n::t("profile.about"));
        Self::field(ui, &mut self.form.about, crate::i18n::t("profile.about_hint"), false);
        Self::label(ui, "NIP-05");
        Self::field(ui, &mut self.form.nip05, "you@domain.com", false);
        Self::label(ui, crate::i18n::t("profile.lightning_address"));
        Self::field(ui, &mut self.form.lud16, "you@wallet.com", true);
        self.status_line(ui);
        let ready = self.seeded || self.started.elapsed() > PROFILE_WAIT;
        let clicked = Button::primary()
            .padding(Margin::symmetric(12, 12))
            .radius(255)
            .full_width()
            .disabled(!ready)
            .text(ui, crate::i18n::t("button.save"))
            .clicked();
        if clicked && ready {
            self.save_profile(services, me);
        }
        if !ready {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }

    fn save_profile(&mut self, services: &mut RouteServices<'_, '_>, _me: [u8; 32]) {
        let Some(keys) = services.current_account_keys() else {
            return;
        };
        let content = self.form.to_content(&self.existing);
        match EventBuilder::new(Kind::Metadata, content.clone()).sign_with_keys(&keys) {
            Ok(ev) => {
                services.broadcast_event(ev);
                self.existing = content;
                self.status = Some(Ok(crate::i18n::t("settings.profile_saved").to_string()));
            }
            Err(e) => self.status = Some(Err(e.to_string())),
        }
    }

    fn seed_profile(&mut self, services: &RouteServices<'_, '_>) {
        if self.seeded {
            return;
        }
        let me = Self::me(services);
        let filter = nostrdb::Filter::new().kinds([0]).authors(&[me]).limit(1).build();
        let content = services
            .ctx
            .ndb
            .query(services.tx, &[filter], 1)
            .ok()
            .and_then(|r| r.into_iter().next())
            .map(|r| r.note.content().to_string());
        if let Some(c) = content {
            self.form = ProfileForm::from_content(&c);
            self.existing = c;
            self.seeded = true;
        }
    }

    fn wallet(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        Self::back(ui, services);
        Self::title(ui, crate::i18n::t("settings.wallet"));
        if !Self::signed_in(services) {
            ui.label(theme::medium("Log in with a private key to connect a wallet", 15.0).color(theme::MUTED));
            return;
        }
        let connected = services.ctx.accounts.get_selected_wallet().map(|w| w.wallet.uri.clone());
        match connected {
            Some(uri) => {
                let balance = services
                    .ctx
                    .accounts
                    .get_selected_wallet_mut()
                    .and_then(|w| w.wallet.get_balance())
                    .map(|b| b.as_ref().map(|v| *v).map_err(|e| e.to_string()));
                Self::label(ui, crate::i18n::t("settings.connected_wallet"));
                ui.label(RichText::new(mask_uri(&uri)).monospace().size(14.0).color(theme::TEXT));
                match balance {
                    Some(Ok(msats)) => {
                        ui.label(theme::display(
                            crate::i18n::tf("common.sats_amount", &[("n", &crate::zap::format_sats((msats / 1000) as f32))]),
                            30.0,
                        ).color(theme::PRIMARY));
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(ERROR));
                    }
                    None => {
                        ui.label(theme::medium(crate::i18n::t("settings.loading_balance"), 14.0).color(theme::MUTED));
                        ui.ctx().request_repaint_after(Duration::from_millis(500));
                    }
                }
                if Button::new().text(ui, crate::i18n::t("settings.disconnect_wallet")).clicked() {
                    services.ctx.accounts.update_current_account(|a| a.wallet = None);
                    self.status = None;
                }
            }
            None => {
                Self::body(ui, crate::i18n::t("settings.wallet_paste"));
                Self::label(ui, "nostr+walletconnect://");
                Self::field(ui, &mut self.wallet_uri, "nostr+walletconnect://...", true);
                self.status_line(ui);
                let enabled = !self.wallet_uri.trim().is_empty();
                let clicked = Button::primary()
                    .padding(Margin::symmetric(12, 12))
                    .radius(255)
                    .full_width()
                    .disabled(!enabled)
                    .text(ui, crate::i18n::t("button.connect"))
                    .clicked();
                if clicked && enabled {
                    match Wallet::new(self.wallet_uri.trim().to_string()) {
                        Ok(w) => {
                            services
                                .ctx
                                .accounts
                                .update_current_account(|a| a.wallet = Some(ZapWallet::new(w)));
                            self.wallet_uri.clear();
                            self.status = None;
                        }
                        Err(e) => self.status = Some(Err(e.to_string())),
                    }
                }
            }
        }
    }

    fn body(ui: &mut Ui, text: &str) {
        ui.add(
            egui::Label::new(RichText::new(text).size(15.0).color(theme::TEXT_SOFT))
                .wrap_mode(egui::TextWrapMode::Wrap),
        );
    }

    fn publish_relays(&mut self, services: &mut RouteServices<'_, '_>, tags: Vec<Vec<String>>) {
        let Some(keys) = services.current_account_keys() else {
            return;
        };
        let tags = tags.into_iter().filter_map(|t| Tag::parse(t).ok());
        match EventBuilder::new(Kind::RelayList, "").tags(tags).sign_with_keys(&keys) {
            Ok(ev) => {
                services.broadcast_event(ev);
                self.status = None;
            }
            Err(e) => self.status = Some(Err(e.to_string())),
        }
    }

    fn relays(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        Self::back(ui, services);
        Self::title(ui, crate::i18n::t("settings.relays"));
        if !Self::signed_in(services) {
            ui.label(theme::medium(crate::i18n::t("settings.relays_login"), 15.0).color(theme::MUTED));
            return;
        }
        let tags = Self::own_note_tags(services, 10_002);
        let current = crate::relay_list::relays(&tags);
        if current.is_empty() {
            Self::body(ui, crate::i18n::t("settings.relays_default"));
            for r in crate::relays::DEFAULT_RELAYS {
                ui.label(RichText::new(*r).monospace().size(14.0).color(theme::MUTED));
            }
        }
        let mut remove = None;
        for r in &current {
            ui.horizontal(|ui| {
                ui.label(RichText::new(r).monospace().size(14.0).color(theme::TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if Button::new().text(ui, crate::i18n::t("button.remove")).clicked() {
                        remove = Some(r.clone());
                    }
                });
            });
        }
        if let Some(r) = remove {
            let next = crate::relay_list::without_relay(tags.clone(), &r);
            self.publish_relays(services, next);
            return;
        }
        Self::label(ui, crate::i18n::t("settings.add_relay"));
        Self::field(ui, &mut self.relay_input, "wss://relay.example.com", true);
        self.status_line(ui);
        let valid = crate::relay_list::normalize(&self.relay_input).is_some();
        if Button::primary().disabled(!valid).text(ui, crate::i18n::t("button.add")).clicked() {
            if valid {
                let base = if current.is_empty() {
                    crate::relays::DEFAULT_RELAYS
                        .iter()
                        .fold(tags.clone(), |t, r| crate::relay_list::with_relay(t, r))
                } else {
                    tags.clone()
                };
                let next = crate::relay_list::with_relay(base, &self.relay_input);
                self.publish_relays(services, next);
                self.relay_input.clear();
            }
        }
    }
}

fn mask_uri(uri: &str) -> String {
    let head: String = uri.chars().take(40).collect();
    if uri.chars().count() > 40 {
        format!("{}...", head)
    } else {
        head
    }
}

impl NostrWidget for SettingsPage {
    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            Frame::new().inner_margin(Margin::symmetric(16, 16)).show(ui, |ui| {
                let width = ui.available_width().min(560.0);
                ui.set_max_width(width);
                ui.spacing_mut().item_spacing.y = 12.0;
                match self.section {
                    SettingsSection::Menu => self.menu(ui, services),
                    SettingsSection::Profile => self.profile(ui, services),
                    SettingsSection::Wallet => self.wallet(ui, services),
                    SettingsSection::Relays => self.relays(ui, services),
                    SettingsSection::Broadcast => self.broadcast.render(ui, services),
                }
            });
        });
        ui.response()
    }

    fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        if self.section == SettingsSection::Profile {
            self.seed_profile(services);
        }
        Ok(())
    }
}
