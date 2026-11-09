use crate::profiles::ProfileLoader;
use crate::route::{page, RouteAction, RouteServices, RouteType, SettingsSection};
use crate::widgets::{Header, NostrWidget, Sidebar};
use egui::epaint::Margin;
use egui::Ui;
use log::{error, info};
use nostrdb::{Filter, Transaction};
use notedeck::{
    AppContext, AppResponse, ScopedSubIdentity, SubConfig, SubKey, SubOwnerKey,
};
use poll_promise::Promise;
use std::collections::HashMap;
use std::sync::mpsc;

pub struct ZapStreamApp {
    current: RouteType,
    history: Vec<RouteType>,
    routes_rx: mpsc::Receiver<RouteType>,
    routes_tx: mpsc::Sender<RouteType>,

    #[cfg(target_os = "android")]
    app: android_activity::AndroidApp,

    widget: Box<dyn NostrWidget>,
    profiles: ProfileLoader,
    fetch: HashMap<String, Promise<ehttp::Result<ehttp::Response>>>,
    fonts_set: bool,
    frame_stats: Option<crate::frame_stats::FrameStats>,
    startup_link: Option<crate::link::NostrLink>,
    startup_page: Option<String>,
    render_state: Option<egui_wgpu::RenderState>,
    relay_health: crate::relay_health::RelayHealth,
    locale_loaded: bool,
    last_link_poll: std::time::Instant,
    startup_url: Option<String>,
}

impl ZapStreamApp {
    #[cfg(target_os = "android")]
    pub fn new(app: android_activity::AndroidApp, render_state: Option<egui_wgpu::RenderState>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            current: RouteType::HomePage,
            history: Vec::new(),
            widget: Box::new(page::HomePage::new(render_state.clone())),
            profiles: ProfileLoader::new(),
            routes_tx: tx,
            routes_rx: rx,
            fetch: HashMap::new(),
            fonts_set: false,
            frame_stats: crate::frame_stats::FrameStats::enabled(),
            startup_link: startup_link_from_env(),
            startup_page: None,
            render_state,
            relay_health: Default::default(),
            locale_loaded: false,
            last_link_poll: std::time::Instant::now(),
            startup_url: startup_url(),
            app,
        }
    }

    #[cfg(not(target_os = "android"))]
    pub fn new(render_state: Option<egui_wgpu::RenderState>) -> Self {
        Self::base(render_state)
    }

    #[cfg(not(target_os = "android"))]
    fn base(render_state: Option<egui_wgpu::RenderState>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            current: RouteType::HomePage,
            history: Vec::new(),
            widget: Box::new(page::HomePage::new(render_state.clone())),
            profiles: ProfileLoader::new(),
            routes_tx: tx,
            routes_rx: rx,
            fetch: HashMap::new(),
            fonts_set: false,
            frame_stats: crate::frame_stats::FrameStats::enabled(),
            startup_link: startup_link_from_env(),
            startup_page: None,
            render_state,
            relay_health: Default::default(),
            locale_loaded: false,
            last_link_poll: std::time::Instant::now(),
            startup_url: startup_url(),
        }
    }

    fn load_locale(&mut self, ctx: &AppContext<'_>) {
        if self.locale_loaded {
            return;
        }
        self.locale_loaded = true;
        let dir = ctx.path.path(notedeck::DataPathType::Setting);
        let code = crate::i18n::load_saved(&dir)
            .or_else(|| crate::i18n::detect_system_locale().map(str::to_string));
        if let Some(code) = code {
            crate::i18n::set_locale(&code);
        }
    }

    fn push_current(&mut self) {
        if self.history.last() != Some(&self.current) {
            self.history.push(self.current.clone());
        }
        if self.history.len() > 50 {
            self.history.remove(0);
        }
    }

    fn open_route(&mut self, route: RouteType) {
        self.current = route;
        match &self.current {
            RouteType::HomePage => {
                self.widget = Box::new(page::HomePage::new(self.render_state.clone()));
            }
            RouteType::EventPage { link, .. } => {
                self.widget = Box::new(page::StreamPage::new_from_link(
                    link.clone(),
                    self.render_state.clone(),
                ));
            }
            RouteType::LoginPage => {
                self.widget = Box::new(page::LoginPage::new());
            }
            RouteType::ProfilePage { link } => {
                self.widget = Box::new(page::ProfilePage::new(
                    link.id.as_bytes().try_into().unwrap(),
                ));
            }
            RouteType::CategoriesPage => {
                self.widget = Box::new(page::CategoriesPage::new());
            }
            RouteType::CategoryPage { id } => {
                self.widget = Box::new(page::ListingPage::new(page::Listing::Category(id.clone())));
            }
            RouteType::HashtagPage { id } => {
                self.widget = Box::new(page::ListingPage::new(page::Listing::Hashtag(id.clone())));
            }
            RouteType::VideosPage => {
                self.widget = Box::new(page::ListingPage::new(page::Listing::Videos));
            }
            RouteType::ShortsPage => {
                self.widget = Box::new(page::ListingPage::new(page::Listing::Shorts));
            }
            RouteType::SearchPage { query } => {
                self.widget = Box::new(page::SearchPage::new(query.clone()));
            }
            RouteType::SettingsPage { section } => {
                self.widget = Box::new(page::SettingsPage::new(*section));
            }
            RouteType::Action(_) => unreachable!(),
        }
    }

    fn go_back(&mut self) {
        if let Some(prev) = back_target(self.widget.exit_fullscreen(), &mut self.history) {
            self.open_route(prev);
        }
    }

    fn poll_deep_links(&mut self, ctx: &AppContext<'_>) {
        let mut link = self
            .startup_url
            .take()
            .and_then(|u| crate::deep_link::parse(&u));
        if link.is_none() && self.last_link_poll.elapsed() >= std::time::Duration::from_millis(500) {
            self.last_link_poll = std::time::Instant::now();
            link = crate::deep_link::take_pending(&ctx.path.path(notedeck::DataPathType::Setting));
        }
        match link {
            Some(crate::deep_link::DeepLink::Stream(link)) => {
                self.widget = Box::new(page::StreamPage::new_from_link(
                    link.clone(),
                    self.render_state.clone(),
                ));
                self.push_current();
                self.current = RouteType::EventPage { link, event: None };
            }
            Some(crate::deep_link::DeepLink::Profile(pk)) => {
                self.widget = Box::new(page::ProfilePage::new(pk));
                self.push_current();
                self.current = RouteType::ProfilePage {
                    link: crate::link::NostrLink::profile(&pk),
                };
            }
            Some(crate::deep_link::DeepLink::WalletReturn) => crate::nwa::signal_return(),
            None => {}
        }
    }

    fn setup_fonts(&mut self, ctx: &egui::Context) {
        if self.fonts_set {
            return;
        }
        self.fonts_set = true;
        crate::theme::install_fonts(ctx);
        ctx.add_font_provider(std::sync::Arc::new(crate::font_fallback::CjkPunctuation::default()));
    }
}

fn back_target<T>(exited_fullscreen: bool, history: &mut Vec<T>) -> Option<T> {
    if exited_fullscreen {
        None
    } else {
        history.pop()
    }
}

fn startup_url() -> Option<String> {
    std::env::var("ZAP_STREAM_URL").ok().or_else(|| {
        std::env::args()
            .skip(1)
            .find(|a| crate::deep_link::parse(a).is_some())
    })
}

fn startup_link_from_env() -> Option<crate::link::NostrLink> {
    let v = std::env::var("ZAP_STREAM_OPEN").ok()?;
    let (author, d) = v.split_once(':')?;
    let author: [u8; 32] = nostr::prelude::hex::decode(author).ok()?.try_into().ok()?;
    Some(crate::link::NostrLink::new(
        crate::link::NostrLinkType::Coordinate,
        crate::link::IdOrStr::Str(d.to_string()),
        Some(30_311),
        Some(author),
        vec![],
    ))
}

fn profiles_sub_identity() -> ScopedSubIdentity {
    ScopedSubIdentity::account(
        SubOwnerKey::new("zap-stream-profiles"),
        SubKey::new("profiles"),
    )
}

impl notedeck::App for ZapStreamApp {
    fn render(&mut self, ctx: &mut AppContext<'_>, ui: &mut Ui) -> AppResponse {
        self.setup_fonts(ui.ctx());
        crate::android_window::pump(ui.ctx());
        self.load_locale(ctx);
        crate::theme::apply_style(ui.ctx());
        if let Some(s) = self.frame_stats.as_mut() {
            s.tick();
        }
        if let Some((step, at)) = crate::frame_stats::debug_scroll_step() {
            ui.ctx().input_mut(|i| {
                i.events.push(egui::Event::PointerMoved(egui::pos2(at[0], at[1])));
                i.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, step),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                })
            });
            ui.ctx().request_repaint();
        }

        if let Ok(v) = std::env::var("ZAP_STREAM_PAGE") {
            if self.startup_page.is_none() {
                self.startup_page = Some(v);
            }
            std::env::remove_var("ZAP_STREAM_PAGE");
        }
        if let Ok(nsec) = std::env::var("ZAP_STREAM_NSEC") {
            if let Some(kp) = crate::route::parse_login_key(&nsec) {
                let pk = kp.pubkey;
                ctx.accounts.add_account(kp);
                ctx.select_account(&pk);
            }
            std::env::remove_var("ZAP_STREAM_NSEC");
        }
        if let Some(page_name) = self.startup_page.take() {
            match page_name.as_str() {
                "login" => {
                    self.widget = Box::new(page::LoginPage::new());
                    self.current = RouteType::LoginPage;
                }
                "categories" => {
                    self.widget = Box::new(page::CategoriesPage::new());
                    self.current = RouteType::CategoriesPage;
                }
                "videos" => {
                    self.widget = Box::new(page::ListingPage::new(page::Listing::Videos));
                    self.current = RouteType::VideosPage;
                }
                "shorts" => {
                    self.widget = Box::new(page::ListingPage::new(page::Listing::Shorts));
                    self.current = RouteType::ShortsPage;
                }
                other => {
                    if let Some(id) = other.strip_prefix("category:").filter(|_| cfg!(debug_assertions)) {
                        self.widget = Box::new(page::ListingPage::new(page::Listing::Category(
                            id.to_string(),
                        )));
                        self.current = RouteType::CategoryPage { id: id.to_string() };
                    }
                    if let Some(id) = other.strip_prefix("hashtag:") {
                        self.widget = Box::new(page::ListingPage::new(page::Listing::Hashtag(
                            id.to_string(),
                        )));
                        self.current = RouteType::HashtagPage { id: id.to_string() };
                    }
                    if let Some(sec) = other.strip_prefix("settings") {
                        let section = match sec {
                            ":profile" => SettingsSection::Profile,
                            ":wallet" => SettingsSection::Wallet,
                            ":relays" => SettingsSection::Relays,
                            ":live" => SettingsSection::Broadcast,
                            _ => SettingsSection::Menu,
                        };
                        self.widget = Box::new(page::SettingsPage::new(section));
                        self.current = RouteType::SettingsPage { section };
                    }
                    if let Some(id) = other.strip_prefix("category:") {
                        self.widget = Box::new(page::ListingPage::new(page::Listing::Category(
                            id.to_string(),
                        )));
                        self.current = RouteType::CategoryPage { id: id.to_string() };
                    }
                    if let Some(q) = other.strip_prefix("search:") {
                        self.widget = Box::new(page::SearchPage::new(q.to_string()));
                        self.current = RouteType::SearchPage { query: q.to_string() };
                    }
                    if let Some(hex) = other.strip_prefix("profile:") {
                        if let Ok(pk) = nostr::prelude::hex::decode(hex) {
                            if let Ok(pk) = <[u8; 32]>::try_from(pk.as_slice()) {
                                let link = crate::link::NostrLink::profile(&pk);
                                self.widget = Box::new(page::ProfilePage::new(pk));
                                self.current = RouteType::ProfilePage { link };
                            }
                        }
                    }
                }
            }
        }

        if let Some(link) = self.startup_link.take() {
            self.widget = Box::new(page::StreamPage::new_from_link(
                link.clone(),
                self.render_state.clone(),
            ));
            self.current = RouteType::EventPage { link, event: None };
        }

        self.poll_deep_links(ctx);

        let back_pressed = ui.ctx().input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::BrowserBack)
                || i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)
        });
        if back_pressed {
            self.go_back();
        }

        let released = crate::sub::take_released();
        if !released.is_empty() {
            let mut api = ctx.remote.scoped_subs(ctx.accounts);
            for owner in released {
                api.drop_owner(owner);
            }
        }

        while let Ok(r) = self.routes_rx.try_recv() {
            if let RouteType::Action(a) = r {
                match a {
                    RouteAction::DemandProfile(p) => {
                        self.profiles.demand(p);
                    }
                    RouteAction::Back => {
                        self.go_back();
                    }
                    RouteAction::SetLocale(code) => {
                        crate::i18n::set_locale(&code);
                        let dir = ctx.path.path(notedeck::DataPathType::Setting);
                        crate::i18n::save(&dir, &code);
                    }
                    RouteAction::SelectAccount(pk) => {
                        ctx.select_account(&pk);
                    }
                    RouteAction::Logout(pk) => {
                        let dir = ctx.path.path(notedeck::DataPathType::Setting);
                        crate::signer::forget_amber(&dir, &pk.hex());
                        crate::bunker::forget_bunker(&dir, &pk.hex());
                        ctx.remove_account(&pk);
                        self.history.clear();
                        self.open_route(RouteType::HomePage);
                    }
                }
            } else {
                if r != self.current {
                    self.push_current();
                }
                self.open_route(r);
            }
        }

        let (connected, connecting) = crate::relay_health::count(ctx);
        let health = self
            .relay_health
            .observe(connected, connecting, std::time::Instant::now());
        if health != crate::relay_health::Health::Online {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }

        let app_frame = egui::containers::Frame::default().outer_margin(self.frame_margin());
        let egui_ctx = ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(app_frame)
            .show(ui, |ui| {
                let tx = Transaction::new(ctx.ndb).expect("transaction");
                let wide = ui.available_width() >= crate::theme::WIDE_BREAKPOINT;
                let active = match &self.current {
                    RouteType::HomePage => "streams",
                    RouteType::VideosPage => "videos",
                    RouteType::ShortsPage => "shorts",
                    RouteType::CategoriesPage | RouteType::CategoryPage { .. } => "categories",
                    _ => "",
                };
                ui.scope_builder(egui::UiBuilder::new().layout(egui::Layout::top_down(crate::bidi::text_align())), |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    let mut svc = RouteServices::new(
                        egui_ctx,
                        &tx,
                        ctx,
                        self.routes_tx.clone(),
                        &mut self.fetch,
                    );
                    crate::signer::set_repaint(&svc.egui);
                    svc.flush_signed();
                    {
                        let _t = crate::frame_stats::Section::start("header");
                        Header::new(!self.history.is_empty()).render(ui, &mut svc, &tx);
                    }
                    crate::widgets::relay_banner(ui, health);
                    {
                        let _t = crate::frame_stats::Section::start("update");
                        if let Err(e) = self.widget.update(&mut svc) {
                            error!("{}", e);
                        }
                    }
                    ui.horizontal_top(|ui| {
                        if wide {
                            ui.allocate_ui_with_layout(
                                egui::vec2(crate::theme::SIDEBAR_WIDTH, ui.available_height()),
                                egui::Layout::top_down(crate::bidi::text_align()),
                                |ui| {
                                    egui::Frame::new()
                                        .inner_margin(egui::Margin::symmetric(12, 12))
                                        .show(ui, |ui| {
                                            Sidebar { active }.render(ui, &mut svc);
                                        });
                                },
                            );
                        }
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: if wide { 32 } else { 0 },
                                right: if wide { 32 } else { 0 },
                                top: 0,
                                bottom: 0,
                            })
                            .show(ui, |ui| {
                                ui.scope_builder(egui::UiBuilder::new().layout(egui::Layout::top_down(crate::bidi::text_align())), |ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                                    let _t = crate::frame_stats::Section::start("page");
                                    self.widget.render(ui, &mut svc);
                                });
                            });
                    });
                    crate::widgets::report::show(ui, &mut svc);
                });
            });

        let profiles = self.profiles.next();
        if !profiles.is_empty() {
            info!("Profiles: {:?}", profiles);
            let config = SubConfig::builder(vec![Filter::new()
                .kinds([0])
                .authors(&profiles)
                .build()])
            .accounts_read_important()
            .with_explicit_relays(
                crate::relays::DEFAULT_RELAYS
                    .iter()
                    .filter_map(|u| enostr::NormRelayUrl::new(u).ok()),
                notedeck::SubRelayPolicy::accounts_read_important(),
            )
            .build();
            ctx.remote
                .scoped_subs(ctx.accounts)
                .set_sub(profiles_sub_identity(), config);
        }

        if let Some(s) = self.frame_stats.as_mut() {
            s.end();
        }
        AppResponse::none()
    }
}

impl ZapStreamApp {
    fn frame_margin(&self) -> Margin {
        let i = crate::safe_area::insets();
        Margin {
            left: i.left.round() as i8,
            right: i.right.round() as i8,
            top: i.top.round() as i8,
            bottom: i.bottom.round() as i8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::back_target;

    #[test]
    fn back_exits_fullscreen_before_popping_history() {
        let mut history = vec![1, 2];
        assert_eq!(back_target(true, &mut history), None);
        assert_eq!(history, vec![1, 2]);
        assert_eq!(back_target(false, &mut history), Some(2));
        assert_eq!(history, vec![1]);
    }
}
