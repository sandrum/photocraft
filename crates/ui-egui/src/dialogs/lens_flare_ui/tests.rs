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
    assert!(app.framework.dialogs.get::<LightFlareDialog>().unwrap().preview.is_some(), "the dialog shows the image with the flare");
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

/// The image takes the light as the preview does: with the modal dialog open, a click and a drag on
/// the canvas reach it and move the light there.
#[test]
fn a_click_and_a_drag_on_the_image_move_the_light() {
    use crate::canvas::testing::{clear_of_dialogs, frame, move_to, press, release};
    let none = egui::Modifiers::NONE;
    let mut app = app();
    let ctx = app.ctx().clone();
    crate::menus::invoke(&mut app, &ctx, cmds::CREATE, json!({})).unwrap();
    frame(&mut app);
    clear_of_dialogs(&mut app, [16.0, 18.0]);
    let light = |app: &PhotocraftApp| {
        let f = &app.framework.dialogs.get::<LightFlareDialog>().unwrap().fields;
        (num(f, "x", f32::NAN), num(f, "y", f32::NAN))
    };
    let near = |(x, y): (f32, f32), (wx, wy): (f32, f32)| (x - wx).abs() < 0.03 && (y - wy).abs() < 0.03;
    // The document is 64 × 36, so the click at (16, 18) is a quarter across and halfway down.
    move_to(&mut app, [16.0, 18.0], none);
    press(&mut app, [16.0, 18.0], none);
    release(&mut app, [16.0, 18.0], none);
    let clicked = light(&app);
    assert!(near(clicked, (0.25, 0.5)), "a click on the image placed the light there: {clicked:?}");
    press(&mut app, [16.0, 18.0], none);
    move_to(&mut app, [16.0, 24.0], none);
    move_to(&mut app, [16.0, 30.0], none);
    release(&mut app, [16.0, 30.0], none);
    let dragged = light(&app);
    assert!(near(dragged, (0.25, 30.0 / 36.0)), "a drag on the image carried the light down: {clicked:?} -> {dragged:?}");
    assert!(app.framework.dialogs.is_open::<LightFlareDialog>(), "placing the light keeps the dialog open");
}

/// The dialog covers most of the canvas, so its preview is where the light is placed: a click and
/// a drag on it move the light there.
#[test]
fn a_click_and_a_drag_on_the_preview_move_the_light() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    let mut app = app();
    crate::theme::install_fonts(app.ctx());
    let ctx = app.ctx().clone();
    crate::menus::invoke(&mut app, &ctx, cmds::CREATE, json!({})).unwrap();
    let mut h = Harness::builder()
        .with_context(ctx)
        .with_size(vec2(1400.0, 1100.0))
        .build_ui_state(|ui, app: &mut Box<PhotocraftApp>| crate::dialogs::run(app, ui), app);
    h.run_steps(4);
    let light = |h: &Harness<'static, Box<PhotocraftApp>>| {
        let f = &h.state().framework.dialogs.get::<LightFlareDialog>().unwrap().fields;
        (num(f, "x", f32::NAN), num(f, "y", f32::NAN))
    };
    let pad =
        h.query_all_by_role(egui::accesskit::Role::Slider).find(|n| n.accesskit_node().label().is_some_and(|l| l.starts_with("Light Source"))).unwrap().rect();
    let (pad_min, size) = (pad.min, pad.size());
    assert!(size.x >= 400.0, "the preview is large: {size:?}");
    let at = |fx: f32, fy: f32| pad_min + vec2(size.x * fx, size.y * fy);
    let (x0, y0) = light(&h);
    let press = |h: &mut Harness<'static, Box<PhotocraftApp>>, pos, pressed| {
        h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
        h.run_steps(2);
    };
    h.hover_at(at(0.8, 0.7));
    press(&mut h, at(0.8, 0.7), true);
    press(&mut h, at(0.8, 0.7), false);
    let (x1, y1) = light(&h);
    assert!(x1 > x0 + 0.3 && y1 > y0 + 0.3, "a click moved the light towards the bottom right: {x0},{y0} -> {x1},{y1}");
    press(&mut h, at(0.8, 0.7), true);
    for k in 1..=5 {
        h.hover_at(at(0.8 - 0.12 * k as f32, 0.7 - 0.1 * k as f32));
        h.run_steps(1);
    }
    press(&mut h, at(0.2, 0.2), false);
    let (x2, y2) = light(&h);
    assert!(x2 < x1 - 0.4 && y2 < y1 - 0.3, "a drag carried the light up and left: {x1},{y1} -> {x2},{y2}");
}
