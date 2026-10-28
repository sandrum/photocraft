//! The Lens Flare dialog: Layer › New Fill Layer › Lens Flare… and double-clicking a Lens Flare
//! layer's thumbnail open it. A preset library in four tabs (Anamorphic, Spherical, Sun Flares,
//! Streaks) with rendered thumbnails, a position pad for the light source, the flare's controls
//! (Scale, Brightness, Aspect, Source, Artifacts, Rotation), a colour wheel (Hue and Tint) and a
//! lens-dirt Texture with its Opacity and Flip X / Flip Y, previewed live on the canvas from a
//! document snapshot. OK runs one engine command (`layer.newFillLayer.lensFlare` for a new layer,
//! `layer.fill.lensFlare.set` for an existing one), so it is one History step; Cancel changes
//! nothing. The layer stays a flare: reopen it to change anything until the image is final.
//! The fields are the command's params, so `ui.dialog.set` / `ui.dialog.confirm` drive it.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{Color32, Rect, Sense, Stroke, Vec2, pos2, vec2};
use photocraft_doc::lens_flare::{self, FlareCategory, TextureKind, range};
use photocraft_doc::{DocId, Document, Fill, LayerContent};
use photocraft_engine::lens_flare_cmds as cmds;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

const MARK: &str = "__lensFlare";
/// The fields that are command params (besides the target in `__params`).
const KEYS: [&str; 15] =
    ["preset", "x", "y", "scale", "brightness", "aspect", "source", "artifacts", "rotation", "hue", "tint", "texture", "textureOpacity", "flipX", "flipY"];
/// The dialog's width: five preset thumbnails across.
pub(crate) const WIDTH: f32 = 720.0;
const THUMB: Vec2 = vec2(128.0, 72.0);

pub(crate) struct Preview {
    document: DocId,
    revision: u64,
    dialog: u64,
    params: Value,
    shown: Arc<Document>,
    key: u64,
}

/// Is this dialog the Lens Flare dialog?
pub(crate) fn owns(f: &Map<String, Value>) -> bool {
    f.contains_key(MARK)
}

fn fields(command: &str, target: Value, values: Value) -> Map<String, Value> {
    let mut f = Map::new();
    f.insert(MARK.into(), json!(true));
    f.insert("__command".into(), json!(command));
    f.insert("__label".into(), json!("Lens Flare"));
    f.insert("__params".into(), target);
    f.insert("__tab".into(), values.get("category").cloned().unwrap_or(json!("anamorphic")));
    for k in KEYS {
        if let Some(v) = values.get(k) {
            f.insert(k.into(), v.clone());
        }
    }
    f.insert("__orig".into(), values);
    f
}

/// Edit the active Lens Flare layer (its thumbnail, Layer › Layer Content Options…).
pub(crate) fn open(app: &mut PhotocraftApp) -> Option<u64> {
    let st = app.session.active()?;
    let layer = st.active_layer?;
    let LayerContent::Fill(Fill::LensFlare(flare)) = &st.doc.layer(layer)?.content else { return None };
    let f = fields(cmds::SET, json!({"layer": layer.0, "document": st.doc.id.0}), cmds::params_of(flare));
    Some(app.ui.open_dialog(crate::state::DialogKind::Command, f))
}

/// Layer › New Fill Layer › Lens Flare…: the dialog in front of creating the layer, starting from
/// the default preset with its light in the upper left third.
pub(crate) fn open_new(app: &mut PhotocraftApp) -> Result<u64, String> {
    if let Some(Err(why)) = photocraft_engine::commands::find(cmds::CREATE).map(|c| (c.enabled)(&app.session)) {
        return Err(why);
    }
    let st = app.session.active().ok_or("no document open")?;
    let flare = lens_flare::preset(cmds::DEFAULT_PRESET).ok_or("no lens flare presets")?.flare();
    let f = fields(cmds::CREATE, json!({"document": st.doc.id.0}), cmds::params_of(&flare));
    Ok(app.ui.open_dialog(crate::state::DialogKind::Command, f))
}

/// The command and its params: the target plus the dialog's values.
fn command(app: &PhotocraftApp, f: &Map<String, Value>) -> Option<(&'static str, Value)> {
    let cmd = match f.get("__command").and_then(Value::as_str)? {
        cmds::SET => cmds::SET,
        cmds::CREATE => cmds::CREATE,
        _ => return None,
    };
    let mut params = f.get("__params").filter(|p| p.is_object()).cloned()?;
    for k in KEYS {
        if let Some(v) = f.get(k) {
            params[k] = v.clone();
        }
    }
    // An edit that keeps the preset must keep the layer's elements (a preset re-applies them).
    if cmd == cmds::SET && f.get("preset") == f.get("__orig").and_then(|o| o.get("preset")) {
        params.as_object_mut()?.remove("preset");
    }
    let params = if cmd == cmds::CREATE {
        params.as_object_mut()?.remove("document");
        crate::vector_ui::with_active_path(app, cmd, params)
    } else {
        params
    };
    Some((cmd, params))
}

/// An edit dialog whose values are still the layer's.
fn unchanged(f: &Map<String, Value>) -> bool {
    f.get("__command").and_then(Value::as_str) == Some(cmds::SET) && KEYS.iter().all(|k| f.get(*k) == f.get("__orig").and_then(|o| o.get(*k)))
}

/// The document the dialog targets, if it is still the active one.
fn target_is_active(app: &PhotocraftApp, f: &Map<String, Value>) -> bool {
    let target = f.get("__params").and_then(|p| p.get("document")).and_then(Value::as_u64);
    app.session.active().is_some_and(|st| target == Some(st.doc.id.0))
}

/// OK: one engine command. A stale document never receives the edit.
pub(crate) fn confirm(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<Value, String> {
    if !target_is_active(app, f) {
        return Err("the target document is no longer active".into());
    }
    if unchanged(f) {
        return Ok(json!({"unchanged": true}));
    }
    let (cmd, params) = command(app, f).ok_or("invalid Lens Flare dialog")?;
    app.run(cmd, params)
}

/// The canvas preview while the dialog is open: the document as OK would leave it.
pub(crate) fn display_doc(app: &mut PhotocraftApp, idx: usize) -> Option<(Arc<Document>, u64)> {
    let dialog = app.ui.dialogs.iter().rev().find(|d| owns(&d.fields));
    let Some(d) = dialog else {
        app.lens_flare_preview = None;
        return None;
    };
    let st = app.session.documents().get(idx)?;
    if !target_is_active(app, &d.fields) || app.session.active_index() != Some(idx) || unchanged(&d.fields) {
        return None;
    }
    let (cmd, params) = command(app, &d.fields)?;
    if let Some(p) = &app.lens_flare_preview
        && p.document == st.doc.id
        && p.revision == st.revision
        && p.dialog == d.id
        && p.params == params
    {
        return Some((p.shown.clone(), p.key));
    }
    // Out-of-range values typed by automation show the committed document; OK reports them.
    let doc = cmds::preview(&app.session, cmd, &params).ok()?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (st.doc.id.0, st.revision, d.id, params.to_string()).hash(&mut hash);
    let key = (1 << 47) | (1 << 46) | (hash.finish() & ((1 << 36) - 1));
    let (document, revision, dialog) = (st.doc.id, st.revision, d.id);
    let shown = Arc::new(photocraft_engine::mode_cmds::display_document(&doc).unwrap_or(doc));
    app.lens_flare_preview = Some(Preview { document, revision, dialog, params, shown: shown.clone(), key });
    Some((shown, key))
}

fn num(f: &Map<String, Value>, k: &str, d: f32) -> f32 {
    f.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).map_or(d, |v| v as f32)
}

fn flag(f: &Map<String, Value>, k: &str) -> bool {
    f.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// Store a slider's value, rounded to what its field shows.
fn put(f: &mut Map<String, Value>, k: &str, v: f32, (lo, hi): (f32, f32), step: f32) {
    let v = ((v.clamp(lo, hi) / step).round() * step).clamp(lo, hi);
    f.insert(k.into(), json!(v));
}

/// A preset's thumbnail: the flare over a dark night gradient, rendered once per preset.
fn thumbnail(ctx: &egui::Context, preset: &lens_flare::FlarePreset) -> egui::TextureHandle {
    let id = egui::Id::unique(("lens-flare-thumb", preset.name));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return t;
    }
    let (w, h) = (THUMB.x as i32 * 2, THUMB.y as i32 * 2);
    let rect = photocraft_geom::Rect::new(0, 0, w, h);
    let mut flare = preset.flare();
    flare.source = (0.32, 0.4);
    let px = photocraft_compose::lens_flare::render(&flare, rect, rect);
    let pixels: Vec<Color32> = px
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let y = (i / w as usize) as f32 / h as f32;
            let bg = [0.03 + 0.05 * y, 0.035 + 0.04 * y, 0.06 + 0.02 * y];
            let c = |k: usize| {
                let v = bg[k] + p[k] * p[3] * (1.0 - bg[k]);
                (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8
            };
            Color32::from_rgb(c(0), c(1), c(2))
        })
        .collect();
    let image = egui::ColorImage { size: [w as usize, h as usize], source_size: vec2(w as f32, h as f32), pixels };
    let tex = ctx.load_texture(format!("lens-flare-{}", preset.name), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// The preset tabs and the thumbnail grid of the selected tab.
fn library(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let tab = f.get("__tab").and_then(Value::as_str).unwrap_or("anamorphic").to_string();
    let current = f.get("preset").and_then(Value::as_str).unwrap_or_default().to_string();
    ui.horizontal(|ui| {
        for c in FlareCategory::ALL {
            if crate::widgets::pill_tab(ui, c.label(), tab == c.id()).clicked() {
                f.insert("__tab".into(), json!(c.id()));
            }
        }
    });
    let category = FlareCategory::ALL.into_iter().find(|c| c.id() == tab).unwrap_or_default();
    let presets: Vec<_> = lens_flare::presets(category).collect();
    ui.add_space(4.0);
    for row in presets.chunks(5) {
        ui.horizontal(|ui| {
            for p in row {
                let selected = p.name == current;
                let (rect, resp) = ui.allocate_exact_size(THUMB + vec2(0.0, 18.0), Sense::click());
                let img = Rect::from_min_size(rect.min, THUMB);
                if ui.is_rect_visible(rect) {
                    let tex = thumbnail(ui.ctx(), p);
                    ui.painter().image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    let border = if selected {
                        Stroke::new(2.0, t.accent)
                    } else if resp.hovered() {
                        Stroke::new(1.0, t.text_dim)
                    } else {
                        Stroke::new(1.0, t.field_border)
                    };
                    ui.painter().rect_stroke(img, t.radius_sm, border, egui::StrokeKind::Inside);
                    let color = if selected { t.accent } else { t.text_dim };
                    ui.painter().text(pos2(img.center().x, img.bottom() + 9.0), egui::Align2::CENTER_CENTER, tl!(p.name), egui::FontId::proportional(11.0), color);
                }
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, p.name));
                if resp.clicked() {
                    f.insert("preset".into(), json!(p.name));
                }
            }
        });
    }
}

/// The light source pad: the canvas's aspect, with a marker the user clicks or drags.
fn position_pad(app: &PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let aspect = app.session.active().map_or(16.0 / 9.0, |st| st.doc.size.width.max(1) as f32 / st.doc.size.height.max(1) as f32);
    let w = 180.0f32;
    let h = (w / aspect).clamp(60.0, 180.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click_and_drag());
    if (resp.clicked() || resp.dragged())
        && let Some(p) = resp.interact_pointer_pos().filter(|p| p.x.is_finite() && p.y.is_finite())
    {
        put(f, "x", (p.x - rect.left()) / rect.width(), range::POSITION, 0.001);
        put(f, "y", (p.y - rect.top()) / rect.height(), range::POSITION, 0.001);
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, t.radius_sm, Color32::from_rgb(14, 16, 24));
        let (x, y) = (num(f, "x", 0.3), num(f, "y", 0.3));
        let src = pos2(rect.left() + x * rect.width(), rect.top() + y * rect.height());
        // The flare's axis through the frame centre, where its ghosts fall.
        let mirror = rect.center() + (rect.center() - src);
        painter.line_segment([src, mirror], Stroke::new(1.0, t.text_faint.gamma_multiply(0.5)));
        for k in [0.5f32, 1.0, 1.5] {
            painter.circle_stroke(src + (rect.center() - src) * k, 3.0 + 2.0 * k, Stroke::new(1.0, t.text_faint.gamma_multiply(0.6)));
        }
        painter.circle_filled(src, 6.0, Color32::from_rgba_unmultiplied(255, 240, 210, 80));
        painter.circle(src, 3.5, Color32::WHITE, Stroke::new(1.5, t.accent));
        ui.painter().rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, tl!("Light Source")));
    resp.on_hover_cursor(egui::CursorIcon::Crosshair);
}

/// A control slider: label, field, range, default, suffix and step.
type Ctl = (&'static str, &'static str, (f32, f32), f32, &'static str, f32);

/// One control slider bound to its field.
fn control(ui: &mut egui::Ui, f: &mut Map<String, Value>, (label, k, r, default, suffix, step): Ctl) {
    let mut v = num(f, k, default);
    let gestures = crate::widgets::RowGestures { reset: Some(default), wheel_step: Some(step * 10.0), ..Default::default() };
    if crate::widgets::slider_row_with(ui, tl!(label), &mut v, r.0..=r.1, suffix, None, gestures).changed() {
        put(f, k, v, r, step);
    }
}

/// The dialog body, in LensFlare-studio order: Effects (the library), then Edit (position and
/// controls), Color and Textures side by side.
pub(crate) fn body(app: &PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.set_width(WIDTH);
    library(ui, f);
    crate::widgets::hairline(ui);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(190.0);
            crate::widgets::section_label(ui, tl!("Light Source"));
            position_pad(app, ui, f);
            ui.label(egui::RichText::new(tl!("Click or drag to place the light.")).small().color(Tokens::get(ui.ctx()).text_faint));
            if crate::widgets::secondary_button(ui, tl!("Reset"), 80.0).clicked() {
                for (k, v) in [
                    ("scale", 1.0),
                    ("brightness", 1.0),
                    ("aspect", 1.0),
                    ("source", 1.0),
                    ("artifacts", 1.0),
                    ("rotation", 0.0),
                    ("hue", 0.0),
                    ("tint", 0.0),
                ] {
                    f.insert(k.into(), json!(v));
                }
            }
        });
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.set_width(250.0);
            crate::widgets::section_label(ui, tl!("Edit"));
            control(ui, f, ("Scale", "scale", range::SCALE, 1.0, "×", 0.01));
            control(ui, f, ("Brightness", "brightness", range::BRIGHTNESS, 1.0, "×", 0.01));
            control(ui, f, ("Aspect", "aspect", range::ASPECT, 1.0, "×", 0.01));
            control(ui, f, ("Source", "source", range::SOURCE, 1.0, "×", 0.01));
            control(ui, f, ("Artifacts", "artifacts", range::ARTIFACTS, 1.0, "×", 0.01));
            control(ui, f, ("Rotation", "rotation", range::ROTATION, 0.0, "°", 1.0));
        });
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.set_width(250.0);
            crate::widgets::section_label(ui, tl!("Color"));
            let mut hue = num(f, "hue", 0.0);
            let stops = crate::widgets::hue_stops();
            let gestures = crate::widgets::RowGestures { reset: Some(0.0), wheel_step: Some(10.0), ..Default::default() };
            if crate::widgets::slider_row_with(ui, tl!("Hue"), &mut hue, 0.0..=360.0, "°", Some(&stops), gestures).changed() {
                put(f, "hue", hue, range::HUE, 1.0);
            }
            control(ui, f, ("Tint", "tint", range::TINT, 0.0, "", 0.01));
            ui.add_space(4.0);
            crate::widgets::section_label(ui, tl!("Texture"));
            let mut kind = f.get("texture").and_then(Value::as_str).and_then(TextureKind::from_id).unwrap_or_default();
            let options: Vec<(TextureKind, &str)> = TextureKind::ALL.iter().map(|k| (*k, k.label())).collect();
            if crate::widgets::dropdown(ui, "lens-flare-texture", tl!("Texture"), &mut kind, &options, 160.0) {
                f.insert("texture".into(), json!(kind.id()));
            }
            ui.add_enabled_ui(kind != TextureKind::None, |ui| {
                control(ui, f, ("Opacity", "textureOpacity", (0.0, 1.0), 0.6, "", 0.01));
                ui.horizontal(|ui| {
                    for (k, label) in [("flipX", "Flip X"), ("flipY", "Flip Y")] {
                        let mut on = flag(f, k);
                        if crate::widgets::checkbox(ui, &mut on, tl!(label)).changed() {
                            f.insert(k.into(), json!(on));
                        }
                    }
                });
            });
        });
    });
}

#[cfg(test)]
mod tests;
