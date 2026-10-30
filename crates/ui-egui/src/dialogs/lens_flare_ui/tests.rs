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
    assert_eq!(crate::dialogs::title(d), "Light Flare");
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
    let id = app.run(cmds::CREATE, json!({"preset": "Six Blades", "x": 0.2})).unwrap()["layer"].as_u64().unwrap();
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
    assert_eq!((f["preset"].clone(), f["x"].clone(), f["__tab"].clone()), (json!("Six Blades"), json!(0.2f32), json!("spherical")));
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
    set(&mut app, dialog, json!({"preset": "Silver Thread"}));
    assert!(display_doc(&mut app, 0).is_some());
    crate::dialogs::cancel(&mut app, dialog).unwrap();
    assert!(Arc::ptr_eq(&before, &app.session.active().unwrap().doc));
    let dialog = open(&mut app).unwrap();
    set(&mut app, dialog, json!({"scale": 2.0}));
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    assert!(crate::dialogs::confirm(&mut app, dialog).is_err());
    // Explicit params run the engine directly; no document is an error, not a dialog.
    let r = crate::menus::invoke(&mut app, &ctx, cmds::CREATE, json!({"preset": "Pinprick"})).unwrap();
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
    h.get_by_label("Orbital").click();
    h.run_steps(3);
    assert_eq!(h.state_mut().ui.dialog_mut(dialog).unwrap().fields["preset"], json!("Orbital"));
    crate::dialogs::confirm(h.state_mut(), dialog).unwrap();
    h.run_steps(3);
    let st = h.state().session.active().unwrap();
    assert!(matches!(&st.doc.layer(st.active_layer.unwrap()).unwrap().content, LayerContent::Fill(Fill::LensFlare(f)) if f.preset == "Orbital"));
}

/// A 400×300 document filling a canvas round the open Light Flare dialog, with the Brush tool.
fn canvas_harness() -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app.run("edit.fill", json!({"color": "#336699"})).unwrap();
    app.sync_views();
    app.ui.extras.rulers = false;
    app.ui.tool = crate::state::Tool::Brush;
    let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut PhotocraftApp| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into()))) {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| crate::canvas::document_area(app, ui));
            crate::dialogs::show(app, &ctx);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    open_new(h.state_mut()).unwrap();
    h.run_steps(4);
    h
}

fn xf(h: &Harness<'static, PhotocraftApp>) -> crate::canvas::ViewXform {
    crate::canvas::ViewXform::active(h.state()).unwrap()
}

fn drag(h: &mut Harness<'static, PhotocraftApp>, path: &[egui::Pos2]) {
    use egui::{Event, Modifiers, PointerButton};
    let (first, last) = (path[0], *path.last().unwrap());
    h.event(Event::PointerMoved(first));
    h.event(Event::PointerButton { pos: first, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for p in &path[1..] {
        h.event(Event::PointerMoved(*p));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: last, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

fn light(h: &Harness<'static, PhotocraftApp>) -> (f64, f64) {
    let f = &h.state().ui.dialogs.last().unwrap().fields;
    (f["x"].as_f64().unwrap(), f["y"].as_f64().unwrap())
}

#[test]
fn clicking_and_dragging_on_the_canvas_moves_the_light_at_any_zoom_without_painting() {
    let mut h = canvas_harness();
    let before = h.state().session.active().unwrap().doc.clone();
    let steps_before = steps(h.state());
    // Zoomed in and panned, then zoomed out with the image small in the window: the light lands
    // under the pointer, inside or outside the frame, and follows a drag.
    for (zoom, center) in [(3.0f32, [120.0f32, 90.0]), (1.0, [200.0, 150.0]), (0.5, [300.0, 50.0])] {
        let v = &mut h.state_mut().ui.views[0];
        (v.zoom, v.center, v.fit_pending) = (zoom, center, false);
        h.run_steps(2);
        let path = [pos2(40.0, 120.0), pos2(60.0, 300.0), pos2(90.0, 620.0)];
        drag(&mut h, &path);
        let size = (400.0, 300.0);
        let want = xf(&h).to_doc(*path.last().unwrap());
        let want = ((want[0] / size.0).clamp(-1.0, 2.0), (want[1] / size.1).clamp(-1.0, 2.0));
        let got = light(&h);
        assert!((got.0 - want.0).abs() < 0.002 && (got.1 - want.1).abs() < 0.002, "zoom {zoom}: {got:?} vs {want:?}");
    }
    let (x, y) = light(&h);
    assert!(!(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y), "the last drag ends outside the frame: {x}, {y}");
    // The Brush never ran: the document is the same snapshot, with no new History step.
    assert!(Arc::ptr_eq(&before, &h.state().session.active().unwrap().doc));
    assert_eq!(steps(h.state()), steps_before);
    // The preview follows the light; OK commits it as one step.
    let (shown, _) = display_doc(h.state_mut(), 0).unwrap();
    let flare = shown.layers.iter().find_map(|l| match &l.content {
        LayerContent::Fill(Fill::LensFlare(f)) => Some(f.source),
        _ => None,
    });
    assert_eq!(flare, Some((x as f32, y as f32)));
    let dialog = h.state().ui.dialogs.last().unwrap().id;
    crate::dialogs::confirm(h.state_mut(), dialog).unwrap();
    h.run_steps(2);
    assert_eq!(steps(h.state()), steps_before + 1);
    assert!(!placing(h.state()), "a closed dialog no longer owns the canvas");
}

#[test]
fn agents_place_the_light_with_ui_pointer_and_no_tool_runs() {
    let mut app = app();
    app.sync_views();
    app.ui.tool = crate::state::Tool::Brush;
    let ctx = egui::Context::default();
    let dialog = open_new(&mut app).unwrap();
    let before = app.session.active().unwrap().doc.clone();
    let events = json!([{"kind": "down", "x": 16, "y": 9}, {"kind": "move", "x": 48, "y": 27}, {"kind": "up", "x": 48, "y": 27}]);
    let (req, _rx) = crate::control::ControlRequest::new("ui.pointer", json!({"events": events}));
    let _ = crate::control::handle(&mut app, &ctx, &req);
    let f = &app.ui.dialog_mut(dialog).unwrap().fields;
    let (x, y) = (f["x"].as_f64().unwrap(), f["y"].as_f64().unwrap());
    assert!((x - 0.75).abs() < 1e-4 && (y - 0.75).abs() < 1e-4, "{x}, {y}");
    assert!(Arc::ptr_eq(&before, &app.session.active().unwrap().doc), "no stroke");
    // Without the dialog, nothing is intercepted.
    crate::dialogs::cancel(&mut app, dialog).unwrap();
    assert!(!place_at(&mut app, [1.0, 1.0]));
}

#[test]
fn the_pad_and_look_images_are_cached_per_revision() {
    let mut app = app();
    let ctx = egui::Context::default();
    let dialog = open_new(&mut app).unwrap();
    let f = app.ui.dialog_mut(dialog).unwrap().fields.clone();
    let a = canvas::thumbs(&app, &ctx, &f).unwrap();
    let b = canvas::thumbs(&app, &ctx, &f).unwrap();
    assert_eq!(a.pad.id(), b.pad.id(), "no recomposite per frame");
    assert_eq!(a.looks.len(), looks::LOOKS.len());
    assert_eq!(a.pad.size(), [64, 36], "a small document is never upscaled");
    app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
    let c = canvas::thumbs(&app, &ctx, &f).unwrap();
    assert_ne!(a.pad.id(), c.pad.id(), "a new revision redraws");
    assert_eq!(canvas::cover_uv([200, 100], 2.0), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
    let uv = canvas::cover_uv([400, 100], 2.0);
    assert!((uv.min.x - 0.25).abs() < 1e-6 && uv.min.y == 0.0);
}

#[test]
fn the_filters_tab_picks_a_look_that_ok_commits_with_the_flare_in_one_step() {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 64, "height": 36})).unwrap();
    let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    h.run_steps(4);
    let count = steps(h.state());
    let dialog = open_new(h.state_mut()).unwrap();
    h.run_steps(4);
    use egui_kittest::kittest::Queryable;
    h.get_by_label("Filters").click();
    h.run_steps(3);
    h.get_by_label("Moonlight").click();
    h.run_steps(3);
    assert_eq!(h.state_mut().ui.dialog_mut(dialog).unwrap().fields["look"], json!("moonlight"));
    let (shown, _) = display_doc(h.state_mut(), 0).unwrap();
    assert!(shown.layers.iter().any(|l| l.name == "Moonlight Look"), "the preview shows the look");
    crate::dialogs::confirm(h.state_mut(), dialog).unwrap();
    h.run_steps(3);
    assert_eq!(steps(h.state()), count + 1, "flare and look are one step");
    let st = h.state().session.active().unwrap();
    let flare = st.active_layer.unwrap();
    assert_eq!(cmds::look_of(&st.doc, flare), "moonlight");
    // Reopened, the dialog shows the look; another look replaces it, None removes it.
    let dialog = open(h.state_mut()).unwrap();
    assert_eq!(h.state_mut().ui.dialog_mut(dialog).unwrap().fields["look"], json!("moonlight"));
    set(h.state_mut(), dialog, json!({"look": "crossLab"}));
    crate::dialogs::confirm(h.state_mut(), dialog).unwrap();
    let names = |h: &Harness<'_, PhotocraftApp>| h.state().session.active().unwrap().doc.layers.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    assert!(names(&h).contains(&"Cross Lab Look".to_string()) && !names(&h).contains(&"Moonlight Look".to_string()), "{:?}", names(&h));
    let dialog = open(h.state_mut()).unwrap();
    set(h.state_mut(), dialog, json!({"look": "none"}));
    crate::dialogs::confirm(h.state_mut(), dialog).unwrap();
    assert!(!names(&h).iter().any(|n| n.ends_with(" Look")), "{:?}", names(&h));
    assert_eq!(steps(h.state()), count + 3);
}
