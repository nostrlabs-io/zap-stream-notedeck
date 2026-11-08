use crate::note_ref::NoteRef;
use crate::note_util::NoteUtil;
use crate::route::{image_from_cache, RouteServices};
use crate::sub::SubRef;
use crate::theme::{self, SURFACE};
use crate::widgets::sub_or_poll;
use egui::{Frame, Label, Margin, Sense, TextWrapMode, Ui};
use nostrdb::{Filter, Note};
use std::collections::HashSet;

pub const CARD_LIST_KIND: u64 = 17_777;
pub const CARD_KIND: u64 = 37_777;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub d: String,
    pub created_at: u64,
    pub title: Option<String>,
    pub image: Option<String>,
    pub link: Option<String>,
    pub content: String,
}

pub fn card_d_tags(list_tags: &[Vec<String>]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in list_tags {
        if t.first().map(String::as_str) != Some("a") {
            continue;
        }
        let Some(a) = t.get(1) else { continue };
        let d = a.splitn(3, ':').nth(2).unwrap_or("");
        if !d.is_empty() && !out.iter().any(|o| o == d) {
            out.push(d.to_string());
        }
    }
    out
}

pub fn card_from_tags(tags: &[Vec<String>], content: &str, created_at: u64) -> Card {
    let first = |k: &str| {
        tags.iter()
            .find(|t| t.first().map(String::as_str) == Some(k))
            .and_then(|t| t.get(1))
            .filter(|v| !v.is_empty())
            .cloned()
    };
    Card {
        d: first("d").unwrap_or_default(),
        created_at,
        title: first("title").or_else(|| first("subject")),
        image: first("image"),
        link: first("r"),
        content: content.to_string(),
    }
}

pub fn resolve_cards(order: &[String], cards: Vec<Card>) -> Vec<Card> {
    order
        .iter()
        .filter_map(|d| {
            cards
                .iter()
                .filter(|c| &c.d == d)
                .max_by_key(|c| c.created_at)
                .cloned()
        })
        .collect()
}

fn note_card(note: &Note<'_>) -> Card {
    card_from_tags(&note.tags_vec(), note.content(), note.created_at())
}

pub struct StreamCards {
    host: [u8; 32],
    lists: HashSet<NoteRef>,
    lists_sub: Option<SubRef>,
    cards: HashSet<NoteRef>,
    cards_sub: Option<SubRef>,
    subscribed_for: Vec<String>,
}

impl StreamCards {
    pub fn new(host: [u8; 32]) -> Self {
        Self {
            host,
            lists: HashSet::new(),
            lists_sub: None,
            cards: HashSet::new(),
            cards_sub: None,
            subscribed_for: Vec::new(),
        }
    }

    pub fn host(&self) -> &[u8; 32] {
        &self.host
    }

    fn order(&self, services: &mut RouteServices<'_, '_>) -> Vec<String> {
        let newest = self
            .lists
            .iter()
            .filter_map(|r| services.ctx.ndb.get_note_by_key(services.tx, r.key).ok())
            .max_by_key(|n| n.created_at());
        newest
            .map(|n| card_d_tags(&n.tags_vec()))
            .unwrap_or_default()
    }

    pub fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        let list_filter = Filter::new()
            .kinds([CARD_LIST_KIND])
            .authors([&self.host])
            .limit(1)
            .build();
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.lists,
            &mut self.lists_sub,
            vec![list_filter],
        )?;
        let order = self.order(services);
        if order != self.subscribed_for {
            if order.is_empty() {
                return Ok(());
            }
            self.cards_sub = None;
            self.cards.clear();
            let ds: Vec<&str> = order.iter().map(String::as_str).collect();
            let card_filter = Filter::new()
                .kinds([CARD_KIND])
                .authors([&self.host])
                .tags(ds, 'd')
                .build();
            self.subscribed_for = order;
            sub_or_poll(
                services.ctx,
                services.tx,
                &mut self.cards,
                &mut self.cards_sub,
                vec![card_filter],
            )?;
        } else if self.cards_sub.is_some() {
            sub_or_poll(
                services.ctx,
                services.tx,
                &mut self.cards,
                &mut self.cards_sub,
                vec![],
            )?;
        }
        Ok(())
    }

    pub fn render(&self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        let order = self.order(services);
        let found: Vec<Card> = self
            .cards
            .iter()
            .filter_map(|r| services.ctx.ndb.get_note_by_key(services.tx, r.key).ok())
            .filter(|n| n.pubkey() == &self.host)
            .map(|n| note_card(&n))
            .collect();
        for card in resolve_cards(&order, found) {
            Frame::new()
                .fill(SURFACE)
                .corner_radius(theme::ROUNDING_DEFAULT)
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 8.0;
                    if let Some(title) = &card.title {
                        ui.vertical_centered(|ui| {
                            ui.add(
                                Label::new(theme::semibold(title.as_str(), 20.0))
                                    .wrap_mode(TextWrapMode::Wrap),
                            );
                        });
                    }
                    if let Some(image) = &card.image {
                        let w = ui.available_width();
                        let size = egui::vec2(w, w * 9.0 / 16.0);
                        let img = image_from_cache(
                            services.ctx.img_cache,
                            services.ctx.media_jobs.sender(),
                            ui,
                            image,
                            Some(size),
                        );
                        if let Some(img) = img {
                            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                            img.fit_to_exact_size(size)
                                .corner_radius(theme::ROUNDING_DEFAULT)
                                .paint_at(ui, rect);
                            if let Some(link) = &card.link {
                                if resp.clicked() {
                                    crate::android_window::open_url(ui.ctx(), link);
                                }
                            }
                        }
                    }
                    if !card.content.is_empty() {
                        ui.add(Label::new(card.content.as_str()).wrap_mode(TextWrapMode::Wrap));
                    }
                    if let Some(link) = &card.link {
                        if ui.link(link.as_str()).clicked() {
                            crate::android_window::open_url(ui.ctx(), link);
                        }
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn card(d: &str, at: u64) -> Card {
        Card {
            d: d.into(),
            created_at: at,
            title: None,
            image: None,
            link: None,
            content: String::new(),
        }
    }

    #[test]
    fn d_tags_follow_list_order_and_skip_junk() {
        let tags = vec![
            tag(&["a", "37777:pk:second"]),
            tag(&["e", "x"]),
            tag(&["a", "37777:pk:first"]),
            tag(&["a", "37777:pk:second"]),
            tag(&["a", "badtag"]),
        ];
        assert_eq!(card_d_tags(&tags), vec!["second", "first"]);
    }

    #[test]
    fn resolve_orders_by_list_not_arrival_and_drops_missing() {
        let order = vec!["b".to_string(), "missing".to_string(), "a".to_string()];
        let out = resolve_cards(&order, vec![card("a", 1), card("b", 2)]);
        assert_eq!(out.iter().map(|c| c.d.as_str()).collect::<Vec<_>>(), ["b", "a"]);
    }

    #[test]
    fn resolve_keeps_newest_duplicate() {
        let out = resolve_cards(&["a".to_string()], vec![card("a", 1), card("a", 9), card("a", 5)]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].created_at, 9);
    }

    #[test]
    fn card_title_falls_back_to_subject() {
        let c = card_from_tags(&[tag(&["d", "x"]), tag(&["subject", "S"]), tag(&["r", "https://a"])], "body", 3);
        assert_eq!(c.title.as_deref(), Some("S"));
        assert_eq!(c.link.as_deref(), Some("https://a"));
        let c = card_from_tags(&[tag(&["title", "T"]), tag(&["subject", "S"]), tag(&["image", ""])], "", 3);
        assert_eq!(c.title.as_deref(), Some("T"));
        assert_eq!(c.image, None);
    }
}
