//! Lens Blur with a depth map, as depth layers.
//!
//! Depth runs from 0 (near) to 1 (far). The window is split into depth layers
//! with tent-shaped membership, so neighbouring layers blend without banding.
//! Each layer is spread by the iris kernel for its own circle of confusion with
//! an FFT convolution (a scatter: every pixel becomes a bokeh shape), and the
//! layers are composited far to near with "over". Blurred foreground therefore
//! spills over sharp background as through a real lens, instead of stopping at
//! a hard edge. The coverage accumulated through the layers is divided out at
//! the end, which fills the holes occluders leave in the layers behind them.
//!
//! The layer centres and kernels depend only on the parameters, never on the
//! window, so results are independent of the tiling.

use photocraft_geom::Rect;
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

/// Circle-of-confusion step between neighbouring layers, in pixels.
const LAYER_STEP: f32 = 2.0;
/// Upper bound on the layer count (a 100 px iris gets 51).
const MAX_LAYERS: usize = 64;
/// Kernel supersampling per axis, for antialiased iris edges.
const SUPERSAMPLE: usize = 4;

/// Layer count for an iris of `rmax` pixels (centres at `k / (count - 1)`).
pub(super) fn layer_count(rmax: f32) -> usize {
    ((rmax.max(0.0) / LAYER_STEP).ceil() as usize).clamp(1, MAX_LAYERS) + 1
}

/// The circle of confusion of layer `k`, in pixels.
fn layer_coc(k: usize, count: usize, focal: f32, rmax: f32) -> f32 {
    rmax * (k as f32 / (count - 1) as f32 - focal).abs()
}

/// Antialiased iris kernel of radius `r` (side `2⌈r⌉ + 1`), normalised to sum 1.
/// `inside(u, v)` takes coordinates in −1…1 with v pointing down.
pub(super) fn iris_kernel(r: f32, inside: &impl Fn(f32, f32) -> bool) -> (Vec<f32>, usize) {
    let half = r.max(0.0).ceil() as usize;
    let side = 2 * half + 1;
    let mut k = vec![0.0f32; side * side];
    let step = 1.0 / SUPERSAMPLE as f32;
    let mut total = 0.0f32;
    for (j, row) in k.chunks_exact_mut(side).enumerate() {
        for (i, v) in row.iter_mut().enumerate() {
            let mut hits = 0u32;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let u = i as f32 - half as f32 - 0.5 + (sx as f32 + 0.5) * step;
                    let w = j as f32 - half as f32 - 0.5 + (sy as f32 + 0.5) * step;
                    if inside(u / r, w / r) {
                        hits += 1;
                    }
                }
            }
            *v = hits as f32;
            total += *v;
        }
    }
    if total <= 0.0 {
        // A degenerate shape: keep the pixel where it is.
        k.fill(0.0);
        k[half * side + half] = 1.0;
    } else {
        k.iter_mut().for_each(|v| *v /= total);
    }
    (k, side)
}

/// Smallest length ≥ `n` whose only prime factors are 2, 3 and 5 (fast for RustFFT).
fn smooth_len(n: usize) -> usize {
    let mut m = n.max(1);
    loop {
        let mut r = m;
        for p in [2, 3, 5] {
            while r.is_multiple_of(p) {
                r /= p;
            }
        }
        if r == 1 {
            return m;
        }
        m += 1;
    }
}

/// A 2-D complex FFT of `w × h` done as row transforms, a transpose and row transforms again.
/// The spectrum is left transposed (`h`-long rows); [`Fft2::inverse`] undoes that.
struct Fft2 {
    w: usize,
    h: usize,
    row: Arc<dyn Fft<f32>>,
    col: Arc<dyn Fft<f32>>,
    row_inv: Arc<dyn Fft<f32>>,
    col_inv: Arc<dyn Fft<f32>>,
    tmp: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl Fft2 {
    fn new(planner: &mut FftPlanner<f32>, w: usize, h: usize) -> Fft2 {
        let row = planner.plan_fft_forward(w);
        let col = planner.plan_fft_forward(h);
        let row_inv = planner.plan_fft_inverse(w);
        let col_inv = planner.plan_fft_inverse(h);
        let scratch_len = [&row, &col, &row_inv, &col_inv].iter().map(|f| f.get_inplace_scratch_len()).max().unwrap_or(0);
        Fft2 { w, h, row, col, row_inv, col_inv, tmp: vec![Complex32::default(); w * h], scratch: vec![Complex32::default(); scratch_len] }
    }

    fn forward(&mut self, data: &mut [Complex32]) {
        for r in data.chunks_exact_mut(self.w) {
            self.row.process_with_scratch(r, &mut self.scratch);
        }
        transpose(data, &mut self.tmp, self.w, self.h);
        for r in self.tmp.chunks_exact_mut(self.h) {
            self.col.process_with_scratch(r, &mut self.scratch);
        }
        data.copy_from_slice(&self.tmp);
    }

    fn inverse(&mut self, data: &mut [Complex32]) {
        for r in data.chunks_exact_mut(self.h) {
            self.col_inv.process_with_scratch(r, &mut self.scratch);
        }
        transpose(data, &mut self.tmp, self.h, self.w);
        for r in self.tmp.chunks_exact_mut(self.w) {
            self.row_inv.process_with_scratch(r, &mut self.scratch);
        }
        let norm = 1.0 / (self.w * self.h) as f32;
        for (d, t) in data.iter_mut().zip(&self.tmp) {
            *d = t * norm;
        }
    }
}

/// `src` is `w × h` row-major; `dst` becomes `h × w`.
fn transpose(src: &[Complex32], dst: &mut [Complex32], w: usize, h: usize) {
    const B: usize = 32;
    for by in (0..h).step_by(B) {
        for bx in (0..w).step_by(B) {
            for y in by..(by + B).min(h) {
                for x in bx..(bx + B).min(w) {
                    dst[x * h + y] = src[y * w + x];
                }
            }
        }
    }
}

/// Inclusive-exclusive bounding box in window pixel coordinates.
#[derive(Clone, Copy)]
struct Bbox {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl Bbox {
    const EMPTY: Bbox = Bbox { x0: usize::MAX, y0: usize::MAX, x1: 0, y1: 0 };

    fn add(&mut self, x: usize, y: usize) {
        self.x0 = self.x0.min(x);
        self.y0 = self.y0.min(y);
        self.x1 = self.x1.max(x + 1);
        self.y1 = self.y1.max(y + 1);
    }

    fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }
}

/// Layered depth-of-field blur of one window.
///
/// `p` is the premultiplied window (`ww × wh × n`), `depth` its depth per pixel (0 near … 1
/// far). Returns the premultiplied result over `out` (which lies inside `win`, at least the
/// largest kernel radius from its edges where the image continues).
#[allow(clippy::too_many_arguments)]
pub(super) fn layered(
    p: &[f32],
    depth: &[f32],
    win: Rect,
    out: Rect,
    n: usize,
    focal: f32,
    rmax: f32,
    inside: &impl Fn(f32, f32) -> bool,
) -> Vec<f32> {
    let (ww, wh) = (win.width() as usize, win.height() as usize);
    let (ow, oh) = (out.width() as usize, out.height() as usize);
    let (ox, oy) = ((out.x0 - win.x0) as usize, (out.y0 - win.y0) as usize);
    let count = layer_count(rmax);
    let scale = (count - 1) as f32;
    let m = n + 1; // channels plus layer coverage

    // The pixels of each layer (window index and weight): a pixel belongs to the two layers
    // around its depth, with tent weights summing to 1.
    let mut boxes = vec![Bbox::EMPTY; count];
    let mut members: Vec<Vec<(u32, f32)>> = vec![Vec::new(); count];
    for y in 0..wh {
        for x in 0..ww {
            let i = y * ww + x;
            let t = depth[i].clamp(0.0, 1.0) * scale;
            let lo = (t.floor() as usize).min(count - 1);
            let f = t - lo as f32;
            if f < 1.0 {
                boxes[lo].add(x, y);
                members[lo].push((i as u32, 1.0 - f));
            }
            if f > 0.0 && lo + 1 < count {
                boxes[lo + 1].add(x, y);
                members[lo + 1].push((i as u32, f));
            }
        }
    }

    let mut acc = vec![0.0f32; ow * oh * m];
    let mut planner = FftPlanner::<f32>::new();
    let mut layer = Vec::new();
    // Far (1) to near (0).
    for k in (0..count).rev() {
        let b = boxes[k];
        if b.is_empty() {
            continue;
        }
        let coc = layer_coc(k, count, focal, rmax);
        // Reach in pixels, plus a grid cell for the interpolation of coarse layers.
        let half = if coc < 0.5 { 0 } else { coc.ceil() as usize + grid_for(coc) };
        // Output pixels this layer can reach.
        let rx0 = b.x0.saturating_sub(half).max(ox);
        let ry0 = b.y0.saturating_sub(half).max(oy);
        let rx1 = (b.x1 + half).min(ox + ow);
        let ry1 = (b.y1 + half).min(oy + oh);
        if rx0 >= rx1 || ry0 >= ry1 {
            continue;
        }
        let (rw, rh) = (rx1 - rx0, ry1 - ry0);
        layer.clear();
        layer.resize(rw * rh * m, 0.0f32);
        if half == 0 {
            for &(i, w) in &members[k] {
                let (x, y) = (i as usize % ww, i as usize / ww);
                if x < rx0 || x >= rx1 || y < ry0 || y >= ry1 {
                    continue;
                }
                let d = ((y - ry0) * rw + (x - rx0)) * m;
                let s = i as usize * n;
                for c in 0..n {
                    layer[d + c] = p[s + c] * w;
                }
                layer[d + n] = w;
            }
        } else {
            spread(p, &members[k], win, n, b, coc, inside, &mut planner, (rx0, ry0, rw, rh), &mut layer);
        }
        // Composite over what is behind.
        for y in 0..rh {
            let arow = ((ry0 - oy + y) * ow + (rx0 - ox)) * m;
            let lrow = y * rw * m;
            for x in 0..rw {
                let a = &mut acc[arow + x * m..arow + x * m + m];
                let l = &layer[lrow + x * m..lrow + x * m + m];
                if l[n] <= 0.0 {
                    continue;
                }
                let keep = 1.0 - l[n].min(1.0);
                for (av, lv) in a.iter_mut().zip(l) {
                    *av = lv + *av * keep;
                }
            }
        }
    }

    let mut res = vec![0.0f32; ow * oh * n];
    for i in 0..ow * oh {
        let a = &acc[i * m..i * m + m];
        let cover = a[n];
        let dst = &mut res[i * n..i * n + n];
        if cover > 1e-6 {
            for c in 0..n {
                dst[c] = a[c] / cover;
            }
        } else {
            // Nothing landed here (can't happen with a full depth map); keep the source pixel.
            let (x, y) = (ox + i % ow, oy + i / ow);
            dst.copy_from_slice(&p[(y * ww + x) * n..(y * ww + x) * n + n]);
        }
    }
    res
}

/// Coarsest grid a layer is blurred on: a pixel cell side of up to this many pixels.
pub(crate) const MAX_GRID: usize = 8;
/// A layer is blurred on a coarser grid only while its iris stays at least this many cells wide
/// in radius, so the bokeh edges stay crisp.
const MIN_GRID_RADIUS: f32 = 6.0;

/// Grid cell side for a layer whose iris is `coc` pixels: big blurs are computed on a coarser
/// grid (a quarter of the work per halving), then interpolated back.
fn grid_for(coc: f32) -> usize {
    let mut s = 1;
    while s * 2 <= MAX_GRID && coc / (2 * s) as f32 >= MIN_GRID_RADIUS {
        s *= 2;
    }
    s
}

/// Convolves one layer (`p × weight` plus the weight itself, for the `members` within `b`) with the iris kernel of
/// radius `coc` and writes the region `(rx0, ry0, rw, rh)` of the result into `layer`.
///
/// The layer is averaged into cells of `grid × grid` pixels aligned to document coordinates
/// (so tiles agree), blurred with the iris scaled down by `grid`, and sampled back bilinearly.
#[allow(clippy::too_many_arguments)]
fn spread(
    p: &[f32],
    members: &[(u32, f32)],
    win: Rect,
    n: usize,
    b: Bbox,
    coc: f32,
    inside: &impl Fn(f32, f32) -> bool,
    planner: &mut FftPlanner<f32>,
    (rx0, ry0, rw, rh): (usize, usize, usize, usize),
    layer: &mut [f32],
) {
    let m = n + 1;
    let ww = win.width() as usize;
    let g = grid_for(coc);
    let gi = g as i64;
    // Cell of a window pixel, in document cell coordinates.
    let cell_x = |x: usize| (i64::from(win.x0) + x as i64).div_euclid(gi);
    let cell_y = |y: usize| (i64::from(win.y0) + y as i64).div_euclid(gi);
    let (cx0, cy0) = (cell_x(b.x0), cell_y(b.y0));
    let (cw, ch) = ((cell_x(b.x1 - 1) - cx0 + 1) as usize, (cell_y(b.y1 - 1) - cy0 + 1) as usize);
    let rc = coc / g as f32;
    let half = rc.ceil() as usize;
    // Linear convolution of the cells with the kernel, plus one cell for interpolation.
    let (fw, fh) = (smooth_len(cw + 2 * half + 2), smooth_len(ch + 2 * half + 2));
    let mut fft = Fft2::new(planner, fw, fh);

    let (kernel, side) = iris_kernel(rc, inside);
    let mut kspec = vec![Complex32::default(); fw * fh];
    for j in 0..side {
        for i in 0..side {
            // Kernel centre at the origin, negative offsets wrapped.
            let x = (i + fw - half) % fw;
            let y = (j + fh - half) % fh;
            kspec[y * fw + x].re = kernel[j * side + i];
        }
    }
    fft.forward(&mut kspec);

    // Bilinear taps (cell index relative to cx0/cy0, wrapped into the transform, and weight)
    // for every output column and row. With g = 1 these land exactly on one cell.
    let taps = |r0: usize, len: usize, origin: i32, c0: i64, f: usize| -> Vec<(usize, usize, f32)> {
        (r0..r0 + len)
            .map(|v| {
                let doc = (i64::from(origin) + v as i64) as f64;
                let pos = (doc - (g as f64 - 1.0) / 2.0) / g as f64 - c0 as f64;
                let i0 = pos.floor();
                let t = (pos - i0) as f32;
                let wrap = |i: f64| (i as i64).rem_euclid(f as i64) as usize;
                (wrap(i0), wrap(i0 + 1.0), t)
            })
            .collect()
    };
    let xt = taps(rx0, rw, win.x0, cx0, fw);
    let yt = taps(ry0, rh, win.y0, cy0, fh);

    // Average the layer into cells, all channels in one pass.
    let cell_area = (g * g) as f32;
    let mut cells = vec![0.0f32; cw * ch * m];
    for &(i, w) in members {
        let (x, y) = (i as usize % ww, i as usize / ww);
        let s = i as usize * n;
        let d = ((cell_y(y) - cy0) as usize * cw + (cell_x(x) - cx0) as usize) * m;
        let wa = w / cell_area;
        for c in 0..n {
            cells[d + c] += p[s + c] * wa;
        }
        cells[d + n] += wa;
    }
    // Two real channels per complex transform: real part channel c, imaginary part c + 1.
    let mut buf = vec![Complex32::default(); fw * fh];
    let mut conv = vec![0.0f32; fw * fh * m];
    for c in (0..m).step_by(2) {
        buf.fill(Complex32::default());
        for y in 0..ch {
            for x in 0..cw {
                let src = &cells[(y * cw + x) * m..];
                let z = &mut buf[y * fw + x];
                z.re = src[c];
                if c + 1 < m {
                    z.im = src[c + 1];
                }
            }
        }
        fft.forward(&mut buf);
        for (z, k) in buf.iter_mut().zip(&kspec) {
            *z *= k;
        }
        fft.inverse(&mut buf);
        for (i, z) in buf.iter().enumerate() {
            conv[i * m + c] = z.re;
            if c + 1 < m {
                conv[i * m + c + 1] = z.im;
            }
        }
    }
    for (y, &(y0, y1, ty)) in yt.iter().enumerate() {
        for (x, &(x0, x1, tx)) in xt.iter().enumerate() {
            let taps = [(y0 * fw + x0, (1.0 - tx) * (1.0 - ty)), (y0 * fw + x1, tx * (1.0 - ty)), (y1 * fw + x0, (1.0 - tx) * ty), (y1 * fw + x1, tx * ty)];
            let d = (y * rw + x) * m;
            let dst = &mut layer[d..d + m];
            for &(i, w) in &taps {
                if w > 0.0 {
                    for (o, v) in dst.iter_mut().zip(&conv[i * m..i * m + m]) {
                        *o += v * w;
                    }
                }
            }
            dst.iter_mut().for_each(|v| *v = v.max(0.0));
        }
    }
}
