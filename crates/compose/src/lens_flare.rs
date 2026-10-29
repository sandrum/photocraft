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
    render_quantized(flare, rect, canvas, None)
}

/// [`render`] for a document whose depth has `quantum` levels ([`crate::adjustment_quantum`]):
/// the light is dithered and rounded to those levels, as a gradient fill is. A flare's glows fall
/// off over thousands of pixels, so undithered they reach the 8-bit image as wide one-level
/// rings (tinted where the channels round at different radii); the noise breaks them up.
pub fn render_quantized(flare: &LensFlare, rect: Rect, canvas: Rect, quantum: Option<f32>) -> Vec<[f32; 4]> {
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
            let mut l = burn(l);
            // Only where the flare adds light: the rest of the layer stays transparent.
            if let Some(q) = quantum
                && l.iter().any(|v| *v > 0.0)
            {
                dither(&mut l, x, y, q);
            }
            if let Some(p) = px.get_mut(row * w + col) {
                *p = to_rgba(l);
            }
        }
    }
    px
}

/// Light into `0..=1`: hot channels spill into the others (cores burn to white).
fn burn(mut l: [f32; 3]) -> [f32; 3] {
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
    l
}

/// Rounds light to `q` levels with triangular noise of ±1 level, the same in every channel (no
/// colour speckle). Uniform ±½-level noise, as gradients use, still leaves equal runs where the
/// light sits near a level for long, and a glow's falloff does for hundreds of pixels; the
/// triangular one makes the rounding error independent of the light, so no band survives.
fn dither(l: &mut [f32; 3], x: i32, y: i32, q: f32) {
    let n = photocraft_color::dither_noise(x, y) + photocraft_color::dither_noise(x.wrapping_add(7919), y.wrapping_add(104_729)) - 1.0;
    for v in l.iter_mut() {
        *v = ((*v * q + n + 0.5).floor() / q).clamp(0.0, 1.0);
    }
}

/// Burnt light to straight RGBA: `alpha = max`, `rgb = light / alpha`.
fn to_rgba(l: [f32; 3]) -> [f32; 4] {
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
    Glow {
        r: f32,
        gauss: bool,
    },
    /// Local frame `(cos, sin)`; `len` along; across, a crisp core `th` wide in a soft skirt
    /// `halo` wide.
    Streak {
        cos: f32,
        sin: f32,
        len: f32,
        th: f32,
        halo: f32,
    },
    Star {
        rot: f32,
        n: f32,
        len: f32,
        th: f32,
    },
    Rays {
        rot: f32,
        count: u32,
        len: f32,
        th: f32,
        seed: u32,
    },
    Ring {
        r: f32,
        w: f32,
        chroma: f32,
        arc: f32,
        dir: (f32, f32),
    },
    Ghost {
        r: f32,
        sides: u32,
        rot: f32,
        rim: f32,
        soft: f32,
        chroma: f32,
    },
    /// Centred on the cone's apex (behind the source); `dir` points into the frame.
    Wash {
        dir: (f32, f32),
        reach: f32,
        spread: f32,
        chroma: f32,
        peak: f32,
    },
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
        let ut = u * (DETAIL_REF / diag).powf(DETAIL_SHRINK).min(1.0);
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
                | FlareElement::Ghost { color, intensity, .. }
                | FlareElement::Wash { color, intensity, .. } => (*color, finite(*intensity, 0.0).clamp(0.0, 4.0)),
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
                    let gauss = falloff >= 1.5;
                    // A Gaussian glow is a hot core: a detail, so it stays a point of light.
                    let r = (pos(radius) * if gauss { ut } else { u }).max(0.3);
                    let reach = r * if gauss { 3.0 } else { 8.5 };
                    Elem { cx: sx, cy: sy, col, shape: Shape::Glow { r, gauss }, bbox: BBox::around(sx, sy, reach * aspect.max(1.0), reach) }
                }
                FlareElement::Streak { length, thickness, angle, .. } => {
                    let a = rot + finite(angle, 0.0).to_radians();
                    let (s, c) = a.sin_cos();
                    let len = (pos(length) * u * aspect).max(1.0);
                    let th = (pos(thickness) * ut).max(0.35);
                    let halo = 3.0 * (pos(thickness) * u).max(0.35);
                    // Far enough for the skirt to fade out (0.25·e⁻⁹), not end in a visible edge.
                    let across = (halo * 9.0).max(th * 3.0);
                    let bbox = BBox::around(sx, sy, (len * c).abs() + (across * s).abs(), (len * s).abs() + (across * c).abs());
                    Elem { cx: sx, cy: sy, col, shape: Shape::Streak { cos: c, sin: s, len, th, halo }, bbox }
                }
                FlareElement::Star { rays, length, thickness, angle, .. } => {
                    let n = rays.clamp(1, photocraft_doc::lens_flare::range::MAX_RAYS) as f32;
                    let len = (pos(length) * u).max(1.0);
                    let th = (pos(thickness) * ut).max(0.35);
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
                    let shape = Shape::Ghost { r, sides, rot, rim: finite(rim, 0.0).clamp(0.0, 2.0), soft: finite(softness, 0.0).clamp(0.02, 1.0), chroma };
                    let reach = r * (1.25 + 0.06 * chroma) / if sides > 0 { (std::f32::consts::PI / sides as f32).cos() } else { 1.0 };
                    Elem { cx, cy, col, shape, bbox: BBox::around(cx, cy, reach, reach) }
                }
                FlareElement::Wash { reach, spread, chroma, angle, .. } => {
                    let reach = (pos(reach) * u).max(1.0);
                    // Into the frame: from the source towards the centre (or along the rotation
                    // when the source is the centre), turned by the rotation and the element's angle.
                    let base = if away != (0.0, 0.0) { away.1.atan2(away.0) } else { 0.0 };
                    let (s, c) = (base + rot + finite(angle, 0.0).to_radians()).sin_cos();
                    // The light enters where the line from the source outwards leaves the frame (at
                    // most a reach behind the source; at the source when it is outside the frame).
                    let exit = |p: f32, d: f32, lo: f32, hi: f32| {
                        if d > 1e-6 {
                            (lo - p) / -d
                        } else if d < -1e-6 {
                            (hi - p) / -d
                        } else {
                            f32::INFINITY
                        }
                    };
                    let (x0, y0, x1, y1) = (canvas.x0 as f32, canvas.y0 as f32, canvas.x1 as f32, canvas.y1 as f32);
                    let back = exit(sx, c, x0, x1).min(exit(sy, s, y0, y1)).clamp(0.0, reach * WASH_APEX);
                    let (cx, cy) = (sx - c * back, sy - s * back);
                    let spread = finite(spread, 30.0).clamp(1.0, 180.0).to_radians();
                    let chroma = finite(chroma, 0.0).clamp(0.0, 2.0);
                    let peak = col[0].max(col[1]).max(col[2]);
                    let shape = Shape::Wash { dir: (c, s), reach, spread, chroma, peak };
                    Elem { cx, cy, col, shape, bbox: BBox::around(cx, cy, reach * WASH_REACH, reach * WASH_REACH) }
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
        let grain = match t.kind {
            TextureKind::FilmGrain => Some((hash01(hash2((qx * 1.5) as i32, (qy * 1.5) as i32, 77)) - 0.5, 0.9)),
            // Clumps a few texture units wide, with a little fine grain on top.
            TextureKind::CoarseGrain => {
                let clump = value_noise(qx / 4.0, qy / 4.0, 78) + 0.6 * value_noise(qx / 1.6, qy / 1.6, 79);
                let fine = hash01(hash2((qx * 1.5) as i32, (qy * 1.5) as i32, 80));
                Some(((clump / 1.6 - 0.5) * 1.6 + 0.25 * (fine - 0.5), 1.1))
            }
            // One value per pixel, whatever the canvas size: the finest grain the image can hold.
            TextureKind::FineGrain => Some((hash01(hash2(tx.floor() as i32, ty.floor() as i32, 81)) - 0.5, 0.6)),
            _ => None,
        };
        if let Some((n, amount)) = grain {
            let lum = l[0].max(l[1]).max(l[2]);
            for v in l.iter_mut() {
                *v *= 1.0 + t.opacity * amount * n;
            }
            // A faint veil of grain in the broad light too.
            let d = (x - self.sx).hypot(y - self.sy);
            let broad = (-d / self.tex_radius).exp() * (1.0 - lum.min(1.0));
            for (v, c) in l.iter_mut().zip(self.tex_color) {
                *v += t.opacity * 0.08 * (n + 0.5).max(0.0) * broad * c;
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
            Shape::Streak { cos, sin, len, th, halo } => {
                let along = (dx * cos + dy * sin).abs();
                if along >= len {
                    return;
                }
                let perp = (-dx * sin + dy * cos).abs();
                let core = perp / th;
                let k = along / len;
                let v = ((-core * core).exp() * 0.75 + (-perp / halo).exp() * 0.25) * (-2.2 * k).exp() * (1.0 - k);
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
            Shape::Wash { dir, reach, spread, chroma, peak } => {
                let d = dx.hypot(dy);
                if d >= reach * WASH_REACH {
                    return;
                }
                let along = dx * dir.0 + dy * dir.1;
                let across = (-dx * dir.1 + dy * dir.0).abs();
                let theta = across.atan2(along);
                // The cone opens to a half-disc near where the light enters, so the edge glows
                // broadly instead of ending in a point.
                let open = spread + std::f32::consts::PI * (-d / (0.45 * reach)).exp();
                let cone = (-(theta / open).powi(2)).exp();
                let fade = (-d / reach).exp();
                // A faint veil all round, the cone carrying most of the light.
                add(l, self.col, fade * (0.05 + 0.95 * cone));
                if chroma > 0.0 && cone > CUTOFF {
                    // Red outside, blue inside: a faint rainbow fringe where the wash fades.
                    let (r0, w) = (reach * 0.9, reach * 0.1);
                    for (k, v) in l.iter_mut().enumerate() {
                        let e = (d - r0 * (1.0 + 0.06 * chroma * (1.0 - k as f32))) / w;
                        *v += peak * 0.18 * chroma * cone * (-e * e).exp();
                    }
                }
            }
        }
    }
}

/// How far behind the source a wash may start (at the frame edge), in reaches.
const WASH_APEX: f32 = 1.0;
/// Reaches after which a wash's light is below [`CUTOFF`].
const WASH_REACH: f32 = 8.5;

/// The canvas diagonal (px) up to which thin details (streak and star cores, hot cores) scale
/// with the flare like everything else: about a preset thumbnail's. Past it they grow more
/// slowly than the canvas ([`DETAIL_SHRINK`]), so a large image keeps the crisp lines and points
/// of the thumbnail instead of a uniformly enlarged, soft copy of it, while glows, halos, ghosts,
/// washes, rays and a streak's soft skirt keep the flare's proportions.
const DETAIL_REF: f32 = 300.0;
/// How much slower than the canvas details grow past [`DETAIL_REF`]: they scale with
/// `diag^(1 − DETAIL_SHRINK)`. At ½ the cores of a 7000 px image read as specks at a fit-to-window
/// zoom; 0.3 keeps them hot points with crisp edges.
const DETAIL_SHRINK: f32 = 0.3;

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

/// An independent `0..1` value drawn from hash `h` for property `k` (rehashed, since XOR-ing a
/// small constant into `h` changes only its low bits and leaves the draws correlated).
fn sub(h: u32, k: u32) -> f32 {
    hash01(hash2(h as i32, k as i32, 0x2c1b))
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
        TextureKind::None | TextureKind::FilmGrain | TextureKind::CoarseGrain | TextureKind::FineGrain => return 0.0,
        TextureKind::GlassArcs => return glass_arcs(x, y),
        TextureKind::HazyGlass => {
            // Low, uneven fog with fine dust caught in it.
            // Octaves on rotated lattices, so the fog shows no grid.
            let (rx, ry) = (0.8 * x - 0.6 * y, 0.6 * x + 0.8 * y);
            let fog = 0.5 * value_noise(x / 150.0, y / 150.0, 31) + 0.3 * value_noise(rx / 60.0, ry / 60.0, 32) + 0.2 * value_noise(ry / 23.0, -rx / 23.0, 36);
            return 0.3 * smoothstep(0.25, 0.85, fog) + 0.6 * dirt(TextureKind::Dust, x, y) * (0.4 + fog);
        }
        TextureKind::BrushedSteel => {
            // Lattice stretched ~300:1 along x: long parallel lines that drift in brightness.
            let lines = 0.6 * value_noise(x / 260.0, y / 0.9, 33) + 0.4 * value_noise(x / 70.0, y / 0.45, 34);
            let sheen = value_noise(x / 300.0, y / 300.0, 35);
            return smoothstep(0.35, 0.95, lines) * (0.15 + 0.3 * sheen);
        }
        TextureKind::Smudge => {
            let n = 0.6 * value_noise(x / 140.0, y / 140.0, 11) + 0.3 * value_noise(x / 55.0, y / 55.0, 12) + 0.1 * value_noise(x / 18.0, y / 18.0, 13);
            return smoothstep(0.5, 0.85, n) * 0.6;
        }
        TextureKind::Dust => (30.0, 0.45, 1.0, 4.0, 21),
        TextureKind::BigSpots => (140.0, 0.55, 25.0, 60.0, 22),
        TextureKind::HexDust => (90.0, 0.5, 14.0, 38.0, 23),
        TextureKind::Raindrops => (70.0, 0.5, 8.0, 26.0, 24),
        TextureKind::Scratches => (200.0, 0.8, 50.0, 95.0, 25),
        TextureKind::SoftOrbs => (45.0, 0.6, 6.0, 20.0, 26),
        // Spots reach 6× their radius sideways, which stays inside one cell.
        TextureKind::StretchedSpecks => (40.0, 0.5, 1.0, 3.0, 27),
        TextureKind::BokehBubbles => (230.0, 0.5, 45.0, 105.0, 28),
    };
    let (cx, cy) = ((x / cell).floor() as i32, (y / cell).floor() as i32);
    let mut best = 0.0f32;
    for j in -1..=1 {
        for i in -1..=1 {
            let h = hash2(cx + i, cy + j, seed);
            if hash01(h) > prob {
                continue;
            }
            let px = (cx + i) as f32 * cell + sub(h, 0x68e3) * cell;
            let py = (cy + j) as f32 * cell + sub(h, 0x2b79) * cell;
            let r = rmin + (rmax - rmin) * sub(h, 0x4cf5);
            let b = 0.35 + 0.65 * sub(h, 0x1d87);
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
                    if d > 1.2 { 0.0 } else { b * (0.18 * (1.0 - smoothstep(0.8, 1.0, d)) + 0.9 * (-((d - 0.88) / 0.1).powi(2)).exp()) }
                }
                TextureKind::Scratches => {
                    let ang = sub(h, 0x7a1c) * std::f32::consts::PI;
                    let (s, c) = ang.sin_cos();
                    let along = (dx * c + dy * s).abs();
                    if along > r {
                        0.0
                    } else {
                        let perp = (-dx * s + dy * c).abs();
                        b * 0.8 * (-(perp / 0.7).powi(2)).exp() * (1.0 - along / r)
                    }
                }
                TextureKind::SoftOrbs => {
                    let d = dx.hypot(dy) / r;
                    b * 0.4 * (1.0 - smoothstep(0.2, 1.0, d))
                }
                TextureKind::StretchedSpecks => {
                    let d = (dx / (r * 6.0)).hypot(dy / r);
                    b * (1.0 - smoothstep(0.3, 1.0, d))
                }
                TextureKind::BokehBubbles => {
                    let d = dx.hypot(dy) / r;
                    if d > 1.1 {
                        0.0
                    } else {
                        // A flat body that brightens to a crisp rim, with a faint inner ring.
                        let body = (1.0 - smoothstep(0.96, 1.02, d)) * (0.22 + 0.18 * d * d);
                        let rim = (-((d - 0.97) / 0.035).powi(2)).exp();
                        let inner = 0.12 * (-((d - 0.7) / 0.05).powi(2)).exp();
                        b * 0.7 * (body + 0.55 * rim + inner)
                    }
                }
                _ => 0.0,
            };
            best = best.max(v);
        }
    }
    best
}

/// Curved streaks: arcs around two far-away centres, broken up along their length, as reflections
/// on a bent sheet of glass.
fn glass_arcs(x: f32, y: f32) -> f32 {
    let mut best = 0.0f32;
    for (cx, cy, seed) in [(-250.0f32, 950.0f32, 41), (1250.0, -420.0, 42)] {
        let (dx, dy) = (x - cx, y - cy);
        let r = dx.hypot(dy);
        // Arc length along the circle, so streaks keep their width whatever the radius.
        let s = dy.atan2(dx) * r;
        let band = value_noise(r / 4.0, s / 160.0, seed);
        let streak = smoothstep(0.7, 0.97, band);
        let patch = smoothstep(0.45, 0.85, value_noise(r / 140.0, s / 220.0, seed + 7));
        best = best.max(0.4 * streak * patch);
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

    /// One row of a large soft glow at 8 bits: the light each pixel adds (`alpha · rgb`).
    fn glow_row(quantum: Option<f32>) -> Vec<f32> {
        let mut f =
            lens_flare::LensFlare::new(FlareCategory::SunFlare, "x", vec![FlareElement::Glow { radius: 250.0, intensity: 0.4, color: [1.0; 3], falloff: 1.0 }]);
        f.source = (0.0, 0.5);
        let c = Rect::new(0, 0, 4000, 2000);
        let row = Rect::new(0, 1000, 4000, 1001);
        render_quantized(&f, row, c, quantum).iter().map(|p| p[0] * p[3]).collect()
    }

    /// The longest run of equal values in `v`: the width of a band.
    fn widest_band(v: &[f32]) -> usize {
        v.chunk_by(|a, b| a == b).map(<[f32]>::len).max().unwrap_or(0)
    }

    #[test]
    fn an_8_bit_glow_is_dithered_not_banded() {
        let smooth = glow_row(None);
        let undithered: Vec<f32> = smooth.iter().map(|v| (v * 255.0).round() / 255.0).collect();
        let dithered = glow_row(Some(255.0));
        // Rounded as is, the falloff reaches 8 bits as one-level bands tens of pixels wide.
        assert!(widest_band(&undithered[400..]) > 40, "{}", widest_band(&undithered[400..]));
        // Dithered, every value is on an 8-bit level, the bands break up, and the light is kept.
        assert!(dithered.iter().all(|v| ((v * 255.0).round() - v * 255.0).abs() < 1e-3));
        // Equal neighbours still happen by chance in noise, but no run is near a band's width.
        assert!(widest_band(&dithered[400..2000]) < 24, "{}", widest_band(&dithered[400..2000]));
        for (a, b) in smooth.chunks(200).zip(dithered.chunks(200)) {
            let mean = |s: &[f32]| s.iter().sum::<f32>() / s.len() as f32;
            assert!((mean(a) - mean(b)).abs() < 0.6 / 255.0, "{} vs {}", mean(a), mean(b));
        }
        // No noise where the flare adds nothing.
        let f =
            lens_flare::LensFlare::new(FlareCategory::SunFlare, "x", vec![FlareElement::Glow { radius: 2.0, intensity: 1.0, color: [1.0; 3], falloff: 2.0 }]);
        assert!(render_quantized(&f, Rect::new(150, 0, 160, 10), canvas(), Some(255.0)).iter().all(|p| *p == [0.0; 4]));
    }

    /// A lone horizontal streak's light down the column through its source, from the centre out.
    fn streak_column(canvas: Rect) -> Vec<f32> {
        let mut f = lens_flare::LensFlare::new(
            FlareCategory::Streak,
            "x",
            vec![FlareElement::Streak { length: 900.0, thickness: 3.0, intensity: 0.9, color: [1.0; 3], angle: 0.0 }],
        );
        f.source = (0.5, 0.5);
        let (cx, cy) = ((canvas.x0 + canvas.x1) / 2, (canvas.y0 + canvas.y1) / 2);
        render(&f, Rect::new(cx, cy, cx + 1, canvas.y1), canvas).iter().map(|p| p[0] * p[3]).collect()
    }

    /// How far from the centre `v` first falls below `frac` of its peak, as a fraction of `of`.
    fn reach(v: &[f32], frac: f32, of: i32) -> f32 {
        let peak = v.first().copied().unwrap_or(0.0);
        v.iter().position(|x| *x < peak * frac).unwrap_or(v.len()) as f32 / of as f32
    }

    #[test]
    fn thin_details_stay_fine_on_large_canvases_and_skirts_fade_out() {
        let (small, big) = (Rect::new(0, 0, 256, 144), Rect::new(0, 0, 7200, 4050));
        let (s, b) = (streak_column(small), streak_column(big));
        // The core (half its peak) is much finer on the large canvas, relative to its height ...
        assert!(reach(&b, 0.5, 4050) < reach(&s, 0.5, 144) / 3.0, "{} vs {}", reach(&b, 0.5, 4050), reach(&s, 0.5, 144));
        // ... while the soft skirt keeps the flare's proportions.
        let (bs, ss) = (reach(&b, 0.05, 4050), reach(&s, 0.05, 144));
        assert!((bs - ss).abs() < ss * 0.35, "skirt {bs} vs {ss}");
        // Past the core the skirt fades to nothing instead of ending at the element's box: it
        // only falls, and never by more than a hundredth of the peak from one pixel to the next.
        let skirt = b.iter().position(|v| *v < b[0] * 0.2).unwrap_or(b.len());
        assert!(b[skirt..].windows(2).all(|w| w[1] <= w[0] + 1e-6 && w[0] - w[1] < b[0] * 0.01), "a step in the skirt");
        assert!(b.last().is_some_and(|v| *v < 1e-4));
        // Thumbnails are unchanged: up to the reference diagonal a detail scales with the flare.
        let ref_side = (DETAIL_REF / 2f32.sqrt()) as i32;
        let t = streak_column(Rect::new(0, 0, ref_side, ref_side));
        let t2 = streak_column(Rect::new(0, 0, ref_side / 2, ref_side / 2));
        assert!((reach(&t, 0.5, ref_side) - reach(&t2, 0.5, ref_side / 2)).abs() < 0.02);
    }

    #[test]
    fn controls_move_and_scale_the_light() {
        let base = lens_flare::preset("Bare Bulb").map(|p| p.flare()).unwrap_or_else(|| lens_flare::LensFlare::new(FlareCategory::SunFlare, "x", vec![]));
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
        let base = lens_flare::preset("Silver Thread").map(|p| p.flare()).unwrap_or_else(|| lens_flare::LensFlare::new(FlareCategory::Streak, "x", vec![]));
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
        for (reach, spread, chroma, angle) in [(f32::NAN, f32::NAN, f32::NAN, f32::NAN), (1e30, -5.0, 1e9, f32::INFINITY), (0.0, 0.0, -1.0, 0.0)] {
            f.elements = vec![FlareElement::Wash { reach, spread, intensity: 1.0, color: [1.0; 3], chroma, angle }];
            for source in [(0.5, 0.5), (-1.0, 2.0), (0.0, 0.0)] {
                f.source = source;
                assert!(render(&f, canvas(), canvas()).iter().flatten().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{reach} {spread} {source:?}");
            }
        }
    }

    fn wash_flare(chroma: f32) -> lens_flare::LensFlare {
        let mut f = lens_flare::LensFlare::new(
            FlareCategory::SunFlare,
            "x",
            vec![FlareElement::Wash { reach: 400.0, spread: 25.0, intensity: 0.6, color: [1.0, 0.8, 0.5], chroma, angle: 0.0 }],
        );
        f.source = (0.1, 0.5);
        f
    }

    #[test]
    fn wash_pours_in_from_the_edge_past_the_source() {
        let px = render(&wash_flare(0.0), canvas(), canvas());
        // The source is at x = 16; the frame centre is to its right.
        let lum = |x: usize, y: usize| at(&px, 160, x, y)[3];
        assert!(lum(1, 45) > lum(60, 45), "brightest where the light enters, at the left edge");
        assert!(lum(60, 45) > lum(16, 1), "the cone points into the frame, not sideways: {} vs {}", lum(60, 45), lum(16, 1));
        assert!(lum(60, 45) > 0.05);
        // Turning it moves the cone: 90° points it down.
        let mut down = wash_flare(0.0);
        down.elements = vec![FlareElement::Wash { reach: 400.0, spread: 25.0, intensity: 0.6, color: [1.0, 0.8, 0.5], chroma: 0.0, angle: 90.0 }];
        let turned = render(&down, canvas(), canvas());
        assert!(at(&turned, 160, 16, 85)[3] > at(&px, 160, 16, 85)[3]);
    }

    #[test]
    fn wash_chroma_adds_a_rainbow_fringe() {
        let plain = render(&wash_flare(0.0), canvas(), canvas());
        let fringed = render(&wash_flare(2.0), canvas(), canvas());
        assert_ne!(plain, fringed);
        // Somewhere along the axis the fringe separates the channels in both directions.
        let row: Vec<[f32; 4]> = (0..160).map(|x| at(&fringed, 160, x, 45)).collect();
        let base: Vec<[f32; 4]> = (0..160).map(|x| at(&plain, 160, x, 45)).collect();
        let redder = row.iter().zip(&base).any(|(f, p)| f[0] * f[3] - p[0] * p[3] > (f[2] * f[3] - p[2] * p[3]) + 0.01);
        let bluer = row.iter().zip(&base).any(|(f, p)| f[2] * f[3] - p[2] * p[3] > (f[0] * f[3] - p[0] * p[3]) + 0.01);
        assert!(redder && bluer, "a red and a blue band");
    }

    #[test]
    fn wash_tiles_match_one_render() {
        let mut f = wash_flare(1.0);
        f.texture.kind = TextureKind::GlassArcs;
        let whole = render(&f, canvas(), canvas());
        let tile = Rect::new(10, 5, 90, 70);
        let part = render(&f, tile, canvas());
        for y in 5..70 {
            for x in 10..90 {
                assert_eq!(part[(y - 5) * 80 + (x - 10)], at(&whole, 160, x, y));
            }
        }
    }

    #[test]
    fn every_texture_renders_deterministically_and_shows() {
        let mut base = lens_flare::preset("Morning Spill").map(|p| p.flare()).unwrap_or_else(|| wash_flare(0.0));
        base.texture.opacity = 1.0;
        let c = Rect::new(0, 0, 320, 180);
        let none = render(&base, c, c);
        let mut seen: Vec<Vec<[f32; 4]>> = Vec::new();
        for kind in TextureKind::ALL.into_iter().filter(|k| *k != TextureKind::None) {
            let mut f = base.clone();
            f.texture.kind = kind;
            let a = render(&f, c, c);
            assert_eq!(a, render(&f, c, c), "{kind:?} is deterministic");
            assert!(a.iter().flatten().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "{kind:?}");
            let changed = a.iter().zip(&none).filter(|(p, q)| (p[3] - q[3]).abs() > 1e-3).count();
            assert!(changed > a.len() / 200, "{kind:?} changes only {changed} pixels");
            assert!(!seen.contains(&a), "{kind:?} looks like another texture");
            seen.push(a);
            // Flipping moves it.
            f.texture.flip_x = true;
            assert_ne!(render(&f, c, c), seen[seen.len() - 1], "{kind:?} flips");
        }
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

    /// Dumps every texture under one flare (`FLARE_DUMP=dir cargo test -p photocraft-compose dump -- --ignored`).
    #[test]
    #[ignore]
    fn dump_textures() {
        let Ok(dir) = std::env::var("FLARE_DUMP") else { return };
        let c = Rect::new(0, 0, 480, 270);
        let Some(p) = photocraft_doc::lens_flare::preset("Morning Spill") else { return };
        for (i, kind) in TextureKind::ALL.into_iter().enumerate() {
            let mut f = p.flare();
            f.source = (0.3, 0.35);
            f.texture.kind = kind;
            f.texture.opacity = 1.0;
            let px = render(&f, c, c);
            let bytes: Vec<u8> = px.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
            std::fs::write(format!("{dir}/tex{i:02}_{}.raw", kind.id()), bytes).unwrap();
        }
    }
}
