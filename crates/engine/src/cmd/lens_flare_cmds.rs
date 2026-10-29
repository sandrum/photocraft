//! Lens Flare fill layers (Layer › New Fill Layer › Lens Flare…): an editable flare from the
//! preset library (anamorphic, spherical, sun flares, streaks) that stays adjustable until the
//! image is final. Creating one and editing one are each one undoable step; the flare is a fill
//! layer, so its masks, opacity, blend mode (Screen by default) and stacking work as for any
//! layer, and several flare layers can light one image.

use photocraft_color::BlendMode;
use photocraft_doc::lens_flare::{self, FlareCategory, LensFlare, TextureKind, range};
use photocraft_doc::{Document, Fill, Layer, LayerContent, LayerId};
use serde_json::{Value, json};

use crate::commands::{CommandSpec, check_pixels_unlocked, layer_param};
use crate::{EngineError, Result, Session};

/// Layer › New Fill Layer › Lens Flare….
pub const CREATE: &str = "layer.newFillLayer.lensFlare";
/// Edit a Lens Flare fill layer (its dialog's OK).
pub const SET: &str = "layer.fill.lensFlare.set";
/// The preset library.
pub const PRESETS: &str = "layer.fill.lensFlare.presets";
/// Give a Lens Flare layer a look (the dialog's Filters tab).
pub const LOOK: &str = "layer.fill.lensFlare.look";
/// The preset a new flare starts from.
pub const DEFAULT_PRESET: &str = "Blue Horizon";

pub mod looks;

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

/// The flare `p` asks for, each param optional and defaulting to `base`: `preset` replaces the
/// elements (keeping the user's controls), the rest set the controls.
pub fn edited(cmd: &str, base: &LensFlare, p: &Value) -> Result<LensFlare> {
    let mut f = base.clone();
    if let Some(v) = p.get("preset").filter(|v| !v.is_null()) {
        let name = v.as_str().unwrap_or_default();
        let pr = lens_flare::preset(name).ok_or_else(|| bad(cmd, format!("no lens flare preset \"{name}\" (see {PRESETS})")))?;
        f.category = pr.category;
        f.preset = pr.name.into();
        f.elements = pr.elements();
    }
    let num = |k: &str, (min, max): (f32, f32)| -> Result<Option<f32>> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|v| v.is_finite() && (f64::from(min)..=f64::from(max)).contains(v))
                .map(|v| Some(v as f32))
                .ok_or_else(|| bad(cmd, format!("`{k}` must be a number within {min}..{max}"))),
        }
    };
    let flag = |k: &str| -> Result<Option<bool>> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or_else(|| bad(cmd, format!("`{k}` is true or false"))),
        }
    };
    if let Some(v) = num("x", range::POSITION)? {
        f.source.0 = v;
    }
    if let Some(v) = num("y", range::POSITION)? {
        f.source.1 = v;
    }
    for (k, r, slot) in [
        ("scale", range::SCALE, &mut f.scale),
        ("brightness", range::BRIGHTNESS, &mut f.brightness),
        ("aspect", range::ASPECT, &mut f.aspect),
        ("source", range::SOURCE, &mut f.source_gain),
        ("artifacts", range::ARTIFACTS, &mut f.artifacts),
        ("rotation", range::ROTATION, &mut f.rotation),
        ("hue", range::HUE, &mut f.hue),
        ("tint", range::TINT, &mut f.tint),
        ("textureOpacity", (0.0, 1.0), &mut f.texture.opacity),
    ] {
        if let Some(v) = num(k, r)? {
            *slot = v;
        }
    }
    if let Some(v) = p.get("texture").filter(|v| !v.is_null()) {
        let id = v.as_str().unwrap_or_default();
        f.texture.kind = TextureKind::from_id(id).ok_or_else(|| {
            let ids: Vec<&str> = TextureKind::ALL.iter().map(|t| t.id()).collect();
            bad(cmd, format!("`texture` is one of {}", ids.join(", ")))
        })?;
    }
    if let Some(v) = flag("flipX")? {
        f.texture.flip_x = v;
    }
    if let Some(v) = flag("flipY")? {
        f.texture.flip_y = v;
    }
    Ok(f.sanitized())
}

/// The flare's controls as command params (what the dialog edits).
pub fn params_of(f: &LensFlare) -> Value {
    json!({
        "preset": f.preset,
        "category": f.category.id(),
        "x": f.source.0,
        "y": f.source.1,
        "scale": f.scale,
        "brightness": f.brightness,
        "aspect": f.aspect,
        "source": f.source_gain,
        "artifacts": f.artifacts,
        "rotation": f.rotation,
        "hue": f.hue,
        "tint": f.tint,
        "texture": f.texture.kind.id(),
        "textureOpacity": f.texture.opacity,
        "flipX": f.texture.flip_x,
        "flipY": f.texture.flip_y,
    })
}

/// The flare a new layer gets: `preset` (default [`DEFAULT_PRESET`]) with `p`'s controls.
fn new_flare(p: &Value) -> Result<LensFlare> {
    let name = match p.get("preset") {
        None | Some(Value::Null) => DEFAULT_PRESET,
        Some(v) => v.as_str().ok_or_else(|| bad(CREATE, "`preset` is a preset name"))?,
    };
    let base = lens_flare::preset(name).ok_or_else(|| bad(CREATE, format!("no lens flare preset \"{name}\" (see {PRESETS})")))?.flare();
    edited(CREATE, &base, p)
}

fn blend_param(cmd: &str, p: &Value) -> Result<BlendMode> {
    match p.get("blend") {
        None | Some(Value::Null) => Ok(BlendMode::Screen),
        Some(v) => v
            .as_str()
            .and_then(crate::commands::blend_from_str)
            .filter(|m| *m != BlendMode::PassThrough)
            .ok_or_else(|| bad(cmd, "`blend` is a layer blend mode name, e.g. \"screen\" or \"linearDodge\"")),
    }
}

/// Insert a new Lens Flare layer into `doc` above `active` (the CREATE edit, shared with the
/// dialog's preview).
fn insert(doc: &mut Document, active: &mut Option<LayerId>, p: &Value, default_mask: bool) -> Result<LayerId> {
    if !p.is_object() {
        return Err(bad(CREATE, "params must be an object"));
    }
    let flare = new_flare(p)?;
    let blend = blend_param(CREATE, p)?;
    let look = looks::param(CREATE, p)?;
    let mut l = Layer::new(doc.next_layer_name("Lens Flare"), LayerContent::Fill(Fill::LensFlare(Box::new(flare))));
    l.blend = blend;
    crate::commands::mask_new_layer(doc, &mut l, p, CREATE, default_mask)?;
    let id = crate::layer_place::insert_new(doc, *active, l, crate::layer_place::Place::Auto);
    *active = Some(id);
    if let Some(look) = look {
        looks::apply(doc, id, look)?;
    }
    Ok(id)
}

/// The id of the look `id`'s flare has ("none" without one).
pub fn look_of(doc: &Document, id: LayerId) -> String {
    looks::find(doc, id).map_or_else(|| looks::NONE.to_string(), |(_, look)| look)
}

fn create(s: &mut Session, p: &Value) -> Result<Value> {
    let p = if p.is_null() { json!({}) } else { p.clone() };
    let default_mask = crate::commands::layer_panel_option(s, "useDefaultMasksOnFillLayers", true);
    let id = s.edit("New Lens Flare Layer", |doc, active| insert(doc, active, &p, default_mask))?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let flare = match st.doc.layer(id).map(|l| &l.content) {
        Some(LayerContent::Fill(Fill::LensFlare(f))) => params_of(f),
        _ => Value::Null,
    };
    Ok(json!({"layer": id.0, "flare": flare, "look": look_of(&st.doc, id)}))
}

/// The target layer, its flare and the flare `p` gives it, checked against the active document.
fn target(s: &Session, p: &Value) -> Result<(LayerId, LensFlare, LensFlare)> {
    if !p.is_object() || p.get("layer").is_some_and(|v| v.as_u64().is_none() && !v.is_string()) {
        return Err(bad(SET, "params must be an object and layer a layer id or name"));
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if let Some(document) = p.get("document")
        && document.as_u64() != Some(st.doc.id.0)
    {
        return Err(bad(SET, "the target document is no longer active"));
    }
    let id = layer_param(s, p)?;
    let layer = st.doc.layer(id).ok_or(EngineError::NoLayer(id))?;
    let LayerContent::Fill(Fill::LensFlare(original)) = &layer.content else { return Err(bad(SET, "the target must be a Lens Flare layer")) };
    let flare = edited(SET, original, p)?;
    Ok((id, (**original).clone(), flare))
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    let (id, original, flare) = target(s, p)?;
    let blend = match p.get("blend") {
        None | Some(Value::Null) => None,
        Some(_) => Some(blend_param(SET, p)?),
    };
    let look = looks::param(SET, p)?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let blend_changes = blend.is_some_and(|b| st.doc.layer(id).is_some_and(|l| l.blend != b));
    let look_changes = look.is_some_and(|k| looks::changes(&st.doc, id, k));
    if flare != original || blend_changes {
        check_pixels_unlocked(&st.doc, id)?;
    }
    if flare != original || blend_changes || look_changes {
        s.edit("Lens Flare", |doc, _| {
            let l = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
            l.content = LayerContent::Fill(Fill::LensFlare(Box::new(flare.clone())));
            if let Some(b) = blend {
                l.blend = b;
            }
            if let Some(k) = look {
                looks::apply(doc, id, k)?;
            }
            Ok(())
        })?;
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    Ok(json!({"layer": id.0, "flare": params_of(&flare), "look": look_of(&st.doc, id)}))
}

/// A look for a Lens Flare layer: replaces its look group (or removes it with "none"), one step.
fn set_look(s: &mut Session, p: &Value) -> Result<Value> {
    let look = looks::param(LOOK, p)?.ok_or_else(|| bad(LOOK, "`look` is required (\"none\" removes the look)"))?;
    let (id, _, _) = target(s, p).map_err(|e| match e {
        EngineError::BadParams { msg, .. } => bad(LOOK, msg),
        e => e,
    })?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if looks::changes(&st.doc, id, look) {
        s.edit(&format!("Look: {}", look.name), |doc, _| looks::apply(doc, id, look).map(|_| ()))?;
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let group = looks::find(&st.doc, id).map(|(g, _)| g.0);
    Ok(json!({"layer": id.0, "look": look_of(&st.doc, id), "group": group}))
}

fn presets(_: &mut Session, p: &Value) -> Result<Value> {
    let only = match p.get("category") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_str()
                .and_then(|c| FlareCategory::ALL.into_iter().find(|k| k.id().eq_ignore_ascii_case(c) || k.label().eq_ignore_ascii_case(c)))
                .ok_or_else(|| bad(PRESETS, "`category` is anamorphic, spherical, sunFlare or streak"))?,
        ),
    };
    let list: Vec<Value> = FlareCategory::ALL
        .into_iter()
        .filter(|c| only.is_none_or(|o| o == *c))
        .map(|c| json!({"category": c.id(), "label": c.label(), "presets": lens_flare::presets(c).map(|p| p.name).collect::<Vec<_>>()}))
        .collect();
    let textures: Vec<&str> = TextureKind::ALL.iter().map(|t| t.id()).collect();
    let looks: Vec<Value> = looks::LOOKS.iter().map(|l| json!({"id": l.id, "name": l.name})).collect();
    Ok(json!({"categories": list, "textures": textures, "looks": looks}))
}

/// The active document as [`SET`] or [`CREATE`] with `p` would leave it, without a history
/// step: the Lens Flare dialog's live preview, so it shows exactly what OK commits.
pub fn preview(s: &Session, cmd: &str, p: &Value) -> Result<Document> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let mut doc = (*st.doc).clone();
    match cmd {
        SET => {
            let (id, _, flare) = target(s, p)?;
            let l = doc.layer_mut(id).ok_or(EngineError::NoLayer(id))?;
            l.content = LayerContent::Fill(Fill::LensFlare(Box::new(flare)));
            if p.get("blend").is_some_and(|v| !v.is_null()) {
                l.blend = blend_param(SET, p)?;
            }
            if let Some(k) = looks::param(SET, p)? {
                looks::apply(&mut doc, id, k)?;
            }
        }
        CREATE => {
            let mut active = st.active_layer;
            let default_mask = crate::commands::layer_panel_option(s, "useDefaultMasksOnFillLayers", true);
            insert(&mut doc, &mut active, p, default_mask)?;
        }
        other => return Err(bad(other, "no Lens Flare preview for this command")),
    }
    Ok(doc)
}

const CREATE_PARAMS: &str = r##"{"preset":name="Blue Horizon","x":-1..2?,"y":-1..2? (light source as a fraction of the canvas),"scale":0.1..5?,"brightness":0..3?,"aspect":0.25..4?,"source":0..3? (glow/streak strength),"artifacts":0..3? (ghost strength),"rotation":-180..180?,"hue":0..360?,"tint":0..1? (colour wheel),"texture":"none|filmGrain|dust|bigSpots|hexDust|raindrops|scratches|smudge"?,"textureOpacity":0..1?,"flipX":bool?,"flipY":bool?,"blend":mode name="screen","look":look id="none" (a colour grade above the flare: adjustment layers in a "… Look" group; see layer.fill.lensFlare.presets),"path":"work"|saved path name|{…path}? (the active path: becomes the vector mask; else the selection is the layer mask)} → {layer, flare, look}"##;
const SET_PARAMS: &str = r##"{"layer":id|name? (default active; a Lens Flare layer),"document":id? (reject a stale dialog),"preset":name? (replaces the elements, keeps the controls),"x","y","scale","brightness","aspect","source","artifacts","rotation","hue","tint","texture","textureOpacity","flipX","flipY","blend","look" as for layer.newFillLayer.lensFlare; omitted values keep the layer's} → {layer, flare, look}"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: CREATE,
            label: "Lens Flare…",
            menu: &["Layer", "New Fill Layer"],
            shortcut: None,
            params: CREATE_PARAMS,
            enabled: |s| s.active().map(|_| ()).ok_or_else(|| "no document open".into()),
            run: create,
            journal: true,
        },
        CommandSpec {
            id: SET,
            label: "Lens Flare…",
            menu: &[],
            shortcut: None,
            params: SET_PARAMS,
            enabled: |s| s.active().map(|_| ()).ok_or_else(|| "no document open".into()),
            run: set,
            journal: true,
        },
        CommandSpec {
            id: PRESETS,
            label: "Lens Flare Presets",
            menu: &[],
            shortcut: None,
            params: r##"{"category":"anamorphic|spherical|sunFlare|streak"?} → {categories:[{category,label,presets:[name]}],textures:[id],looks:[{id,name}]}"##,
            enabled: |_| Ok(()),
            run: presets,
            journal: false,
        },
        CommandSpec {
            id: LOOK,
            label: "Lens Flare Look",
            menu: &[],
            shortcut: None,
            params: r##"{"layer":id|name? (default active; a Lens Flare layer),"document":id? (reject a stale dialog),"look":"none|goldenHour|blueHour|tealAmber|silverBleach|fadedPrint|moonlight|oldSepia|punch|vivid|noir|crossLab" (adjustment layers in a pass-through "… Look" group directly above the flare; a new look replaces the old, "none" removes it)} → {layer, look, group}"##,
            enabled: |s| s.active().map(|_| ()).ok_or_else(|| "no document open".into()),
            run: set_look,
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests;
