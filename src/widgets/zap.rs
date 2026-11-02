use crate::link::NostrLink;
use crate::route::RouteServices;
use crate::signer::SignHandle;
use crate::stream_info::StreamInfo;
use crate::theme::{
    FONT_SIZE_LG, MARGIN_DEFAULT, NEUTRAL_700, NEUTRAL_800, NEUTRAL_900, PRIMARY, ROUNDING_DEFAULT,
};
use crate::widgets::{Button, NativeTextInput};
use crate::zap::format_sats;
use anyhow::{anyhow, bail};
use egui::text::{LayoutJob, TextWrapping};
use egui::{vec2, Frame, Grid, Response, RichText, TextFormat, TextWrapMode, Ui, Widget};
use lnurl::pay::{LnURLPayInvoice, PayResponse};
use nostr::prelude::{hex, ZapRequestData};
use nostr::{serde_json, EventBuilder, JsonUtil, Kind, PublicKey, Tag, Url};
use nostrdb::Note;
use notedeck::get_current_wallet;
use nwc::nostr::nips::nip47::PayInvoiceResponse;
use poll_promise::Promise;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use std::task::Poll;

pub enum ZapTarget<'a> {
    PublicKey { pubkey: [u8; 32] },
    Event { event: &'a Note<'a> },
}

impl Display for ZapTarget<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ZapTarget::PublicKey { pubkey } => write!(f, "{}", hex::encode(pubkey)),
            ZapTarget::Event { event } => write!(f, "{}", hex::encode(event.id())),
        }
    }
}

#[derive(Clone)]
pub enum ZapState {
    NotStarted,
    Ready {
        service: PayResponse,
    },
    Signing {
        handle: SignHandle,
        callback: String,
        amount: u64,
        nostr: bool,
    },
    FetchingInvoice {
        callback: String,
    },
    Invoice {
        invoice: LnURLPayInvoice,
    },
    Paying {
        invoice: LnURLPayInvoice,
        payment: Arc<Mutex<Promise<Result<PayInvoiceResponse, nwc::Error>>>>,
    },
    Paid,
    Error(String),
}

pub struct ZapButton<'a> {
    target: ZapTarget<'a>,
}

impl<'a> ZapButton<'a> {
    pub fn pubkey(pubkey: [u8; 32]) -> Self {
        Self {
            target: ZapTarget::PublicKey { pubkey },
        }
    }

    pub fn event(event: &'a Note<'a>) -> Self {
        Self {
            target: ZapTarget::Event { event },
        }
    }

    pub fn render(self, ui: &mut Ui, services: &mut RouteServices) -> Response {
        let can_sign = services.can_sign();
        let resp = Button::pill()
            .disabled(!can_sign)
            .text(ui, crate::i18n::t("button.zap"));
        let open_id = egui::Id::new("zap_modal_open");
        if !can_sign {
            return resp.on_hover_text(crate::i18n::t("zap.login_required"));
        }
        if resp.clicked()
            || std::env::var_os("ZAP_STREAM_DEBUG_ZAP").is_some_and(|_| {
                std::env::remove_var("ZAP_STREAM_DEBUG_ZAP");
                true
            })
        {
            ui.ctx().data_mut(|d| d.insert_temp(open_id, true));
        }
        let open = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(open_id))
            .unwrap_or(false);
        if !open {
            return resp;
        }

        let modal = egui::Modal::new(egui::Id::new("zapper")).frame(
            Frame::new()
                .fill(crate::theme::SURFACE_LOW)
                .stroke(egui::Stroke::new(1.0, crate::theme::SURFACE))
                .corner_radius(16)
                .inner_margin(egui::Margin::same(24)),
        );
        let modal_resp = modal.show(ui.ctx(), |ui| {
            ui.set_width(360.0_f32.min((ui.ctx().content_rect().width() - 64.0).max(200.0)));
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);

            let pubkey = match &self.target {
                ZapTarget::PublicKey { pubkey } => pubkey,
                ZapTarget::Event { event } => event.host(),
            };

            let zap_state = services.get("zap_state").unwrap_or(ZapState::NotStarted);
            match &zap_state {
                ZapState::NotStarted => match services.fetch_lnurlp(pubkey) {
                    Ok(Poll::Ready(r)) => services.set("zap_state", ZapState::Ready { service: r }),
                    Err(e) => services.set("zap_state", ZapState::Error(e.to_string())),
                    _ => {}
                },
                ZapState::Signing { handle, callback, amount, nostr } => {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                    match handle.poll() {
                        None => {}
                        Some(Ok(ev)) => match Self::callback_url(callback, *amount, nostr.then_some(&ev)) {
                            Ok(url) => services.set(
                                "zap_state",
                                ZapState::FetchingInvoice { callback: url.to_string() },
                            ),
                            Err(e) => services.set("zap_state", ZapState::Error(e.to_string())),
                        },
                        Some(Err(e)) => services.set("zap_state", ZapState::Error(e.to_string())),
                    }
                }
                ZapState::FetchingInvoice { callback } => {
                    match self.zap_get_invoice(callback, services) {
                        Ok(Poll::Ready(s)) => {
                            let next = match get_current_wallet(
                                services.ctx.accounts,
                                services.ctx.global_wallet,
                            ) {
                                Some(w) => ZapState::Paying {
                                    payment: Arc::new(Mutex::new(w.wallet.pay_invoice(&s.pr))),
                                    invoice: s,
                                },
                                None => ZapState::Invoice { invoice: s },
                            };
                            services.set("zap_state", next)
                        }
                        Err(e) => services.set("zap_state", ZapState::Error(e.to_string())),
                        _ => {}
                    }
                }
                ZapState::Paying { invoice, payment } => match payment
                    .lock()
                    .unwrap()
                    .ready()
                    .map(|r| r.as_ref().map(|_| ()).map_err(|e| e.to_string()))
                {
                    Some(Ok(())) => services.set("zap_state", ZapState::Paid),
                    Some(Err(e)) => {
                        services.set("zap_error", e);
                        services.set(
                            "zap_state",
                            ZapState::Invoice {
                                invoice: invoice.clone(),
                            },
                        )
                    }
                    None => {}
                },
                _ => {}
            }

            match &zap_state {
                ZapState::Ready { service } => {
                    self.render_input(ui, services, pubkey, service);
                }
                ZapState::Signing { .. } => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.spinner();
                        ui.label(
                            crate::theme::medium(crate::i18n::t("signer.waiting"), 14.0)
                                .color(crate::theme::MUTED),
                        );
                        ui.add_space(24.0);
                    });
                }
                ZapState::Paying { .. } => {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(250));
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.spinner();
                        ui.label(
                            crate::theme::medium("Paying with your wallet", 14.0)
                                .color(crate::theme::MUTED),
                        );
                        ui.add_space(24.0);
                    });
                }
                ZapState::Paid => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.label(
                            crate::theme::display_bold_text("Zap sent", 26.0)
                                .color(crate::theme::ZAP),
                        );
                        ui.add_space(24.0);
                    });
                }
                ZapState::Invoice { invoice } => {
                    ui.vertical_centered(|ui| {
                        if let Some(e) = services
                            .get::<String>("zap_error")
                            .filter(|e| !e.is_empty())
                        {
                            ui.label(
                                RichText::new(crate::i18n::tf("zap.wallet_failed", &[("error", &e)]))
                                    .color(crate::theme::MUTED),
                            );
                        }
                        qr_code(ui, invoice.pr.as_bytes(), 256.0);

                        let mut job = LayoutJob::default();
                        job.wrap = TextWrapping::from_wrap_mode_and_width(
                            TextWrapMode::Truncate,
                            ui.available_width(),
                        );
                        job.append(&invoice.pr, 0.0, TextFormat::default());
                        ui.label(job);
                        ui.horizontal(|ui| {
                            if Button::new().text(ui, crate::i18n::t("zap.copy_invoice")).clicked() {
                                ui.ctx().copy_text(invoice.pr.clone());
                            }
                            if Button::primary()
                                .text(ui, crate::i18n::t("button.open_wallet"))
                                .clicked()
                            {
                                crate::android_window::open_url(
                                    ui.ctx(),
                                    &format!("lightning:{}", invoice.pr),
                                );
                            }
                        });
                        if get_current_wallet(services.ctx.accounts, services.ctx.global_wallet)
                            .is_none()
                        {
                            Self::render_connect_wallet(ui, services);
                        }
                    });
                }
                ZapState::Error(e) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.label(crate::theme::display_bold_text("Cannot zap", 22.0));
                        ui.label(RichText::new(e).color(crate::theme::MUTED));
                        ui.add_space(8.0);
                        if Button::new().text(ui, crate::i18n::t("button.close")).clicked() {
                            ui.close();
                        }
                    });
                }
                _ => {}
            }
        });

        if modal_resp.should_close() {
            services.set("zap_state", ZapState::NotStarted);
            services.set("zap_error", String::new());
            ui.ctx().data_mut(|d| d.insert_temp(open_id, false));
        }
        resp
    }

    fn render_connect_wallet(ui: &mut Ui, services: &mut RouteServices) {
        ui.add_space(8.0);
        ui.label(
            crate::theme::medium("Connect a wallet to pay zaps automatically", 13.0)
                .color(crate::theme::MUTED),
        );
        let mut uri = services.get("nwc_uri").unwrap_or(String::new());
        let old = uri.clone();
        NativeTextInput::new(&mut uri)
            .with_frame(true)
            .with_hint_text("nostr+walletconnect://...")
            .ui(ui);
        if uri != old {
            services.set("nwc_uri", uri.clone());
        }
        if Button::new().text(ui, crate::i18n::t("zap.connect_wallet")).clicked() {
            match notedeck::Wallet::new(uri.trim().to_string()) {
                Ok(w) => {
                    services.ctx.global_wallet.wallet = Some(w.into());
                    services.ctx.global_wallet.save_wallet();
                    services.set("zap_error", String::new());
                }
                Err(e) => services.set("zap_error", e.to_string()),
            }
        }
    }

    fn zap_get_invoice(
        &self,
        callback: &str,
        services: &mut RouteServices,
    ) -> anyhow::Result<Poll<LnURLPayInvoice>> {
        match services.fetch(callback) {
            Poll::Ready(Ok(r)) => {
                if r.ok {
                    let inv: LnURLPayInvoice = serde_json::from_slice(&r.bytes)?;
                    Ok(Poll::Ready(inv))
                } else {
                    bail!("{}", crate::i18n::tf("error.response_code", &[("code", &r.status.to_string())]));
                }
            }
            Poll::Ready(Err(e)) => Err(anyhow!("{}", e)),
            Poll::Pending => Ok(Poll::Pending),
        }
    }

    fn render_input(
        &self,
        ui: &mut Ui,
        services: &mut RouteServices,
        pubkey: &[u8; 32],
        service: &PayResponse,
    ) {
        let target_name = match self.target {
            ZapTarget::PublicKey { pubkey } => services.profile(&pubkey).and_then(|p| p.name()),
            ZapTarget::Event { event } => {
                let host = event.host();
                services.profile(host).and_then(|p| p.name())
            }
        };
        let fallback_name = self.target.to_string();
        let target_name = target_name.unwrap_or(&fallback_name);
        ui.label(crate::theme::display_bold_text(
            crate::i18n::tf("zap.title", &[("name", &target_name)]),
            26.0,
        ));

        ui.label(crate::theme::medium(crate::i18n::t("zap.amount"), 14.0).color(crate::theme::MUTED));

        // amount buttons
        const SATS_AMOUNTS: &[u64] = &[
            21, 69, 121, 420, 1_000, 2_100, 4_200, 10_000, 21_000, 42_000, 69_000, 100_000,
            210_000, 500_000, 1_000_000,
        ];
        let selected_amount = services.get("zap_amount").unwrap_or(0);
        const COLS: u32 = 5;
        const GAP: f32 = 6.0;
        let cell = ((ui.available_width() - GAP * (COLS as f32 - 1.0)) / COLS as f32).floor();
        for row in SATS_AMOUNTS.chunks(COLS as usize) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                for x in row {
                    ui.allocate_ui_with_layout(
                        vec2(cell, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            let clicked = Button::new()
                                .full_width()
                                .with_color(if selected_amount == *x {
                                    PRIMARY
                                } else {
                                    NEUTRAL_800
                                })
                                .padding(egui::Margin::symmetric(4, 8))
                                .text(ui, &format_sats(*x as f32))
                                .clicked();
                            if clicked {
                                services.set("zap_amount", *x);
                            }
                        },
                    );
                }
            });
        }

        // comment section
        let mut zap_comment = services.get("zap_comment").unwrap_or(String::new());
        ui.label(
            crate::theme::medium(format!("Your comment for {}", target_name), 14.0)
                .color(crate::theme::MUTED),
        );
        let old_len = zap_comment.len();
        Frame::new()
            .stroke(egui::Stroke::new(1.0, crate::theme::BORDER))
            .corner_radius(ROUNDING_DEFAULT)
            .show(ui, |ui| {
                NativeTextInput::new(&mut zap_comment)
                    .with_hint_text(crate::i18n::t("zap.comment_hint"))
                    .with_frame(true)
                    .ui(ui)
            });

        if Button::primary()
            .radius(255)
            .padding(egui::Margin::symmetric(24, 10))
            .text(ui, crate::i18n::t("button.zap"))
            .clicked()
        {
            // on-click setup callback URL and transition state
            match self.zap_callback(
                services,
                pubkey,
                &zap_comment,
                selected_amount * 1_000,
                &service,
            ) {
                Ok(next) => {
                    services.set("zap_error", String::new());
                    services.set("zap_state", next)
                }
                Err(e) => services.set("zap_state", ZapState::Error(e.to_string())),
            }
        }

        if zap_comment.len() != old_len {
            services.set("zap_comment", zap_comment);
        }
    }

    fn zap_callback(
        &self,
        services: &mut RouteServices,
        pubkey: &[u8; 32],
        zap_comment: &str,
        amount: u64,
        lnurlp: &PayResponse,
    ) -> anyhow::Result<ZapState> {
        let relays: Vec<Url> = services
            .ctx
            .accounts
            .selected_account_write_relays()
            .into_iter()
            .filter_map(|r| match r {
                enostr::RelayId::Websocket(u) => Url::parse(u.as_str()).ok(),
                _ => None,
            })
            .collect();
        if relays.is_empty() {
            bail!("{}", crate::i18n::t("error.no_relays"));
        }
        let mut req = ZapRequestData::new(PublicKey::from_slice(pubkey)?, relays)
            .message(zap_comment)
            .amount(amount);
        match &self.target {
            ZapTarget::Event { event } => {
                req.event_coordinate = Some(
                    NostrLink::from_note(event)
                        .try_into()
                        .map_err(|e| anyhow!("{:?}", e))?,
                );
                req.event_id = crate::stream_info::StreamInfo::goal(*event)
                    .map(nostr::EventId::from_byte_array);
            }
            _ => {}
        };

        let req_tags: Vec<Tag> = req.into();
        let Some(signer) = services.signer() else {
            bail!("{}", crate::i18n::t("error.not_logged_in"))
        };

        let builder = EventBuilder::new(Kind::ZapRequest, zap_comment).tags(req_tags);
        Ok(ZapState::Signing {
            handle: signer.sign(builder),
            callback: lnurlp.callback.clone(),
            amount,
            nostr: lnurlp.nostr_pubkey.is_some(),
        })
    }

    fn callback_url(callback: &str, amount: u64, zap_request: Option<&nostr::Event>) -> anyhow::Result<Url> {
        let mut url = Url::parse(callback)?;
        url.query_pairs_mut()
            .append_pair("amount", amount.to_string().as_str());
        if let Some(ev) = zap_request {
            url.query_pairs_mut().append_pair("nostr", ev.as_json().as_str());
        }
        Ok(url)
    }
}

fn qr_code(ui: &mut Ui, data: &[u8], size: f32) {
    let Ok(code) = qrcode::QrCode::new(data) else {
        return;
    };
    let n = code.width();
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
    let quiet = 2.0;
    let cell = size / (n as f32 + quiet * 2.0);
    for (i, c) in code.to_colors().iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let x = (i % n) as f32;
            let y = (i / n) as f32;
            let min = rect.min + vec2((x + quiet) * cell, (y + quiet) * cell);
            painter.rect_filled(
                egui::Rect::from_min_size(min, vec2(cell, cell)),
                0.0,
                egui::Color32::BLACK,
            );
        }
    }
}
