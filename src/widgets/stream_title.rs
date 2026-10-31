use crate::note_util::NoteUtil;
use crate::route::RouteServices;
use crate::contacts;
use crate::stream_info::{game_label, is_game_tag, StreamInfo, StreamStatus};
use crate::theme::{self, MUTED, PRIMARY, SURFACE, TEXT, TEXT_SOFT};
use crate::widgets::zap::ZapButton;
use crate::widgets::{Avatar, Button, Pill};
use egui::{Frame, Label, Margin, Response, RichText, TextWrapMode, Ui};
use nostrdb::Note;

pub struct StreamTitle<'a> {
    event: &'a Note<'a>,
}

impl<'a> StreamTitle<'a> {
    pub fn new(event: &'a Note<'a>) -> StreamTitle<'a> {
        StreamTitle { event }
    }

    pub fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        Frame::new()
            .inner_margin(Margin::symmetric(0, 8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8., 10.0);

                let size = if ui.available_width() < 560.0 { 24.0 } else { 30.72 };
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.add(
                        Label::new(theme::display(
                            self.event.title().unwrap_or(crate::i18n::t("common.untitled")),
                            size,
                        ))
                        .wrap_mode(TextWrapMode::Wrap),
                    );
                });

                let status = self.event.status();
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);
                    let host = *self.event.host();
                    let profile = services.profile(&host);
                    Avatar::from_profile(&profile)
                        .size(32.)
                        .render(ui, services.ctx);
                    let name = profile
                        .and_then(|p| p.display_name().or(p.name()))
                        .unwrap_or(crate::i18n::t("common.anon"))
                        .to_string();
                    ui.label(theme::semibold(name, 16.0).color(TEXT));
                    ui.add_space(8.0);

                    let me = *services.ctx.accounts.get_selected_account().key.pubkey.bytes();
                    if me != host && services.can_sign() {
                        let hex = nostr::prelude::hex::encode(host);
                        let following = contacts::is_following(&services.own_contact_tags(), &hex);
                        let btn = if following { Button::new() } else { Button::primary() };
                        let label = if following { crate::i18n::t("button.unfollow") } else { crate::i18n::t("button.follow") };
                        if btn.text(ui, label).clicked() {
                            services.set_following(&host, !following);
                        }
                    }
                    ZapButton::event(self.event).render(ui, services);

                    if let Some(url) = self.event.share_url() {
                        let id = ui.id().with("share-copied");
                        let copied = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
                        let label = if copied { crate::i18n::t("button.link_copied") } else { crate::i18n::t("button.share") };
                        if Button::new().text(ui, label).clicked() {
                            ui.ctx().copy_text(url);
                            ui.data_mut(|d| d.insert_temp(id, true));
                        }
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                    let color = match status {
                        StreamStatus::Live => PRIMARY,
                        _ => SURFACE,
                    };
                    ui.add(Pill::new(&status.to_string()).color(color));
                    if status == StreamStatus::Live {
                        if let Some(v) = self.event.viewers() {
                            ui.add(Pill::new(&format!("{} viewers", v)));
                        }
                    }
                    if status == StreamStatus::Planned {
                        ui.add(Pill::new(&format!(
                            "Starts {}",
                            format_starts(self.event.starts())
                        )));
                    }
                });

                if let Some(summary) = self
                    .event
                    .get_tag_value("summary")
                    .and_then(|r| r.variant().str())
                {
                    if !summary.is_empty() {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.add(
                                Label::new(RichText::new(summary).color(TEXT_SOFT))
                                    .wrap_mode(TextWrapMode::Wrap),
                            );
                        });
                    }
                }

                let game = self.event.game_id();
                let tags: Vec<String> = self
                    .tags()
                    .into_iter()
                    .filter(|t| !is_game_tag(t))
                    .collect();
                if game.is_some() || !tags.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
                        if let Some(g) = game {
                            let r = ui
                                .add(
                                    Label::new(theme::semibold(game_label(&g), 15.0).color(PRIMARY))
                                        .sense(egui::Sense::click()),
                                )
                                .on_hover_and_drag_cursor(egui::CursorIcon::PointingHand);
                            if r.clicked() {
                                services.navigate(crate::route::RouteType::CategoryPage { id: g });
                            }
                        }
                        for t in tags {
                            let r = ui
                                .add(
                                    Label::new(theme::medium(format!("#{}", t), 15.0).color(MUTED))
                                        .sense(egui::Sense::click()),
                                )
                                .on_hover_and_drag_cursor(egui::CursorIcon::PointingHand);
                            if r.clicked() {
                                services.navigate(crate::route::RouteType::HashtagPage {
                                    id: t.to_lowercase(),
                                });
                            }
                        }
                    });
                }
            })
            .response
    }

    fn tags(&self) -> Vec<String> {
        let mut out = Vec::new();
        for tag in self.event.tags() {
            if tag.get_unchecked(0).variant().str() == Some("t") {
                if let Some(v) = tag.get_unchecked(1).variant().str() {
                    if !v.is_empty() {
                        out.push(v.to_string());
                    }
                }
            }
        }
        out
    }
}

fn format_starts(ts: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match ts.checked_sub(now) {
        Some(left) if left > 0 => format!("in {}", crate::player_health::format_countdown(left)),
        _ => "soon".to_string(),
    }
}
