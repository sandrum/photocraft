use super::*;
use photocraft_geom::Rect;
use std::sync::Arc;

fn session(depth: u32) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 80, "height": 45, "depth": depth})).unwrap();
    s
}

fn flare_of(s: &Session, id: u64) -> LensFlare {
    match &s.active().unwrap().doc.layer(LayerId(id)).unwrap().content {
        LayerContent::Fill(Fill::LensFlare(f)) => (**f).clone(),
        other => panic!("not a flare: {}", other.kind_name()),
    }
}

fn pixel(s: &Session, x: i32, y: i32) -> [f32; 4] {
    photocraft_compose::render(&s.active().unwrap().doc, Rect::new(x, y, x + 1, y + 1)).px[0]
}

#[test]
fn new_flare_layer_lights_the_image_and_undoes_in_one_step() {
    for depth in [8, 16, 32] {
        let mut s = session(depth);
        s.execute("edit.fill", json!({"color": "#202020"})).ok();
        let before = pixel(&s, 20, 11);
        let count = s.active().unwrap().history.entries().len();
        let r = s.execute(CREATE, json!({"preset": "Bare Bulb", "x": 0.25, "y": 0.25})).unwrap();
        let id = r["layer"].as_u64().unwrap();
        let st = s.active().unwrap();
        let l = st.doc.layer(LayerId(id)).unwrap();
        assert_eq!(l.blend, BlendMode::Screen);
        assert!(l.name.starts_with("Lens Flare"));
        assert_eq!(st.history.entries().len(), count + 1);
        let lit = pixel(&s, 20, 11);
        assert!(lit[0] > before[0] + 0.3, "{depth}-bit: {before:?} → {lit:?}");
        s.execute("edit.undo", json!({})).unwrap();
        assert!(s.active().unwrap().doc.layer(LayerId(id)).is_none());
    }
}

#[test]
fn set_edits_controls_and_preset_keeping_position() {
    let mut s = session(8);
    let id = s.execute(CREATE, json!({"x": 0.2, "y": 0.3})).unwrap()["layer"].as_u64().unwrap();
    s.execute(SET, json!({"layer": id, "scale": 2.0, "brightness": 1.5, "hue": 200, "tint": 0.5, "texture": "hexDust", "flipX": true})).unwrap();
    let f = flare_of(&s, id);
    assert_eq!((f.scale, f.brightness, f.hue, f.tint), (2.0, 1.5, 200.0, 0.5));
    assert_eq!(f.texture.kind, TextureKind::HexDust);
    assert!(f.texture.flip_x);
    s.execute(SET, json!({"layer": id, "preset": "Sun Burst"})).unwrap();
    let g = flare_of(&s, id);
    assert_eq!(g.preset, "Sun Burst");
    assert_eq!(g.category, FlareCategory::SunFlare);
    assert_eq!((g.source, g.scale), ((0.2, 0.3), 2.0), "a new preset keeps the user's controls");
    // An unchanged edit is not a history step.
    let n = s.active().unwrap().history.entries().len();
    s.execute(SET, json!({"layer": id})).unwrap();
    assert_eq!(s.active().unwrap().history.entries().len(), n);
}

#[test]
fn preview_matches_what_the_commands_commit() {
    let mut s = session(16);
    let p = json!({"preset": "Six Blades", "x": 0.7, "y": 0.2, "artifacts": 2.0});
    let shown = preview(&s, CREATE, &p).unwrap();
    s.execute(CREATE, p).unwrap();
    let all = Rect::new(0, 0, 80, 45);
    assert_eq!(photocraft_compose::render(&shown, all).px, photocraft_compose::render(&s.active().unwrap().doc, all).px);
    let id = s.active().unwrap().active_layer.unwrap();
    let before = s.active().unwrap().doc.clone();
    let p = json!({"layer": id.0, "rotation": 45, "aspect": 2.0, "blend": "linearDodge"});
    let shown = preview(&s, SET, &p).unwrap();
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc), "previewing is not an edit");
    s.execute(SET, p).unwrap();
    assert_eq!(shown, *s.active().unwrap().doc);
    assert!(preview(&s, "edit.fill", &json!({})).is_err());
}

#[test]
fn flare_survives_a_pcraft_round_trip() {
    let mut s = session(8);
    let id = s.execute(CREATE, json!({"preset": "Prime Classic", "texture": "dust", "x": 0.1})).unwrap()["layer"].as_u64().unwrap();
    let doc = s.active().unwrap().doc.clone();
    let bytes = photocraft_format::save_to_bytes(&doc, &Default::default()).unwrap();
    let back = photocraft_format::load_from_bytes(&bytes).unwrap();
    let flare = |d: &Document| match &d.layers.iter().find(|l| l.name.starts_with("Lens Flare")).unwrap().content {
        LayerContent::Fill(Fill::LensFlare(f)) => (**f).clone(),
        _ => panic!("lost the flare"),
    };
    assert_eq!(flare(&back), flare_of(&s, id));
}

#[test]
fn bad_params_are_errors_not_panics() {
    let mut s = session(8);
    let before = s.active().unwrap().doc.layers.len();
    for p in [
        json!({"preset": "Nope"}),
        json!({"preset": 5}),
        json!({"scale": 0}),
        json!({"scale": "big"}),
        json!({"brightness": 1e9}),
        json!({"x": f64::MAX}),
        json!({"texture": "glitter"}),
        json!({"flipX": 1}),
        json!({"blend": "nope"}),
        json!({"blend": "passThrough"}),
        json!({"path": "Nope"}),
        json!([1, 2]),
    ] {
        assert!(s.execute(CREATE, p.clone()).is_err(), "{p}");
    }
    assert_eq!(s.active().unwrap().doc.layers.len(), before);
    let bg = s.active().unwrap().doc.layers[0].id;
    assert!(s.execute(SET, json!({"layer": bg.0, "scale": 2})).is_err(), "not a flare layer");
    assert!(s.execute(SET, json!({"layer": -1})).is_err());
    assert!(s.execute(SET, json!({"document": 999_999})).is_err());
    assert!(s.execute(PRESETS, json!({"category": "nope"})).is_err());
}

#[test]
fn presets_lists_every_category() {
    let mut s = Session::new();
    let r = s.execute(PRESETS, json!({})).unwrap();
    let cats = r["categories"].as_array().unwrap();
    assert_eq!(cats.len(), 4);
    assert!(cats.iter().all(|c| c["presets"].as_array().is_some_and(|p| p.len() >= 8)));
    let one = s.execute(PRESETS, json!({"category": "streak"})).unwrap();
    assert_eq!(one["categories"].as_array().unwrap().len(), 1);
}

#[test]
fn psd_export_writes_the_flare_as_pixels() {
    let mut s = session(8);
    s.execute(CREATE, json!({"preset": "Sun Glint", "x": 0.5, "y": 0.5})).unwrap();
    let doc = s.active().unwrap().doc.clone();
    let out = photocraft_io::export(&doc, "x.psd", &Default::default()).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("lens flare")), "{:?}", out.warnings);
    let back = photocraft_io::import("x.psd", &out.bytes).unwrap().document;
    let all = Rect::new(0, 0, 80, 45);
    let (a, b) = (photocraft_compose::render(&doc, all).px, photocraft_compose::render(&back, all).px);
    let worst = a.iter().zip(&b).flat_map(|(p, q)| p.iter().zip(q).map(|(x, y)| (x - y).abs())).fold(0.0f32, f32::max);
    assert!(worst < 0.02, "PSD looks different: {worst}");
}

#[test]
fn edge_wash_and_new_textures_light_the_image_at_every_depth() {
    for depth in [8, 16, 32] {
        let mut s = session(depth);
        s.execute("edit.fill", json!({"color": "#202020"})).ok();
        let far = pixel(&s, 40, 22);
        let id = s.execute(CREATE, json!({"preset": "Morning Spill", "x": 0.05, "y": 0.5})).unwrap()["layer"].as_u64().unwrap();
        // The wash carries light from the left edge well into the frame.
        let lit = pixel(&s, 40, 22);
        assert!(lit[0] > far[0] + 0.05, "{depth}-bit: {far:?} → {lit:?}");
        s.execute(SET, json!({"layer": id, "texture": "bokehBubbles", "textureOpacity": 1.0})).unwrap();
        assert_eq!(flare_of(&s, id).texture.kind, TextureKind::BokehBubbles);
        let all = Rect::new(0, 0, 80, 45);
        assert!(photocraft_compose::render(&s.active().unwrap().doc, all).px.iter().flatten().all(|v| v.is_finite()), "{depth}-bit");
    }
}
fn look_groups(s: &Session) -> Vec<(LayerId, String)> {
    s.active().unwrap().doc.walk().into_iter().filter(|(_, _, l)| l.is_group() && l.xmp.is_some()).map(|(_, _, l)| (l.id, l.name.clone())).collect()
}

#[test]
fn a_look_adds_replaces_and_removes_its_group_in_one_step_each() {
    for depth in [8, 16, 32] {
        let mut s = session(depth);
        s.execute("edit.fill", json!({"color": "#806040"})).ok();
        let id = s.execute(CREATE, json!({"x": 0.2, "y": 0.2})).unwrap()["layer"].as_u64().unwrap();
        let plain = pixel(&s, 60, 35);
        let n = s.active().unwrap().history.entries().len();
        let r = s.execute(LOOK, json!({"layer": id, "look": "noir"})).unwrap();
        assert_eq!(r["look"], json!("noir"));
        assert_eq!(s.active().unwrap().history.entries().len(), n + 1, "{depth}-bit: one step");
        let groups = look_groups(&s);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1, "Noir Look");
        assert_eq!(r["group"], json!(groups[0].0.0));
        // Directly above the flare, pass-through, holding adjustment layers; the flare stays active.
        let st = s.active().unwrap();
        let i = st.doc.layers.iter().position(|l| l.id.0 == id).unwrap();
        let g = &st.doc.layers[i + 1];
        assert_eq!((g.id, g.blend), (groups[0].0, BlendMode::PassThrough));
        assert!(g.children().unwrap().iter().all(|c| matches!(c.content, LayerContent::Adjustment(_))));
        assert_eq!(st.active_layer, Some(LayerId(id)));
        let grey = pixel(&s, 60, 35);
        assert!((grey[0] - grey[2]).abs() < 0.02 && (plain[0] - plain[2]).abs() > 0.1, "{depth}-bit: Noir greys the image {plain:?} → {grey:?}");
        // A second look replaces the first.
        s.execute(LOOK, json!({"layer": id, "look": "Golden Hour"})).unwrap();
        let groups = look_groups(&s);
        assert_eq!(groups.len(), 1, "replaced, not stacked");
        assert_eq!(groups[0].1, "Golden Hour Look");
        assert_eq!(look_of(&s.active().unwrap().doc, LayerId(id)), "goldenHour");
        assert_eq!(s.active().unwrap().history.entries().len(), n + 2);
        // The same look again is no step; None removes it.
        s.execute(LOOK, json!({"layer": id, "look": "goldenHour"})).unwrap();
        assert_eq!(s.active().unwrap().history.entries().len(), n + 2);
        s.execute(LOOK, json!({"layer": id, "look": "none"})).unwrap();
        assert!(look_groups(&s).is_empty());
        assert_eq!(pixel(&s, 60, 35), plain);
        // Undo restores each look in turn.
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(look_groups(&s)[0].1, "Golden Hour Look");
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(look_groups(&s)[0].1, "Noir Look");
        s.execute("edit.undo", json!({})).unwrap();
        assert!(look_groups(&s).is_empty());
        assert_eq!(pixel(&s, 60, 35), plain);
    }
}

#[test]
fn every_look_builds_and_changes_the_image() {
    let mut s = session(8);
    s.execute("edit.fill", json!({"color": "#6a8aa0"})).ok();
    let id = s.execute(CREATE, json!({"x": 0.1, "y": 0.1})).unwrap()["layer"].as_u64().unwrap();
    let plain = pixel(&s, 70, 40);
    assert_eq!(looks::LOOKS[0].id, looks::NONE);
    assert!((8..=13).contains(&(looks::LOOKS.len() - 1)));
    for l in &looks::LOOKS[1..] {
        assert!(!l.adjustments(photocraft_doc::ColorMode::Rgb).unwrap().is_empty(), "{}", l.id);
        s.execute(LOOK, json!({"layer": id, "look": l.id})).unwrap();
        assert_ne!(pixel(&s, 70, 40), plain, "{} changes the image", l.id);
    }
    let listed = s.execute(PRESETS, json!({})).unwrap();
    assert_eq!(listed["looks"].as_array().unwrap().len(), looks::LOOKS.len());
}

#[test]
fn look_rides_along_with_create_set_and_preview_as_one_step() {
    let mut s = session(16);
    let n = s.active().unwrap().history.entries().len();
    let p = json!({"preset": "Bare Bulb", "look": "tealAmber"});
    let shown = preview(&s, CREATE, &p).unwrap();
    let r = s.execute(CREATE, p).unwrap();
    assert_eq!((r["look"].clone(), s.active().unwrap().history.entries().len()), (json!("tealAmber"), n + 1));
    let all = Rect::new(0, 0, 80, 45);
    assert_eq!(photocraft_compose::render(&shown, all).px, photocraft_compose::render(&s.active().unwrap().doc, all).px);
    let id = r["layer"].as_u64().unwrap();
    // SET changes the look (and nothing else) in one step; previewing it is no edit.
    let p = json!({"layer": id, "look": "vivid"});
    let shown = preview(&s, SET, &p).unwrap();
    s.execute(SET, p).unwrap();
    assert_eq!(s.active().unwrap().history.entries().len(), n + 2);
    assert_eq!(photocraft_compose::render(&shown, all).px, photocraft_compose::render(&s.active().unwrap().doc, all).px);
    assert_eq!(look_groups(&s).len(), 1);
    // Omitting look keeps it.
    s.execute(SET, json!({"layer": id, "scale": 2.0})).unwrap();
    assert_eq!(look_of(&s.active().unwrap().doc, LayerId(id)), "vivid");
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(look_of(&s.active().unwrap().doc, LayerId(id)), "tealAmber");
}

#[test]
fn a_look_goes_above_layers_clipped_to_the_flare_and_survives_a_pcraft_round_trip() {
    let mut s = session(8);
    let id = s.execute(CREATE, json!({})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.newAdjustmentLayer.invert", json!({})).unwrap();
    let clip = s.active().unwrap().active_layer.unwrap();
    s.edit("clip", |doc, _| {
        doc.layer_mut(clip).unwrap().clipped = true;
        Ok(())
    })
    .unwrap();
    s.execute(LOOK, json!({"layer": id, "look": "punch"})).unwrap();
    let st = s.active().unwrap();
    let names: Vec<_> = st.doc.layers.iter().map(|l| (l.name.clone(), l.clipped)).collect();
    let i = st.doc.layers.iter().position(|l| l.id.0 == id).unwrap();
    assert_eq!(st.doc.layers[i + 1].id, clip, "{names:?}");
    assert_eq!((st.doc.layers[i + 2].name.as_str(), st.doc.layers[i + 2].clipped), ("Punch Look", false));
    let bytes = photocraft_format::save_to_bytes(&st.doc, &Default::default()).unwrap();
    let back = photocraft_format::load_from_bytes(&bytes).unwrap();
    assert_eq!(look_of(&back, LayerId(id)), "punch");
}

#[test]
fn bad_look_params_are_errors_not_panics() {
    let mut s = session(8);
    let id = s.execute(CREATE, json!({})).unwrap()["layer"].as_u64().unwrap();
    let before = s.active().unwrap().doc.clone();
    for p in [
        json!({}),
        json!({"layer": id}),
        json!({"layer": id, "look": 3}),
        json!({"layer": id, "look": "sparkly"}),
        json!({"layer": id, "look": ""}),
        json!({"layer": id, "look": null}),
        json!({"layer": -1, "look": "noir"}),
        json!({"layer": 999_999, "look": "noir"}),
        json!({"layer": id, "look": "noir", "document": 999_999}),
        json!({"layer": id, "look": "noir", "x": 1e9}),
        json!([1]),
        json!("noir"),
    ] {
        assert!(s.execute(LOOK, p.clone()).is_err(), "{p}");
    }
    let bg = s.active().unwrap().doc.layers[0].id;
    assert!(s.execute(LOOK, json!({"layer": bg.0, "look": "noir"})).is_err(), "not a flare layer");
    assert!(s.execute(CREATE, json!({"look": "sparkly"})).is_err());
    assert!(s.execute(SET, json!({"layer": id, "look": 1})).is_err());
    assert!(Arc::ptr_eq(&before, &s.active().unwrap().doc), "nothing changed");
    assert!(Session::new().execute(LOOK, json!({"look": "noir"})).is_err(), "no document");
}
