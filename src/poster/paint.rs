use super::chain::Candidates;
use super::service::{self, Poster, Request};
use crate::route::RouteServices;
use crate::stream_info::{StreamInfo, StreamStatus};
use crate::theme::{self, SURFACE, TEXT};
use egui::epaint::{Brush, CornerRadius, Mesh, RectShape, Shape};
use egui::{pos2, vec2, Color32, FontId, Painter, Rect};
use nostrdb::Note;
use std::sync::Arc;

pub fn poster_for(
    ui: &egui::Ui,
    services: &mut RouteServices<'_, '_>,
    note: &Note<'_>,
) -> Option<Poster> {
    let svc = service::global();
    if !svc.has_dir() {
        svc.set_dir(&services.ctx.path.path(notedeck::DataPathType::Cache));
    }
    let live = note.status() == StreamStatus::Live;
    let key = note.stream_key();
    let avatar = || {
        services
            .profile(note.host())
            .and_then(|p| p.picture())
            .map(str::to_string)
    };
    let ctx = ui.ctx().clone();
    svc.lookup(&ctx, &key, live, || Request {
        key: key.clone(),
        candidates: Candidates {
            image: note.image().filter(|s| !s.is_empty()).map(str::to_string),
            thumbs: note.thumbs(),
            streaming: note.streaming().map(str::to_string),
            live,
        },
        avatar: avatar(),
    })
}

pub fn tint_gradient(tint: Option<Color32>) -> (Color32, Color32) {
    let base = tint.unwrap_or(SURFACE);
    let mix = |c: Color32, to: Color32, t: f32| {
        let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        Color32::from_rgb(l(c.r(), to.r()), l(c.g(), to.g()), l(c.b(), to.b()))
    };
    let dark = Color32::from_rgb(13, 12, 15);
    (mix(base, dark, 0.45), mix(base, dark, 0.85))
}

fn cover_uv(tex: [usize; 2], rect: Rect) -> Rect {
    let ta = tex[0] as f32 / tex[1].max(1) as f32;
    let ra = rect.width() / rect.height().max(1.0);
    if ta > ra {
        let w = ra / ta;
        Rect::from_min_max(pos2((1.0 - w) / 2.0, 0.0), pos2((1.0 + w) / 2.0, 1.0))
    } else {
        let h = ta / ra;
        Rect::from_min_max(pos2(0.0, (1.0 - h) / 2.0), pos2(1.0, (1.0 + h) / 2.0))
    }
}

pub fn paint_poster(painter: &Painter, rect: Rect, radius: u8, poster: Option<&Poster>, title: &str) {
    if let Some(tex) = poster.and_then(|p| p.texture.as_ref()) {
        let mut shape = RectShape::filled(rect, CornerRadius::same(radius), Color32::WHITE);
        shape.brush = Some(Arc::new(Brush {
            fill_texture_id: tex.id(),
            uv: cover_uv(tex.size(), rect),
        }));
        painter.add(shape);
        return;
    }
    let (top, bottom) = tint_gradient(poster.and_then(|p| p.tint));
    painter.rect_filled(rect, CornerRadius::same(radius), bottom);
    let band = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y));
    let mut mesh = Mesh::default();
    mesh.colored_vertex(band.left_top(), top);
    mesh.colored_vertex(band.right_top(), top);
    mesh.colored_vertex(band.right_bottom(), bottom);
    mesh.colored_vertex(band.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    let clip = painter.with_clip_rect(rect.shrink(radius as f32 * 0.3));
    clip.add(Shape::mesh(mesh));
    if title.is_empty() {
        return;
    }
    let size = (rect.height() * 0.14).clamp(14.0, 30.0);
    let galley = painter.layout(
        title.to_string(),
        FontId::new(size, theme::display_family()),
        TEXT.gamma_multiply(0.85),
        (rect.width() - 32.0).max(40.0),
    );
    let pos = rect.center() - vec2(galley.size().x / 2.0, galley.size().y / 2.0);
    painter.with_clip_rect(rect).galley(pos, galley, Color32::PLACEHOLDER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_darkens_the_tint_and_defaults_to_surface() {
        let (top, bottom) = tint_gradient(Some(Color32::from_rgb(200, 100, 50)));
        assert!(top.r() < 200 && bottom.r() < top.r());
        let (t, _) = tint_gradient(None);
        assert!(t.r() <= SURFACE.r());
    }

    #[test]
    fn cover_uv_crops_the_longer_axis() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        let uv = cover_uv([200, 100], r);
        assert!((uv.width() - 0.5).abs() < 1e-5 && (uv.height() - 1.0).abs() < 1e-5);
        let uv = cover_uv([100, 200], r);
        assert!((uv.height() - 0.5).abs() < 1e-5);
    }
}
