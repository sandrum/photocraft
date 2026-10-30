//! Lens Flare fills on the GPU (`fs_flare`) against the CPU renderer
//! (`photocraft_compose::lens_flare`): every preset and every texture at 8 bits, spot checks at
//! 16 and 32 bits, masks, blend modes and the paged compositor. Skips when no GPU adapter exists.
//!
//! The shader repeats the CPU's arithmetic, but GPU `exp`, `atan2`, `sin` and `cos` are not
//! correctly rounded, so a pixel can land one dither level apart (one 8-bit step); a texture
//! lattice coordinate can also fall on the other side of a cell edge, rarely. The checks bound
//! the worst pixel (two steps, though Intel's Vulkan driver stays within one), the mean and the
//! share of pixels more than one step apart.

use photocraft_color::{BlendMode, Color, ColorMode, SampleType};
use photocraft_doc::lens_flare::{self, LensFlare, PRESETS, TextureKind};
use photocraft_doc::{Document, Fill, Layer, LayerContent, LayerMask};
use photocraft_geom::{Rect, Size};

use crate::parity::{Gpu, gpu, shadow};

/// One 8-bit step and a few ulps (as `parity::TOL`).
const STEP: f32 = 1.0 / 255.0 + 4.0 * f32::EPSILON;

/// Premultiplied differences of a render: worst channel, mean over all channels, and how many
/// pixels are more than one 8-bit step apart.
#[derive(Clone, Copy, Debug, Default)]
struct Diff {
    max: f32,
    sum: f64,
    n: u64,
    over_step: u64,
    /// Pixels more than half a step apart (that round to another 8-bit level).
    differ: u64,
    pixels: u64,
}

impl Diff {
    fn add(&mut self, cpu: &[[f32; 4]], gpu: &[[f32; 4]]) {
        assert_eq!(cpu.len(), gpu.len());
        for (c, g) in cpu.iter().zip(gpu) {
            let pc = [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
            let pg = [g[0] * g[3], g[1] * g[3], g[2] * g[3], g[3]];
            let mut worst = 0.0f32;
            for k in 0..4 {
                let d = (pc[k] - pg[k]).abs();
                let d = if d.is_nan() { 9.0 } else { d };
                worst = worst.max(d);
                self.sum += f64::from(d);
                self.n += 1;
            }
            self.max = self.max.max(worst);
            self.over_step += u64::from(worst > STEP);
            self.differ += u64::from(worst > 0.5 / 255.0);
            self.pixels += 1;
        }
    }

    fn mean(&self) -> f64 {
        self.sum / self.n.max(1) as f64
    }

    fn merge(&mut self, o: &Diff) {
        self.max = self.max.max(o.max);
        self.sum += o.sum;
        self.n += o.n;
        self.over_step += o.over_step;
        self.differ += o.differ;
        self.pixels += o.pixels;
    }

    fn report(&self) -> String {
        format!(
            "max {:.3}/255, mean {:.2e}/255, {} of {} pixels over half a step, {} over one",
            self.max * 255.0,
            self.mean() * 255.0,
            self.differ,
            self.pixels,
            self.over_step
        )
    }

    /// Within one step everywhere but a few pixels (at most `rare` per million), none of
    /// which is off by more than `max` steps, and a mean well under a step.
    fn check(&self, what: &str, max_steps: f32, rare_per_million: u64) {
        assert!(self.max <= max_steps * STEP, "{what}: {}", self.report());
        assert!(self.over_step * 1_000_000 <= rare_per_million * self.pixels, "{what}: {}", self.report());
        assert!(self.mean() < 0.01 / 255.0, "{what}: {}", self.report());
    }
}

/// GPU (whole textures and paged) vs CPU over the whole document.
fn diff(g: &mut Gpu, doc: &Document, what: &str) -> Diff {
    let cpu = photocraft_compose::render(doc, doc.bounds());
    let mut out = Diff::default();
    for comp in [&mut g.comp, &mut g.paged] {
        let px = photocraft_gpu::render_to_vec(comp, &g.device, &g.queue, doc, doc.bounds()).unwrap_or_else(|e| panic!("{what}: {e}"));
        out.add(&cpu.px, &px);
    }
    out
}

/// Deterministic pseudo-random values in 0..1.
fn rnd(seed: u32, i: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9e37_79b9) ^ i.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0
}

/// `flare` alone on a transparent document, in Normal: the layer's own pixels.
fn alone(flare: LensFlare, size: Size, depth: SampleType) -> Document {
    let mut d = Document::new("flare", size, ColorMode::Rgb, depth);
    d.layers.push(Layer::new("flare", LayerContent::Fill(Fill::LensFlare(Box::new(flare)))));
    d
}

/// `preset`'s flare with its controls varied from its index `i`: source, rotation, aspect, hue.
fn varied(i: usize, preset: &lens_flare::FlarePreset) -> LensFlare {
    let mut f = preset.flare();
    let r = |k: u32| rnd(i as u32 + 1, k);
    f.source = (-0.1 + 1.2 * r(0), -0.1 + 1.2 * r(1));
    if i % 3 == 1 {
        f.rotation = -180.0 + 360.0 * r(2);
        f.aspect = 0.5 + 1.5 * r(3);
    }
    if i % 4 == 2 {
        f.hue = 360.0 * r(4);
        f.tint = r(5);
        f.scale = 0.5 + 1.5 * r(6);
    }
    f
}

#[test]
fn every_preset_matches_the_cpu_at_8_bits() {
    let Some(mut g) = gpu() else { return };
    let mut all = Diff::default();
    for (i, preset) in PRESETS.iter().enumerate() {
        let d = diff(&mut g, &alone(varied(i, preset), Size::new(384, 216), SampleType::U8), preset.name);
        d.check(preset.name, 2.0, 200);
        all.merge(&d);
    }
    eprintln!("lens flare presets at 8 bits ({}): {}", PRESETS.len(), all.report());
    all.check("all presets", 2.0, 20);
}

#[test]
fn every_texture_matches_the_cpu_at_8_bits() {
    let Some(mut g) = gpu() else { return };
    let base = lens_flare::preset("Morning Spill").map(|p| p.flare()).expect("preset");
    let mut all = Diff::default();
    for (i, kind) in TextureKind::ALL.into_iter().enumerate() {
        let mut f = base.clone();
        f.source = (0.3, 0.35);
        f.texture.kind = kind;
        f.texture.opacity = 1.0;
        f.texture.flip_x = i % 2 == 1;
        f.texture.flip_y = i % 4 >= 2;
        let d = diff(&mut g, &alone(f, Size::new(480, 270), SampleType::U8), kind.id());
        eprintln!("texture {}: {}", kind.id(), d.report());
        d.check(kind.id(), 2.0, 200);
        all.merge(&d);
    }
    eprintln!("lens flare textures at 8 bits ({}): {}", TextureKind::ALL.len(), all.report());
    all.check("all textures", 2.0, 20);
}

/// 16- and 32-bit documents (dithered to 32768 levels, and not at all), a large canvas where thin
/// details shrink, and flares screened over an image under a mask.
#[test]
fn deep_documents_masks_and_large_canvases_match_the_cpu() {
    let Some(mut g) = gpu() else { return };
    let picks = ["Prime Classic", "Morning Spill", "Silver Thread", "Bare Bulb"];
    let mut all = Diff::default();
    for depth in [SampleType::U16, SampleType::F32] {
        for (n, name) in picks.iter().enumerate() {
            let mut f = lens_flare::preset(name).map(|p| p.flare()).expect("preset");
            f.source = (0.35, 0.4);
            f.texture.kind = [TextureKind::HexDust, TextureKind::FilmGrain, TextureKind::GlassArcs, TextureKind::None][n];
            let d = diff(&mut g, &alone(f, Size::new(320, 180), depth), &format!("{name} {depth:?}"));
            d.check(&format!("{name} {depth:?}"), 1.0, 0);
            all.merge(&d);
        }
    }
    eprintln!("lens flare at 16 and 32 bits: {}", all.report());

    // Screened over an image, masked, at a size past the crisp-detail reference.
    let mut d = Document::with_background("big", Size::new(1500, 900), ColorMode::Rgb, SampleType::U8, Color::rgba(0.2, 0.3, 0.4, 1.0));
    for (k, name) in ["Prime Classic", "Silver Thread"].iter().enumerate() {
        let mut f = lens_flare::preset(name).map(|p| p.flare()).expect("preset");
        f.source = (0.3 + 0.4 * k as f32, 0.45);
        let mut l = Layer::new(*name, LayerContent::Fill(Fill::LensFlare(Box::new(f))));
        l.blend = BlendMode::Screen;
        if k == 1 {
            let mut m = LayerMask::reveal_all();
            m.surface.fill_rect(Rect::new(0, 0, 700, 900), &[0.25]);
            l.mask = Some(m);
            l.opacity = 0.8;
        }
        d.layers.push(l);
    }
    let big = diff(&mut g, &d, "screened, masked, large");
    eprintln!("lens flares screened and masked, 1500x900: {}", big.report());
    big.check("screened, masked, large", 2.0, 20);

    // With a layer style: the effects take the flare as their layer.
    let mut f = lens_flare::preset("Bare Bulb").map(|p| p.flare()).expect("preset");
    f.source = (0.4, 0.5);
    let mut d = alone(f, Size::new(400, 240), SampleType::U8);
    d.layers[0].effects.items.push(photocraft_doc::Effect::DropShadow(shadow(BlendMode::Multiply, 0.75, 120.0, 6.0, 8.0, 0.0)));
    let styled = diff(&mut g, &d, "drop shadow");
    eprintln!("lens flare with a drop shadow: {}", styled.report());
    styled.check("drop shadow", 2.0, 200);
}

/// Full renders of a 7259×3780 (27 MP) document with 0, 1, 4 and 8 flares screened over a
/// background, on the GPU (no readback, no reuse of the lower stack) and on the CPU
/// (`cargo test -p photocraft-gpu --release --test it flare_render_times -- --ignored --nocapture`).
#[test]
#[ignore]
fn flare_render_times() {
    let Some(mut g) = gpu() else { return };
    g.comp.set_composite_cache_budget(0);
    let size = Size::new(7259, 3780);
    let names = ["Prime Classic", "Morning Spill", "Silver Thread", "Bare Bulb"];
    for n in [0usize, 1, 4, 8] {
        let mut d = Document::with_background("timing", size, ColorMode::Rgb, SampleType::U8, Color::rgba(0.2, 0.3, 0.4, 1.0));
        for k in 0..n {
            let mut f = lens_flare::preset(names[k % names.len()]).map(|p| p.flare()).expect("preset");
            f.source = (0.1 + 0.8 * rnd(k as u32, 0), 0.1 + 0.8 * rnd(k as u32, 1));
            if k == 1 {
                f.texture.kind = TextureKind::HexDust;
            }
            let mut l = Layer::new("flare", LayerContent::Fill(Fill::LensFlare(Box::new(f))));
            l.blend = BlendMode::Screen;
            d.layers.push(l);
        }
        let gpu_ms = |g: &mut Gpu| {
            let t = std::time::Instant::now();
            g.comp.render(&g.device, &g.queue, &d, d.bounds(), |_, _| {}).expect("composites on the GPU");
            let _ = g.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
            t.elapsed().as_secs_f64() * 1000.0
        };
        let cold = gpu_ms(&mut g);
        let warm = (0..3).map(|_| gpu_ms(&mut g)).fold(f64::INFINITY, f64::min);
        let t = std::time::Instant::now();
        let cpu = photocraft_compose::render(&d, d.bounds());
        let cpu_ms = t.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(cpu.px.len(), 7259 * 3780);
        eprintln!("{n} flare(s), 7259x3780: GPU {warm:.0} ms (first {cold:.0} ms), CPU {cpu_ms:.0} ms");
    }
}
