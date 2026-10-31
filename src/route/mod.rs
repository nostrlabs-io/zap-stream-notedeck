use crate::link::NostrLink;
use anyhow::{anyhow, bail};
use egui::load::SizedTexture;
use egui::{Context, Id, Image, Ui, Vec2};
use ehttp::Response;
use enostr::RelayId;
use lnurl::lightning_address::LightningAddress;
use lnurl::pay::PayResponse;
use log::{info, warn};
use nostr::{serde_json, Event, EventBuilder, JsonUtil, Keys, Kind, Tag};
use nostrdb::{NdbProfile, NoteKey, Transaction};
use notedeck::media::AnimationMode;
use notedeck::{AppContext, ImageType, Images, MediaJobSender, PixelDimensions};
use poll_promise::Promise;
use std::collections::HashMap;
use std::sync::mpsc;
use std::task::Poll;

mod home;
pub mod listing;
mod login;
mod profile;
mod search;
mod settings;
mod stream;

pub use login::parse_key as parse_login_key;

pub mod page {
    pub use super::home::HomePage;
    pub use super::listing::{CategoriesPage, Listing, ListingPage};
    pub use super::login::LoginPage;
    pub use super::profile::ProfilePage;
    pub use super::search::SearchPage;
    pub use super::settings::SettingsPage;
    pub use super::stream::StreamPage;
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SettingsSection {
    Menu,
    Profile,
    Wallet,
    Relays,
}

#[derive(PartialEq)]
pub enum RouteType {
    HomePage,
    EventPage {
        link: NostrLink,
        event: Option<NoteKey>,
    },
    ProfilePage {
        link: NostrLink,
    },
    LoginPage,
    CategoriesPage,
    CategoryPage { id: String },
    HashtagPage { id: String },
    VideosPage,
    ShortsPage,
    SearchPage { query: String },
    SettingsPage { section: SettingsSection },

    // special kind for modifying route state
    Action(RouteAction),
}

#[derive(PartialEq)]
pub enum RouteAction {
    DemandProfile([u8; 32]),
    SelectAccount(enostr::Pubkey),
    Logout(enostr::Pubkey),
    SetLocale(String),
}

pub struct RouteServices<'a, 'ctx> {
    pub egui: Context,
    pub tx: &'a Transaction,
    pub ctx: &'a mut AppContext<'ctx>,

    router: mpsc::Sender<RouteType>,
    fetch: &'a mut HashMap<String, Promise<ehttp::Result<Response>>>,
}

impl<'a, 'ctx> RouteServices<'a, 'ctx> {
    pub fn new(
        egui: Context,
        tx: &'a Transaction,
        ctx: &'a mut AppContext<'ctx>,
        router: mpsc::Sender<RouteType>,
        fetch: &'a mut HashMap<String, Promise<ehttp::Result<Response>>>,
    ) -> Self {
        Self {
            egui,
            tx,
            ctx,
            router,
            fetch,
        }
    }

    pub fn navigate(&self, route: RouteType) {
        self.router.send(route).expect("route send failed");
        self.egui.request_repaint();
    }

    pub fn action(&self, route: RouteAction) {
        self.router
            .send(RouteType::Action(route))
            .expect("route send failed");
        self.egui.request_repaint();
    }

    pub fn broadcast_event(&mut self, event: Event) {
        let ev_json = event.as_json();
        if let Err(e) = self
            .ctx
            .ndb
            .process_event_with(&ev_json, nostrdb::IngestMetadata::new().client(true))
        {
            warn!("Failed to submit event {}", e);
        }
        let mut relays = self
            .ctx
            .accounts
            .selected_account_write_relays()
            .into_iter()
            .collect::<Vec<RelayId>>();
        for url in crate::relays::DEFAULT_RELAYS {
            if let Ok(u) = enostr::NormRelayUrl::new(url) {
                let id = RelayId::Websocket(u);
                if !relays.contains(&id) {
                    relays.push(id);
                }
            }
        }
        self.ctx
            .remote
            .publisher_explicit()
            .publish_event_json(ev_json, relays);
    }

    /// Load/Fetch profiles
    pub fn profile(&self, pk: &[u8; 32]) -> Option<NdbProfile<'a>> {
        let p = self
            .ctx
            .ndb
            .get_profile_by_pubkey(self.tx, pk)
            .map(|p| p.record().profile())
            .ok()
            .flatten();
        if p.is_none() {
            self.action(RouteAction::DemandProfile(*pk));
        }
        p
    }

    /// Create a poll_promise fetch
    pub fn fetch(&mut self, url: &str) -> Poll<&ehttp::Result<Response>> {
        if !self.fetch.contains_key(url) {
            let (sender, promise) = Promise::new();
            let request = ehttp::Request::get(url);
            let ctx = self.egui.clone();
            ehttp::fetch(request, move |response| {
                sender.send(response);
                ctx.request_repaint();
            });
            info!("Fetching {}", url);
            self.fetch.insert(url.to_string(), promise);
        }
        self.fetch.get(url).expect("fetch").poll()
    }

    pub fn fetch_lnurlp(&mut self, pubkey: &[u8; 32]) -> anyhow::Result<Poll<PayResponse>> {
        let target = self
            .profile(pubkey)
            .and_then(|p| p.lud16())
            .ok_or(anyhow!("No lightning address found"))?;

        let addr = LightningAddress::new(target)?;
        match self.fetch(&addr.lnurlp_url()) {
            Poll::Ready(Ok(r)) => {
                if r.ok {
                    let rsp: PayResponse = serde_json::from_slice(&r.bytes)?;
                    Ok(Poll::Ready(rsp))
                } else {
                    bail!("Invalid response code {}", r.status);
                }
            }
            Poll::Ready(Err(e)) => Err(anyhow!("{}", e)),
            Poll::Pending => Ok(Poll::Pending),
        }
    }

    pub fn write_live_chat_msg(&self, link: &NostrLink, msg: &str) -> Option<Event> {
        if msg.is_empty() {
            return None;
        }
        if let Some(key) = self.current_account_keys() {
            EventBuilder::new(Kind::LiveEventMessage, msg)
                .tag(Tag::parse(link.to_tag()).unwrap())
                .sign_with_keys(&key)
                .ok()
        } else {
            None
        }
    }

    pub fn can_sign(&self) -> bool {
        self.current_account_keys().is_some()
    }

    pub fn own_contact_tags(&self) -> Vec<Vec<String>> {
        let me = *self.ctx.accounts.get_selected_account().key.pubkey.bytes();
        let filter = nostrdb::Filter::new().kinds([3]).authors(&[me]).limit(1).build();
        self.ctx
            .ndb
            .query(self.tx, &[filter], 1)
            .ok()
            .and_then(|r| r.into_iter().next())
            .map(|r| crate::contacts::tags_from_note(&r.note))
            .unwrap_or_default()
    }

    pub fn set_following(&mut self, target: &[u8; 32], follow: bool) {
        let Some(keys) = self.current_account_keys() else {
            return;
        };
        let hex = nostr::prelude::hex::encode(target);
        let tags = crate::contacts::with_follow(self.own_contact_tags(), &hex, follow);
        let tags = tags.into_iter().filter_map(|t| Tag::parse(t).ok());
        if let Ok(ev) = EventBuilder::new(Kind::ContactList, "")
            .tags(tags)
            .sign_with_keys(&keys)
        {
            self.broadcast_event(ev);
        }
    }

    pub fn current_account_keys(&self) -> Option<Keys> {
        self.ctx
            .accounts
            .get_selected_account()
            .key
            .secret_key
            .as_ref()
            .map(|k| Keys::new(k.clone()))
    }

    /// Simple wrapper around egui temp data
    pub fn get<T: Clone + 'static>(&self, k: &str) -> Option<T> {
        let id = Id::new(k);
        self.egui.data(|d| d.get_temp(id))
    }

    /// Simple wrapper around egui temp data
    pub fn set<T: Clone + Send + Sync + 'static>(&mut self, k: &str, v: T) {
        self.egui.data_mut(|d| d.insert_temp(Id::new(k), v));
    }
}

pub fn image_from_cache<'a>(
    img_cache: &'a mut Images,
    jobs: &MediaJobSender,
    ui: &mut Ui,
    url: &str,
    size: Option<Vec2>,
) -> Option<Image<'a>> {
    if url.is_empty() {
        return None;
    }
    let img_type = match size {
        Some(s) => {
            let ppp = ui.ctx().pixels_per_point();
            ImageType::Content(Some(PixelDimensions {
                x: (s.x * ppp) as u32,
                y: (s.y * ppp) as u32,
            }))
        }
        None => ImageType::Content(None),
    };
    img_cache
        .latest_texture(jobs, ui, url, img_type, AnimationMode::NoAnimation)
        .map(|t| Image::new(SizedTexture::from_handle(t)))
}
