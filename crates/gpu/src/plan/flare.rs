//! Lens Flare fills on the GPU: the flare laid out by `photocraft_compose::lens_flare::prepare`,
//! packed into a pass's LUT rows for `fs_flare` (`flare.wgsl`), which evaluates it per pixel.
//!
//! Layout (one flat float array over the rows, index `i` at texel `(i % 4096, i / 4096)`):
//! a header of [`HEADER`] floats (see `lf_pixel` / `lf_texture` in the shader), then one record
//! of [`STRIDE`] floats per element (kind, centre, colour, bounding box, shape constants), then
//! each Rays element's table (angle, length, strength per ray).

use photocraft_compose::lens_flare::{self, Prepared, Shape};
use photocraft_doc::lens_flare::{LensFlare, TextureKind};
use photocraft_geom::Rect;

const HEADER: usize = 32;
const STRIDE: usize = 20;
const ROW: usize = 4096;

// Element kinds (`LF_*` in the shader).
const GLOW: f32 = 0.0;
const GLOW_GAUSS: f32 = 1.0;
const STREAK: f32 = 2.0;
const STAR: f32 = 3.0;
const RAYS: f32 = 4.0;
const RING: f32 = 5.0;
const GHOST: f32 = 6.0;
const WASH: f32 = 7.0;

/// `flare` on `canvas`, rounded to `quantum` levels with the triangular dither (`None`: float),
/// as LUT rows for `fs_flare`.
pub(super) fn rows(flare: &LensFlare, canvas: Rect, quantum: Option<f32>) -> Vec<[f32; ROW]> {
    pack(&lens_flare::prepare(flare, canvas), quantum)
}

fn pack(p: &Prepared, quantum: Option<f32>) -> Vec<[f32; ROW]> {
    let b = |v: bool| if v { 1.0 } else { 0.0 };
    let t = &p.texture;
    let kind = TextureKind::ALL.iter().position(|k| *k == t.kind).unwrap_or(0) as f32;
    let (ox, oy, cw, ch) = p.canvas;
    let mut v = vec![0.0f32; HEADER];
    v[..24].copy_from_slice(&[
        p.elems.len() as f32,
        p.sx,
        p.sy,
        p.cos,
        p.sin,
        p.inv_aspect,
        kind,
        t.opacity,
        b(t.flip_x),
        b(t.flip_y),
        ox,
        oy,
        cw,
        ch,
        p.tex_unit,
        p.tex_radius,
        p.tex_color[0],
        p.tex_color[1],
        p.tex_color[2],
        lens_flare::CUTOFF,
        b(p.has_texture()),
        quantum.unwrap_or(0.0),
        lens_flare::DITHER_DX as f32,
        lens_flare::DITHER_DY as f32,
    ]);
    // Ray tables follow the element records.
    let mut tables = Vec::new();
    let table_start = HEADER + STRIDE * p.elems.len();
    for e in &p.elems {
        let mut r = [0.0f32; STRIDE];
        let (kind, params): (f32, Vec<f32>) = match &e.shape {
            Shape::Glow { r, gauss } => (if *gauss { GLOW_GAUSS } else { GLOW }, vec![*r]),
            Shape::Streak { cos, sin, len, th, halo } => (STREAK, vec![*cos, *sin, *len, *th, *halo]),
            Shape::Star { rot, n, len, th } => (STAR, vec![*rot, std::f32::consts::TAU / n, *len, *th]),
            Shape::Rays { rot, len, th, rays } => {
                let at = (table_start + tables.len()) as f32;
                for ray in rays {
                    tables.extend([ray.phi, ray.reach, ray.strength]);
                }
                (RAYS, vec![*rot, std::f32::consts::TAU / rays.len().max(1) as f32, *len, *th, rays.len() as f32, at])
            }
            Shape::Ring { r, w, chroma, arc, dir } => (RING, vec![*r, *w, *chroma, *arc, dir.0, dir.1]),
            Shape::Ghost { r, sides, rot, rim, soft, chroma } => {
                let step = if *sides > 0 { std::f32::consts::TAU / *sides as f32 } else { 0.0 };
                (GHOST, vec![*r, *sides as f32, *rot, *rim, *soft, *chroma, step, (step / 2.0).cos()])
            }
            Shape::Wash { dir, reach, spread, chroma, peak } => (WASH, vec![dir.0, dir.1, *reach, *spread, *chroma, *peak, *reach * lens_flare::WASH_REACH]),
        };
        r[0] = kind;
        r[1] = e.cx;
        r[2] = e.cy;
        r[3..6].copy_from_slice(&e.col);
        r[6..10].copy_from_slice(&[e.bbox.x0, e.bbox.y0, e.bbox.x1, e.bbox.y1]);
        for (dst, src) in r[10..].iter_mut().zip(params) {
            *dst = src;
        }
        v.extend(r);
    }
    v.extend(tables);
    v.chunks(ROW)
        .map(|c| {
            let mut row = [0.0f32; ROW];
            row[..c.len()].copy_from_slice(c);
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::lens_flare::{FlareCategory, FlareElement, PRESETS};

    #[test]
    fn every_preset_packs_its_elements_and_ray_tables() {
        let canvas = Rect::new(0, 0, 640, 360);
        for preset in PRESETS {
            let f = preset.flare();
            let p = lens_flare::prepare(&f, canvas);
            let rows = pack(&p, Some(255.0));
            let at = |i: usize| rows[i / ROW][i % ROW];
            assert_eq!(at(0), p.elems.len() as f32, "{}", preset.name);
            assert_eq!(at(21), 255.0);
            let rays: usize = p.elems.iter().map(|e| if let Shape::Rays { rays, .. } = &e.shape { rays.len() } else { 0 }).sum();
            assert!(rows.len() * ROW >= HEADER + STRIDE * p.elems.len() + 3 * rays);
            for (i, e) in p.elems.iter().enumerate() {
                let r = HEADER + i * STRIDE;
                assert_eq!([at(r + 1), at(r + 2)], [e.cx, e.cy]);
                if let Shape::Rays { rays, .. } = &e.shape {
                    let table = at(r + 15) as usize;
                    for (k, ray) in rays.iter().enumerate() {
                        assert_eq!([at(table + 3 * k), at(table + 3 * k + 1), at(table + 3 * k + 2)], [ray.phi, ray.reach, ray.strength]);
                    }
                }
            }
        }
    }

    #[test]
    fn the_largest_flare_fits_the_lut() {
        let ray = FlareElement::Rays { count: u32::MAX, length: 100.0, intensity: 1.0, color: [1.0; 3], seed: 7 };
        let f = LensFlare::new(FlareCategory::SunFlare, "x", vec![ray; 1000]);
        let rows = rows(&f, Rect::new(0, 0, 100, 100), None);
        let max = photocraft_doc::lens_flare::range::MAX_ELEMENTS;
        let need = HEADER + STRIDE * max + 3 * max * photocraft_doc::lens_flare::range::MAX_RAYS as usize;
        assert_eq!(rows.len(), need.div_ceil(ROW));
        assert_eq!(rows[0][0], max as f32);
    }
}
