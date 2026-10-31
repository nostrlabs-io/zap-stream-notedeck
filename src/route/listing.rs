use crate::note_ref::NoteRef;
use crate::note_view::NotesView;
use crate::route::{RouteServices, RouteType};
use crate::stream_info::StreamInfo;
use crate::sub::SubRef;
use crate::theme::{self, MUTED, SURFACE, TEXT};
use crate::widgets::{sub_or_poll, NostrWidget, StreamList};
use egui::{CursorIcon, Frame, Id, Margin, Response, RichText, ScrollArea, Sense, Ui};
use nostrdb::{Filter, Note};
use std::collections::HashSet;

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
        .map(|(_, n)| *n)
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
            Listing::Videos => "Videos".to_string(),
            Listing::Shorts => "Shorts".to_string(),
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
}

impl ListingPage {
    pub fn new(kind: Listing) -> Self {
        Self {
            kind,
            events: HashSet::new(),
            sub: None,
        }
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
                        ui.label(theme::display(self.kind.title(), if wide { 40.0 } else { 30.0 }));
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
        )
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
                        ui.label(theme::display("Categories", if wide { 40.0 } else { 30.0 }));
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
                                            ui.vertical(|ui| {
                                                ui.label(theme::display(*name, 20.0).color(TEXT));
                                                let noun = if count == 1 { "stream" } else { "streams" };
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
