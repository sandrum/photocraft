# Light Flare layers

Layer › New Fill Layer › **Light Flare…** adds an editable flare: a fill layer whose content is a
`Fill::LensFlare` (`crates/doc/src/lens_flare.rs`), rendered by the compositor
(`crates/compose/src/lens_flare.rs`). Unlike Filter › Render › Lens Flare, which paints into a
pixel layer once, the flare stays a description you can reopen and change until the image is
final (double-click the layer thumbnail, or Layer › Layer Content Options…).

## What a flare is

- **Elements** from the preset: glows, anamorphic streaks, star spikes, fine rays, halo rings and
  arcs, ghosts (round or polygonal, with rims and chromatic edges) along the axis from the
  light through the flare's **center point** (the frame centre unless moved), and edge washes.
  Each is a **part** the parts editor edits on its own (below).
- **Edge wash** (`FlareElement::Wash`): edge light, as when the sun sits just outside the frame.
  It starts where the line from the frame centre through the source leaves the frame (at most one
  `reach` behind the source; at the source itself when that is outside the frame) and fans into
  the frame in a soft cone `spread` degrees wide, fading over `reach`. Near where it enters, the
  cone opens to a broad half-disc, so the edge glows instead of ending in a point. `chroma` adds a
  faint rainbow fringe where the wash fades, and `angle` (plus the flare's Rotation) turns it. It
  counts as a source element (sized by Source).
- **Controls** the user changes, laid out so the dialog fits without scrolling (the image itself
  shows the flare live, so the dialog has no preview of its own):
  - beside the parts editor (**Edit**): Brightness, Scale, the layer's **Blend** mode (Screen by default;
    also Linear Dodge (Add), Lighten, Color Dodge, Overlay, Soft Light, Normal), **Flip Horizontal**
    and **Flip Vertical** (mirror the whole flare: the light and center point move to the mirrored
    places, the rotation and every part's own angle turn the other way, and the texture flips with
    it), and Reset;
  - **Light**: **Spread** (a tone curve on the flare's light: below 1 only the hot core is left,
    above 1 the faint halo and ghosts spread further), Source Size (size of the glow, streaks and
    rays at the light), **Source Brightness** (their strength: 0 leaves only the lens artifacts),
    Artifacts (strength of the ghosts and rings off the light);
  - **Shape**: Aspect (horizontal stretch of the glow at the light: above 1 the wide anamorphic band,
    below 1 a tall oval), Rotation, **Softness** (thin lines thicken and edges blur), **Color Fringe**
    (red and blue separate about the light: each part as three one-colour copies, red larger and blue
    smaller for a positive value, the other way round for a negative one);
  - **Color**: **Saturation** (0 = white light, its parts' rainbow edges included; 2 = stronger),
    and the colour wheel (Hue + Tint).
- **Texture**: lens dirt or film texture that the flare's light reveals, with Opacity and
  Flip X / Flip Y. All are procedural (no image assets), seeded and evaluated per pixel at a
  bounded cost (at most a 3×3 cell search or a few noise lookups), in units of the canvas
  diagonal so they look the same at any resolution:
  - grain: Film Grain, Coarse Grain (clumps), Fine Grain (one value per pixel);
  - specks and spots: Dust, Stretched Specks (horizontally stretched, anamorphic), Soft Orbs
    (many soft discs), Large Blots, Hex Dust, Bokeh Bubbles (large discs with bright rims), Water
    Beads (drops with bright rims);
  - surfaces: Scratches, Smudge, Hazy Glass (diffuse fog with dust), Glass Arcs (curved streaky
    reflections), Brushed Steel (fine parallel lines).

The preset library has four tabs (Anamorphic, Spherical, Sun Flares, Streaks), 24 original
presets each (96 in all). Sun Flares includes edge-wash looks (Morning Spill, Corner Bloom,
Window Light, Prism Leak, Low Sun Spill, Blue Hour Leak, Ember Leak, Silver Spill, Forked Light,
Hazy Noon) and large chromatic rims (Rainbow Rim, Ring of Day). A new preset keeps the user's
controls and position. Preset names are original; don't reuse names from other flare tools.

## Editing the parts

The parts editor, under the library and beside the Edit column, lists the flare's parts, numbered, with their kind (Glow, Streak, Star,
Rays, Ring, Ghost, Edge Wash) and, for a ring or ghost off the light, where it sits on the axis
(`Ghost at 0.70`: 0 = the light, 1 = the center point, 2 = as far again). A part's radio selects it
(click, or the arrow keys); its **Show** check box hides it without losing its settings. Under the
list: **Add Part** (a menu of the kinds; the new part starts on the light, or half way to the center
point for a ghost), **Duplicate**, **Delete**, **Up** / **Down** (the order is only the list's:
the light adds up the same), the center point's position and **Reset Center**. Beside the
list, the selected part's own settings and its **Color**:

- Glow: Brightness, Size, Falloff (1 = a wide skirt, 2 = a tight core).
- Streak: Brightness, Length, Thickness, Angle. Star: Brightness, Points, Length, Thickness, Angle.
  Rays: Brightness, Count, Length, Variation (another random set of rays).
- Ring: Brightness, Position, Radius, Width, Rainbow Edge, Half Ring (keeps the side facing away
  from the light). Ghost: Brightness, Position, Radius, Sides (below 3: round), Rim, Softness,
  Rainbow Edge. Edge Wash: Brightness, Reach, Spread, Rainbow Edge, Angle.

Sizes are thousandths of the image's diagonal before the flare's Scale; Source Size and Artifacts
still scale the parts at the light and along the axis. Picking a preset replaces the parts with its
own (all shown). The parts editor and the flare's own controls stay in place whatever the library
tab.

## The center point

The axis the ghosts and rings line up on runs from the light through the center point. While the
dialog is open the image marks it with a diamond (the light with a ring, the axis as a thin line
through both); Alt-click or Alt-drag on the image moves it (−1…2 of the canvas, like the light).
Reset Center (under the parts list) puts it back in the middle.
An edge wash fans into the frame towards it.

## Placing the light

While the dialog is open, click or drag **on the image** to move the light source; the canvas
shows the flare following it, as OK leaves it, with a ring at the light and a diamond at the center
point (the dialog tool's `paint`, from `lens_flare_ui::markers`). The dialog is modal, so it lets the primary press
through its modal layer to the document's canvas (`Sampler::pass_canvas_gesture`, as the Pattern
Fill and Layer Style dialogs do), and the canvas's dialog tool runs the `FlareLight` pick
(`canvas_input`) instead of the selected tool: no tool paints or selects, the pointer is a
crosshair, and the wheel, Space and the middle button still scroll, zoom and pan. The light follows
the pointer at any zoom, pan or rotation (screen → document through the canvas view), and may sit
outside the frame (−1…2 of the canvas size). Agents do the same with `ui.pointer` events
(document coordinates) while the dialog is on top. The Filters thumbnails are rendered once per
document revision.

## Looks (Filters tab)

The **Filters** tab grades the whole image with a look, previewed on thumbnails of the document:
None, Golden Hour, Blue Hour, Teal & Amber, Silver Bleach, Faded Print, Moonlight, Old Sepia,
Punch, Vivid, Noir and Cross Lab. A look is ordinary adjustment layers (Curves, Hue/Saturation,
Color Balance, Photo Filter, Vibrance, Brightness/Contrast, Black & White) in a collapsed
pass-through group named "<Look> Look", directly above the flare and the layers clipped to it, so
it stays editable and its opacity sets its strength. The group carries an XMP tag
(`urn:photocraft:light-flare:1`, look id and flare layer id), so picking another look replaces it
and None removes it instead of stacking. OK commits the flare and its look as one History step.

## How it composites

The layer renders light as straight RGBA with `alpha = max(light)`, and is created with the
**Screen** blend mode, which makes it exactly `image + light·(1 − image)`. Everything else is a
normal layer: masks (paint the mask to hide the flare behind a subject), opacity, blend mode,
clipping, stacking several flare layers, and adjustment layers above them as colour filters.

The GPU compositor evaluates the flare in a shader (`fs_flare` in `crates/gpu/src/flare.wgsl`):
`lens_flare::prepare` lays the flare out on the canvas once (positions, sizes, colours, each
element's constants and each ray's angle, length and strength; Saturation, Source Brightness,
Softness and Color Fringe are settled there, the fringe as three one-colour copies of each part), `crates/gpu/src/plan/flare.rs`
packs that into the pass's LUT texture, and the shader repeats the CPU's per-pixel arithmetic,
textures, the Spread curve (header slot 24) and dither. It matches the CPU within one 8-bit step (`crates/gpu/tests/it/lens_flare.rs`
checks every preset and texture). CMYK documents still composite flares on the CPU. On an Intel
Iris Xe a 27 MP render costs about 80 ms per flare on the GPU, against 0.3–1 s on 12 CPU cores.

## Files

- `.pcraft` saves the flare as data (editable on reopen), with its parts, the hidden ones and the
  center point. Files from before the center point load with it at the frame centre.
- PSD has no flare fill, so export writes the layer as its rendered pixels (with a warning).

## Commands

- `layer.newFillLayer.lightFlare` `{preset, x, y, scale, brightness, aspect, source, artifacts,
  rotation, hue, tint, spread (0.2…3), saturation (0…2), sourceBrightness (0…2), softness (0…1),
  fringe (−1…1), texture, textureOpacity, flipX, flipY, centerX, centerY, elements, hidden, blend,
  look, path}`: `elements` is the list of parts as the result's `flare.elements` gives them
  (`{kind: "glow"|"streak"|"star"|"rays"|"ring"|"ghost"|"wash", …}`) and replaces the preset's;
  `hidden` lists the indices of parts kept but not rendered.
- `layer.fill.lightFlare.set` `{layer, …the same}`: omitted values keep the layer's.
- `layer.fill.lightFlare.look` `{layer?, document?, look}`: replace the flare's look (`"none"`
  removes it), one History step → `{layer, look, group}`.
- `layer.fill.lightFlare.presets` `{category?}`: the library (presets, textures, looks), for agents.

## Previewing presets

`FLARE_DUMP=dir cargo test -p photocraft-compose --lib dump -- --ignored` writes every preset
(`NN_Name.raw`) and every texture under one flare (`texNN_id.raw`) as raw 480×270 RGBA `f32`
(little-endian, straight alpha); `FLARE_TEX=<id>` renders the presets over a texture.
