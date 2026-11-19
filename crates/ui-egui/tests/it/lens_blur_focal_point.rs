//! Lens Blur's focal point: with a depth map, a click on the dialog's preview pane sets `focalPoint` to the document
//! pixel under the click and Focal Distance to the depth there; without one, a click only moves the view. OK sends the
//! point to the command.
//! Public control API and the accessibility tree only; no GPU adapter is needed.

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use photocraft_ui_egui::{
    PhotocraftApp,
    control::{ControlRequest, Reply, handle},
    dialogs, i18n,
    theme::AppTheme,
};
use serde_json::{Value, json};

const COMMAND: &str = "filter.blur.lensBlur";
const SIZE: [u32; 2] = [64, 48];

fn harness() -> Harness<'static, Box<PhotocraftApp>> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    app.run("file.new", json!({"width": SIZE[0], "height": SIZE[1], "background": "transparent"})).unwrap();
    app.run("paint.stroke", json!({"points": [[16, 16, 1], [48, 32, 1]], "size": 12, "color": "#ff8000"})).unwrap();
    let h = Harness::builder().with_context(app.ctx().clone()).with_size(vec2(1200.0, 900.0)).build_ui_state(
        |ui, app: &mut Box<PhotocraftApp>| {
            let _language = i18n::language_scope(i18n::Lang::EN);
            dialogs::run(app, ui);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, AppTheme::pro());
    h
}

/// Sends one control request, requires success and returns its `result`.
fn control(h: &mut Harness<'_, Box<PhotocraftApp>>, method: &str, params: Value) -> Value {
    let ctx = h.ctx.clone();
    let (request, _rx) = ControlRequest::new(method, params);
    let Reply::Done(reply) = handle(h.state_mut(), &ctx, &request) else { panic!("{method} did not finish") };
    assert_eq!(reply["ok"], true, "{method}: {reply}");
    h.run_steps(3);
    reply["result"].clone()
}

/// The value under `key` anywhere in an inspect reply.
fn find<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map.get(key).or_else(|| map.values().find_map(|v| find(v, key))),
        Value::Array(items) => items.iter().find_map(|v| find(v, key)),
        _ => None,
    }
}

fn focal_point(h: &mut Harness<'_, Box<PhotocraftApp>>) -> Option<Value> {
    let inspected = control(h, "ui.inspect", json!({}));
    find(&inspected, "focalPoint").filter(|v| !v.is_null()).cloned()
}

/// Opens Lens Blur with the layer's transparency as its depth map.
fn open(h: &mut Harness<'_, Box<PhotocraftApp>>) -> Value {
    let opened = control(h, "ui.menu.invoke", json!({"id": COMMAND}));
    let id = opened["dialog"].clone();
    control(h, "ui.dialog.set", json!({"dialog": id, "field": "depthMap", "value": "transparency"}));
    h.run_steps(4);
    id
}

fn click_preview(h: &mut Harness<'_, Box<PhotocraftApp>>) {
    h.get_by_role_and_label(Role::Group, "Preview").click();
    h.run_steps(4);
}

#[test]
fn with_a_depth_map_a_preview_click_picks_the_pixel_there() {
    let mut h = harness();
    let id = open(&mut h);
    click_preview(&mut h);
    let point = focal_point(&mut h).expect("the click set the focal point");
    let [x, y] = [point[0].as_f64().unwrap(), point[1].as_f64().unwrap()];
    assert!((0.0..f64::from(SIZE[0])).contains(&x) && (0.0..f64::from(SIZE[1])).contains(&y), "on the image: {point}");
    assert_eq!((x.fract(), y.fract()), (0.0, 0.0), "a whole pixel: {point}");
    // The row's clear button carries the row's name, as the framework names a custom row's first control.
    assert!(h.query_by_role_and_label(Role::Button, "Focal Point").is_some(), "the point shows with its clear button");

    let before = h.state().session.active().unwrap().revision;
    control(&mut h, "ui.dialog.confirm", json!({"dialog": id}));
    assert!(h.state().session.active().unwrap().revision > before, "OK applied Lens Blur with the focal point");
}

#[test]
fn a_focal_point_needs_a_depth_map_and_clear_forgets_it() {
    let mut h = harness();
    let id = open(&mut h);
    click_preview(&mut h);
    assert!(focal_point(&mut h).is_some());
    h.get_by_role_and_label(Role::Button, "Focal Point").click();
    h.run_steps(3);
    assert_eq!(focal_point(&mut h), None, "the clear button forgets the point");

    // Without a depth map every pixel blurs alike: a click only moves the view.
    control(&mut h, "ui.dialog.set", json!({"dialog": id, "field": "depthMap", "value": "none"}));
    click_preview(&mut h);
    assert_eq!(focal_point(&mut h), None);
}

#[test]
fn the_focal_distance_follows_the_depth_under_the_point() {
    let mut h = harness();
    // An alpha channel white in the middle, black at the edges: the depth map.
    for (method, params) in [
        ("select.rect", json!({"x": 16, "y": 12, "width": 32, "height": 24})),
        ("select.saveSelection", json!({"name": "Depth"})),
        ("select.deselect", json!({})),
    ] {
        h.state_mut().run(method, params).unwrap();
    }
    let opened = control(&mut h, "ui.menu.invoke", json!({"id": COMMAND}));
    let id = opened["dialog"].clone();
    control(&mut h, "ui.dialog.set", json!({"dialog": id, "field": "depthMap", "value": "channel:0"}));
    h.run_steps(4);
    click_preview(&mut h);
    let point = focal_point(&mut h).expect("the click set the focal point");
    let distance = |h: &mut Harness<'_, Box<PhotocraftApp>>| find(&control(h, "ui.inspect", json!({})), "focalDistance").and_then(Value::as_f64);
    assert_eq!(distance(&mut h), Some(255.0), "the middle of the depth map is white: {point}");

    // Invert reads the depth map the other way: the slider follows.
    control(&mut h, "ui.dialog.set", json!({"dialog": id, "field": "invert", "value": true}));
    h.run_steps(3);
    assert_eq!(distance(&mut h), Some(0.0));

    // Moving the slider takes over from the point.
    control(&mut h, "ui.dialog.set", json!({"dialog": id, "field": "focalDistance", "value": 100}));
    h.run_steps(3);
    assert_eq!(focal_point(&mut h), None, "the slider cleared the point");
    assert_eq!(distance(&mut h), Some(100.0));
}
