use photocraft_doc::{Fill, LayerContent, LayerId};
use serde_json::json;

use super::*;

fn app() -> Box<PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 64, "height": 36, "depth": 16})).unwrap();
    app
}

fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.entries().len()
}

fn set(app: &mut PhotocraftApp, id: DialogId, values: Value) {
    let ctx = app.ctx().clone();
    for (k, v) in values.as_object().unwrap() {
        app.framework.dialogs.set_json_field(&ctx, id, k, v.clone()).unwrap();
    }
}

fn confirm(app: &mut PhotocraftApp, id: DialogId) {
    let ctx = app.ctx().clone();
    crate::dialogs::confirm(app, &ctx, id).unwrap();
}

fn flare(app: &PhotocraftApp, id: LayerId) -> lens_flare::LensFlare {
    match &app.session.active().unwrap().doc.layer(id).unwrap().content {
        LayerContent::Fill(Fill::LensFlare(f)) => (**f).clone(),
        _ => panic!("not a flare"),
    }
}

#[test]
fn new_light_flare_opens_the_dialog_previews_and_ok_is_one_step() {
    let mut app = app();
    let ctx = app.ctx().clone();
    let before = app.session.active().unwrap().doc.clone();
    let count = steps(&app);
    crate::menus::invoke(&mut app, &ctx, cmds::CREATE, json!({})).unwrap();
    let open = app.framework.dialogs.open_dialogs();
    assert_eq!(open[0].key.as_deref(), Some("lightFlare"));
    let dialog = open[0].id;
    assert!(Arc::ptr_eq(&before, &app.session.active().unwrap().doc), "no layer until OK");
    set(&mut app, dialog, json!({"preset": "Sun Burst", "x": 0.8, "y": 0.2, "scale": 1.5, "texture": "dust"}));
    let (shown, key) = display_doc(&mut app, 0).unwrap();
    let (again, again_key) = display_doc(&mut app, 0).unwrap();
    assert!(Arc::ptr_eq(&shown, &again) && key == again_key, "the preview is cached");
    assert!(shown.layers.len() > before.layers.len());
    assert_eq!(steps(&app), count, "previewing does not edit");
    confirm(&mut app, dialog);
    assert_eq!(steps(&app), count + 1);
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let f = flare(&app, layer);
    assert_eq!((f.preset.as_str(), f.source, f.scale, f.texture.kind), ("Sun Burst", (0.8, 0.2), 1.5, TextureKind::Dust));
    assert!(display_doc(&mut app, 0).is_none() && app.light_flare.preview.is_none());
}

#[test]
fn content_options_reopen_the_flare_and_a_canvas_click_moves_the_light() {
    let mut app = app();
    let ctx = app.ctx().clone();
    app.run(cmds::CREATE, json!({"preset": "Six Blades", "x": 0.2})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let count = steps(&app);
    let reply = crate::menus::invoke(&mut app, &ctx, "layer.layerContentOptions", Value::Null).unwrap();
    assert!(reply.get("dialog").is_some(), "{reply}");
    let dialog = app.framework.dialogs.open_dialogs()[0].id;
    assert!(display_doc(&mut app, 0).is_none(), "unchanged: no preview");
    assert!(placing(&app));
    assert!(place_at(&mut app, [48.0, 9.0]));
    confirm(&mut app, dialog);
    assert_eq!(steps(&app), count + 1);
    let f = flare(&app, layer);
    assert_eq!(f.preset, "Six Blades", "the same preset keeps its elements");
    assert!((f.source.0 - 0.75).abs() < 1e-4 && (f.source.1 - 0.25).abs() < 1e-4, "the light is where the click was: {:?}", f.source);
}
