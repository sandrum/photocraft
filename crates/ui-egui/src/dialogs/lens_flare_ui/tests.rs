use egui::vec2;
use egui_kittest::Harness;
use photocraft_doc::{Fill, LayerContent, LayerId};
use serde_json::json;

use super::*;

fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 64, "height": 36, "depth": 16})).unwrap();
    app
}

fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.entries().len()
}

fn set(app: &mut PhotocraftApp, dialog: u64, values: serde_json::Value) {
    let d = app.ui.dialog_mut(dialog).unwrap();
    for (k, v) in values.as_object().unwrap() {
        d.fields.insert(k.clone(), v.clone());
    }
}

fn flare(app: &PhotocraftApp, id: u64) -> lens_flare::LensFlare {
    match &app.session.active().unwrap().doc.layer(LayerId(id)).unwrap().content {
        LayerContent::Fill(Fill::LensFlare(f)) => (**f).clone(),
        _ => panic!("not a flare"),
    }
}

#[test]
fn new_lens_flare_opens_the_dialog_previews_and_ok_is_one_step() {
    let mut app = app();
    let ctx = egui::Context::default();
    let before = app.session.active().unwrap().doc.clone();
    let count = steps(&app);
    let dialog = crate::menus::invoke(&mut app, &ctx, cmds::CREATE, json!({})).unwrap()["dialog"].as_u64().unwrap();
    let d = app.ui.dialogs.last().unwrap();
    assert!(owns(&d.fields));
    assert_eq!(crate::dialogs::title(d), "Lens Flare");
    assert_eq!(d.fields["preset"], json!(cmds::DEFAULT_PRESET));
    assert!(Arc::ptr_eq(&before, &app.session.active().unwrap().doc), "no layer until OK");
    set(&mut app, dialog, json!({"preset": "Sun Burst", "x": 0.8, "y": 0.2, "scale": 1.5, "hue": 30.0, "tint": 0.4, "texture": "dust"}));
    let (shown, key) = display_doc(&mut app, 0).unwrap();
    let (again, again_key) = display_doc(&mut app, 0).unwrap();
    assert!(Arc::ptr_eq(&shown, &again) && key == again_key, "the preview is cached");
    assert_eq!(shown.layers.len(), before.layers.len() + 1);
    assert_eq!(steps(&app), count, "previewing does not edit");
    let r = crate::dialogs::confirm(&mut app, dialog).unwrap();
    assert_eq!(steps(&app), count + 1);
    let f = flare(&app, r["layer"].as_u64().unwrap());
    assert_eq!((f.preset.as_str(), f.source, f.scale, f.texture.kind), ("Sun Burst", (0.8, 0.2), 1.5, TextureKind::Dust));
    assert!(display_doc(&mut app, 0).is_none() && app.lens_flare_preview.is_none());
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(*app.session.active().unwrap().doc, *before);
}

#[test]
fn content_options_reopen_the_flare_and_keep_its_elements() {
    let mut app = app();
    let ctx = egui::Context::default();
    let id = app.run(cmds::CREATE, json!({"preset": "Hexagons", "x": 0.2})).unwrap()["layer"].as_u64().unwrap();
    // Hand-tuned elements (e.g. from a file) survive an edit that keeps the preset.
    let mut tuned = flare(&app, id);
    tuned.elements.truncate(2);
    app.session
        .edit("tune", |doc, _| {
            doc.layer_mut(LayerId(id)).unwrap().content = LayerContent::Fill(Fill::LensFlare(Box::new(tuned.clone())));
            Ok(())
        })
        .unwrap();
    let count = steps(&app);
    let dialog = crate::menus::invoke(&mut app, &ctx, "layer.layerContentOptions", serde_json::Value::Null).unwrap()["dialog"].as_u64().unwrap();
    let f = &app.ui.dialog_mut(dialog).unwrap().fields;
    assert!(owns(f));
    assert_eq!((f["preset"].clone(), f["x"].clone(), f["__tab"].clone()), (json!("Hexagons"), json!(0.2f32), json!("spherical")));
    assert!(display_doc(&mut app, 0).is_none(), "unchanged: no preview");
    crate::dialogs::confirm(&mut app, dialog).unwrap();
    assert_eq!(steps(&app), count, "unchanged OK is no step");
    let dialog = open(&mut app).unwrap();
    set(&mut app, dialog, json!({"brightness": 2.0, "flipY": true}));
    crate::dialogs::confirm(&mut app, dialog).unwrap();
    let f = flare(&app, id);
    assert_eq!((f.brightness, f.texture.flip_y, f.elements.len()), (2.0, true, 2));
    // Cancel changes nothing; a stale dialog never edits another document.
    let before = app.session.active().unwrap().doc.clone();
    let dialog = open(&mut app).unwrap();
    set(&mut app, dialog, json!({"preset": "White Line"}));
    assert!(display_doc(&mut app, 0).is_some());
    crate::dialogs::cancel(&mut app, dialog).unwrap();
    assert!(Arc::ptr_eq(&before, &app.session.active().unwrap().doc));
    let dialog = open(&mut app).unwrap();
    set(&mut app, dialog, json!({"scale": 2.0}));
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    assert!(crate::dialogs::confirm(&mut app, dialog).is_err());
    // Explicit params run the engine directly; no document is an error, not a dialog.
    let r = crate::menus::invoke(&mut app, &ctx, cmds::CREATE, json!({"preset": "Glow Dot"})).unwrap();
    assert!(r["layer"].as_u64().is_some());
    let mut empty = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    assert!(crate::menus::invoke(&mut empty, &ctx, cmds::CREATE, json!({})).is_err());
}

#[test]
fn the_dialog_draws_and_a_thumbnail_click_picks_the_preset() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 64, "height": 36})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    h.run_steps(4);
    let ctx = h.ctx.clone();
    let dialog = crate::menus::invoke(h.state_mut(), &ctx, cmds::CREATE, json!({})).unwrap()["dialog"].as_u64().unwrap();
    h.run_steps(6);
    for tab in ["sunFlare", "streak", "spherical"] {
        set(h.state_mut(), dialog, json!({"__tab": tab}));
        h.run_steps(3);
    }
    use egui_kittest::kittest::Queryable;
    h.get_by_label("Space Lens").click();
    h.run_steps(3);
    assert_eq!(h.state_mut().ui.dialog_mut(dialog).unwrap().fields["preset"], json!("Space Lens"));
    crate::dialogs::confirm(h.state_mut(), dialog).unwrap();
    h.run_steps(3);
    let st = h.state().session.active().unwrap();
    assert!(matches!(&st.doc.layer(st.active_layer.unwrap()).unwrap().content, LayerContent::Fill(Fill::LensFlare(f)) if f.preset == "Space Lens"));
}
