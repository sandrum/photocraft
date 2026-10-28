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
        let r = s.execute(CREATE, json!({"preset": "White Source", "x": 0.25, "y": 0.25})).unwrap();
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
    let p = json!({"preset": "Hexagons", "x": 0.7, "y": 0.2, "artifacts": 2.0});
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
