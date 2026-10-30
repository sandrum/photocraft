//! Looks for a Light Flare layer (the dialog's Filters tab): a colour grade made of ordinary
//! adjustment layers (Curves, Hue/Saturation, Color Balance, Photo Filter, Vibrance, …) in one
//! pass-through group directly above the flare, so it grades the whole image, flare included,
//! and stays editable like any adjustment. The group carries an XMP tag naming the look and its
//! flare layer, so picking another look replaces it and "None" removes it instead of stacking.

use photocraft_color::BlendMode;
use photocraft_doc::{Adjustment, ColorMode, Document, Layer, LayerContent, LayerId};
use serde_json::Value;

use super::bad;
use crate::Result;

/// A look: an id (the command's `look` param), its display name, and its adjustment layers
/// bottom to top as `(kind, params)` for [`crate::adjust_params::from_params`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub id: &'static str,
    pub name: &'static str,
    layers: &'static [(&'static str, &'static str)],
}

/// The look that removes a flare's look.
pub const NONE: &str = "none";

/// The library, "None" first. Original grades built from PhotoCraft's own adjustments.
pub const LOOKS: &[Look] = &[
    Look { id: NONE, name: "None", layers: &[] },
    Look { id: "goldenHour", name: "Golden Hour", layers: &[("photoFilter", r#"{"filter":"warming85","density":35}"#), ("vibrance", r#"{"vibrance":15}"#)] },
    Look {
        id: "blueHour",
        name: "Blue Hour",
        layers: &[("photoFilter", r#"{"filter":"cooling80","density":35}"#), ("curves", r#"{"points":[[0,8],[255,250]]}"#)],
    },
    Look {
        id: "tealAmber",
        name: "Teal & Amber",
        layers: &[("colorBalance", r#"{"shadows":[-20,0,25],"midtones":[0,0,0],"highlights":[18,0,-22]}"#), ("vibrance", r#"{"vibrance":20}"#)],
    },
    Look {
        id: "silverBleach",
        name: "Silver Bleach",
        layers: &[("hueSaturation", r#"{"saturation":-50}"#), ("curves", r#"{"points":[[0,0],[64,46],[192,214],[255,255]]}"#)],
    },
    Look {
        id: "fadedPrint",
        name: "Faded Print",
        layers: &[("curves", r#"{"points":[[0,36],[128,130],[255,232]]}"#), ("hueSaturation", r#"{"saturation":-25}"#)],
    },
    Look {
        id: "moonlight",
        name: "Moonlight",
        layers: &[
            ("curves", r#"{"points":[[0,0],[128,96],[255,215]]}"#),
            ("hueSaturation", r#"{"saturation":-55}"#),
            ("photoFilter", r#"{"filter":"deepBlue","density":40}"#),
        ],
    },
    Look {
        id: "oldSepia",
        name: "Old Sepia",
        layers: &[("hueSaturation", r#"{"colorize":true,"hue":35,"saturation":30}"#), ("curves", r#"{"points":[[0,20],[255,240]]}"#)],
    },
    Look {
        id: "punch",
        name: "Punch",
        layers: &[("curves", r#"{"points":[[0,0],[64,44],[192,214],[255,255]]}"#), ("vibrance", r#"{"vibrance":25,"saturation":5}"#)],
    },
    Look { id: "vivid", name: "Vivid", layers: &[("vibrance", r#"{"vibrance":45,"saturation":15}"#), ("brightnessContrast", r#"{"contrast":15}"#)] },
    Look { id: "noir", name: "Noir", layers: &[("blackWhite", "{}"), ("curves", r#"{"points":[[0,0],[70,40],[190,220],[255,255]]}"#)] },
    Look {
        id: "crossLab",
        name: "Cross Lab",
        layers: &[("curves", r#"{"red":[[0,0],[64,50],[192,215],[255,255]],"green":[[0,0],[64,58],[192,200],[255,255]],"blue":[[0,30],[255,215]]}"#)],
    },
];

/// The look with this id (case-insensitive) or display name.
pub fn look(id: &str) -> Option<&'static Look> {
    LOOKS.iter().find(|l| l.id.eq_ignore_ascii_case(id) || l.name.eq_ignore_ascii_case(id))
}

impl Look {
    /// The look's adjustments, bottom to top, for a document in `mode`.
    pub fn adjustments(&self, mode: ColorMode) -> Result<Vec<Adjustment>> {
        self.layers
            .iter()
            .map(|(kind, params)| {
                let p: Value = serde_json::from_str(params).map_err(|e| bad(super::LOOK, format!("look \"{}\": {e}", self.id)))?;
                crate::adjust_params::from_params(kind, &p, None, mode)
            })
            .collect()
    }
}

const TAG_LOOK: &str = "pcLightFlare:look=\"";
const TAG_LAYER: &str = "pcLightFlare:layer=\"";

/// The XMP packet that marks a look group: which look, for which flare layer.
fn tag(look: &str, flare: LayerId) -> String {
    format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
         <rdf:Description rdf:about=\"\" xmlns:pcLightFlare=\"urn:photocraft:light-flare:1\" {TAG_LOOK}{look}\" {TAG_LAYER}{}\"/>\
         </rdf:RDF></x:xmpmeta>",
        flare.0
    )
}

/// The quoted value after `key` in `xmp`.
fn attr<'a>(xmp: &'a str, key: &str) -> Option<&'a str> {
    let rest = xmp.split_once(key)?.1;
    Some(rest.split_once('"')?.0)
}

/// A look group's (look id, flare layer id), if `l` is one.
fn tagged(l: &Layer) -> Option<(&str, Option<u64>)> {
    if !l.is_group() {
        return None;
    }
    let xmp = l.xmp.as_deref()?;
    let look = attr(xmp, TAG_LOOK)?;
    Some((look, attr(xmp, TAG_LAYER).and_then(|v| v.parse().ok())))
}

/// The look group of `flare` and its look id: the group tagged with the flare's id, else (ids
/// change when a file goes through PSD) a tagged group directly above the flare.
pub fn find(doc: &Document, flare: LayerId) -> Option<(LayerId, String)> {
    let by_id = doc.walk().into_iter().find_map(|(_, _, l)| tagged(l).filter(|(_, f)| *f == Some(flare.0)).map(|(look, _)| (l.id, look.to_string())));
    by_id.or_else(|| {
        let path = doc.path_of(flare)?;
        let (&i, parent) = path.split_last()?;
        let sib = if parent.is_empty() { &doc.layers[..] } else { doc.layer_at(parent)?.children()? };
        let above = sib.iter().skip(i.checked_add(1)?).find(|l| !l.clipped)?;
        tagged(above).map(|(look, _)| (above.id, look.to_string()))
    })
}

/// The id of the look `p["look"]` names (`None` when the param is absent).
pub fn param(cmd: &str, p: &Value) -> Result<Option<&'static Look>> {
    match p.get("look") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let id = v.as_str().ok_or_else(|| bad(cmd, "`look` is a look id (see layer.fill.lightFlare.presets)"))?;
            look(id).map(Some).ok_or_else(|| {
                let ids: Vec<&str> = LOOKS.iter().map(|l| l.id).collect();
                bad(cmd, format!("no look \"{id}\": one of {}", ids.join(", ")))
            })
        }
    }
}

/// Would giving `flare` the look `want` change `doc`?
pub fn changes(doc: &Document, flare: LayerId, want: &Look) -> bool {
    match find(doc, flare) {
        Some((_, current)) => current != want.id,
        None => want.id != NONE,
    }
}

/// Give `flare` the look `want`: drop its current look group and (unless "None") add the new
/// one directly above the flare and the layers clipped to it. Returns the new group's id.
pub fn apply(doc: &mut Document, flare: LayerId, want: &Look) -> Result<Option<LayerId>> {
    if !changes(doc, flare, want) {
        return Ok(find(doc, flare).map(|(id, _)| id));
    }
    let adjustments = want.adjustments(doc.mode)?;
    if let Some((old, _)) = find(doc, flare) {
        doc.remove(old);
    }
    if want.id == NONE {
        return Ok(None);
    }
    let children: Vec<Layer> = adjustments.into_iter().map(|a| Layer::new(a.label().to_string(), LayerContent::Adjustment(a))).collect();
    let mut group = Layer::group(format!("{} Look", want.name), children);
    group.blend = BlendMode::PassThrough;
    group.xmp = Some(tag(want.id, flare).into());
    if let LayerContent::Group(g) = &mut group.content {
        g.expanded = false;
    }
    let id = group.id;
    let path = doc.path_of(flare).ok_or(crate::EngineError::NoLayer(flare))?;
    let (&i, parent) = path.split_last().ok_or(crate::EngineError::NoLayer(flare))?;
    let sib =
        if parent.is_empty() { &mut doc.layers } else { doc.layer_at_mut(parent).and_then(Layer::children_mut).ok_or(crate::EngineError::NoLayer(flare))? };
    // Above the layers clipped to the flare, so the grade isn't clipped to it.
    let mut at = i.saturating_add(1).min(sib.len());
    while sib.get(at).is_some_and(|l| l.clipped) {
        at += 1;
    }
    sib.insert(at, group);
    Ok(Some(id))
}
