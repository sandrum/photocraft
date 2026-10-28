# Lens Flare layers

Layer › New Fill Layer › **Lens Flare…** adds an editable flare: a fill layer whose content is a
`Fill::LensFlare` (`crates/doc/src/lens_flare.rs`), rendered by the compositor
(`crates/compose/src/lens_flare.rs`). Unlike Filter › Render › Lens Flare, which paints into a
pixel layer once, the flare stays a description you can reopen and change until the image is
final (double-click the layer thumbnail, or Layer › Layer Content Options…).

## What a flare is

- **Elements** from the preset: glows, anamorphic streaks, star spikes, fine rays, halo rings and
  arcs, and ghosts (round or polygonal, with rims and chromatic edges) along the axis from the
  light through the frame centre.
- **Controls** the user changes: light source position, Scale, Brightness, Aspect (horizontal
  stretch of the source glow), Source (strength of the elements at the light), Artifacts
  (strength of the ghosts), Rotation, and a colour wheel (Hue + Tint).
- **Texture**: lens dirt that the flare's light reveals (Film Grain, Dust, Big Spots, Hex Dust,
  Raindrops, Scratches, Smudge) with Opacity and Flip X / Flip Y.

The preset library has four tabs (Anamorphic, Spherical, Sun Flares, Streaks), ten original
presets each. A new preset keeps the user's controls and position.

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
- `layer.fill.lensFlare.presets` `{category?}`: the library, for agents.
