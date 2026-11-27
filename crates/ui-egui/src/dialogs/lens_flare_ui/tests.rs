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
    assert_eq!(f.elements, lens_flare::preset("Sun Burst").unwrap().elements(), "the preset set on the dialog brings its own parts");
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
    assert!(place_at(&mut app, [48.0, 9.0], false));
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

/// Alt on the image moves the centre point the lens axis runs through, not the light; OK commits
/// the dialog's part edits (a part hidden, the centre point moved) as one History step.
#[test]
fn alt_on_the_image_moves_the_centre_point_and_part_edits_commit_in_one_step() {
    use crate::canvas::testing::{clear_of_dialogs, frame, move_to, press, release};
    let mut app = app();
    let ctx = app.ctx().clone();
    app.run(cmds::CREATE, json!({"preset": "Six Blades", "x": 0.2, "y": 0.3})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    let parts = flare(&app, layer).elements.len();
    crate::menus::invoke(&mut app, &ctx, "layer.layerContentOptions", Value::Null).unwrap();
    let dialog = app.framework.dialogs.open_dialogs()[0].id;
    frame(&mut app);
    clear_of_dialogs(&mut app, [48.0, 27.0]);
    let fields = |app: &PhotocraftApp| app.framework.dialogs.get::<LightFlareDialog>().unwrap().fields.clone();
    let alt = egui::Modifiers::ALT;
    move_to(&mut app, [48.0, 27.0], alt);
    press(&mut app, [48.0, 27.0], alt);
    release(&mut app, [48.0, 27.0], alt);
    let f = fields(&app);
    assert!((num(&f, "centerX", 0.0) - 0.75).abs() < 0.03 && (num(&f, "centerY", 0.0) - 0.75).abs() < 0.03, "Alt placed the centre point");
    assert!((num(&f, "x", 0.0) - 0.2).abs() < 1e-3, "and left the light where it was");
    // the canvas marks both where they now are (64 × 36 document)
    let (light, center) = markers(&app).expect("markers while the dialog places the light");
    assert!((light[0] - 0.2 * 64.0).abs() < 0.1 && (center[0] - 48.0).abs() < 2.0 && (center[1] - 27.0).abs() < 2.0, "{light:?} {center:?}");
    // the Parts tab's edits: hide the second part, delete the last
    let mut f = fields(&app);
    parts::edit::toggle(&mut f, 1);
    parts::edit::delete(&mut f, parts - 1);
    set(&mut app, dialog, json!({"elements": f["elements"], "hidden": f["hidden"]}));
    let count = steps(&app);
    confirm(&mut app, dialog);
    assert_eq!(steps(&app), count + 1, "one step");
    let g = flare(&app, layer);
    assert_eq!((g.elements.len(), g.is_hidden(1), g.preset.as_str()), (parts - 1, true, "Six Blades"));
    assert!((g.center.0 - 0.75).abs() < 0.03 && (g.source.0 - 0.2).abs() < 1e-3);
}

/// The parts editor by pointer (beside the Edit column, on every tab): a click on a part's radio
/// selects it (its settings show beside the list), and its Show check box hides it.
#[test]
fn the_parts_editor_selects_and_hides_parts() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    let mut app = app();
    crate::theme::install_fonts(app.ctx());
    crate::theme::apply(app.ctx(), &crate::theme::AppTheme::pro());
    let ctx = app.ctx().clone();
    crate::menus::invoke(&mut app, &ctx, cmds::CREATE, json!({})).unwrap();
    let dialog = app.framework.dialogs.open_dialogs()[0].id;
    set(&mut app, dialog, json!({"preset": "Six Blades", "__tab": "streak"}));
    let mut h = Harness::builder()
        .with_context(ctx)
        .with_size(vec2(1400.0, 1100.0))
        .build_ui_state(|ui, app: &mut Box<PhotocraftApp>| crate::dialogs::run(app, ui), app);
    h.run_steps(4);
    let fields = |h: &Harness<'static, Box<PhotocraftApp>>| h.state().framework.dialogs.get::<LightFlareDialog>().unwrap().fields.clone();
    let click = |h: &mut Harness<'static, Box<PhotocraftApp>>, pos: egui::Pos2| {
        h.hover_at(pos);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
            h.run_steps(2);
        }
    };
    let rect_of = |h: &Harness<'static, Box<PhotocraftApp>>, role: egui::accesskit::Role, starts: &str| {
        h.query_all_by_role(role).find(|n| n.accesskit_node().label().is_some_and(|l| l.starts_with(starts))).map(|n| n.rect())
    };
    let row = rect_of(&h, egui::accesskit::Role::RadioButton, "2. ").expect("a radio per part");
    click(&mut h, row.center());
    assert_eq!(parts::selected(&fields(&h)), 1, "a click on a part's radio selects it");
    let shows: Vec<_> = h
        .query_all_by_role(egui::accesskit::Role::CheckBox)
        .filter(|n| n.accesskit_node().label().is_some_and(|l| l.starts_with("Show")))
        .map(|n| n.rect())
        .collect();
    assert!(shows.len() >= 2, "a Show check box per part");
    click(&mut h, shows[1].center());
    assert_eq!(parts::hidden(&fields(&h)), vec![1], "the second part's Show hides it");
    if let Some(dir) = std::env::var_os("PHOTOCRAFT_TEST_SHOTS") {
        let img = h.render().expect("render");
        img.save(std::path::Path::new(&dir).join("light-flare-parts.png")).expect("save");
    }
}

/// egui's debug warning for an id used twice in one pass ("Second use of widget ID …") never shows
/// over the dialog: craft's measuring run lays the body out once more in some passes, and the
/// dialog's own egui widgets must not reuse the shown pass's ids there. On every tab.
#[test]
fn the_dialog_reuses_no_widget_id_in_a_pass() {
    use egui_kittest::Harness;
    let mut app = app();
    crate::theme::install_fonts(app.ctx());
    let ctx = app.ctx().clone();
    ctx.options_mut(|o| o.warn_on_id_clash = true);
    crate::menus::invoke(&mut app, &ctx, cmds::CREATE, json!({})).unwrap();
    let dialog = app.framework.dialogs.open_dialogs()[0].id;
    let mut h = Harness::builder()
        .with_context(ctx)
        .with_size(vec2(1400.0, 1100.0))
        .build_ui_state(|ui, app: &mut Box<PhotocraftApp>| crate::dialogs::run(app, ui), app);
    let clashes = |h: &Harness<'static, Box<PhotocraftApp>>| -> Vec<String> {
        fn walk(s: &egui::Shape, out: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) if t.galley.text().contains("use of") => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &h.output().shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    };
    for tab in ["anamorphic", "filters", "streak"] {
        set(h.state_mut(), dialog, json!({"__tab": tab}));
        for _ in 0..6 {
            h.run_steps(1);
            let c = clashes(&h);
            assert!(c.is_empty(), "{tab}: {c:?}");
        }
        if tab == "anamorphic"
            && let Some(dir) = std::env::var_os("PHOTOCRAFT_TEST_SHOTS")
        {
            let img = h.render().expect("render");
            img.save(std::path::Path::new(&dir).join("light-flare-edit.png")).expect("save");
        }
    }
}

/// Flip Horizontal mirrors the whole flare (light, centre point, rotation, the parts' angles, the
/// texture) and a second flip brings it back; the Blend picked in the dialog is the layer's after
/// OK, with the new settings, in one step.
#[test]
fn flips_mirror_the_whole_flare_and_blend_and_the_new_settings_commit_in_one_step() {
    let mut app = app();
    let ctx = app.ctx().clone();
    app.run(cmds::CREATE, json!({"preset": "Six Blades", "x": 0.2, "y": 0.3, "rotation": 30.0})).unwrap();
    let layer = app.session.active().unwrap().active_layer.unwrap();
    crate::menus::invoke(&mut app, &ctx, "layer.layerContentOptions", Value::Null).unwrap();
    let dialog = app.framework.dialogs.open_dialogs()[0].id;
    let fields = |app: &PhotocraftApp| app.framework.dialogs.get::<LightFlareDialog>().unwrap().fields.clone();
    let before = fields(&app);
    assert_eq!(before["blend"], "screen", "the layer's blend mode");
    let mut f = before.clone();
    mirror(&mut f, true);
    assert!((num(&f, "x", 0.0) - 0.8).abs() < 1e-3 && (num(&f, "y", 0.0) - 0.3).abs() < 1e-3, "the light moved across");
    assert_eq!((num(&f, "rotation", 0.0), flag(&f, "flipX")), (-30.0, !flag(&before, "flipX")));
    let angles = |f: &Map<String, Value>| f["elements"].as_array().unwrap().iter().filter_map(|p| p.get("angle").and_then(Value::as_f64)).collect::<Vec<_>>();
    assert_eq!(angles(&f), angles(&before).iter().map(|a| -a).collect::<Vec<_>>(), "every part's angle turns the other way");
    mirror(&mut f, true);
    for k in ["x", "y", "centerX", "rotation", "flipX", "elements"] {
        assert_eq!(f.get(k).map(Value::to_string).map(|v| v.replace("-0.0", "0.0")), before.get(k).map(Value::to_string), "{k} after two flips");
    }
    // a mirrored flare with a new blend and the new settings, committed as one step
    mirror(&mut f, false);
    set(
        &mut app,
        dialog,
        json!({"y": f["y"], "centerY": f["centerY"], "rotation": f["rotation"], "elements": f["elements"], "flipY": f["flipY"],
        "blend": "lineardodge", "spread": 1.6, "saturation": 0.4, "sourceBrightness": 0.5, "softness": 0.3, "fringe": 0.5}),
    );
    let count = steps(&app);
    confirm(&mut app, dialog);
    assert_eq!(steps(&app), count + 1, "one step");
    let st = app.session.active().unwrap();
    assert_eq!(st.doc.layer(layer).unwrap().blend, BlendMode::LinearDodge);
    let g = flare(&app, layer);
    assert!((g.source.1 - 0.7).abs() < 1e-3 && g.rotation == -30.0 && g.texture.flip_y);
    assert_eq!((g.spread, g.saturation, g.source_brightness, g.softness, g.fringe), (1.6, 0.4, 0.5, 0.3, 0.5));
}
