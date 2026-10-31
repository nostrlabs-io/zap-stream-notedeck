use crate::note_ref::NoteRef;
use crate::note_view::NotesView;
use crate::route::RouteServices;
use crate::sub::SubRef;
use crate::contacts;
use crate::stream_info::{StreamInfo, StreamStatus};
use crate::theme::{self, MUTED, PRIMARY, TEXT_SOFT};
use crate::widgets::zap::ZapButton;
use crate::widgets::{sub_or_poll, Avatar, Button, NostrWidget, Pill, StreamList};
use nostr::{PublicKey, ToBech32};
use egui::{vec2, Frame, Id, Label, Margin, Response, RichText, ScrollArea, TextWrapMode, Ui};
use nostrdb::{Filter, Note};
use std::collections::HashSet;

pub struct ProfilePage {
    pubkey: [u8; 32],
    events: HashSet<NoteRef>,
    sub: Option<SubRef>,
    followers: HashSet<NoteRef>,
    followers_sub: Option<SubRef>,
    copied: bool,
}

impl ProfilePage {
    pub fn new(pubkey: [u8; 32]) -> Self {
        Self {
            pubkey,
            events: HashSet::new(),
            sub: None,
            followers: HashSet::new(),
            followers_sub: None,
            copied: false,
        }
    }

    fn stat(ui: &mut Ui, value: usize, label: &str) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(theme::display(value.to_string(), 30.0).color(PRIMARY));
            ui.label(theme::medium(label, 14.0).color(MUTED));
        });
    }
}

impl NostrWidget for ProfilePage {
    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        let profile = services.profile(&self.pubkey);
        let wide = ui.available_width() >= theme::WIDE_BREAKPOINT;
        let is_live = self.events.iter().any(|e| {
            services
                .ctx
                .ndb
                .get_note_by_key(services.tx, e.key)
                .map(|n| n.status() == StreamStatus::Live && n.host() == &self.pubkey)
                .unwrap_or(false)
        });

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(if wide { 0 } else { 16 }, 16))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(16.0, 14.0);

                        ui.horizontal_top(|ui| {
                            Avatar::from_profile(&profile)
                                .size(132.0)
                                .ring(PRIMARY)
                                .render(ui, services.ctx);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 8.0;
                                let name = profile
                                    .and_then(|p| p.display_name().or(p.name()))
                                    .unwrap_or(crate::i18n::t("common.anon"))
                                    .to_string();
                                ui.add(
                                    Label::new(theme::display(
                                        name,
                                        if wide { 48.0 } else { 32.0 },
                                    ))
                                    .wrap_mode(TextWrapMode::Wrap),
                                );
                                if is_live {
                                    ui.add(Pill::new(crate::i18n::t("status.live")).color(PRIMARY));
                                }
                                if let Some(nip05) = profile.and_then(|p| p.nip05()) {
                                    if !nip05.is_empty() {
                                        ui.label(theme::medium(nip05, 14.5).color(PRIMARY));
                                    }
                                }
                                if let Some(addr) = profile.and_then(|p| p.lud16()) {
                                    if !addr.is_empty() {
                                        ui.label(theme::medium(format!("\u{26a1} {}", addr), 14.5).color(MUTED));
                                    }
                                }
                                if let Some(site) = profile.and_then(|p| p.website()) {
                                    if !site.is_empty() {
                                        ui.label(theme::medium(site, 14.5).color(MUTED));
                                    }
                                }
                            });
                        });

                        if let Some(about) = profile.and_then(|p| p.about()) {
                            if !about.is_empty() {
                                ui.add(
                                    Label::new(RichText::new(about).size(16.0).color(TEXT_SOFT))
                                        .wrap_mode(TextWrapMode::Wrap),
                                );
                            }
                        }

                        let hex = nostr::prelude::hex::encode(self.pubkey);
                        let me = *services.ctx.accounts.get_selected_account().key.pubkey.bytes();
                        let can_sign = services.current_account_keys().is_some();
                        let is_me = me == self.pubkey;
                        ui.horizontal(|ui| {
                            if !is_me && can_sign {
                                let following =
                                    contacts::is_following(&services.own_contact_tags(), &hex);
                                let label = if following { crate::i18n::t("button.unfollow") } else { crate::i18n::t("button.follow") };
                                let btn = if following { Button::new() } else { Button::primary() };
                                if btn.text(ui, label).clicked() {
                                    services.set_following(&self.pubkey, !following);
                                }
                            }
                            ZapButton::pubkey(self.pubkey).render(ui, services);
                            let npub = PublicKey::from_slice(&self.pubkey)
                                .ok()
                                .and_then(|k| k.to_bech32().ok())
                                .unwrap_or_default();
                            let copy_label = if self.copied { crate::i18n::t("button.copied") } else { crate::i18n::t("button.copy_npub") };
                            if Button::new().text(ui, copy_label).clicked() {
                                ui.ctx().copy_text(npub);
                                self.copied = true;
                            }
                        });

                        let count = self.events.len();
                        let followers = self.followers.len();
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 48.0;
                            Self::stat(ui, followers, crate::i18n::t("profile.followers"));
                            Self::stat(ui, count, crate::i18n::t("profile.past_streams"));
                        });

                        let events: Vec<Note> = self
                            .events
                            .iter()
                            .filter_map(|e| {
                                services.ctx.ndb.get_note_by_key(services.tx, e.key).ok()
                            })
                            .collect();

                        StreamList::new(
                            Id::new("profile-streams"),
                            NotesView::from_vec(events.iter().collect()),
                            None::<RichText>,
                        )
                        .render(ui, services);
                    });
            });
        ui.response()
    }

    fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.followers,
            &mut self.followers_sub,
            vec![Filter::new().kinds([3]).pubkeys(&[self.pubkey]).build()],
        )?;
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.events,
            &mut self.sub,
            vec![
                Filter::new()
                    .kinds([30_311])
                    .authors(&[self.pubkey])
                    .build(),
                Filter::new()
                    .kinds([30_311])
                    .pubkeys(&[self.pubkey])
                    .build(),
            ],
        )
    }
}
