//! Rendering of Lens Flare fill layers ([`photocraft_doc::lens_flare`]).
//!
//! The flare is light added to the image. Each pixel sums its elements' light `L` (RGB, may be
//! above 1 near the source), spills the excess of a hot channel into the others so cores burn
//! to white, clamps, and is returned as straight RGBA with `alpha = max(L)` and
//! `rgb = L / alpha`. Composited with Screen (the default blend of a flare layer) that is
//! exactly `bg + L·(1 − bg)`, a screen of the light over the image; with Normal it is close to
//! it. Transparent where the flare adds nothing, so masks, opacity and other layers behave as
//! with any fill.
//!
//! Elements are prepared once per call with their canvas-space bounding box, and only those
//! that reach the rendered rect are evaluated per pixel.

use photocraft_doc::lens_flare::{FlareElement, LensFlare, TextureKind};
use photocraft_geom::Rect;

/// Light below this contributes nothing visible even at 16 bits.
const CUTOFF: f32 = 1.0 / 4096.0;

/// The flare's pixels over `rect`, laid out on `canvas` (straight RGBA, row-major).
pub fn render(flare: &LensFlare, rect: Rect, canvas: Rect) -> Vec<[f32; 4]> {
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    let mut px = vec![[0.0f32; 4]; w * h];
    if w == 0 || h == 0 || canvas.is_empty() {
        return px;
    }
    let prep = Prepared::new(flare, canvas);
    let live: Vec<&Elem> = prep.elems.iter().filter(|e| e.bbox.reaches(rect)).collect();
    let texture = prep.texture.kind != TextureKind::None && prep.texture.opacity > 0.0;
    if live.is_empty() && !texture {
        return px;
    }
    for (row, y) in (rect.y0..rect.y1).enumerate() {
        let fy = y as f32 + 0.5;
        for (col, x) in (rect.x0..rect.x1).enumerate() {
            let fx = x as f32 + 0.5;
            let mut l = [0.0f32; 3];
            for e in &live {
                e.add(fx, fy, &prep, &mut l);
            }
            if texture {
                prep.texture_light(fx, fy, &mut l);
            }
            if let Some(p) = px.get_mut(row * w + col) {
                *p = to_rgba(l);
            }
        }
    }
    px
}

/// Light to straight RGBA: hot channels spill into the others (cores burn to white), then
/// `alpha = max`, `rgb = light / alpha`.
fn to_rgba(mut l: [f32; 3]) -> [f32; 4] {
    for v in &mut l {
        *v = if v.is_finite() { v.max(0.0) } else { 0.0 };
    }
    let max = l[0].max(l[1]).max(l[2]);
    if max > 1.0 {
        let spill = (max - 1.0) * 0.6;
        for v in &mut l {
            *v = (*v + spill).min(1.0);
        }
    }
    let a = l[0].max(l[1]).max(l[2]).min(1.0);
    if a < CUTOFF {
        return [0.0; 4];
    }
    [(l[0] / a).min(1.0), (l[1] / a).min(1.0), (l[2] / a).min(1.0), a]
}

#[derive(Clone, Copy)]
struct BBox {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl BBox {
    fn around(cx: f32, cy: f32, rx: f32, ry: f32) -> BBox {
        BBox { x0: cx - rx, y0: cy - ry, x1: cx + rx, y1: cy + ry }
    }
    fn reaches(&self, r: Rect) -> bool {
        self.x1 >= r.x0 as f32 && self.x0 <= r.x1 as f32 && self.y1 >= r.y0 as f32 && self.y0 <= r.y1 as f32
    }
}

enum Shape {
    Glow { r: f32, gauss: bool },
    /// Local frame `(cos, sin)`; `len` along, `th` across.
    Streak { cos: f32, sin: f32, len: f32, th: f32 },
    Star { rot: f32, n: f32, len: f32, th: f32 },
    Rays { rot: f32, count: u32, len: f32, th: f32, seed: u32 },
    Ring { r: f32, w: f32, chroma: f32, arc: f32, dir: (f32, f32) },
    Ghost { r: f32, sides: u32, rot: f32, rim: f32, soft: f32, chroma: f32 },
}

struct Elem {
    cx: f32,
    cy: f32,
    /// Colour × intensity × gain × brightness.
    col: [f32; 3],
    shape: Shape,
    bbox: BBox,
}

struct Prepared {
    elems: Vec<Elem>,
    /// Source position and the local frame of the source's glows (rotation, aspect).
    sx: f32,
    sy: f32,
    cos: f32,
    sin: f32,
    inv_aspect: f32,
    texture: photocraft_doc::lens_flare::FlareTexture,
    /// Texture lookup: canvas origin and size, texture units per pixel.
    canvas: (f32, f32, f32, f32),
    tex_unit: f32,
    /// Broad light the texture shows in: radius around the source and its colour.
    tex_radius: f32,
    tex_color: [f32; 3],
}

fn hue_rgb(h: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    }
}

/// `c` moved `tint` of the way towards the hue at the same peak value.
fn tinted(c: [f32; 3], hue: [f32; 3], tint: f32) -> [f32; 3] {
    let peak = c[0].max(c[1]).max(c[2]);
    let mut out = [0.0; 3];
    for k in 0..3 {
        out[k] = c[k] + (hue[k] * peak - c[k]) * tint;
    }
    out
}

fn finite(v: f32, d: f32) -> f32 {
    if v.is_finite() { v } else { d }
}

impl Prepared {
    fn new(flare: &LensFlare, canvas: Rect) -> Prepared {
        let f = flare.clone().sanitized();
        let (cw, ch) = (canvas.width() as f32, canvas.height() as f32);
        let diag = cw.hypot(ch).max(1.0);
        let u = diag / 1000.0 * f.scale;
        let (sx, sy) = (canvas.x0 as f32 + f.source.0 * cw, canvas.y0 as f32 + f.source.1 * ch);
        let (mx, my) = (canvas.x0 as f32 + cw / 2.0, canvas.y0 as f32 + ch / 2.0);
        let rot = f.rotation.to_radians();
        let (sin, cos) = rot.sin_cos();
        let aspect = f.aspect;
        let hue = hue_rgb(f.hue);
        let axis = |t: f32| (sx + (mx - sx) * t, sy + (my - sy) * t);
        let away = {
            let (dx, dy) = (mx - sx, my - sy);
            let n = dx.hypot(dy);
            if n > 1e-3 { (dx / n, dy / n) } else { (0.0, 0.0) }
        };
        let mut elems = Vec::with_capacity(f.elements.len());
        let mut tex_color = [0.0f32; 3];
        for e in &f.elements {
            let gain = if e.at_source() { f.source_gain } else { f.artifacts } * f.brightness;
            let (color, intensity) = match e {
                FlareElement::Glow { color, intensity, .. }
                | FlareElement::Streak { color, intensity, .. }
                | FlareElement::Star { color, intensity, .. }
                | FlareElement::Rays { color, intensity, .. }
                | FlareElement::Ring { color, intensity, .. }
                | FlareElement::Ghost { color, intensity, .. } => (*color, finite(*intensity, 0.0).clamp(0.0, 4.0)),
            };
            let color = color.map(|c| finite(c, 0.0).clamp(0.0, 1.0));
            let c = tinted(color, hue, f.tint);
            for k in 0..3 {
                tex_color[k] += c[k] * intensity;
            }
            let col = c.map(|v| v * intensity * gain);
            if col.iter().all(|v| *v <= 0.0) {
                continue;
            }
            let pos = |v: f32| finite(v, 0.0).max(0.0);
            let el = match *e {
                FlareElement::Glow { radius, falloff, .. } => {
                    let r = (pos(radius) * u).max(0.3);
                    let gauss = falloff >= 1.5;
                    let reach = r * if gauss { 3.0 } else { 8.5 };
                    Elem { cx: sx, cy: sy, col, shape: Shape::Glow { r, gauss }, bbox: BBox::around(sx, sy, reach * aspect.max(1.0), reach) }
                }
                FlareElement::Streak { length, thickness, angle, .. } => {
                    let a = rot + finite(angle, 0.0).to_radians();
                    let (s, c) = a.sin_cos();
                    let len = (pos(length) * u * aspect).max(1.0);
                    let th = (pos(thickness) * u).max(0.35);
                    let across = th * 6.0;
                    let bbox = BBox::around(sx, sy, (len * c).abs() + (across * s).abs(), (len * s).abs() + (across * c).abs());
                    Elem { cx: sx, cy: sy, col, shape: Shape::Streak { cos: c, sin: s, len, th }, bbox }
                }
                FlareElement::Star { rays, length, thickness, angle, .. } => {
                    let n = rays.clamp(1, photocraft_doc::lens_flare::range::MAX_RAYS) as f32;
                    let len = (pos(length) * u).max(1.0);
                    let th = (pos(thickness) * u).max(0.35);
                    let shape = Shape::Star { rot: rot + finite(angle, 0.0).to_radians(), n, len, th };
                    Elem { cx: sx, cy: sy, col, shape, bbox: BBox::around(sx, sy, len, len) }
                }
                FlareElement::Rays { count, length, seed, .. } => {
                    let len = (pos(length) * u).max(1.0);
                    let count = count.clamp(1, photocraft_doc::lens_flare::range::MAX_RAYS);
                    let shape = Shape::Rays { rot, count, len, th: (0.7 * u).max(0.35), seed };
                    Elem { cx: sx, cy: sy, col, shape, bbox: BBox::around(sx, sy, len, len) }
                }
                FlareElement::Ring { t, radius, width, chroma, arc, .. } => {
                    let (cx, cy) = axis(finite(t, 0.0));
                    let r = (pos(radius) * u).max(0.5);
                    let w = (pos(width) * u).max(0.35);
                    let chroma = finite(chroma, 0.0).clamp(0.0, 2.0);
                    let reach = r * (1.0 + 0.06 * chroma) + 3.0 * w;
                    let shape = Shape::Ring { r, w, chroma, arc: finite(arc, 0.0).clamp(0.0, 1.0), dir: away };
                    Elem { cx, cy, col, shape, bbox: BBox::around(cx, cy, reach, reach) }
                }
                FlareElement::Ghost { t, radius, sides, rim, softness, chroma, .. } => {
                    let (cx, cy) = axis(finite(t, 0.0));
                    let r = (pos(radius) * u).max(0.5);
                    let sides = if sides >= 3 { sides.min(16) } else { 0 };
                    let chroma = finite(chroma, 0.0).clamp(0.0, 2.0);
                    let shape = Shape::Ghost {
                        r,
                        sides,
                        rot,
                        rim: finite(rim, 0.0).clamp(0.0, 2.0),
                        soft: finite(softness, 0.0).clamp(0.02, 1.0),
                        chroma,
                    };
                    let reach = r * (1.25 + 0.06 * chroma) / if sides > 0 { (std::f32::consts::PI / sides as f32).cos() } else { 1.0 };
                    Elem { cx, cy, col, shape, bbox: BBox::around(cx, cy, reach, reach) }
                }
            };
            elems.push(el);
        }
        let peak = tex_color[0].max(tex_color[1]).max(tex_color[2]);
        let tex_color = if peak > 0.0 { tex_color.map(|v| 0.35 + 0.65 * v / peak) } else { [1.0; 3] };
        Prepared {
            elems,
            sx,
            sy,
            cos,
            sin,
            inv_aspect: 1.0 / aspect,
            texture: f.texture,
            canvas: (canvas.x0 as f32, canvas.y0 as f32, cw, ch),
            tex_unit: 1000.0 / diag,
            tex_radius: 380.0 * u,
            tex_color: tex_color.map(|v| v * f.brightness * f.source_gain.max(0.2)),
        }
    }

    /// The texture's contribution: dirt lit by the flare's broad light, or grain over its light.
    fn texture_light(&self, x: f32, y: f32, l: &mut [f32; 3]) {
        let t = &self.texture;
        let (ox, oy, cw, ch) = self.canvas;
        let (mut tx, mut ty) = (x - ox, y - oy);
        if t.flip_x {
            tx = cw - tx;
        }
        if t.flip_y {
            ty = ch - ty;
        }
        let (qx, qy) = (tx * self.tex_unit, ty * self.tex_unit);
        if t.kind == TextureKind::FilmGrain {
            let n = hash01(hash2((qx * 1.5) as i32, (qy * 1.5) as i32, 77)) - 0.5;
            let lum = l[0].max(l[1]).max(l[2]);
            for v in l.iter_mut() {
                *v *= 1.0 + t.opacity * 0.9 * n;
            }
            // A faint veil of grain in the broad light too.
            let d = (x - self.sx).hypot(y - self.sy);
            let broad = (-d / self.tex_radius).exp() * (1.0 - lum.min(1.0));
            for (v, c) in l.iter_mut().zip(self.tex_color) {
                *v += t.opacity * 0.08 * (n + 0.5) * broad * c;
            }
            return;
        }
        let dirt = dirt(t.kind, qx, qy);
        if dirt <= 0.0 {
            return;
        }
        let d = (x - self.sx).hypot(y - self.sy);
        let lum = l[0].max(l[1]).max(l[2]).min(2.0);
        let lit = 0.55 * (-d / self.tex_radius).exp() + 0.5 * lum;
        for (v, c) in l.iter_mut().zip(self.tex_color) {
            *v += t.opacity * dirt * lit * c;
        }
    }
}

impl Elem {
    fn add(&self, x: f32, y: f32, p: &Prepared, l: &mut [f32; 3]) {
        if x < self.bbox.x0 || x > self.bbox.x1 || y < self.bbox.y0 || y > self.bbox.y1 {
            return;
        }
        let (dx, dy) = (x - self.cx, y - self.cy);
        match self.shape {
            Shape::Glow { r, gauss } => {
                // In the source's frame, stretched by Aspect.
                let lx = (dx * p.cos + dy * p.sin) * p.inv_aspect;
                let ly = -dx * p.sin + dy * p.cos;
                let d = lx.hypot(ly) / r;
                let v = if gauss { (-d * d).exp() } else { (-d).exp() };
                add(l, self.col, v);
            }
            Shape::Streak { cos, sin, len, th } => {
                let along = (dx * cos + dy * sin).abs();
                if along >= len {
                    return;
                }
                let across = (-dx * sin + dy * cos).abs() / th;
                let k = along / len;
                let v = ((-across * across).exp() * 0.75 + (-across / 3.0).exp() * 0.25) * (-2.2 * k).exp() * (1.0 - k);
                add(l, self.col, v);
            }
            Shape::Star { rot, n, len, th } => {
                let d = dx.hypot(dy);
                if d >= len {
                    return;
                }
                let step = std::f32::consts::TAU / n;
                let a = dy.atan2(dx) - rot;
                let delta = a - (a / step).round() * step;
                let along = d * delta.cos();
                if along <= 0.0 {
                    return;
                }
                let perp = d * delta.sin().abs();
                let k = along / len;
                let w = th * (1.0 + 2.0 * k);
                let v = (-(perp / w).powi(2)).exp() * (1.0 - k).powi(2);
                add(l, self.col, v);
            }
            Shape::Rays { rot, count, len, th, seed } => {
                let d = dx.hypot(dy);
                if d >= len || d < 0.5 {
                    return;
                }
                let step = std::f32::consts::TAU / count as f32;
                let a = (dy.atan2(dx) - rot).rem_euclid(std::f32::consts::TAU);
                let bin = (a / step) as i32;
                let mut v = 0.0f32;
                for b in [bin - 1, bin, bin + 1] {
                    let k = b.rem_euclid(count as i32);
                    let hseed = hash2(k, seed as i32, 0x51ed);
                    let phi = (k as f32 + hash01(hseed)) * step;
                    let reach = len * (0.25 + 0.75 * hash01(hseed ^ 0x9e37));
                    let delta = {
                        let raw = a - phi;
                        raw - (raw / std::f32::consts::TAU).round() * std::f32::consts::TAU
                    };
                    let along = d * delta.cos();
                    if along <= 0.0 || along >= reach {
                        continue;
                    }
                    let perp = d * delta.sin().abs();
                    let strength = 0.35 + 0.65 * hash01(hseed ^ 0x7f4a);
                    v += strength * (-(perp / th).powi(2)).exp() * (1.0 - along / reach).powf(1.5);
                }
                add(l, self.col, v);
            }
            Shape::Ring { r, w, chroma, arc, dir } => {
                let d = dx.hypot(dy);
                let mut facing = 1.0;
                if arc > 0.0 && d > 1e-3 && (dir.0 != 0.0 || dir.1 != 0.0) {
                    let c = (dx * dir.0 + dy * dir.1) / d;
                    facing = 1.0 - arc + arc * smoothstep(-0.1, 0.7, c);
                }
                for (k, v) in l.iter_mut().enumerate() {
                    let rk = r * (1.0 + chroma * 0.03 * (1.0 - k as f32));
                    let e = (d - rk) / w;
                    *v += self.col[k] * (-e * e).exp() * facing;
                }
            }
            Shape::Ghost { r, sides, rot, rim, soft, chroma } => {
                let mut dd = dx.hypot(dy) / r;
                if sides > 0 {
                    let step = std::f32::consts::TAU / sides as f32;
                    let a = (dy.atan2(dx) - rot).rem_euclid(step) - step / 2.0;
                    dd *= (step / 2.0).cos() / a.cos();
                }
                if dd >= 1.25 + 0.06 * chroma {
                    return;
                }
                for (k, v) in l.iter_mut().enumerate() {
                    let e = dd / (1.0 + chroma * 0.04 * (1.0 - k as f32));
                    let body = 1.0 - smoothstep(1.0 - soft, 1.0, e);
                    let edge = (-((e - 0.97) / 0.05).powi(2)).exp();
                    let shade = body * (1.0 - 0.5 * rim + 0.5 * rim * e * e * e) + 0.6 * rim * edge;
                    *v += self.col[k] * shade;
                }
            }
        }
    }
}

fn add(l: &mut [f32; 3], col: [f32; 3], v: f32) {
    if v > CUTOFF {
        for k in 0..3 {
            l[k] += col[k] * v;
        }
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if (e1 - e0).abs() < f32::EPSILON {
        return if x < e0 { 0.0 } else { 1.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash2(x: i32, y: i32, seed: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ (seed as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^ (h >> 15)
}

fn hash01(h: u32) -> f32 {
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Value noise in `0..=1` at `(x, y)` (lattice spacing 1).
fn value_noise(x: f32, y: f32, seed: i32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (ix, iy) = (x0 as i32, y0 as i32);
    let v = |i: i32, j: i32| hash01(hash2(ix + i, iy + j, seed));
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = v(0, 0) + (v(1, 0) - v(0, 0)) * sx;
    let bottom = v(0, 1) + (v(1, 1) - v(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// Lens dirt of `kind` at `(x, y)` in texture units (a thousandth of the canvas diagonal):
/// `0` = clean glass, `1` = the brightest speck.
fn dirt(kind: TextureKind, x: f32, y: f32) -> f32 {
    // (cell size, probability, radius range, seed) of the spotted kinds.
    let (cell, prob, rmin, rmax, seed) = match kind {
        TextureKind::None | TextureKind::FilmGrain => return 0.0,
        TextureKind::Smudge => {
            let n = 0.6 * value_noise(x / 140.0, y / 140.0, 11) + 0.3 * value_noise(x / 55.0, y / 55.0, 12) + 0.1 * value_noise(x / 18.0, y / 18.0, 13);
            return smoothstep(0.5, 0.85, n) * 0.6;
        }
        TextureKind::Dust => (30.0, 0.45, 1.0, 4.0, 21),
        TextureKind::BigSpots => (140.0, 0.55, 25.0, 60.0, 22),
        TextureKind::HexDust => (90.0, 0.5, 14.0, 38.0, 23),
        TextureKind::Raindrops => (70.0, 0.5, 8.0, 26.0, 24),
        TextureKind::Scratches => (200.0, 0.8, 50.0, 95.0, 25),
    };
    let (cx, cy) = ((x / cell).floor() as i32, (y / cell).floor() as i32);
    let mut best = 0.0f32;
    for j in -1..=1 {
        for i in -1..=1 {
            let h = hash2(cx + i, cy + j, seed);
            if hash01(h) > prob {
                continue;
            }
            let px = (cx + i) as f32 * cell + hash01(h ^ 0x68e3) * cell;
            let py = (cy + j) as f32 * cell + hash01(h ^ 0x2b79) * cell;
            let r = rmin + (rmax - rmin) * hash01(h ^ 0x4cf5);
            let b = 0.35 + 0.65 * hash01(h ^ 0x1d87);
            let (dx, dy) = (x - px, y - py);
            let v = match kind {
                TextureKind::Dust => b * (1.0 - smoothstep(r * 0.4, r, dx.hypot(dy))),
                TextureKind::BigSpots => {
                    let d = dx.hypot(dy) / r;
                    b * 0.55 * ((1.0 - smoothstep(0.55, 1.0, d)) + 0.5 * (-((d - 0.95) / 0.08).powi(2)).exp())
                }
                TextureKind::HexDust => {
                    let step = std::f32::consts::TAU / 6.0;
                    let a = dy.atan2(dx).rem_euclid(step) - step / 2.0;
                    let d = dx.hypot(dy) / r * (step / 2.0).cos() / a.cos();
                    b * 0.6 * (1.0 - smoothstep(0.75, 1.0, d))
                }
                TextureKind::Raindrops => {
                    let d = dx.hypot(dy) / r;
                    if d > 1.2 {
                        0.0
                    } else {
                        b * (0.18 * (1.0 - smoothstep(0.8, 1.0, d)) + 0.9 * (-((d - 0.88) / 0.1).powi(2)).exp())
                    }
                }
                TextureKind::Scratches => {
                    let ang = hash01(h ^ 0x7a1c) * std::f32::consts::PI;
                    let (s, c) = ang.sin_cos();
                    let along = (dx * c + dy * s).abs();
                    if along > r {
                        0.0
                    } else {
                        let perp = (-dx * s + dy * c).abs();
                        b * 0.8 * (-(perp / 0.7).powi(2)).exp() * (1.0 - along / r)
                    }
                }
                TextureKind::None | TextureKind::FilmGrain | TextureKind::Smudge => 0.0,
            };
            best = best.max(v);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::lens_flare::{self, FlareCategory, PRESETS};

    fn canvas() -> Rect {
        Rect::new(0, 0, 160, 90)
    }

    fn at(px: &[[f32; 4]], w: usize, x: usize, y: usize) -> [f32; 4] {
        px[y * w + x]
    }

    #[test]
    fn every_preset_renders_finite_light_brightest_at_the_source() {
        for p in PRESETS {
            let mut f = p.flare();
            f.source = (0.25, 0.4);
            let px = render(&f, canvas(), canvas());
            assert!(px.iter().flatten().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{}", p.name);
            // The source is the corner of these four pixels.
            let src = [(39, 35), (40, 35), (39, 36), (40, 36)].iter().map(|&(x, y)| at(&px, 160, x, y)[3]).fold(0.0f32, f32::max);
            assert!(src > 0.7, "{}: source alpha {src}", p.name);
            assert!(px.iter().all(|q| q[3] <= src + 1e-6), "{}: something is brighter than the source", p.name);
        }
    }

    #[test]
    fn tiles_match_one_render() {
        let mut f = lens_flare::preset("Prime Classic").map(|p| p.flare()).unwrap_or_else(|| lens_flare::LensFlare::new(FlareCategory::Spherical, "x", vec![]));
        f.texture.kind = TextureKind::HexDust;
        f.rotation = 20.0;
        let whole = render(&f, canvas(), canvas());
        let tile = Rect::new(64, 32, 128, 64);
        let part = render(&f, tile, canvas());
        for y in 32..64 {
            for x in 64..128 {
                assert_eq!(part[(y - 32) * 64 + (x - 64)], at(&whole, 160, x, y));
            }
        }
    }

    #[test]
    fn controls_move_and_scale_the_light() {
        let base = lens_flare::preset("White Source").map(|p| p.flare()).unwrap_or_else(|| lens_flare::LensFlare::new(FlareCategory::SunFlare, "x", vec![]));
        let mut f = base.clone();
        f.source = (0.75, 0.5);
        let px = render(&f, canvas(), canvas());
        assert!(at(&px, 160, 120, 45)[3] > at(&px, 160, 40, 45)[3]);
        let mut dim = base.clone();
        dim.brightness = 0.0;
        assert!(render(&dim, canvas(), canvas()).iter().all(|p| p[3] == 0.0), "brightness 0 adds no light");
        let mut big = base.clone();
        big.scale = 3.0;
        let small = render(&base, canvas(), canvas());
        let large = render(&big, canvas(), canvas());
        assert!(at(&large, 160, 70, 27)[3] > at(&small, 160, 70, 27)[3], "a larger flare reaches further");
    }

    #[test]
    fn tint_moves_colour_towards_the_hue() {
        let base = lens_flare::preset("White Line").map(|p| p.flare()).unwrap_or_else(|| lens_flare::LensFlare::new(FlareCategory::Streak, "x", vec![]));
        let mut red = base.clone();
        red.source = (0.5, 0.5);
        red.hue = 0.0;
        red.tint = 1.0;
        let px = render(&red, canvas(), canvas());
        let p = at(&px, 160, 120, 45);
        assert!(p[3] > 0.0 && p[0] > p[2] + 0.2, "{p:?}");
    }

    #[test]
    fn hostile_flares_do_not_panic() {
        let mut f = lens_flare::LensFlare::new(FlareCategory::Anamorphic, "x", vec![]);
        f.elements = vec![
            FlareElement::Glow { radius: f32::NAN, intensity: f32::INFINITY, color: [f32::NAN; 3], falloff: f32::NAN },
            FlareElement::Star { rays: u32::MAX, length: 1e30, thickness: -1.0, intensity: 1.0, color: [1.0; 3], angle: f32::NAN },
            FlareElement::Rays { count: 0, length: 0.0, intensity: 1.0, color: [1.0; 3], seed: u32::MAX },
            FlareElement::Ghost { t: f32::NAN, radius: 0.0, intensity: 1.0, color: [1.0; 3], sides: u32::MAX, rim: 9.0, softness: -3.0, chroma: 1e9 },
        ];
        f.scale = f32::NAN;
        f.texture.kind = TextureKind::Scratches;
        let px = render(&f, canvas(), canvas());
        assert!(px.iter().flatten().all(|v| v.is_finite()));
        assert!(render(&f, Rect::EMPTY, canvas()).is_empty());
        assert!(render(&f, canvas(), Rect::EMPTY).iter().all(|p| p[3] == 0.0));
    }
}

#[cfg(test)]
mod dump {
    use super::*;
    use photocraft_doc::lens_flare::PRESETS;

    /// Dumps every preset as raw RGBA f32 (`FLARE_DUMP=dir cargo test -p photocraft-compose dump -- --ignored`).
    #[test]
    #[ignore]
    fn dump_presets() {
        let Ok(dir) = std::env::var("FLARE_DUMP") else { return };
        let c = Rect::new(0, 0, 480, 270);
        for (i, p) in PRESETS.iter().enumerate() {
            let mut f = p.flare();
            f.source = (0.3, 0.35);
            if let Ok(t) = std::env::var("FLARE_TEX") {
                f.texture.kind = TextureKind::from_id(&t).unwrap_or_default();
                f.texture.opacity = 1.0;
            }
            let px = render(&f, c, c);
            let bytes: Vec<u8> = px.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
            std::fs::write(format!("{dir}/{i:02}_{}.raw", p.name.replace(' ', "_")), bytes).unwrap();
        }
    }
}
