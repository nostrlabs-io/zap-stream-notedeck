use crate::note_ref::NoteRef;
use crate::route::RouteServices;
use crate::sub::SubRef;
use crate::theme::{self, BORDER, MUTED, ON_PRIMARY, ZAP};
use crate::widgets::sub_or_poll;
use crate::zap::{format_sats, Zap};
use egui::{vec2, Align2, Rect, Sense, Ui};
use nostr::prelude::hex;
use nostrdb::Filter;
use std::collections::HashSet;

pub struct Goal {
    id: [u8; 32],
    goal: HashSet<NoteRef>,
    goal_sub: Option<SubRef>,
    zaps: HashSet<NoteRef>,
    zaps_sub: Option<SubRef>,
}

pub fn progress(total_msats: u64, target_msats: u64) -> f32 {
    if target_msats == 0 {
        return 0.0;
    }
    (total_msats as f32 / target_msats as f32).clamp(0.0, 1.0)
}

impl Goal {
    pub fn new(id: [u8; 32]) -> Self {
        Self {
            id,
            goal: HashSet::new(),
            goal_sub: None,
            zaps: HashSet::new(),
            zaps_sub: None,
        }
    }

    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }

    pub fn update(&mut self, services: &mut RouteServices<'_, '_>) -> anyhow::Result<()> {
        let goal_filter = Filter::new().kinds([9_041]).ids([&self.id]).limit(1).build();
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.goal,
            &mut self.goal_sub,
            vec![goal_filter],
        )?;
        let zap_filter = Filter::new()
            .kinds([9_735])
            .tags([hex::encode(self.id).as_str()], 'e')
            .build();
        sub_or_poll(
            services.ctx,
            services.tx,
            &mut self.zaps,
            &mut self.zaps_sub,
            vec![zap_filter],
        )
    }

    pub fn render(&self, ui: &mut Ui, services: &mut RouteServices<'_, '_>) {
        let Some(goal_ref) = self.goal.iter().next() else {
            return;
        };
        let Ok(note) = services.ctx.ndb.get_note_by_key(services.tx, goal_ref.key) else {
            return;
        };
        let target = crate::note_util::NoteUtil::get_tag_value(&note, "amount")
            .and_then(|v| v.variant().str().and_then(|s| s.parse::<u64>().ok()))
            .unwrap_or(0);
        let title = note.content().to_string();

        let mut total = 0u64;
        for r in &self.zaps {
            if let Ok(z) = services.ctx.ndb.get_note_by_key(services.tx, r.key) {
                if let Ok(zap) = Zap::from_receipt(z) {
                    total += zap.amount;
                }
            }
        }
        paint_goal(ui, &title, total, target);
    }
}

pub fn debug_goal() -> Option<(String, u64, u64)> {
    std::env::var_os("ZAP_STREAM_DEBUG_GOAL")
        .map(|_| ("Upgrade the studio camera".to_string(), 420_000_000, 1_000_000_000))
}

pub fn paint_goal(ui: &mut Ui, title: &str, total: u64, target: u64) {
    {
        let fraction = progress(total, target);
        let remaining_sats = target.saturating_sub(total) / 1000;

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                ui.label(theme::medium(title, 14.0).color(theme::TEXT));
                if remaining_sats > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            theme::medium(
                                format!("{} sats to go", format_sats(remaining_sats as f32)),
                                12.0,
                            )
                            .color(MUTED),
                        );
                    });
                }
            });
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 7.0, BORDER);
            let filled_w = (rect.width() * fraction).max(if total > 0 { 8.0 } else { 0.0 });
            if filled_w > 0.0 {
                painter.rect_filled(
                    Rect::from_min_size(rect.min, vec2(filled_w, rect.height())),
                    7.0,
                    ZAP,
                );
            }
            if remaining_sats == 0 && target > 0 {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Goal reached",
                    egui::FontId::new(10.0, theme::medium_family()),
                    ON_PRIMARY,
                );
            } else {
                painter.text(
                    rect.right_center() - vec2(6.0, 0.0),
                    Align2::RIGHT_CENTER,
                    format_sats((target / 1000) as f32),
                    egui::FontId::new(10.0, theme::medium_family()),
                    theme::TEXT,
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::progress;

    #[test]
    fn zero_target_does_not_divide() {
        assert_eq!(progress(5_000, 0), 0.0);
    }

    #[test]
    fn overfunded_goal_is_clamped() {
        assert_eq!(progress(3_000, 1_000), 1.0);
    }

    #[test]
    fn half_funded() {
        assert_eq!(progress(500, 1_000), 0.5);
    }
}
