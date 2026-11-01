use crate::link::NostrLink;
use anyhow::{anyhow, bail};
use egui::load::SizedTexture;
use egui::{Context, Id, Image, Ui, Vec2};
use ehttp::Response;
use enostr::RelayId;
use lnurl::lightning_address::LightningAddress;
use lnurl::pay::PayResponse;
use log::{info, warn};
use nostr::prelude::hex;
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
    Broadcast,
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
    mutes: std::cell::RefCell<Option<std::collections::HashSet<String>>>,
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
            mutes: std::cell::RefCell::new(None),
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
        let Some(profile) = self.profile(pubkey) else {
            return Ok(Poll::Pending);
        };
        let target = profile
            .lud16()
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

    pub fn write_live_chat_msg(
        &self,
        link: &NostrLink,
        msg: &str,
        emoji: Vec<Vec<String>>,
    ) -> Option<Event> {
        if msg.is_empty() {
            return None;
        }
        let key = self.current_account_keys()?;
        let mut builder = EventBuilder::new(Kind::LiveEventMessage, msg)
            .tag(Tag::parse(link.to_tag()).unwrap());
        for t in emoji {
            if let Ok(tag) = Tag::parse(t) {
                builder = builder.tag(tag);
            }
        }
        builder.sign_with_keys(&key).ok()
    }

    pub fn react(
        &mut self,
        target: &[u8; 32],
        author: &[u8; 32],
        content: &str,
        emoji_url: Option<&str>,
    ) {
        let Some(key) = self.current_account_keys() else {
            return;
        };
        let mut builder = EventBuilder::new(Kind::Reaction, content)
            .tag(Tag::parse(["e".to_string(), hex::encode(target)]).unwrap())
            .tag(Tag::parse(["p".to_string(), hex::encode(author)]).unwrap());
        if let (Some(url), Some(name)) = (emoji_url, content.strip_prefix(':').and_then(|c| c.strip_suffix(':'))) {
            builder = builder.tag(
                Tag::parse(["emoji".to_string(), name.to_string(), url.to_string()]).unwrap(),
            );
        }
        if let Ok(ev) = builder.sign_with_keys(&key) {
            self.broadcast_event(ev);
        }
    }

    pub fn timeout_user(&mut self, user: &[u8; 32], secs: u64) {
        let Some(key) = self.current_account_keys() else {
            return;
        };
        let expiry = nostr::Timestamp::now().as_u64() + secs;
        let builder = EventBuilder::new(Kind::Custom(1314), "")
            .tag(Tag::parse(["p".to_string(), hex::encode(user)]).unwrap())
            .tag(Tag::parse(["expiration".to_string(), expiry.to_string()]).unwrap());
        if let Ok(ev) = builder.sign_with_keys(&key) {
            self.broadcast_event(ev);
        }
    }

    pub fn me(&self) -> Option<[u8; 32]> {
        let account = self.ctx.accounts.get_selected_account();
        account
            .key
            .secret_key
            .as_ref()
            .map(|_| *account.key.pubkey.bytes())
    }

    pub fn can_sign(&self) -> bool {
        self.ctx
            .accounts
            .get_selected_account()
            .key
            .secret_key
            .is_some()
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

    pub fn own_mute_tags(&self) -> Vec<Vec<String>> {
        let me = *self.ctx.accounts.get_selected_account().key.pubkey.bytes();
        let filter = nostrdb::Filter::new().kinds([10_000]).authors(&[me]).limit(1).build();
        self.ctx
            .ndb
            .query(self.tx, &[filter], 1)
            .ok()
            .and_then(|r| r.into_iter().next())
            .map(|r| crate::contacts::tags_from_note(&r.note))
            .unwrap_or_default()
    }

    pub fn is_muted(&self, target: &[u8; 32]) -> bool {
        let mut cache = self.mutes.borrow_mut();
        let set = cache.get_or_insert_with(|| {
            self.own_mute_tags()
                .into_iter()
                .filter(|t| t.first().map(String::as_str) == Some("p"))
                .filter_map(|t| t.get(1).cloned())
                .collect()
        });
        set.contains(&nostr::prelude::hex::encode(target))
    }

    pub fn set_muted(&mut self, target: &[u8; 32], mute: bool) {
        let Some(keys) = self.current_account_keys() else {
            return;
        };
        let hex = nostr::prelude::hex::encode(target);
        let tags = crate::contacts::with_mute(self.own_mute_tags(), &hex, mute);
        let tags = tags.into_iter().filter_map(|t| Tag::parse(t).ok());
        if let Ok(ev) = EventBuilder::new(Kind::MuteList, "")
            .tags(tags)
            .sign_with_keys(&keys)
        {
            *self.mutes.borrow_mut() = None;
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
