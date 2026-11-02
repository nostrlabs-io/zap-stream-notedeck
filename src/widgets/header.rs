use crate::link::NostrLink;
use crate::route::{RouteServices, RouteType};
use crate::theme::{self, BORDER, HEADER_HEIGHT, MUTED, PRIMARY, SURFACE_LOW, TEXT};
use crate::widgets::avatar::Avatar;
use crate::widgets::Button;
use egui::emath::Align;
use egui::{
    vec2, Color32, CursorIcon, Frame, Image, Layout, Margin, Response, Sense, Stroke, TextEdit,
    Ui, Widget,
};
use nostrdb::Transaction;

fn is_fallback(pk: &enostr::Pubkey) -> bool {
    pk.bytes() == notedeck::FALLBACK_PUBKEY().bytes()
}

pub struct Header {
    can_go_back: bool,
}

impl Header {
    fn language_picker(ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        let current = crate::i18n::current_code();
        let button = ui
            .add(
                egui::Label::new(theme::semibold(current.to_uppercase(), 13.0).color(MUTED))
                    .sense(Sense::click()),
            )
            .on_hover_and_drag_cursor(CursorIcon::PointingHand);
        let mut chosen = None;
        if std::env::var("ZAP_STREAM_DEBUG_MENU").as_deref() == Ok("lang") {
            std::env::remove_var("ZAP_STREAM_DEBUG_MENU");
            egui::Popup::open_id(ui.ctx(), egui::Id::new("language-menu"));
        }
        egui::Popup::menu(&button)
            .id(egui::Id::new("language-menu"))
            .show(|ui| {
                ui.set_min_width(160.0);
                ui.spacing_mut().button_padding = egui::vec2(8.0, 5.0);
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for (code, name) in crate::i18n::LOCALES {
                    let active = *code == current;
                    let text = theme::medium(*name, 14.0).color(if active { PRIMARY } else { TEXT });
                    if ui.add(egui::Button::new(text).frame(false)).clicked() {
                        chosen = Some(code.to_string());
                        ui.close();
                    }
                }
                });
            });
        if let Some(code) = chosen {
            services.action(crate::route::RouteAction::SetLocale(code));
        }
    }
}

impl Header {
    pub fn new(can_go_back: bool) -> Self {
        Self { can_go_back }
    }

    pub fn render(
        &mut self,
        ui: &mut Ui,
        services: &mut RouteServices<'_, '_>,
        tx: &Transaction,
    ) -> Response {
        let wide = ui.available_width() >= theme::WIDE_BREAKPOINT;
        let plan = crate::android_window::header_top(ui.ctx().pixels_per_point(), ui.available_width());
        let r = Frame::new()
            .inner_margin(Margin {
                left: (16.0 + plan.left).round().clamp(0.0, 127.0) as i8,
                right: (16.0 + plan.right).round().clamp(0.0, 127.0) as i8,
                top: plan.top.round().clamp(0.0, 127.0) as i8,
                bottom: 0,
            })
            .show(ui, |ui| {
                let size = vec2(ui.available_width(), HEADER_HEIGHT);
                ui.allocate_ui_with_layout(size, Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.;
                    if self.can_go_back {
                        let back = Image::from_bytes(
                            "arrow_left.svg",
                            include_bytes!("../resources/icons/arrow-left.svg"),
                        )
                        .max_height(22.)
                        .sense(Sense::click())
                        .ui(ui)
                        .on_hover_and_drag_cursor(CursorIcon::PointingHand);
                        if back.clicked() {
                            services.action(crate::route::RouteAction::Back);
                        }
                    }
                    let logo = Image::from_bytes(
                        "header_logo.svg",
                        include_bytes!("../resources/logo_amber.svg"),
                    )
                    .max_height(22.)
                    .sense(Sense::click())
                    .ui(ui)
                    .on_hover_and_drag_cursor(CursorIcon::PointingHand);
                    let word = ui
                        .add(
                            egui::Label::new(theme::display_bold_text("zap.stream", 19.).color(PRIMARY))
                                .sense(Sense::click()),
                        )
                        .on_hover_and_drag_cursor(CursorIcon::PointingHand);
                    if logo.clicked() || word.clicked() {
                        services.navigate(RouteType::HomePage);
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.set_height(HEADER_HEIGHT);
                        let acc = services.ctx.accounts.get_selected_account();
                        let logged_in = acc.key.secret_key.is_some()
                            || (!acc.key.pubkey.bytes().iter().all(|b| *b == 0)
                                && !is_fallback(&acc.key.pubkey));
                        if logged_in {
                            let pubkey = acc.key.pubkey;
                            let avatar = Avatar::pubkey(pubkey.bytes(), services.ctx.ndb, tx)
                                .size(32.)
                                .render(ui, services.ctx);
                            let profile = services.profile(pubkey.bytes());
                            let name = profile
                                .and_then(|p| p.display_name().or(p.name()))
                                .unwrap_or(crate::i18n::t("common.anon"))
                                .to_string();
                            let mut action = None;
                            if std::env::var("ZAP_STREAM_DEBUG_MENU").as_deref() == Ok("account") {
                                std::env::remove_var("ZAP_STREAM_DEBUG_MENU");
                                egui::Popup::open_id(ui.ctx(), egui::Id::new("account-menu"));
                            }
                            egui::Popup::menu(&avatar)
                                .id(egui::Id::new("account-menu"))
                                .show(|ui| {
                                    ui.set_min_width(220.0);
                                    ui.spacing_mut().item_spacing.y = 4.0;
                                    ui.label(theme::semibold(&name, 15.0));
                                    ui.label(
                                        theme::medium(
                                            format!("{}...", &pubkey.hex()[..12]),
                                            12.5,
                                        )
                                        .color(MUTED),
                                    );
                                    ui.separator();
                                    if Button::new().text(ui, crate::i18n::t("button.profile")).clicked() {
                                        action = Some("profile");
                                        ui.close();
                                    }
                                    if Button::new().text(ui, crate::i18n::t("button.settings")).clicked() {
                                        action = Some("settings");
                                        ui.close();
                                    }
                                    if Button::new().text(ui, crate::i18n::t("button.logout")).clicked() {
                                        action = Some("logout");
                                        ui.close();
                                    }
                                });
                            match action {
                                Some("profile") => {
                                    services.navigate(RouteType::ProfilePage {
                                        link: NostrLink::profile(pubkey.bytes()),
                                    });
                                }
                                Some("settings") => {
                                    services.navigate(RouteType::SettingsPage {
                                        section: crate::route::SettingsSection::Menu,
                                    });
                                }
                                Some("logout") => {
                                    services.action(crate::route::RouteAction::Logout(pubkey));
                                }
                                _ => {}
                            }
                        } else {
                            let clicked = ui
                                .scope(|ui| {
                                    ui.set_height(34.0);
                                    Button::primary().text(ui, crate::i18n::t("button.login")).clicked()
                                })
                                .inner;
                            if clicked {
                                services.navigate(RouteType::LoginPage);
                            }
                        }

                        Self::language_picker(ui, services);
                        if wide {
                            let w = (ui.available_width() - 32.0).clamp(0.0, 410.0);
                            let qid = egui::Id::new("search-query");
                            let mut query: String =
                                ui.ctx().data(|d| d.get_temp(qid)).unwrap_or_default();
                            let before = query.clone();
                            let mut submitted = false;
                            Frame::new()
                                .fill(SURFACE_LOW)
                                .stroke(Stroke::new(1.0, BORDER))
                                .corner_radius(12)
                                .inner_margin(Margin::symmetric(12, 4))
                                .show(ui, |ui| {
                                    ui.set_width(w - 26.0);
                                    ui.set_height(18.0);
                                    let edit = ui.add(
                                        TextEdit::singleline(&mut query)
                                            .frame(egui::Frame::NONE)
                                            .hint_text(
                                                theme::semibold(crate::i18n::t("common.search"), theme::FONT_SIZE)
                                                    .color(MUTED),
                                            )
                                            .desired_width(f32::INFINITY),
                                    );
                                    submitted = edit.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                });
                            if submitted && !query.trim().is_empty() {
                                services.navigate(RouteType::SearchPage {
                                    query: query.trim().to_string(),
                                });
                                query.clear();
                            }
                            if query != before {
                                ui.ctx().data_mut(|d| d.insert_temp(qid, query));
                            }
                        } else {
                            let r = ui.scope(|ui| {
                                ui.set_height(34.0);
                                Frame::new()
                                .fill(SURFACE_LOW)
                                .stroke(Stroke::new(1.0, BORDER))
                                .corner_radius(12)
                                .inner_margin(Margin::symmetric(12, 6))
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            theme::semibold(crate::i18n::t("common.search"), 14.0)
                                                .color(MUTED),
                                        )
                                        .selectable(false),
                                    )
                                })
                                .response
                            })
                                .response
                                .interact(Sense::click())
                                .on_hover_and_drag_cursor(CursorIcon::PointingHand);
                            if r.clicked() {
                                services.navigate(RouteType::SearchPage {
                                    query: String::new(),
                                });
                            }
                        }
                    });
                });
            });
        let rect = r.response.rect;
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            Stroke::new(1.0, BORDER),
        );
        let _ = (vec2(0., 0.), Color32::TRANSPARENT, TEXT);
        r.response
    }
}
