use crate::note_ref::NoteRef;
use crate::note_view::NotesView;
use crate::route::listing::CATEGORIES;
use crate::route::{RouteServices, RouteType};
use crate::stream_info::StreamInfo;
use crate::sub::SubRef;
use crate::theme::{self, BORDER, MUTED, SURFACE_LOW};
use crate::widgets::{sub_or_poll, Avatar, NostrWidget, StreamList};
use egui::{
    CursorIcon, Frame, Id, Margin, Response, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui,
};
use nostrdb::{Filter, Note};
use std::collections::HashSet;

pub fn matches_query(title: &str, summary: &str, tags: &[String], host_name: &str, q: &str) -> bool {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return false;
    }
    title.to_lowercase().contains(&q)
        || summary.to_lowercase().contains(&q)
        || host_name.to_lowercase().contains(&q)
        || tags.iter().any(|t| t.contains(&q))
}

fn text_score(text: &str, q: &str, exact: u32, prefix: u32, inner: u32) -> u32 {
    let t = text.to_lowercase();
    if t == q {
        exact
    } else if t.starts_with(q) {
        prefix
    } else if t.contains(q) {
        inner
    } else {
        0
    }
}

pub fn relevance(title: &str, summary: &str, tags: &[String], host_name: &str, q: &str) -> u32 {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return 0;
    }
    let tag = tags
        .iter()
        .map(|t| text_score(t, &q, 80, 50, 30))
        .max()
        .unwrap_or(0);
    [
        text_score(title, &q, 100, 90, 70),
        tag,
        text_score(host_name, &q, 60, 55, 40),
        text_score(summary, &q, 10, 10, 10),
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

pub fn profile_relevance(names: &[&str], q: &str) -> u32 {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return 0;
    }
    names
        .iter()
        .map(|n| text_score(n, &q, 100, 80, 50))
        .max()
        .unwrap_or(0)
}

pub fn matching_categories(q: &str) -> Vec<(&'static str, &'static str)> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    CATEGORIES
        .iter()
        .map(|(t, _)| (*t, crate::route::listing::category_name(t).unwrap_or(t)))
        .filter(|(t, n)| t.contains(&q) || n.to_lowercase().contains(&q))
        .collect()
}

pub struct SearchPage {
    input: String,
    query: String,
    events: HashSet<NoteRef>,
    sub: Option<SubRef>,
}

impl SearchPage {
    pub fn new(query: String) -> Self {
        Self {
            input: query.clone(),
            query,
            events: HashSet::new(),
            sub: None,
        }
    }

    fn heading(ui: &mut Ui, text: &str) {
        ui.label(theme::semibold(text, 18.0));
    }
}

impl NostrWidget for SearchPage {
    fn render(&mut self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) -> Response {
        let wide = ui.available_width() >= theme::WIDE_BREAKPOINT;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(if wide { 0 } else { 16 }, 16))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(16.0, 14.0);
                        let edit = Frame::new()
                            .fill(SURFACE_LOW)
                            .stroke(Stroke::new(1.0, BORDER))
                            .corner_radius(12)
                            .inner_margin(Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                let r = ui.add(
                                    TextEdit::singleline(&mut self.input)
                                        .horizontal_align(crate::bidi::text_align())
                                        .frame(egui::Frame::NONE)
                                        .hint_text(
                                            theme::semibold("Search streams, profiles, categories", theme::FONT_SIZE)
                                                .color(MUTED),
                                        )
                                        .desired_width(f32::INFINITY),
                                );
                                crate::android_window::paste_menu(&r);
                                r
                            })
                            .inner;
                        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.query = self.input.trim().to_string();
                        }

                        if self.query.is_empty() {
                            ui.label(theme::medium(crate::i18n::t("search.prompt"), 15.0).color(MUTED));
                            return;
                        }

                        let categories = matching_categories(&self.query);
                        if !categories.is_empty() {
                            Self::heading(ui, crate::i18n::t("nav.categories"));
                            ui.horizontal_wrapped(|ui| {
                                for (tag, name) in categories {
                                    let r = ui
                                        .add(
                                            egui::Label::new(theme::medium(name, 15.0))
                                                .sense(Sense::click()),
                                        )
                                        .on_hover_and_drag_cursor(CursorIcon::PointingHand);
                                    if r.clicked() {
                                        services.navigate(RouteType::CategoryPage {
                                            id: tag.to_string(),
                                        });
                                    }
                                }
                            });
                        }

                        let mut events: Vec<(u32, Note)> = Vec::new();
                        for r in &self.events {
                            if let Ok(n) = services.ctx.ndb.get_note_by_key(services.tx, r.key) {
                                let host = *n.host();
                                let host_name = services
                                    .profile(&host)
                                    .and_then(|p| p.name().map(str::to_string))
                                    .unwrap_or_default();
                                let score = relevance(
                                    n.title().unwrap_or(""),
                                    n.summary().unwrap_or(""),
                                    &n.hashtags(),
                                    &host_name,
                                    &self.query,
                                );
                                if score > 0 {
                                    events.push((score, n));
                                }
                            }
                        }
                        events.sort_by(|a, b| {
                            b.0.cmp(&a.0)
                                .then(b.1.created_at().cmp(&a.1.created_at()))
                        });
                        Self::heading(ui, crate::i18n::t("nav.streams"));
                        if events.is_empty() {
                            ui.label(theme::medium(crate::i18n::t("listing.empty"), 15.0).color(MUTED));
                        }
                        StreamList::new(
                            Id::new("search-streams"),
                            NotesView::from_vec(events.iter().map(|(_, n)| n).collect()),
                            None::<RichText>,
                        )
                        .render(ui, services);

                        let mut profiles: Vec<(u32, [u8; 32])> = services
                            .ctx
                            .ndb
                            .search_profile(services.tx, &self.query, 60)
                            .map(|v| v.into_iter().copied().collect::<Vec<[u8; 32]>>())
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|pk| {
                                let p = services.profile(&pk)?;
                                let score = profile_relevance(
                                    &[p.display_name().unwrap_or(""), p.name().unwrap_or("")],
                                    &self.query,
                                );
                                (score > 0).then_some((score, pk))
                            })
                            .collect();
                        profiles.sort_by(|a, b| b.0.cmp(&a.0));
                        profiles.truncate(12);
                        if !profiles.is_empty() {
                            Self::heading(ui, crate::i18n::t("search.profiles"));
                            for (_, pk) in profiles {
                                let profile = services.profile(&pk);
                                let r = ui
                                    .horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 10.0;
                                        Avatar::from_profile(&profile)
                                            .size(32.0)
                                            .render(ui, services.ctx);
                                        let name = profile
                                            .and_then(|p| p.display_name().or(p.name()))
                                            .unwrap_or("Nostrich")
                                            .to_string();
                                        ui.label(theme::semibold(name, 16.0));
                                    })
                                    .response
                                    .interact(Sense::click())
                                    .on_hover_and_drag_cursor(CursorIcon::PointingHand);
                                if r.clicked() {
                                    services.navigate(RouteType::ProfilePage {
                                        link: crate::link::NostrLink::profile(&pk),
                                    });
                                }
                            }
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
            vec![Filter::new().kinds([30_311]).limit(500).build()],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_title_tag_and_host() {
        let tags = vec!["gaming".to_string()];
        assert!(matches_query("Speedrun Night", "", &tags, "alice", "speed"));
        assert!(matches_query("x", "", &tags, "alice", "GAM"));
        assert!(matches_query("x", "", &tags, "Alice", "alic"));
        assert!(!matches_query("x", "y", &tags, "alice", "zzz"));
        assert!(!matches_query("x", "y", &tags, "alice", "  "));
    }

    #[test]
    fn title_match_outranks_summary_and_host() {
        let none: Vec<String> = vec![];
        let title = relevance("Lofi beats", "", &none, "bob", "lofi");
        let host = relevance("x", "", &none, "lofi bob", "lofi");
        let summary = relevance("x", "chill lofi", &none, "bob", "lofi");
        assert!(title > host && host > summary && summary > 0);
        assert_eq!(relevance("x", "y", &none, "bob", "lofi"), 0);
    }

    #[test]
    fn profiles_without_query_in_name_score_zero() {
        assert_eq!(profile_relevance(&["godsdeth", "Gunson"], "goat"), 0);
        assert!(profile_relevance(&["goat"], "goat") > profile_relevance(&["the goat"], "goat"));
    }

    #[test]
    fn categories_match_name_or_tag() {
        assert!(matching_categories("scien").iter().any(|(t, _)| *t == "science"));
        assert!(matching_categories("").is_empty());
    }
}
