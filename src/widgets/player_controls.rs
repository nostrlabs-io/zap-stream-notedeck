use crate::theme::{self, PRIMARY, TEXT};
use egui::epaint::{Mesh, Shape};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Pos2, Rect, Sense, Stroke,
    Ui,
};

pub const BAR_HEIGHT: f32 = 56.0;
pub const SLOT: f32 = 48.0;
pub const SLOT_GAP: f32 = 8.0;
const SLOT_PITCH: f32 = SLOT + SLOT_GAP;
const SCRIM: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 170);
const ICON: Color32 = Color32::WHITE;

#[derive(Default)]
pub struct ControlsOutput {
    pub toggle_play: bool,
    pub toggle_mute: bool,
    pub volume: Option<u32>,
    pub toggle_fullscreen: bool,
    pub toggle_menu: bool,
    pub enter_pip: bool,
    pub pick_quality: Option<usize>,
    pub hovered: bool,
}

pub struct ControlsInput<'a> {
    pub playing: bool,
    pub muted: bool,
    pub volume: u32,
    pub fullscreen: bool,
    pub quality_label: &'a str,
    pub qualities: &'a [String],
    pub selected_quality: Option<usize>,
    pub menu_open: bool,
    pub live: bool,
    pub elapsed: Option<String>,
    pub viewers: Option<u32>,
}

pub fn exit_fullscreen_slot(rect: Rect) -> Rect {
    Rect::from_center_size(pos2(rect.max.x - 12.0 - SLOT / 2.0, rect.min.y + 12.0 + SLOT / 2.0), vec2(SLOT, SLOT))
}

pub fn paint_exit_fullscreen(ui: &mut Ui, rect: Rect) -> bool {
    let slot = exit_fullscreen_slot(rect);
    ui.painter().circle_filled(slot.center(), 18.0, SCRIM);
    let hit = icon_button(ui, "player-fullscreen", slot);
    paint_fullscreen(ui, slot.center(), true);
    hit
}

fn icon_button(ui: &mut Ui, id: &str, rect: Rect) -> bool {
    let r = ui.interact(rect, Id::new(id), Sense::click());
    if r.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), Color32::from_white_alpha(30));
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    r.clicked()
}

fn paint_play(ui: &Ui, c: Pos2) {
    let s = 8.0;
    let pts = vec![
        pos2(c.x - s * 0.6, c.y - s),
        pos2(c.x + s, c.y),
        pos2(c.x - s * 0.6, c.y + s),
    ];
    ui.painter()
        .add(Shape::convex_polygon(pts, ICON, Stroke::NONE));
}

fn paint_pause(ui: &Ui, c: Pos2) {
    for dx in [-5.0, 2.0] {
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(c.x + dx, c.y - 8.0), vec2(4.0, 16.0)),
            CornerRadius::same(1),
            ICON,
        );
    }
}

fn paint_speaker(ui: &Ui, c: Pos2, muted: bool, volume: u32) {
    let p = ui.painter();
    p.rect_filled(
        Rect::from_min_size(pos2(c.x - 9.0, c.y - 3.0), vec2(5.0, 6.0)),
        CornerRadius::ZERO,
        ICON,
    );
    p.add(Shape::convex_polygon(
        vec![
            pos2(c.x - 4.0, c.y - 3.0),
            pos2(c.x + 1.0, c.y - 8.0),
            pos2(c.x + 1.0, c.y + 8.0),
            pos2(c.x - 4.0, c.y + 3.0),
        ],
        ICON,
        Stroke::NONE,
    ));
    if muted || volume == 0 {
        let s = Stroke::new(2.0, ICON);
        p.line_segment([pos2(c.x + 4.0, c.y - 4.0), pos2(c.x + 10.0, c.y + 4.0)], s);
        p.line_segment([pos2(c.x + 10.0, c.y - 4.0), pos2(c.x + 4.0, c.y + 4.0)], s);
    } else {
        let arcs = if volume > 50 { 2 } else { 1 };
        for i in 0..arcs {
            let r = 4.0 + i as f32 * 4.0;
            let pts: Vec<Pos2> = (-4..=4)
                .map(|k| {
                    let a = k as f32 * 0.17;
                    pos2(c.x + 2.0 + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, Stroke::new(1.6, ICON)));
        }
    }
}

fn paint_fullscreen(ui: &Ui, c: Pos2, exit: bool) {
    let p = ui.painter();
    let s = Stroke::new(2.0, ICON);
    let (reach, arm) = if exit { (3.0, -5.0) } else { (8.0, 5.0) };
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let corner = pos2(c.x + sx * reach, c.y + sy * reach);
        let end_x = pos2(corner.x - sx * arm, corner.y);
        let end_y = pos2(corner.x, corner.y - sy * arm);
        p.line_segment([corner, end_x], s);
        p.line_segment([corner, end_y], s);
    }
}

fn paint_pip(ui: &Ui, c: Pos2) {
    let p = ui.painter();
    let s = Stroke::new(2.0, ICON);
    p.rect_stroke(
        Rect::from_center_size(c, vec2(18.0, 14.0)),
        CornerRadius::same(2),
        s,
        egui::StrokeKind::Inside,
    );
    p.rect_filled(
        Rect::from_min_size(c + vec2(0.0, 0.0), vec2(8.0, 5.0)),
        CornerRadius::same(1),
        ICON,
    );
}

fn paint_volume_slider(ui: &mut Ui, rect: Rect, volume: u32, muted: bool) -> Option<u32> {
    let r = ui.interact(rect, Id::new("player-volume"), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
    let level = if muted { 0.0 } else { volume as f32 / 100.0 };
    let p = ui.painter();
    p.rect_filled(track, CornerRadius::same(2), Color32::from_white_alpha(70));
    let mut fill = track;
    fill.set_right(track.left() + track.width() * level);
    p.rect_filled(fill, CornerRadius::same(2), PRIMARY);
    p.circle_filled(pos2(fill.right(), track.center().y), 6.0, ICON);
    if r.hovered() || r.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if r.dragged() || r.clicked() {
        if let Some(pos) = r.interact_pointer_pos() {
            let v = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            return Some((v * 100.0).round() as u32);
        }
    }
    None
}

fn scrim(ui: &Ui, rect: Rect, top: bool) {
    let clear = Color32::TRANSPARENT;
    let mut mesh = Mesh::default();
    let (a, b) = if top { (SCRIM, clear) } else { (clear, SCRIM) };
    mesh.colored_vertex(rect.left_top(), a);
    mesh.colored_vertex(rect.right_top(), a);
    mesh.colored_vertex(rect.right_bottom(), b);
    mesh.colored_vertex(rect.left_bottom(), b);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}

fn live_badge(ui: &Ui, origin: Pos2, elapsed: Option<&str>, viewers: Option<u32>) {
    let p = ui.painter();
    let live = p.layout_no_wrap("LIVE".into(), FontId::new(12.0, theme::semibold_family()), ICON);
    let badge = Rect::from_min_size(origin, vec2(live.size().x + 16.0, 22.0));
    p.rect_filled(badge, CornerRadius::same(6), Color32::from_rgb(220, 38, 38));
    p.galley(badge.min + vec2(8.0, 4.0), live, Color32::PLACEHOLDER);
    let mut x = badge.right() + 6.0;
    let mut chip = |text: String| {
        let g = p.layout_no_wrap(text, FontId::new(12.0, theme::medium_family()), TEXT);
        let r = Rect::from_min_size(pos2(x, origin.y), vec2(g.size().x + 16.0, 22.0));
        p.rect_filled(r, CornerRadius::same(6), Color32::from_black_alpha(140));
        p.galley(r.min + vec2(8.0, 4.0), g, Color32::PLACEHOLDER);
        x = r.right() + 6.0;
    };
    if let Some(e) = elapsed {
        chip(e.to_string());
    }
    if let Some(v) = viewers {
        chip(crate::i18n::tf("player.watching", &[("n", &v.to_string())]));
    }
}

pub fn show(ui: &mut Ui, rect: Rect, input: &ControlsInput<'_>) -> ControlsOutput {
    let mut out = ControlsOutput::default();
    let top = Rect::from_min_size(rect.min, vec2(rect.width(), 56.0));
    let bar = Rect::from_min_max(pos2(rect.min.x, rect.max.y - BAR_HEIGHT - 8.0), rect.max);
    scrim(ui, bar, false);
    scrim(ui, top, true);
    if input.live {
        live_badge(ui, rect.min + vec2(12.0, 12.0), input.elapsed.as_deref(), input.viewers);
    }

    let y = rect.max.y - SLOT / 2.0 - 4.0;
    let slot = |cx: f32| Rect::from_center_size(pos2(cx, y), vec2(SLOT, SLOT));
    let mut x = rect.min.x + 12.0 + SLOT / 2.0;

    let play = slot(x);
    if icon_button(ui, "player-play", play) {
        out.toggle_play = true;
    }
    if input.playing {
        paint_pause(ui, play.center());
    } else {
        paint_play(ui, play.center());
    }
    x += SLOT_PITCH;

    let mute = slot(x);
    if icon_button(ui, "player-mute", mute) {
        out.toggle_mute = true;
    }
    paint_speaker(ui, mute.center(), input.muted, input.volume);
    x += SLOT_PITCH;

    if rect.width() >= 420.0 {
        let vol = Rect::from_center_size(pos2(x + 40.0, y), vec2(80.0, 24.0));
        out.volume = paint_volume_slider(ui, vol, input.volume, input.muted);
        x += 92.0;
    }
    let _ = x;

    let mut rx = rect.max.x - 12.0 - SLOT / 2.0;
    let fs = slot(rx);
    if icon_button(ui, "player-fullscreen", fs) {
        out.toggle_fullscreen = true;
    }
    paint_fullscreen(ui, fs.center(), input.fullscreen);
    rx -= SLOT_PITCH;

    if crate::android_window::PIP_AVAILABLE {
        let pip = slot(rx);
        if icon_button(ui, "player-pip", pip) {
            out.enter_pip = true;
        }
        paint_pip(ui, pip.center());
        rx -= SLOT_PITCH;
    }

    if !input.qualities.is_empty() {
        let g = ui.painter().layout_no_wrap(
            input.quality_label.to_string(),
            FontId::new(13.0, theme::medium_family()),
            ICON,
        );
        let w = g.size().x + 20.0;
        let q = Rect::from_center_size(pos2(rx - w / 2.0 + 18.0, y), vec2(w, SLOT));
        if icon_button(ui, "player-quality", q) {
            out.toggle_menu = true;
        }
        ui.painter().galley(
            q.center() - g.size() / 2.0,
            g,
            Color32::PLACEHOLDER,
        );
        if input.menu_open {
            let row_h = 32.0;
            let rows = input.qualities.len() + 1;
            let menu_w = 120.0;
            let menu = Rect::from_min_size(
                pos2(q.right() - menu_w, q.top() - 8.0 - row_h * rows as f32),
                vec2(menu_w, row_h * rows as f32),
            );
            ui.painter()
                .rect_filled(menu, CornerRadius::same(10), Color32::from_black_alpha(220));
            let labels = std::iter::once(crate::i18n::t("player.auto")).chain(input.qualities.iter().map(|s| s.as_str()));
            for (i, label) in labels.enumerate() {
                let row = Rect::from_min_size(
                    pos2(menu.left(), menu.top() + row_h * i as f32),
                    vec2(menu_w, row_h),
                );
                let idx = if i == 0 { None } else { Some(i - 1) };
                let r = ui.interact(row, Id::new(("player-q", i)), Sense::click());
                if r.hovered() {
                    ui.painter()
                        .rect_filled(row, CornerRadius::same(8), Color32::from_white_alpha(25));
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                let color = if idx == input.selected_quality { PRIMARY } else { ICON };
                ui.painter().text(
                    row.left_center() + vec2(14.0, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::new(14.0, theme::medium_family()),
                    color,
                );
                if r.clicked() {
                    out.pick_quality = Some(i);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bottom_slots(rect: Rect, with_pip: bool) -> (Rect, Option<Rect>) {
        let y = rect.max.y - SLOT / 2.0 - 4.0;
        let fs_x = rect.max.x - 12.0 - SLOT / 2.0;
        let fs = Rect::from_center_size(pos2(fs_x, y), vec2(SLOT, SLOT));
        let pip = with_pip.then(|| Rect::from_center_size(pos2(fs_x - SLOT_PITCH, y), vec2(SLOT, SLOT)));
        (fs, pip)
    }

    #[test]
    fn slots_are_large_and_separated() {
        let video = Rect::from_min_max(pos2(61.8, 0.0), pos2(793.3, 411.4));
        let (fs, pip) = bottom_slots(video, true);
        let pip = pip.unwrap();
        assert!(fs.width() >= 48.0 && fs.height() >= 48.0);
        assert!(fs.left() - pip.right() >= 8.0);
    }

    #[test]
    fn phone_tap_in_rot1_lands_in_exit_button() {
        let ppp = 2.45;
        let view_left_px = 149.0;
        let raw_x_px = 2028.9;
        let raw_y_px = 920.25;
        let pos = pos2((raw_x_px - view_left_px) / ppp, raw_y_px / ppp);
        let video = Rect::from_min_max(pos2(61.8, 0.0), pos2(793.3, 411.4));
        let (fs, _) = bottom_slots(video, true);
        assert!(fs.contains(pos));
    }

    #[test]
    fn exit_slot_is_top_right_and_large() {
        let video = Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 400.0));
        let slot = exit_fullscreen_slot(video);
        assert!(slot.width() >= 48.0);
        assert!(video.contains_rect(slot));
    }
}
