use crate::link::NostrLink;
use crate::route::RouteServices;
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
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use notedeck::get_current_wallet;
use poll_promise::Promise;
use nwc::nostr::nips::nip47::PayInvoiceResponse;

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
    Ready { service: PayResponse },
    FetchingInvoice { callback: String },
    Invoice { invoice: LnURLPayInvoice },
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
        let resp = Button::pill().disabled(!can_sign).text(ui, crate::i18n::t("button.zap"));
        let open_id = egui::Id::new("zap_modal_open");
        if !can_sign {
            return resp.on_hover_text("Log in with a private key to zap");
        }
        if resp.clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(open_id, true));
        }
        let open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
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
            ui.set_width(360.0);
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
                ZapState::Paying { invoice, payment } => match payment.lock().unwrap().ready().map(|r| r.as_ref().map(|_| ()).map_err(|e| e.to_string())) {
                    Some(Ok(())) => services.set("zap_state", ZapState::Paid),
                    Some(Err(e)) => {
                        services.set("zap_error", e);
                        services.set("zap_state", ZapState::Invoice { invoice: invoice.clone() })
                    }
                    None => {}
                },
                _ => {}
            }

            match &zap_state {
                ZapState::Ready { service } => {
                    self.render_input(ui, services, pubkey, service);
                }
                ZapState::Paying { .. } => {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.spinner();
                        ui.label(crate::theme::medium("Paying with your wallet", 14.0).color(crate::theme::MUTED));
                        ui.add_space(24.0);
                    });
                }
                ZapState::Paid => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.label(crate::theme::display_bold_text("Zap sent", 26.0).color(crate::theme::ZAP));
                        ui.add_space(24.0);
                    });
                }
                ZapState::Invoice { invoice } => {
                    ui.vertical_centered(|ui| {
                        if let Some(e) = services.get::<String>("zap_error").filter(|e| !e.is_empty()) {
                            ui.label(RichText::new(format!("Wallet payment failed: {}", e)).color(crate::theme::MUTED));
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
                            if Button::new().text(ui, "Copy invoice").clicked() {
                                ui.ctx().copy_text(invoice.pr.clone());
                            }
                            if Button::primary().text(ui, crate::i18n::t("button.open_wallet")).clicked() {
                                ui.ctx().open_url(egui::OpenUrl::same_tab(format!(
                                    "lightning:{}",
                                    invoice.pr
                                )));
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
                    ui.label(e);
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
        if Button::new().text(ui, "Connect wallet").clicked() {
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
                    bail!("Invalid response code {}", r.status);
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
        ui.label(crate::theme::display_bold_text(format!("Zap {}", target_name), 26.0));

        ui.label(crate::theme::medium("Zap amount in sats", 14.0).color(crate::theme::MUTED));

        // amount buttons
        const SATS_AMOUNTS: &[u64] = &[
            21, 69, 121, 420, 1_000, 2_100, 4_200, 10_000, 21_000, 42_000, 69_000, 100_000,
            210_000, 500_000, 1_000_000,
        ];
        const COLS: u32 = 5;
        let selected_amount = services.get("zap_amount").unwrap_or(0);
        Grid::new("zap_amounts_grid").show(ui, |ui| {
            let mut ctr = 0;
            for x in SATS_AMOUNTS {
                if Button::new()
                    .with_color(if selected_amount == *x {
                        PRIMARY
                    } else {
                        NEUTRAL_800
                    })
                    .padding(egui::Margin::symmetric(10, 8))
                    .text(ui, &format_sats(*x as f32))
                    .clicked()
                {
                    services.set("zap_amount", *x);
                }
                ctr += 1;
                if ctr % COLS == 0 {
                    ui.end_row();
                }
            }
        });

        // comment section
        let mut zap_comment = services.get("zap_comment").unwrap_or(String::new());
        ui.label(crate::theme::medium(format!("Your comment for {}", target_name), 14.0).color(crate::theme::MUTED));
        let old_len = zap_comment.len();
        NativeTextInput::new(&mut zap_comment)
            .with_frame(true)
            .ui(ui);

        if Button::primary().radius(255).padding(egui::Margin::symmetric(24, 10)).text(ui, crate::i18n::t("button.zap")).clicked() {
            // on-click setup callback URL and transition state
            match self.zap_callback(
                services,
                pubkey,
                &zap_comment,
                selected_amount * 1_000,
                &service,
            ) {
                Ok(callback) => {
                    services.set("zap_error", String::new());
                    services.set(
                    "zap_state",
                    ZapState::FetchingInvoice {
                        callback: callback.to_string(),
                    },
                    )
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
    ) -> anyhow::Result<Url> {
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
            bail!("No relays found");
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
        let keys = if let Some(k) = services.current_account_keys() {
            k
        } else {
            bail!("Not logged in")
        };

        let req_ev = EventBuilder::new(Kind::ZapRequest, zap_comment)
            .tags(req_tags)
            .sign_with_keys(&keys)?;

        let mut url = Url::parse(&lnurlp.callback)?;
        url.query_pairs_mut()
            .append_pair("amount", amount.to_string().as_str());
        if lnurlp.nostr_pubkey.is_some() {
            url.query_pairs_mut()
                .append_pair("nostr", req_ev.as_json().as_str());
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
