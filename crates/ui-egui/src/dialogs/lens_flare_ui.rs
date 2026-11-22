//! The Light Flare dialog: Layer › New Fill Layer › Light Flare… and Layer › Layer Content Options
//! on a Light Flare layer open it. A preset library in four tabs (Anamorphic, Spherical, Sun Flares,
//! Streaks) with rendered thumbnails, a Filters tab of looks, a position pad for the light source,
//! a large preview of the image with the flare that is also the light source pad, the flare's
//! controls (Scale, Brightness, Aspect, Source Size, Artifacts, Rotation), Hue and Tint, and a
//! lens-dirt Texture with its Opacity and Flip X / Flip Y, previewed live on the canvas from a
//! document snapshot. OK runs one engine command (`layer.newFillLayer.lightFlare` for a new layer,
//! `layer.fill.lightFlare.set` for an existing one), so it is one History step; Cancel changes
//! nothing. The fields are the command's params, so `ui.dialog.set` / `ui.dialog.confirm` drive it.
//!
//! While the dialog is on top a click or drag on the image places the light (`canvas_input`'s
//! `FlareLight` pick).

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use craft::behavior::ColumnWidth;
use craft::ui::buttons::Button;
use craft::ui::choice::{Checkbox, Segmented};
use craft::ui::contract::{Bind, Entry, Gap, Props as _, Show as _, ShowContainer as _};
use craft::ui::dialog::{DialogId, DialogResult, DialogStack, StackDialog, dialog_id};
use craft::ui::display::{Divider, Text};
use craft::ui::form::SliderField;
use craft::ui::layout::{CellAlign, GridFit};
use craft::ui::layout_components::{Grid, Stack};
use craft::ui::patterns::form_dialogs::FormDialog;
use craft::ui::pickers::Select;
use craft::ui::runtime::env::Label;
use craft::ui::style::TypeRole;
use egui::{Color32, Rect, Sense, Stroke, Vec2, pos2, vec2};
use photocraft_doc::lens_flare::{self, FlareCategory, TextureKind, range};
use photocraft_doc::{DocId, Document, Fill, LayerContent};
use photocraft_engine::lens_flare_cmds as cmds;
use photocraft_engine::lens_flare_cmds::looks;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

mod canvas;
pub(crate) use canvas::{place_at, placing};

/// The fields that are command params (besides the target in `__params`).
const KEYS: [&str; 16] = [
    "preset",
    "x",
    "y",
    "scale",
    "brightness",
    "aspect",
    "source",
    "artifacts",
    "rotation",
    "hue",
    "tint",
    "texture",
    "textureOpacity",
    "flipX",
    "flipY",
    "look",
];
/// The dialog body's width: five preset thumbnails across.
const WIDTH: f32 = 720.0;
const THUMB: Vec2 = vec2(128.0, 72.0);
/// A look's thumbnail in the Filters tab: six across.
const LOOK_THUMB: Vec2 = vec2(108.0, 61.0);
/// The largest the preview (and light source pad) is drawn.
const PREVIEW: Vec2 = vec2(450.0, 330.0);
/// The tab that holds the looks (the others are preset categories).
const FILTERS: &str = "filters";

fn label(text: &str) -> Label {
    Label::from(text.to_string())
}

/// The app's Light Flare state: the open counter and the cached canvas preview.
#[derive(Default)]
pub(crate) struct State {
    opens: u64,
    preview: Option<Preview>,
}

/// The cached canvas preview of one set of values.
pub(crate) struct Preview {
    document: DocId,
    revision: u64,
    serial: u64,
    params: Value,
    shown: Arc<Document>,
    key: u64,
}

/// The hosted dialog: its values (the command's params plus `__`-prefixed bookkeeping) and the
/// images the app hands it each frame.
pub(crate) struct LightFlareDialog {
    fields: Map<String, Value>,
    /// The document's width over its height, for the pad.
    aspect: f32,
    thumbs: Option<canvas::Thumbs>,
    /// The large preview: the image with the flare, as OK would leave it.
    preview: Option<egui::TextureHandle>,
}

impl StackDialog for LightFlareDialog {
    type Result = Map<String, Value>;

    fn id(&self) -> DialogId {
        dialog_id("photocraft.lightFlare")
    }

    fn key(&self) -> Option<&str> {
        Some("lightFlare")
    }

    fn show(&mut self, ui: &mut egui::Ui, _stack: &DialogStack) -> Option<DialogResult<Self::Result>> {
        let LightFlareDialog { fields, aspect, thumbs, preview } = self;
        let (aspect, thumbs, preview) = (*aspect, thumbs.as_ref(), preview.as_ref());
        let shown = FormDialog::new(label(tl!("Light Flare")), Bind::bound(fields)).alt_resets(false).show(ui, |ui, _form, f| {
            body(ui, f, aspect, thumbs, preview);
            true
        });
        match (shown.accepted, shown.closed) {
            (Some(f), _) => Some(DialogResult::Ok(f)),
            (None, Some(_)) => Some(DialogResult::Cancel),
            (None, None) => None,
        }
    }

    fn confirm(&mut self, _ctx: &egui::Context) -> Option<Self::Result> {
        Some(self.fields.clone())
    }

    fn json_fields(&self) -> Option<Value> {
        Some(Value::Object(self.fields.clone()))
    }

    fn set_json_field(&mut self, ctx: &egui::Context, field: &str, value: Value) -> Result<(), String> {
        if !(KEYS.contains(&field) || field == "__tab") {
            return Err(format!("the Light Flare dialog has no field `{field}`"));
        }
        self.fields.insert(field.into(), value);
        ctx.request_repaint();
        Ok(())
    }
}

fn fields(command: &str, target: Value, values: Value, serial: u64) -> Map<String, Value> {
    let mut f = Map::new();
    f.insert("__command".into(), json!(command));
    f.insert("__params".into(), target);
    f.insert("__serial".into(), json!(serial));
    f.insert("__tab".into(), values.get("category").cloned().unwrap_or(json!("anamorphic")));
    for k in KEYS {
        if let Some(v) = values.get(k) {
            f.insert(k.into(), v.clone());
        }
    }
    f.insert("__orig".into(), values);
    f
}

fn start(app: &mut PhotocraftApp, command: &str, target: Value, values: Value) -> DialogId {
    app.light_flare.opens += 1;
    app.light_flare.preview = None;
    let f = fields(command, target, values, app.light_flare.opens);
    let aspect = app.session.active().map_or(16.0 / 9.0, |st| st.doc.size.width.max(1) as f32 / st.doc.size.height.max(1) as f32);
    let thumbs = canvas::thumbs(app, app.ctx(), &f);
    app.framework.dialogs.open(LightFlareDialog { fields: f, aspect, thumbs, preview: None })
}

/// Edit the active Light Flare layer (Layer › Layer Content Options…).
pub(crate) fn open(app: &mut PhotocraftApp) -> Option<DialogId> {
    let st = app.session.active()?;
    let layer = st.active_layer?;
    let LayerContent::Fill(Fill::LensFlare(flare)) = &st.doc.layer(layer)?.content else { return None };
    let mut values = cmds::params_of(flare);
    values["look"] = json!(cmds::look_of(&st.doc, layer));
    let target = json!({"layer": layer.0, "document": st.doc.id.0});
    Some(start(app, cmds::SET, target, values))
}

/// Layer › New Fill Layer › Light Flare…: the dialog in front of creating the layer, starting from
/// the default preset.
pub(crate) fn open_new(app: &mut PhotocraftApp) -> Result<DialogId, String> {
    if let Some(Err(why)) = photocraft_engine::commands::find(cmds::CREATE).map(|c| (c.enabled)(&app.session)) {
        return Err(why);
    }
    let st = app.session.active().ok_or("no document open")?;
    let flare = lens_flare::preset(cmds::DEFAULT_PRESET).ok_or("no light flare presets")?.flare();
    let mut values = cmds::params_of(&flare);
    values["look"] = json!(looks::NONE);
    let target = json!({"document": st.doc.id.0});
    Ok(start(app, cmds::CREATE, target, values))
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

/// The host's result of the dialog: Cancel drops the preview, OK commits.
pub(crate) fn take(app: &mut PhotocraftApp, _ctx: &egui::Context) {
    let Some(result) = app.framework.dialogs.take::<LightFlareDialog>() else { return };
    app.light_flare.preview = None;
    if let DialogResult::Ok(f) = result
        && let Err(why) = commit(app, &f)
    {
        app.ui.status = why;
        app.ui.status_error = true;
    }
}

/// OK: one engine command. A stale document never receives the edit.
fn commit(app: &mut PhotocraftApp, f: &Map<String, Value>) -> Result<(), String> {
    if !target_is_active(app, f) {
        return Err("the target document is no longer active".into());
    }
    if unchanged(f) {
        return Ok(());
    }
    let (cmd, params) = command(app, f).ok_or("invalid Light Flare dialog")?;
    app.run(cmd, params).map(|_| ())
}

/// The canvas preview while the dialog is open: the document as OK would leave it. Also hands the
/// dialog the pad and look images for the current document state.
pub(crate) fn display_doc(app: &mut PhotocraftApp, idx: usize) -> Option<(Arc<Document>, u64)> {
    let f = app.framework.dialogs.get::<LightFlareDialog>()?.fields.clone();
    if let Some(thumbs) = canvas::thumbs(app, app.ctx(), &f)
        && let Some(d) = app.framework.dialogs.get_mut::<LightFlareDialog>()
    {
        d.thumbs = Some(thumbs);
    }
    let st = app.session.documents().get(idx)?;
    if !target_is_active(app, &f) || app.session.active_index() != Some(idx) {
        return None;
    }
    if unchanged(&f) {
        // The layer as it is: the dialog's preview shows the document itself.
        let (doc, key) = (st.doc.clone(), (st.doc.id.0 << 32) ^ st.revision);
        show_preview(app, &doc, key);
        return None;
    }
    let shown = previewed(app, idx, &f)?;
    show_preview(app, &shown.0, shown.1);
    Some(shown)
}

/// Hand the dialog its large preview of `doc`.
fn show_preview(app: &mut PhotocraftApp, doc: &Document, key: u64) {
    let tex = canvas::preview_texture(app, doc, key);
    if let Some(d) = app.framework.dialogs.get_mut::<LightFlareDialog>() {
        d.preview = Some(tex);
    }
}

/// The document as OK would leave it, rendered once per set of values.
fn previewed(app: &mut PhotocraftApp, idx: usize, f: &Map<String, Value>) -> Option<(Arc<Document>, u64)> {
    let st = app.session.documents().get(idx)?;
    let (cmd, params) = command(app, f)?;
    let serial = f.get("__serial").and_then(Value::as_u64).unwrap_or(0);
    let (document, revision) = (st.doc.id, st.revision);
    if let Some(p) = &app.light_flare.preview
        && p.document == document
        && p.revision == revision
        && p.serial == serial
        && p.params == params
    {
        return Some((p.shown.clone(), p.key));
    }
    // Out-of-range values typed by automation show the committed document; OK reports them.
    let doc = cmds::preview(&app.session, cmd, &params).ok()?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (document.0, revision, serial, params.to_string()).hash(&mut hash);
    let key = (1 << 47) | (1 << 46) | (hash.finish() & ((1 << 36) - 1));
    let shown = Arc::new(photocraft_engine::mode_cmds::display_document(&doc).unwrap_or(doc));
    app.light_flare.preview = Some(Preview { document, revision, serial, params, shown: shown.clone(), key });
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

/// A selectable thumbnail cell with its caption; returns its response.
fn cell(ui: &mut egui::Ui, size: Vec2, caption: &str, selected: bool, paint: impl FnOnce(&egui::Painter, Rect)) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(size + vec2(0.0, 18.0), Sense::click());
    let img = Rect::from_min_size(rect.min, size);
    if ui.is_rect_visible(rect) {
        paint(ui.painter(), img);
        let border = if selected {
            Stroke::new(2.0, t.accent)
        } else if resp.hovered() {
            Stroke::new(1.0, t.text_dim)
        } else {
            Stroke::new(1.0, t.field_border)
        };
        ui.painter().rect_stroke(img, t.radius_sm, border, egui::StrokeKind::Inside);
        let color = if selected { t.accent } else { t.text_dim };
        ui.painter().text(pos2(img.center().x, img.bottom() + 9.0), egui::Align2::CENTER_CENTER, caption, egui::FontId::proportional(11.0), color);
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, caption));
    resp
}

/// The room between side-by-side columns, which holds the rule between them.
const GAP: f32 = 28.0;

/// The preview beside the controls, and the room between them.
const TOP: [ColumnWidth; 3] = [ColumnWidth::Fixed(PREVIEW.x), ColumnWidth::Fixed(GAP), ColumnWidth::Fixed(WIDTH - PREVIEW.x - GAP)];
/// Color beside Texture.
const BOTTOM: [ColumnWidth; 3] = [ColumnWidth::Fixed((WIDTH - GAP) / 2.0), ColumnWidth::Fixed(GAP), ColumnWidth::Fixed((WIDTH - GAP) / 2.0)];

/// Two columns side by side in a craft grid (`layout`: column, gap, column), so each is its own
/// content column (a stacked slider ends at its column's edge instead of spanning the dialog), with
/// a rule down the middle of the room between them.
fn columns(ui: &mut egui::Ui, layout: &'static [ColumnWidth; 3], mut add: impl FnMut(usize, &mut egui::Ui)) {
    let shown = Grid::new(layout).fit(GridFit::Fixed).cells(CellAlign::Top).gap(Gap::None).show(ui, |ui, _| {
        Stack::new().fill_width().show(ui, |ui, _| add(0, ui));
        Stack::new().show(ui, |_, _| {});
        Stack::new().fill_width().show(ui, |ui, _| add(1, ui));
    });
    // The rule between the columns, the height of the taller one.
    let rect = shown.output.rect;
    let ColumnWidth::Fixed(first) = layout[0] else { return };
    let x = rect.left() + first + GAP / 2.0;
    ui.painter().vline(x, rect.y_range(), ui.visuals().widgets.noninteractive.bg_stroke);
}

/// The Filters tab: the image graded by each look; picking one replaces the previous look.
fn look_grid(ui: &mut egui::Ui, f: &mut Map<String, Value>, thumbs: Option<&canvas::Thumbs>) {
    let current = f.get("look").and_then(Value::as_str).unwrap_or(looks::NONE).to_string();
    let indexed: Vec<_> = looks::LOOKS.iter().enumerate().collect();
    for row in indexed.chunks(6) {
        ui.horizontal(|ui| {
            for (i, look) in row {
                let tex = thumbs.and_then(|t| t.looks.get(*i).cloned());
                let resp = cell(ui, LOOK_THUMB, tl!(look.name), current == look.id, |p, img| {
                    p.rect_filled(img, 0.0, Color32::from_rgb(14, 16, 24));
                    if let Some(tex) = tex {
                        p.image(tex.id(), img, canvas::cover_uv(tex.size(), img.aspect_ratio()), Color32::WHITE);
                    }
                });
                if resp.clicked() {
                    f.insert("look".into(), json!(look.id));
                }
            }
        });
    }
}

/// The preset tabs (and Filters) and the thumbnail grid of the selected tab.
fn library(ui: &mut egui::Ui, f: &mut Map<String, Value>, thumbs: Option<&canvas::Thumbs>) {
    let tab = f.get("__tab").and_then(Value::as_str).unwrap_or("anamorphic").to_string();
    let current = f.get("preset").and_then(Value::as_str).unwrap_or_default().to_string();
    let tabs: Vec<Entry<&'static str>> =
        FlareCategory::ALL.iter().map(|c| Entry::new(c.id(), tl!(c.label()))).chain([Entry::new(FILTERS, tl!("Filters"))]).collect();
    let current_tab = tabs.iter().filter_map(|e| e.value().copied()).find(|id| *id == tab).unwrap_or(FILTERS);
    if let Some(&picked) = Segmented::new(tl!("Library"), Bind::given(current_tab), &tabs).show(ui).value() {
        f.insert("__tab".into(), json!(picked));
    }
    ui.add_space(4.0);
    if tab == FILTERS {
        look_grid(ui, f, thumbs);
        return;
    }
    let category = FlareCategory::ALL.into_iter().find(|c| c.id() == tab).unwrap_or_default();
    let presets: Vec<_> = lens_flare::presets(category).collect();
    egui::ScrollArea::vertical().id_salt("lens-flare-presets").max_height(3.0 * (THUMB.y + 26.0)).show(ui, |ui| {
        for row in presets.chunks(5) {
            ui.horizontal(|ui| {
                for p in row {
                    let tex = thumbnail(ui.ctx(), p);
                    let resp = cell(ui, THUMB, tl!(p.name), p.name == current, |painter, img| {
                        painter.image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    });
                    if resp.clicked() {
                        f.insert("preset".into(), json!(p.name));
                    }
                }
            });
        }
    });
}

/// The large preview, which is also the light source pad: the image with the flare as OK would leave
/// it, at the canvas's aspect, with a marker the user clicks or drags to place the light.
fn position_pad(ui: &mut egui::Ui, f: &mut Map<String, Value>, aspect: f32, thumbs: Option<&canvas::Thumbs>, preview: Option<&egui::TextureHandle>) {
    let t = Tokens::get(ui.ctx());
    let aspect = aspect.clamp(0.1, 10.0);
    let (w, h) = if PREVIEW.x / aspect <= PREVIEW.y { (PREVIEW.x, PREVIEW.x / aspect) } else { (PREVIEW.y * aspect, PREVIEW.y) };
    let (rect, resp) = ui.allocate_exact_size(vec2(w.max(40.0), h.max(40.0)), Sense::click_and_drag());
    if (resp.clicked() || resp.dragged() || resp.is_pointer_button_down_on())
        && let Some(p) = resp.interact_pointer_pos().filter(|p| p.x.is_finite() && p.y.is_finite())
    {
        put(f, "x", (p.x - rect.left()) / rect.width(), range::POSITION, 0.001);
        put(f, "y", (p.y - rect.top()) / rect.height(), range::POSITION, 0.001);
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, t.radius_sm, Color32::from_rgb(14, 16, 24));
        // The image with the flare; until that is rendered, the image the flare goes on.
        if let Some(tex) = preview.or(thumbs.map(|t| &t.pad)) {
            painter.image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        let (x, y) = (num(f, "x", 0.3), num(f, "y", 0.3));
        let src = pos2(rect.left() + x * rect.width(), rect.top() + y * rect.height());
        // The flare's axis through the frame centre, where its ghosts fall.
        let mirror = rect.center() + (rect.center() - src);
        painter.line_segment([src, mirror], Stroke::new(1.0, t.text_faint.gamma_multiply(0.5)));
        painter.circle_stroke(src, 7.0, Stroke::new(1.5, Color32::from_black_alpha(160)));
        painter.circle_stroke(src, 7.0, Stroke::new(1.0, Color32::WHITE));
        painter.circle(src, 2.0, Color32::WHITE, Stroke::new(1.0, t.accent));
        ui.painter().rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, tl!("Light Source")));
    resp.on_hover_cursor(egui::CursorIcon::Crosshair);
}

/// A control slider: label, field, range, default, unit and step.
type Ctl = (&'static str, &'static str, (f32, f32), f32, &'static str, f32);

/// One control slider bound to its field.
fn control(ui: &mut egui::Ui, f: &mut Map<String, Value>, (caption, k, r, default, unit, step): Ctl) {
    let mut v = f64::from(num(f, k, default));
    let shown = v;
    let mut slider = SliderField::new(tl!(caption), Bind::bound(&mut v), f64::from(r.0)..=f64::from(r.1));
    if !unit.is_empty() {
        slider = slider.unit(unit);
    }
    slider.show(ui);
    if v != shown {
        put(f, k, v as f32, r, step);
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    Text::new(text.to_string()).role(TypeRole::BodyStrong).show(ui);
}

/// The dialog body, in order: the library, then the light source, its controls, and Color and
/// Texture side by side.
fn body(ui: &mut egui::Ui, f: &mut Map<String, Value>, aspect: f32, thumbs: Option<&canvas::Thumbs>, preview: Option<&egui::TextureHandle>) {
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.set_width(WIDTH);
    library(ui, f, thumbs);
    Divider::horizontal().show(ui);
    columns(ui, &TOP, |i, ui| {
        if i == 0 {
            heading(ui, tl!("Preview"));
            position_pad(ui, f, aspect, thumbs, preview);
            Text::new(tl!("Click or drag on the preview or the image to place the light.").to_string()).role(TypeRole::Caption).show(ui);
            return;
        }
        heading(ui, tl!("Edit"));
        control(ui, f, ("Scale", "scale", range::SCALE, 1.0, "×", 0.01));
        control(ui, f, ("Brightness", "brightness", range::BRIGHTNESS, 1.0, "×", 0.01));
        control(ui, f, ("Aspect", "aspect", range::ASPECT, 1.0, "×", 0.01));
        control(ui, f, ("Source Size", "source", range::SOURCE, 1.0, "×", 0.01));
        control(ui, f, ("Artifacts", "artifacts", range::ARTIFACTS, 1.0, "×", 0.01));
        control(ui, f, ("Rotation", "rotation", range::ROTATION, 0.0, "°", 1.0));
        if Button::new(label(tl!("Reset"))).show(ui).clicked() {
            for (k, v) in
                [("scale", 1.0), ("brightness", 1.0), ("aspect", 1.0), ("source", 1.0), ("artifacts", 1.0), ("rotation", 0.0), ("hue", 0.0), ("tint", 0.0)]
            {
                f.insert(k.into(), json!(v));
            }
        }
    });
    Divider::horizontal().show(ui);
    columns(ui, &BOTTOM, |i, ui| {
        if i == 0 {
            heading(ui, tl!("Color"));
            control(ui, f, ("Hue", "hue", range::HUE, 0.0, "°", 1.0));
            control(ui, f, ("Tint", "tint", range::TINT, 0.0, "", 0.01));
            return;
        }
        heading(ui, tl!("Texture"));
        let kind = f.get("texture").and_then(Value::as_str).and_then(TextureKind::from_id).unwrap_or_default();
        let entries: Vec<Entry<&'static str>> = TextureKind::ALL.iter().map(|k| Entry::new(k.id(), tl!(k.label()))).collect();
        let picked = Select::new(tl!("Texture"), Bind::given(kind.id()), &entries).show(ui);
        if let Some(&id) = picked.events.value() {
            f.insert("texture".into(), json!(id));
        }
        ui.add_enabled_ui(kind != TextureKind::None, |ui| {
            control(ui, f, ("Opacity", "textureOpacity", (0.0, 1.0), 0.6, "", 0.01));
            ui.horizontal(|ui| {
                for (k, caption) in [("flipX", "Flip X"), ("flipY", "Flip Y")] {
                    let mut on = flag(f, k);
                    let before = on;
                    Checkbox::new(Bind::bound(&mut on)).label(tl!(caption)).id_salt(("lens-flare-flip", k)).show(ui);
                    if on != before {
                        f.insert(k.into(), json!(on));
                    }
                }
            });
        });
    });
}

#[cfg(test)]
mod tests;
