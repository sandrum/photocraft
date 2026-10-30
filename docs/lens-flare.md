# Lens Flare layers

Layer › New Fill Layer › **Lens Flare…** adds an editable flare: a fill layer whose content is a
`Fill::LensFlare` (`crates/doc/src/lens_flare.rs`), rendered by the compositor
(`crates/compose/src/lens_flare.rs`). Unlike Filter › Render › Lens Flare, which paints into a
pixel layer once, the flare stays a description you can reopen and change until the image is
final (double-click the layer thumbnail, or Layer › Layer Content Options…).

## What a flare is

- **Elements** from the preset: glows, anamorphic streaks, star spikes, fine rays, halo rings and
  arcs, ghosts (round or polygonal, with rims and chromatic edges) along the axis from the
  light through the frame centre, and edge washes.
- **Edge wash** (`FlareElement::Wash`): edge light, as when the sun sits just outside the frame.
  It starts where the line from the frame centre through the source leaves the frame (at most one
  `reach` behind the source; at the source itself when that is outside the frame) and fans into
  the frame in a soft cone `spread` degrees wide, fading over `reach`. Near where it enters, the
  cone opens to a broad half-disc, so the edge glows instead of ending in a point. `chroma` adds a
  faint rainbow fringe where the wash fades, and `angle` (plus the flare's Rotation) turns it. It
  counts as a source element (scaled by Source).
- **Controls** the user changes: light source position, Scale, Brightness, Aspect (horizontal
  stretch of the source glow), Source (strength of the elements at the light), Artifacts
  (strength of the ghosts), Rotation, and a colour wheel (Hue + Tint).
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

## Placing the light

While the dialog is open, click or drag **on the image** to move the light source, as on the
dialog's position pad (which shows a thumbnail of the document, without the flare being edited).
The light follows the pointer at any zoom, pan or rotation (screen → document through the canvas
`ViewXform`), and may sit outside the frame (−1…2 of the canvas size). A crosshair marks the
source on the canvas. The dialog owns the canvas meanwhile, like Field Blur's pins
(`canvas::handle_dialog_canvas_input`): no tool paints or selects; Space and the middle button
still pan. Agents do the same with `ui.pointer` events (document coordinates) while the dialog
is on top. The pad and Filters thumbnails are rendered once per document revision.

## Looks (Filters tab)

The **Filters** tab grades the whole image with a look, previewed on thumbnails of the document:
None, Golden Hour, Blue Hour, Teal & Amber, Silver Bleach, Faded Print, Moonlight, Old Sepia,
Punch, Vivid, Noir and Cross Lab. A look is ordinary adjustment layers (Curves, Hue/Saturation,
Color Balance, Photo Filter, Vibrance, Brightness/Contrast, Black & White) in a collapsed
pass-through group named "<Look> Look", directly above the flare and the layers clipped to it, so
it stays editable and its opacity sets its strength. The group carries an XMP tag
(`urn:photocraft:lens-flare:1`, look id and flare layer id), so picking another look replaces it
and None removes it instead of stacking. OK commits the flare and its look as one History step.

## How it composites

The layer renders light as straight RGBA with `alpha = max(light)`, and is created with the
**Screen** blend mode, which makes it exactly `image + light·(1 − image)`. Everything else is a
normal layer: masks (paint the mask to hide the flare behind a subject), opacity, blend mode,
clipping, stacking several flare layers, and adjustment layers above them as colour filters.

The GPU compositor evaluates the flare in a shader (`fs_flare` in `crates/gpu/src/flare.wgsl`):
`lens_flare::prepare` lays the flare out on the canvas once (positions, sizes, colours, each
element's constants and each ray's angle, length and strength), `crates/gpu/src/plan/flare.rs`
packs that into the pass's LUT texture, and the shader repeats the CPU's per-pixel arithmetic,
textures and dither. It matches the CPU within one 8-bit step (`crates/gpu/tests/it/lens_flare.rs`
checks every preset and texture). CMYK documents still composite flares on the CPU. On an Intel
Iris Xe a 27 MP render costs about 80 ms per flare on the GPU, against 0.3–1 s on 12 CPU cores.

## Files

- `.pcraft` saves the flare as data (editable on reopen).
- PSD has no flare fill, so export writes the layer as its rendered pixels (with a warning).

## Commands

- `layer.newFillLayer.lensFlare` `{preset, x, y, scale, brightness, aspect, source, artifacts,
  rotation, hue, tint, texture, textureOpacity, flipX, flipY, blend, look, path}`
- `layer.fill.lensFlare.set` `{layer, …the same}`: omitted values keep the layer's.
- `layer.fill.lensFlare.look` `{layer?, document?, look}`: replace the flare's look (`"none"`
  removes it), one History step → `{layer, look, group}`.
- `layer.fill.lensFlare.presets` `{category?}`: the library (presets, textures, looks), for agents.

## Previewing presets

`FLARE_DUMP=dir cargo test -p photocraft-compose --lib dump -- --ignored` writes every preset
(`NN_Name.raw`) and every texture under one flare (`texNN_id.raw`) as raw 480×270 RGBA `f32`
(little-endian, straight alpha); `FLARE_TEX=<id>` renders the presets over a texture.
