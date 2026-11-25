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

pub const MAX_PROFILES: usize = 12;

pub fn rank_streams<T>(mut items: Vec<(u32, u64, T)>) -> Vec<T> {
    items.retain(|(s, _, _)| *s > 0);
    items.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    items.into_iter().map(|(_, _, t)| t).collect()
}

pub fn rank_profiles<T>(mut items: Vec<(u32, T)>, limit: usize) -> Vec<T> {
    items.retain(|(s, _)| *s > 0);
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.into_iter().take(limit).map(|(_, t)| t).collect()
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

                        let mut scored: Vec<(u32, u64, Note)> = Vec::new();
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
                                scored.push((score, n.created_at(), n));
                            }
                        }
                        let events: Vec<Note> = rank_streams(scored);
                        Self::heading(ui, crate::i18n::t("nav.streams"));
                        if events.is_empty() {
                            ui.label(theme::medium(crate::i18n::t("listing.empty"), 15.0).color(MUTED));
                        }
                        StreamList::new(
                            Id::new("search-streams"),
                            NotesView::from_vec(events.iter().collect()),
                            None::<RichText>,
                        )
                        .render(ui, services);

                        let profiles: Vec<[u8; 32]> = rank_profiles(
                            services
                                .ctx
                                .ndb
                                .search_profile(services.tx, &self.query, 60)
                                .map(|v| v.into_iter().copied().collect::<Vec<[u8; 32]>>())
                                .unwrap_or_default()
                                .into_iter()
                                .map(|pk| {
                                    let score = services
                                        .profile(&pk)
                                        .map(|p| {
                                            profile_relevance(
                                                &[p.display_name().unwrap_or(""), p.name().unwrap_or("")],
                                                &self.query,
                                            )
                                        })
                                        .unwrap_or(0);
                                    (score, pk)
                                })
                                .collect(),
                            MAX_PROFILES,
                        );
                        if !profiles.is_empty() {
                            Self::heading(ui, crate::i18n::t("search.profiles"));
                            for pk in profiles {
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
    fn streams_sort_by_score_then_newest_and_drop_zero() {
        let ranked = rank_streams(vec![(10, 900, "summary"), (0, 999, "none"), (70, 1, "old title"), (70, 5, "new title"), (40, 3, "host")]);
        assert_eq!(ranked, vec!["new title", "old title", "host", "summary"]);
    }

    #[test]
    fn tag_beats_host_beats_summary() {
        let tags = vec!["lofi".to_string()];
        let none: Vec<String> = vec![];
        let tag = relevance("x", "", &tags, "bob", "lofi");
        let host = relevance("x", "", &none, "lofi", "lofi");
        let summary = relevance("x", "lofi", &none, "bob", "lofi");
        assert!(tag > host && host > summary);
    }

    #[test]
    fn profiles_exact_then_prefix_then_substring() {
        let q = "goat";
        let ranked = rank_profiles(
            vec![
                (profile_relevance(&["the goat"], q), "sub"),
                (profile_relevance(&["goatee"], q), "prefix"),
                (profile_relevance(&["GOAT"], q), "exact"),
                (profile_relevance(&["gunson"], q), "none"),
            ],
            MAX_PROFILES,
        );
        assert_eq!(ranked, vec!["exact", "prefix", "sub"]);
    }

    #[test]
    fn profiles_cut_after_sorting() {
        let mut items: Vec<(u32, usize)> = (0..13).map(|i| (50, i)).collect();
        items[12].0 = 100;
        let ranked = rank_profiles(items, MAX_PROFILES);
        assert_eq!(ranked.len(), MAX_PROFILES);
        assert_eq!(ranked[0], 12);
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
