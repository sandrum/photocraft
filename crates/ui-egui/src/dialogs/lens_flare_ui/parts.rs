//! The Light Flare dialog's Parts tab: the flare's parts (glows, streaks, stars, rays, rings,
//! ghosts, edge washes) as a list, and the selected part's own settings beside it.
//!
//! The list: a row per part with its number, kind and (for a part off the light) where it sits on
//! the lens axis; its radio selects it and its Show check box hides it (kept, adding no light). Under it: Add Part (a menu of the kinds), Duplicate, Delete, Move Up / Move Down, and the
//! centre point the axis runs through, with Reset Center. Beside it: the selected part's sliders and
//! colour. Everything edits the dialog's `elements` and `hidden` fields (and `centerX` /
//! `centerY`), which OK sends with the other values in one command, so it is one History step.

use craft::ui::buttons::Button;
use craft::ui::choice::{Checkbox, Radio, RadioGroup};
use craft::ui::contract::{Bind, Props as _, Show as _, ShowContainer as _};
use craft::ui::display::Text;
use craft::ui::form::SliderField;
use craft::ui::style::TypeRole;
use photocraft_doc::lens_flare::{FlareElement, PART_KINDS, range};
use serde_json::{Map, Value, json};

use super::{columns, heading, label, num, put};

/// The list beside the selected part's settings.
const LAYOUT: [craft::behavior::ColumnWidth; 3] = [
    craft::behavior::ColumnWidth::Fixed(270.0),
    craft::behavior::ColumnWidth::Fixed(super::GAP),
    craft::behavior::ColumnWidth::Fixed(super::WIDTH - 270.0 - super::GAP),
];
/// The tab's height, as the preset grid's.
const HEIGHT: f32 = super::LIBRARY_HEIGHT;

/// The dialog's parts.
pub(super) fn parts(f: &Map<String, Value>) -> Vec<Value> {
    f.get("elements").and_then(Value::as_array).cloned().unwrap_or_default()
}

/// The indices of the hidden parts, sorted.
pub(super) fn hidden(f: &Map<String, Value>) -> Vec<usize> {
    let mut h: Vec<usize> =
        f.get("hidden").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_u64().map(|v| v as usize)).collect()).unwrap_or_default();
    h.sort_unstable();
    h.dedup();
    h
}

/// The selected part (kept on a part the flare has).
pub(super) fn selected(f: &Map<String, Value>) -> usize {
    let n = parts(f).len();
    f.get("__part").and_then(Value::as_u64).map_or(0, |v| v as usize).min(n.saturating_sub(1))
}

fn store(f: &mut Map<String, Value>, parts: Vec<Value>, hidden: Vec<usize>, select: usize) {
    f.insert("elements".into(), Value::Array(parts));
    f.insert("hidden".into(), json!(hidden));
    f.insert("__part".into(), json!(select));
}

/// The edits the list makes, on the dialog's fields: each keeps `hidden` on the same parts.
pub(super) mod edit {
    use super::{FlareElement, Map, Value, hidden, json, parts, range, store};

    /// Shows or hides part `i`.
    pub fn toggle(f: &mut Map<String, Value>, i: usize) {
        let mut h = hidden(f);
        match h.binary_search(&i) {
            Ok(k) => {
                h.remove(k);
            }
            Err(k) => h.insert(k, i),
        }
        f.insert("hidden".into(), json!(h));
    }

    /// Adds a part of `kind` at the end, selected.
    pub fn add(f: &mut Map<String, Value>, kind: &str) {
        let mut p = parts(f);
        let Some(e) = FlareElement::new_part(kind) else { return };
        if p.len() >= range::MAX_ELEMENTS {
            return;
        }
        p.push(serde_json::to_value(e).unwrap_or_default());
        let n = p.len();
        store(f, p, hidden(f), n - 1);
    }

    /// A copy of part `i` right after it, selected (shown).
    pub fn duplicate(f: &mut Map<String, Value>, i: usize) {
        let mut p = parts(f);
        if i >= p.len() || p.len() >= range::MAX_ELEMENTS {
            return;
        }
        p.insert(i + 1, p[i].clone());
        let h = hidden(f).into_iter().map(|k| if k > i { k + 1 } else { k }).collect();
        store(f, p, h, i + 1);
    }

    /// Deletes part `i`; the part before it is selected.
    pub fn delete(f: &mut Map<String, Value>, i: usize) {
        let mut p = parts(f);
        if i >= p.len() {
            return;
        }
        p.remove(i);
        let h = hidden(f).into_iter().filter(|k| *k != i).map(|k| if k > i { k - 1 } else { k }).collect();
        store(f, p, h, i.saturating_sub(1));
    }

    /// Swaps part `i` with the one `up` (or down) from it, which stays selected.
    pub fn shift(f: &mut Map<String, Value>, i: usize, up: bool) {
        let mut p = parts(f);
        let j = if up { i.checked_sub(1) } else { Some(i + 1) };
        let Some(j) = j.filter(|j| *j < p.len() && i < p.len()) else { return };
        p.swap(i, j);
        let swap = |k: usize| {
            if k == i {
                j
            } else if k == j {
                i
            } else {
                k
            }
        };
        let mut h: Vec<usize> = hidden(f).into_iter().map(swap).collect();
        h.sort_unstable();
        store(f, p, h, j);
    }
}

/// A part's label in the list: its kind, and for a part off the light where it sits on the axis.
fn row_text(k: usize, part: &Value) -> String {
    let kind = part.get("kind").and_then(Value::as_str).unwrap_or_default();
    let name = PART_KINDS.iter().find(|(id, _)| *id == kind).map_or(kind, |(_, l)| *l);
    let name = tl!(name);
    let t = part.get("t").and_then(Value::as_f64).unwrap_or(0.0);
    if matches!(kind, "ring" | "ghost") && t.abs() > 1e-3 {
        crate::i18n::fmt(tl!("{n}. {name} at {t}"), &[("n", &(k + 1).to_string()), ("name", name), ("t", &format!("{t:.2}"))])
    } else {
        format!("{}. {name}", k + 1)
    }
}

/// One setting of a part: caption, field, range, step, unit, whether it is a whole number.
type PartCtl = (&'static str, &'static str, (f32, f32), f32, &'static str, bool);

const BRIGHTNESS: PartCtl = ("Brightness", "intensity", (0.0, 2.0), 0.01, "", false);
const POSITION: PartCtl = ("Position", "t", (-1.0, 3.0), 0.01, "", false);
const RAINBOW: PartCtl = ("Rainbow Edge", "chroma", (0.0, 2.0), 0.01, "", false);
const ANGLE: PartCtl = ("Angle", "angle", (-180.0, 180.0), 1.0, "°", false);

/// The settings of a part of `kind`. Sizes are in thousandths of the image's diagonal (before the
/// flare's Scale); Position is along the axis (0 = the light, 1 = the centre point, 2 = as far again).
fn settings(kind: &str) -> &'static [PartCtl] {
    match kind {
        "glow" => &[BRIGHTNESS, ("Size", "radius", (1.0, 400.0), 1.0, "", false), ("Falloff", "falloff", (1.0, 2.0), 0.01, "", false)],
        "streak" => &[BRIGHTNESS, ("Length", "length", (10.0, 2500.0), 1.0, "", false), ("Thickness", "thickness", (0.2, 60.0), 0.1, "", false), ANGLE],
        "star" => &[
            BRIGHTNESS,
            ("Points", "rays", (2.0, 32.0), 1.0, "", true),
            ("Length", "length", (10.0, 600.0), 1.0, "", false),
            ("Thickness", "thickness", (0.2, 10.0), 0.1, "", false),
            ANGLE,
        ],
        "rays" => &[
            BRIGHTNESS,
            ("Count", "count", (1.0, 256.0), 1.0, "", true),
            ("Length", "length", (10.0, 600.0), 1.0, "", false),
            ("Variation", "seed", (0.0, 999.0), 1.0, "", true),
        ],
        "ring" => &[
            BRIGHTNESS,
            POSITION,
            ("Radius", "radius", (5.0, 600.0), 1.0, "", false),
            ("Width", "width", (0.5, 80.0), 0.1, "", false),
            RAINBOW,
            ("Half Ring", "arc", (0.0, 1.0), 0.01, "", false),
        ],
        "ghost" => &[
            BRIGHTNESS,
            POSITION,
            ("Radius", "radius", (1.0, 300.0), 1.0, "", false),
            ("Sides", "sides", (0.0, 12.0), 1.0, "", true),
            ("Rim", "rim", (0.0, 2.0), 0.01, "", false),
            ("Softness", "softness", (0.02, 1.0), 0.01, "", false),
            RAINBOW,
        ],
        "wash" => &[BRIGHTNESS, ("Reach", "reach", (50.0, 1000.0), 1.0, "", false), ("Spread", "spread", (1.0, 180.0), 1.0, "°", false), RAINBOW, ANGLE],
        _ => &[],
    }
}

/// One setting slider of part `i`.
fn part_control(ui: &mut egui::Ui, f: &mut Map<String, Value>, i: usize, (caption, key, (lo, hi), step, unit, whole): PartCtl) {
    let mut p = parts(f);
    let Some(part) = p.get_mut(i).and_then(Value::as_object_mut) else { return };
    let mut v = part.get(key).and_then(Value::as_f64).unwrap_or(f64::from(lo));
    let shown = v;
    let mut slider = SliderField::new(tl!(caption), Bind::bound(&mut v), f64::from(lo)..=f64::from(hi));
    if !unit.is_empty() {
        slider = slider.unit(unit);
    }
    slider.show(ui);
    if v != shown {
        let v = ((v.clamp(f64::from(lo), f64::from(hi)) / f64::from(step)).round() * f64::from(step)).clamp(f64::from(lo), f64::from(hi));
        part.insert(key.into(), if whole { json!(v.round() as u64) } else { json!(v) });
        f.insert("elements".into(), Value::Array(p));
    }
}

/// The selected part's colour, as a colour button.
fn part_colour(ui: &mut egui::Ui, f: &mut Map<String, Value>, i: usize) {
    let mut p = parts(f);
    let Some(part) = p.get_mut(i).and_then(Value::as_object_mut) else { return };
    let c = part.get("color").and_then(Value::as_array);
    let at = |k: usize| c.and_then(|c| c.get(k)).and_then(Value::as_f64).map_or(1.0, |v| v.clamp(0.0, 1.0) as f32);
    let mut rgb = [at(0), at(1), at(2)];
    let before = rgb;
    ui.horizontal(|ui| {
        Text::new(tl!("Color").to_string()).show(ui);
        let r = ui.color_edit_button_rgb(&mut rgb);
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColorWell, true, tl!("Part Color")));
    });
    if rgb != before {
        part.insert("color".into(), json!(rgb));
        f.insert("elements".into(), Value::Array(p));
    }
}

/// The list of parts: a radio per part selects it (click, or the arrow keys), and the Show check
/// box beside it hides or shows that part.
fn list(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let p = parts(f);
    let h = hidden(f);
    let sel = selected(f);
    let mut toggled = None;
    egui::ScrollArea::vertical().id_salt("lens-flare-parts").max_height(HEIGHT - 96.0).auto_shrink([false, true]).show(ui, |ui| {
        if p.is_empty() {
            Text::new(tl!("No parts: add one below.").to_string()).role(TypeRole::Caption).show(ui);
            return;
        }
        let picked = RadioGroup::new(tl!("Parts").to_owned(), Bind::given(sel.to_string()))
            .show(ui, |ui, _| {
                for (k, part) in p.iter().enumerate() {
                    ui.horizontal(|ui| {
                        Radio::new(k.to_string()).label(row_text(k, part)).show(ui);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let shown = h.binary_search(&k).is_err();
                            let tip = tl!(if shown { "Hide this part" } else { "Show this part" }).to_owned();
                            let cell = Checkbox::new(Bind::given(shown)).label(tl!("Show").to_owned()).tooltip(tip).id_salt(("lens-flare-part-show", k));
                            if cell.show(ui).value().is_some() {
                                toggled = Some(k);
                            }
                        });
                    });
                }
            })
            .output
            .value()
            .and_then(|v| v.parse::<usize>().ok());
        if let Some(k) = picked {
            f.insert("__part".into(), json!(k));
        }
    });
    if let Some(k) = toggled {
        edit::toggle(f, k);
    }
}

/// The buttons under the list, and the centre point.
fn actions(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let n = parts(f).len();
    let sel = selected(f);
    let room = n < range::MAX_ELEMENTS;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(room, |ui| {
            ui.menu_button(tl!("Add Part"), |ui| {
                for (id, name) in PART_KINDS {
                    if ui.button(tl!(name)).clicked() {
                        edit::add(f, id);
                        ui.close();
                    }
                }
            });
        });
        ui.add_enabled_ui(n > 0 && room, |ui| {
            if Button::new(label(tl!("Duplicate"))).show(ui).clicked() {
                edit::duplicate(f, sel);
            }
        });
        ui.add_enabled_ui(n > 0, |ui| {
            if Button::new(label(tl!("Delete"))).show(ui).clicked() {
                edit::delete(f, sel);
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add_enabled_ui(sel > 0, |ui| {
            if Button::new(label(tl!("Move Up"))).show(ui).clicked() {
                edit::shift(f, sel, true);
            }
        });
        ui.add_enabled_ui(sel + 1 < n, |ui| {
            if Button::new(label(tl!("Move Down"))).show(ui).clicked() {
                edit::shift(f, sel, false);
            }
        });
    });
    ui.horizontal(|ui| {
        let (cx, cy) = (num(f, "centerX", 0.5), num(f, "centerY", 0.5));
        Text::new(crate::i18n::fmt(tl!("Center point: {x}, {y}"), &[("x", &format!("{cx:.2}")), ("y", &format!("{cy:.2}"))])).show(ui);
        ui.add_enabled_ui((cx - 0.5).abs() > 1e-4 || (cy - 0.5).abs() > 1e-4, |ui| {
            if Button::new(label(tl!("Reset Center"))).show(ui).clicked() {
                put(f, "centerX", 0.5, range::POSITION, 0.001);
                put(f, "centerY", 0.5, range::POSITION, 0.001);
            }
        });
    });
}

/// The Parts tab: the list and its actions, and beside them the selected part's settings.
pub(super) fn editor(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    columns(ui, &LAYOUT, |col, ui| {
        if col == 0 {
            heading(ui, tl!("Parts"));
            list(ui, f);
            actions(ui, f);
            return;
        }
        let p = parts(f);
        let i = selected(f);
        let Some(kind) = p.get(i).and_then(|v| v.get("kind")).and_then(Value::as_str).map(str::to_string) else {
            heading(ui, tl!("Part"));
            return;
        };
        heading(ui, &row_text(i, &p[i]));
        egui::ScrollArea::vertical().id_salt("lens-flare-part-settings").max_height(HEIGHT).auto_shrink([false, true]).show(ui, |ui| {
            for c in settings(&kind) {
                part_control(ui, f, i, *c);
            }
            part_colour(ui, f, i);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(n: usize, hidden: &[usize]) -> Map<String, Value> {
        let mut f = Map::new();
        let p: Vec<Value> = (0..n).map(|k| serde_json::to_value(FlareElement::new_part(PART_KINDS[k % PART_KINDS.len()].0).unwrap()).unwrap()).collect();
        store(&mut f, p, hidden.to_vec(), 0);
        f
    }

    fn kinds(f: &Map<String, Value>) -> Vec<String> {
        parts(f).iter().map(|p| p["kind"].as_str().unwrap().to_string()).collect()
    }

    /// Hidden marks stay on the same parts through every edit the list makes.
    #[test]
    fn hidden_parts_stay_hidden_through_moves_copies_and_deletes() {
        let mut f = with(4, &[2]); // glow, streak, star (hidden), rays
        edit::shift(&mut f, 2, true);
        assert_eq!((kinds(&f)[1].as_str(), hidden(&f), selected(&f)), ("star", vec![1], 1), "moved up, still hidden, still selected");
        edit::duplicate(&mut f, 0);
        assert_eq!((kinds(&f)[..3].to_vec(), hidden(&f), selected(&f)), (vec!["glow".to_string(), "glow".into(), "star".into()], vec![2], 1));
        edit::delete(&mut f, 0);
        assert_eq!((hidden(&f), kinds(&f)[1].as_str()), (vec![1], "star"));
        edit::delete(&mut f, 1);
        assert!(hidden(&f).is_empty() && !kinds(&f).contains(&"star".to_string()), "deleting the hidden part drops its mark");
        edit::toggle(&mut f, 0);
        edit::toggle(&mut f, 2);
        assert_eq!(hidden(&f), vec![0, 2]);
        edit::toggle(&mut f, 0);
        assert_eq!(hidden(&f), vec![2]);
    }

    /// The parts the list makes are ones the flare command takes.
    #[test]
    fn added_and_edited_parts_are_valid_flare_parts() {
        let mut f = with(0, &[]);
        for (id, _) in PART_KINDS {
            edit::add(&mut f, id);
        }
        assert_eq!(selected(&f), PART_KINDS.len() - 1);
        let els: Vec<FlareElement> = serde_json::from_value(Value::Array(parts(&f))).unwrap();
        assert_eq!(els.len(), PART_KINDS.len());
        // every setting the editor shows is a field of its kind
        for (e, v) in els.iter().zip(parts(&f)) {
            for (_, key, ..) in settings(e.kind_id()) {
                assert!(v.get(*key).is_some(), "{} has no `{key}`", e.kind_id());
            }
        }
        // past the limit, Add and Duplicate do nothing
        let mut f = with(range::MAX_ELEMENTS, &[]);
        edit::add(&mut f, "glow");
        edit::duplicate(&mut f, 0);
        assert_eq!(parts(&f).len(), range::MAX_ELEMENTS);
    }
}
