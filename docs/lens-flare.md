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

## How it composites

The layer renders light as straight RGBA with `alpha = max(light)`, and is created with the
**Screen** blend mode, which makes it exactly `image + light·(1 − image)`. Everything else is a
normal layer: masks (paint the mask to hide the flare behind a subject), opacity, blend mode,
clipping, stacking several flare layers, and adjustment layers above them as colour filters.

## Files

- `.pcraft` saves the flare as data (editable on reopen).
- PSD has no flare fill, so export writes the layer as its rendered pixels (with a warning).
- The GPU compositor falls back to the CPU for documents with a flare layer.

## Commands

- `layer.newFillLayer.lensFlare` `{preset, x, y, scale, brightness, aspect, source, artifacts,
  rotation, hue, tint, texture, textureOpacity, flipX, flipY, blend, path}`
- `layer.fill.lensFlare.set` `{layer, …the same}`: omitted values keep the layer's.
- `layer.fill.lensFlare.presets` `{category?}`: the library, for agents (also lists the texture
  ids).

## Previewing presets

`FLARE_DUMP=dir cargo test -p photocraft-compose --lib dump -- --ignored` writes every preset
(`NN_Name.raw`) and every texture under one flare (`texNN_id.raw`) as raw 480×270 RGBA `f32`
(little-endian, straight alpha); `FLARE_TEX=<id>` renders the presets over a texture.
