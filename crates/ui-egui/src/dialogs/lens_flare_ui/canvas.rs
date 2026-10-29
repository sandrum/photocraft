//! The Lens Flare dialog on the image: while it is on top, a click or drag on the free canvas
//! (`canvas::handle_dialog_canvas_input`, the control channel's pointer events) places the light
//! source, as on the dialog's pad, and a crosshair marks the source on the canvas. The canvas
//! widget is inert under a dialog, so no tool paints or selects meanwhile; Space and the middle
//! button still pan.
//!
//! Also the images the dialog shows: a downscaled composite of the document for the position pad
//! and the same image graded by each look for the Filters tab, built once per document revision.

use std::sync::Arc;

use egui::{Color32, Pos2, Stroke, vec2};
use photocraft_doc::LayerId;
use photocraft_doc::lens_flare::range;
use photocraft_engine::lens_flare_cmds::looks;
use serde_json::{Map, Value};

use super::{num, owns, put, target_is_active};
use crate::PhotocraftApp;
use crate::canvas::ViewXform;

/// Is the Lens Flare dialog on top, targeting the active document (so the canvas places its
/// light)?
pub(crate) fn placing(app: &PhotocraftApp) -> bool {
    app.ui.dialogs.last().is_some_and(|d| owns(&d.fields) && target_is_active(app, &d.fields))
}

/// Place the light at document point `at` (pixels; outside the frame too, within
/// `range::POSITION`). Returns whether the dialog took it.
pub(crate) fn place_at(app: &mut PhotocraftApp, at: [f64; 2]) -> bool {
    if !placing(app) || !at.iter().all(|v| v.is_finite()) {
        return false;
    }
    let Some(size) = app.session.active().map(|st| st.doc.size) else { return false };
    let (w, h) = (f64::from(size.width.max(1)), f64::from(size.height.max(1)));
    let Some(d) = app.ui.dialogs.last_mut() else { return false };
    put(&mut d.fields, "x", (at[0] / w) as f32, range::POSITION, 0.001);
    put(&mut d.fields, "y", (at[1] / h) as f32, range::POSITION, 0.001);
    true
}

/// The light source's crosshair on the canvas while the dialog places it.
pub(crate) fn draw_handle(app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    if !placing(app) {
        return;
    }
    let (Some(d), Some(st)) = (app.ui.dialogs.last(), app.session.active()) else { return };
    let (x, y) = (num(&d.fields, "x", 0.3), num(&d.fields, "y", 0.3));
    let at = xf.to_screen(x * st.doc.size.width as f32, y * st.doc.size.height as f32);
    if !at.x.is_finite() || !at.y.is_finite() {
        return;
    }
    let t = crate::theme::Tokens::get(painter.ctx());
    let (dark, light) = (Stroke::new(3.0, Color32::from_black_alpha(150)), Stroke::new(1.5, Color32::from_white_alpha(235)));
    let arms = |s: Stroke| {
        for dir in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
            painter.line_segment([at + dir * 5.0, at + dir * 14.0], s);
        }
    };
    arms(dark);
    painter.circle_stroke(at, 9.0, dark);
    arms(light);
    painter.circle_stroke(at, 9.0, light);
    painter.circle_filled(at, 2.5, t.accent);
}

/// The dialog's images for one document state.
#[derive(Clone)]
pub(crate) struct Thumbs {
    key: (u64, u64, u64, u64),
    /// The document without the flare being edited (and its look), for the position pad.
    pub(crate) pad: egui::TextureHandle,
    /// That image graded by each of [`looks::LOOKS`], in order.
    pub(crate) looks: Arc<Vec<egui::TextureHandle>>,
}

/// Longest side of the pad image (points × 2 for sharp HiDPI).
const PAD_SIDE: u32 = 360;

/// The pad and look images for the dialog's target, rendered on the first call for each document
/// revision and reused (never a composite per frame).
pub(crate) fn thumbs(app: &PhotocraftApp, ctx: &egui::Context, f: &Map<String, Value>) -> Option<Thumbs> {
    let st = app.session.active()?;
    let editing = f.get("__params").and_then(|p| p.get("layer")).and_then(Value::as_u64);
    let (display, display_key) = crate::canvas::canvas_display(app, &st.doc, None);
    let key = (st.doc.id.0, st.revision, editing.unwrap_or(0), display_key);
    let id = egui::Id::unique("lens-flare-thumbs");
    if let Some(t) = ctx.data(|d| d.get_temp::<Thumbs>(id)).filter(|t| t.key == key) {
        return Some(t);
    }
    // The pad shows the image the flare goes on: without the flare being edited or its look.
    let mut doc = (*st.doc).clone();
    if let Some(layer) = editing.map(LayerId) {
        let look = looks::find(&doc, layer).map(|(g, _)| g);
        for hide in [Some(layer), look].into_iter().flatten() {
            if let Some(l) = doc.layer_mut(hide) {
                l.visible = false;
            }
        }
    }
    let buf = photocraft_compose::thumbnail_buffer(&doc, PAD_SIDE);
    let options = egui::TextureOptions::LINEAR;
    let pad = ctx.load_texture("lens-flare-pad", crate::canvas::display_image(display.as_deref(), &buf), options);
    let graded = looks::LOOKS
        .iter()
        .map(|look| {
            let mut b = buf.clone();
            for adj in look.adjustments(doc.mode).unwrap_or_default() {
                photocraft_compose::adjust::apply(&adj, &mut b);
            }
            ctx.load_texture(format!("lens-flare-look-{}", look.id), crate::canvas::display_image(display.as_deref(), &b), options)
        })
        .collect();
    let t = Thumbs { key, pad, looks: Arc::new(graded) };
    ctx.data_mut(|d| d.insert_temp(id, t.clone()));
    Some(t)
}

/// The part of a `size` texture that fills a cell of aspect `cell` (centred, cropped).
pub(crate) fn cover_uv(size: [usize; 2], cell: f32) -> egui::Rect {
    let aspect = size[0].max(1) as f32 / size[1].max(1) as f32;
    let (u, v) = if aspect > cell { ((1.0 - cell / aspect) / 2.0, 0.0) } else { (0.0, (1.0 - aspect / cell) / 2.0) };
    egui::Rect::from_min_max(Pos2::new(u, v), Pos2::new(1.0 - u, 1.0 - v))
}
