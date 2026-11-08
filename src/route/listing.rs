use crate::note_ref::NoteRef;
use crate::note_view::NotesView;
use crate::route::{RouteServices, RouteType};
use crate::stream_info::StreamInfo;
use crate::sub::SubRef;
use crate::theme::{self, MUTED, SURFACE, TEXT};
use crate::widgets::{sub_or_poll, NostrWidget, StreamList};
use crate::widgets::{Avatar, Pill};
use crate::zap::Zap;
use egui::{CursorIcon, Frame, Id, Margin, Response, RichText, ScrollArea, Sense, Ui};
use nostrdb::{Filter, Note};
use std::collections::HashSet;

const TOP_ZAPPED_LIMIT: usize = 5;

pub fn category_genres(tag: &str) -> &'static [&'static str] {
    match tag.to_lowercase().as_str() {
        "irl" => &["irl"],
        "gaming" => &["gaming"],
        "music" => &["music", "raido"],
        "talk" => &["talk", "just-chatting"],
        "art" => &["art"],
        "gambling" => &["gambling", "casino", "slots"],
        "science" => &["science", "tech", "technology"],
        _ => &[],
    }
}

pub fn category_cover(tag: &str) -> Option<(&'static str, &'static [u8])> {
    match tag.to_lowercase().as_str() {
        "irl" => Some(("bytes://category-irl.jpeg", include_bytes!("../../assets/category/irl.jpeg"))),
        "gaming" => Some(("bytes://category-gaming.jpeg", include_bytes!("../../assets/category/gaming.jpeg"))),
        "music" => Some(("bytes://category-music.jpeg", include_bytes!("../../assets/category/music.jpeg"))),
        "talk" => Some(("bytes://category-talk.jpeg", include_bytes!("../../assets/category/talk.jpeg"))),
        "art" => Some(("bytes://category-art.jpeg", include_bytes!("../../assets/category/art.jpeg"))),
        _ => None,
    }
}

const ZAP_RECEIPT_LIMIT: u64 = 5_000;

fn zap_filter(stream_keys: &[String]) -> Filter {
    let a_tags = stream_a_tags(stream_keys);
    Filter::new()
        .kinds([9_735])
        .tags(a_tags.iter().map(|s| s.as_str()), 'a')
        .limit(ZAP_RECEIPT_LIMIT)
        .build()
}

fn stream_a_tags(streams: &[String]) -> Vec<String> {
    streams.iter().map(|k| format!("30311:{k}")).collect()
}

pub const CATEGORIES: &[(&str, &str)] = &[
    ("irl", "IRL"),
    ("gaming", "Gaming"),
    ("music", "Music"),
    ("talk", "Talk"),
    ("art", "Art"),
    ("gambling", "Gambling"),
    ("science", "Science & Technology"),
    ("news", "News"),
    ("sports", "Sports"),
    ("education", "Education"),
];

pub fn category_name(tag: &str) -> Option<&'static str> {
    CATEGORIES
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(tag))
        .map(|(t, n)| category_label(t, n))
}

fn category_label(tag: &'static str, fallback: &'static str) -> &'static str {
    match tag {
        "irl" => crate::i18n::t("home.tab.irl"),
        "gaming" => crate::i18n::t("home.tab.gaming"),
        "music" => crate::i18n::t("home.tab.music"),
        "talk" => crate::i18n::t("home.tab.talk"),
        "art" => crate::i18n::t("home.tab.art"),
        "gambling" => crate::i18n::t("home.tab.gambling"),
        "science" => crate::i18n::t("home.tab.science"),
        "news" => crate::i18n::t("category.news"),
        "sports" => crate::i18n::t("category.sports"),
        "education" => crate::i18n::t("category.education"),
        _ => fallback,
    }
}

#[derive(Clone, PartialEq)]
pub enum Listing {
    Category(String),
    Hashtag(String),
    Videos,
    Shorts,
}

impl Listing {
    fn title(&self) -> String {
        match self {
            Listing::Category(c) => category_name(c).map(str::to_string).unwrap_or(c.clone()),
            Listing::Hashtag(t) => format!("#{}", t),
            Listing::Videos => crate::i18n::t("nav.videos").to_string(),
            Listing::Shorts => crate::i18n::t("nav.shorts").to_string(),
        }
    }

    fn id(&self) -> String {
        match self {
            Listing::Category(c) => format!("category-{}", c),
            Listing::Hashtag(t) => format!("hashtag-{}", t),
            Listing::Videos => "videos".to_string(),
            Listing::Shorts => "shorts".to_string(),
        }
    }

    fn filters(&self) -> Vec<Filter> {
        match self {
            Listing::Category(t) | Listing::Hashtag(t) => vec![Filter::new()
                .kinds([30_311])
                .tags([t.to_lowercase().as_str()], 't')
                .limit(100)
                .build()],
            Listing::Videos => vec![Filter::new().kinds([30_311]).limit(200).build()],
            Listing::Shorts => vec![Filter::new()
                .kinds([30_311])
                .tags(["shorts"], 't')
                .limit(100)
                .build()],
        }
    }

    fn keep(&self, note: &Note<'_>) -> bool {
        match self {
            Listing::Videos => note.recording().map(|r| !r.is_empty()).unwrap_or(false),
            _ => true,
        }
    }
}

pub struct ListingPage {
    kind: Listing,
    events: HashSet<NoteRef>,
    sub: Option<SubRef>,
    zaps: HashSet<NoteRef>,
    zap_sub: Option<SubRef>,
    zap_streams: usize,
}

impl ListingPage {
    pub fn new(kind: Listing) -> Self {
        Self {
            kind,
            events: HashSet::new(),
            sub: None,
            zaps: HashSet::new(),
            zap_sub: None,
            zap_streams: 0,
        }
    }

    fn category_header(&self, ui: &mut Ui, services: &mut RouteServices<'_, '_>, wide: bool) {
        let Listing::Category(tag) = &self.kind else {
            ui.label(theme::display(self.kind.title(), if wide { 40.0 } else { 30.0 }));
            return;
        };
        let top = self.top_zapped(services);
        let genres = category_genres(tag);
        let cover = category_cover(tag);
        let text = |ui: &mut Ui, services: &mut RouteServices<'_, '_>| {
            crate::bidi::vertical(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(theme::display(self.kind.title(), if wide { 40.0 } else { 30.0 }));
                if !genres.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for g in genres {
                            ui.add(Pill::new(g));
                        }
                    });
                }
                if !top.is_empty() {
                    ui.label(
                        theme::medium(crate::i18n::t("category.most_zapped"), 14.0).color(theme::ZAP),
                    );
                    let max = ui.available_width();
                    egui::ScrollArea::horizontal()
                        .id_salt("category-top-zapped")
                        .max_width(max)
                        .auto_shrink([false, true])
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        ui.spacing_mut().item_spacing.x = 16.0;
                        for (pk, sats) in &top {
                            let profile = services.profile(pk);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                Avatar::from_profile(&profile).size(32.0).render(ui, services.ctx);
                                crate::bidi::vertical(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(
                                        theme::semibold(crate::zap::format_sats(*sats as f32), 14.0)
                                            .color(theme::ZAP),
                                    );
                                    let name = profile
                                        .and_then(|p| p.name())
                                        .unwrap_or(crate::i18n::t("common.anon"))
                                        .to_string();
                                    ui.label(theme::medium(name, 13.0).color(MUTED));
                                });
                            });
                        }
                    });
                        });
                }
            });
        };
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            if let Some((uri, bytes)) = cover {
                ui.add(
                    egui::Image::from_bytes(uri, bytes)
                        .fit_to_exact_size(egui::vec2(if wide { 150.0 } else { 96.0 }, if wide { 200.0 } else { 128.0 }))
                        .corner_radius(theme::ROUNDING_DEFAULT),
                );
            }
            text(ui, services);
        });
    }

    fn top_zapped(&self, services: &mut RouteServices<'_, '_>) -> Vec<([u8; 32], u64)> {
        let zaps: Vec<Zap> = self
            .zaps
            .iter()
            .filter_map(|r| services.ctx.ndb.get_note_by_key(services.tx, r.key).ok())
            .filter_map(|n| Zap::from_receipt(n).ok())
            .collect();
        crate::zap::top_receivers(&zaps, TOP_ZAPPED_LIMIT)
    }

    fn stream_keys(&self, services: &mut RouteServices<'_, '_>) -> Vec<String> {
        let mut keys: Vec<String> = self
            .events
            .iter()
            .filter_map(|e| services.ctx.ndb.get_note_by_key(services.tx, e.key).ok())
            .map(|n| n.stream_key())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    fn update_zaps(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        if !matches!(self.kind, Listing::Category(_)) {
            return Ok(());
        }
        let keys = self.stream_keys(services);
        if keys.is_empty() {
            return Ok(());
        }
        if keys.len() != self.zap_streams {
            self.zap_streams = keys.len();
            self.zap_sub = None;
        }
        sub_or_poll(services.ctx, services.tx, &mut self.zaps, &mut self.zap_sub, vec![zap_filter(&keys)])
    }
}

impl NostrWidget for ListingPage {
    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        let wide = ui.available_width() >= theme::WIDE_BREAKPOINT;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(if wide { 0 } else { 16 }, 16))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(16.0, 14.0);
                        self.category_header(ui, services, wide);
                        let events: Vec<Note> = self
                            .events
                            .iter()
                            .filter_map(|e| services.ctx.ndb.get_note_by_key(services.tx, e.key).ok())
                            .filter(|n| self.kind.keep(n))
                            .collect();
                        if events.is_empty() {
                            ui.label(theme::medium(crate::i18n::t("listing.empty"), 15.0).color(MUTED));
                        }
                        StreamList::new(
                            Id::new(self.kind.id()),
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
            &mut self.events,
            &mut self.sub,
            self.kind.filters(),
        )?;
        self.update_zaps(services)
    }
}

pub struct CategoriesPage {
    events: HashSet<NoteRef>,
    sub: Option<SubRef>,
}

impl CategoriesPage {
    pub fn new() -> Self {
        Self {
            events: HashSet::new(),
            sub: None,
        }
    }
}

impl NostrWidget for CategoriesPage {
    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        let wide = ui.available_width() >= theme::WIDE_BREAKPOINT;
        let mut counts = vec![0usize; CATEGORIES.len()];
        for r in &self.events {
            if let Ok(n) = services.ctx.ndb.get_note_by_key(services.tx, r.key) {
                let tags = n.hashtags();
                for (i, (t, _)) in CATEGORIES.iter().enumerate() {
                    if tags.iter().any(|h| h == t) {
                        counts[i] += 1;
                    }
                }
            }
        }
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(if wide { 0 } else { 16 }, 16))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(16.0, 16.0);
                        ui.label(theme::display(crate::i18n::t("nav.categories"), if wide { 40.0 } else { 30.0 }));
                        let cols = if ui.available_width() < 560.0 { 2 } else { 4 };
                        let w = (ui.available_width() - 16.0 * (cols - 1) as f32) / cols as f32;
                        for (row, chunk) in CATEGORIES.chunks(cols).enumerate() {
                            ui.horizontal(|ui| {
                                for (i, (tag, name)) in chunk.iter().enumerate() {
                                    let count = counts[row * cols + i];
                                    let r = Frame::new()
                                        .fill(SURFACE)
                                        .corner_radius(12)
                                        .inner_margin(Margin::same(16))
                                        .show(ui, |ui| {
                                            ui.set_width(w - 32.0);
                                            ui.set_max_width(w - 32.0);
                                            ui.set_height(56.0);
                                            crate::bidi::vertical(ui, |ui| {
                                                ui.label(theme::display(category_label(tag, name), 20.0).color(TEXT));
                                                let noun = if count == 1 { crate::i18n::t("listing.stream_one") } else { crate::i18n::t("listing.stream_many") };
                                                ui.label(
                                                    theme::medium(format!("{} {}", count, noun), 14.0)
                                                        .color(MUTED),
                                                );
                                            });
                                        })
                                        .response
                                        .interact(Sense::click())
                                        .on_hover_and_drag_cursor(CursorIcon::PointingHand);
                                    if r.clicked() {
                                        services.navigate(RouteType::CategoryPage {
                                            id: tag.to_string(),
                                        });
                                    }
                                }
                            });
                        }
                    });
            });
        ui.response()
    }

    fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.events,
            &mut self.sub,
            vec![Filter::new().kinds([30_311]).limit(300).build()],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tags_prefix_stream_keys_with_kind() {
        let keys = vec!["abc:one".to_string(), "def:two".to_string()];
        assert_eq!(stream_a_tags(&keys), vec!["30311:abc:one", "30311:def:two"]);
    }

    #[test]
    fn zap_filter_targets_receipts_for_stream_a_tags() {
        let keys = vec!["abc:one".to_string()];
        let json = zap_filter(&keys).json().unwrap();
        assert!(json.contains("\"kinds\":[9735]"), "{json}");
        assert!(json.contains("\"#a\":[\"30311:abc:one\"]"), "{json}");
        assert!(json.contains("\"limit\":5000"), "{json}");
    }
}
