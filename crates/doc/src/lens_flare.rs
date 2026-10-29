//! Lens Flare fill layers: an editable, non-destructive flare built from optical elements.
//!
//! A flare is data: its light source position, the global controls a user adjusts until the
//! image is final (scale, brightness, aspect, source and artifact strength, rotation, colour) and
//! the elements its preset is made of (glows, anamorphic streaks, star spikes, fine rays, halo
//! rings and ghosts along the axis through the frame's centre), plus an optional lens-dirt
//! texture that the flare's light reveals. The compositor renders it (`photocraft-compose`
//! `lens_flare`); nothing here draws.
//!
//! Lengths are in flare units: a thousandth of the canvas diagonal, times [`LensFlare::scale`].
//! Colours are RGB in `0..=1` (light added to the image, so black adds nothing).

use serde::{Deserialize, Serialize};

/// The preset library's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlareCategory {
    /// Long horizontal streaks with oval ghosts, as from anamorphic cinema lenses.
    #[default]
    Anamorphic,
    /// Round glows, halos and ghost chains, as from spherical lenses.
    Spherical,
    /// Bright suns with star spikes and fine rays.
    SunFlare,
    /// Thin light streaks and beams, without ghosts.
    Streak,
}

impl FlareCategory {
    pub const ALL: [FlareCategory; 4] = [FlareCategory::Anamorphic, FlareCategory::Spherical, FlareCategory::SunFlare, FlareCategory::Streak];

    pub fn label(self) -> &'static str {
        match self {
            FlareCategory::Anamorphic => "Anamorphic",
            FlareCategory::Spherical => "Spherical",
            FlareCategory::SunFlare => "Sun Flares",
            FlareCategory::Streak => "Streaks",
        }
    }

    /// The id used in command params.
    pub fn id(self) -> &'static str {
        match self {
            FlareCategory::Anamorphic => "anamorphic",
            FlareCategory::Spherical => "spherical",
            FlareCategory::SunFlare => "sunFlare",
            FlareCategory::Streak => "streak",
        }
    }
}

/// A lens-dirt or film texture the flare's light shows up on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextureKind {
    #[default]
    None,
    /// Fine film grain over the whole flare.
    FilmGrain,
    /// Small specks of dust on the front element.
    Dust,
    /// Large soft round spots.
    BigSpots,
    /// Hexagonal spots, as dust seen through a six-blade iris.
    HexDust,
    /// Water drops with bright rims.
    Raindrops,
    /// Thin hairline scratches.
    Scratches,
    /// Broad greasy smudges.
    Smudge,
    /// Heavy, clumpy film grain.
    CoarseGrain,
    /// Pixel-fine film grain.
    FineGrain,
    /// Curved streaky reflections, as in a bent sheet of glass.
    GlassArcs,
    /// Many round soft discs of dust, out of focus.
    SoftOrbs,
    /// A diffuse haze of dust and fog on the glass.
    HazyGlass,
    /// Fine parallel brushing lines, as on a metal plate.
    BrushedSteel,
    /// Dust specks stretched horizontally by an anamorphic lens.
    StretchedSpecks,
    /// Large out-of-focus discs with bright rims.
    BokehBubbles,
}

impl TextureKind {
    pub const ALL: [TextureKind; 16] = [
        TextureKind::None,
        TextureKind::FilmGrain,
        TextureKind::Dust,
        TextureKind::BigSpots,
        TextureKind::HexDust,
        TextureKind::Raindrops,
        TextureKind::Scratches,
        TextureKind::Smudge,
        TextureKind::CoarseGrain,
        TextureKind::FineGrain,
        TextureKind::GlassArcs,
        TextureKind::SoftOrbs,
        TextureKind::HazyGlass,
        TextureKind::BrushedSteel,
        TextureKind::StretchedSpecks,
        TextureKind::BokehBubbles,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TextureKind::None => "None",
            TextureKind::FilmGrain => "Film Grain",
            TextureKind::Dust => "Dust",
            TextureKind::BigSpots => "Large Blots",
            TextureKind::HexDust => "Hex Dust",
            TextureKind::Raindrops => "Water Beads",
            TextureKind::Scratches => "Scratches",
            TextureKind::Smudge => "Smudge",
            TextureKind::CoarseGrain => "Coarse Grain",
            TextureKind::FineGrain => "Fine Grain",
            TextureKind::GlassArcs => "Glass Arcs",
            TextureKind::SoftOrbs => "Soft Orbs",
            TextureKind::HazyGlass => "Hazy Glass",
            TextureKind::BrushedSteel => "Brushed Steel",
            TextureKind::StretchedSpecks => "Stretched Specks",
            TextureKind::BokehBubbles => "Bokeh Bubbles",
        }
    }

    /// The id used in command params.
    pub fn id(self) -> &'static str {
        match self {
            TextureKind::None => "none",
            TextureKind::FilmGrain => "filmGrain",
            TextureKind::Dust => "dust",
            TextureKind::BigSpots => "bigSpots",
            TextureKind::HexDust => "hexDust",
            TextureKind::Raindrops => "raindrops",
            TextureKind::Scratches => "scratches",
            TextureKind::Smudge => "smudge",
            TextureKind::CoarseGrain => "coarseGrain",
            TextureKind::FineGrain => "fineGrain",
            TextureKind::GlassArcs => "glassArcs",
            TextureKind::SoftOrbs => "softOrbs",
            TextureKind::HazyGlass => "hazyGlass",
            TextureKind::BrushedSteel => "brushedSteel",
            TextureKind::StretchedSpecks => "stretchedSpecks",
            TextureKind::BokehBubbles => "bokehBubbles",
        }
    }

    pub fn from_id(id: &str) -> Option<TextureKind> {
        TextureKind::ALL.into_iter().find(|t| t.id().eq_ignore_ascii_case(id) || t.label().eq_ignore_ascii_case(id))
    }
}

/// The texture layer of a flare (the dialog's Textures tab).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlareTexture {
    pub kind: TextureKind,
    /// `0..=1`.
    pub opacity: f32,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
}

impl Default for FlareTexture {
    fn default() -> Self {
        FlareTexture { kind: TextureKind::None, opacity: 0.6, flip_x: false, flip_y: false }
    }
}

/// One optical element of a flare.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FlareElement {
    /// A soft glow at the source. `falloff` 1 = exponential (wide skirt), 2 = Gaussian (tight).
    Glow { radius: f32, intensity: f32, color: [f32; 3], falloff: f32 },
    /// A streak through the source, `length` long either side and `thickness` thick, at `angle`
    /// degrees (0 = horizontal) plus the flare's rotation.
    Streak { length: f32, thickness: f32, intensity: f32, color: [f32; 3], angle: f32 },
    /// `rays` spikes evenly around the source (a diffraction star).
    Star { rays: u32, length: f32, thickness: f32, intensity: f32, color: [f32; 3], angle: f32 },
    /// `count` fine rays of random angle and length up to `length` (sun glints).
    Rays { count: u32, length: f32, intensity: f32, color: [f32; 3], seed: u32 },
    /// A ring of `radius` and `width`, centred on the flare axis at `t` (0 = the source). `chroma`
    /// spreads red and blue apart (a rainbow edge), `arc` keeps only the side facing away from
    /// the source (`0` = the whole ring, `1` = a half ring).
    Ring {
        t: f32,
        radius: f32,
        width: f32,
        intensity: f32,
        color: [f32; 3],
        chroma: f32,
        #[serde(default)]
        arc: f32,
    },
    /// A ghost image of the iris on the axis at `t` (0 = the source, 1 = the frame centre, 2 = the
    /// mirror point). `sides` 0 = round, else a polygon; `rim` brightens its edge; `softness`
    /// blurs it (`0..=1`).
    Ghost { t: f32, radius: f32, intensity: f32, color: [f32; 3], sides: u32, rim: f32, softness: f32, chroma: f32 },
    /// Edge light: a broad soft wash that pours in from the frame edge past the source, as when
    /// the sun sits just outside the frame. It fans out from behind the source towards the frame
    /// centre in a cone `spread` degrees wide (half-angle), fading over `reach`; `chroma` adds a
    /// faint rainbow fringe at the edge of the light, and `angle` turns the wash (degrees) away
    /// from pointing straight at the centre. The flare's rotation turns it too.
    Wash {
        reach: f32,
        spread: f32,
        intensity: f32,
        color: [f32; 3],
        #[serde(default)]
        chroma: f32,
        #[serde(default)]
        angle: f32,
    },
}

impl FlareElement {
    /// Whether the element sits at the light source (scaled by Source) rather than being a lens
    /// artifact along the axis (scaled by Artifacts).
    pub fn at_source(&self) -> bool {
        match self {
            FlareElement::Glow { .. } | FlareElement::Streak { .. } | FlareElement::Star { .. } | FlareElement::Rays { .. } | FlareElement::Wash { .. } => true,
            FlareElement::Ring { t, .. } => t.abs() < 1e-3,
            FlareElement::Ghost { .. } => false,
        }
    }
}

/// Ranges of the user controls (the dialog's sliders and the commands' validation).
pub mod range {
    pub const SCALE: (f32, f32) = (0.1, 5.0);
    pub const BRIGHTNESS: (f32, f32) = (0.0, 3.0);
    pub const ASPECT: (f32, f32) = (0.25, 4.0);
    pub const SOURCE: (f32, f32) = (0.0, 3.0);
    pub const ARTIFACTS: (f32, f32) = (0.0, 3.0);
    pub const ROTATION: (f32, f32) = (-180.0, 180.0);
    pub const HUE: (f32, f32) = (0.0, 360.0);
    pub const TINT: (f32, f32) = (0.0, 1.0);
    /// Position, as a fraction of the canvas; a source may sit outside the frame.
    pub const POSITION: (f32, f32) = (-1.0, 2.0);
    /// Most elements a flare may hold (bounds per-pixel work for hand-written files).
    pub const MAX_ELEMENTS: usize = 64;
    /// Most rays a [`super::FlareElement::Star`] or [`super::FlareElement::Rays`] may have.
    pub const MAX_RAYS: u32 = 256;
}

/// A lens flare fill: the preset it came from, the user's controls and its elements.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LensFlare {
    pub category: FlareCategory,
    /// The preset's name (shown in the dialog; the elements are stored, so an unknown name
    /// still renders).
    pub preset: String,
    /// Light source, as a fraction of the canvas (`(0.5, 0.5)` = its centre).
    pub source: (f32, f32),
    /// Size multiplier.
    pub scale: f32,
    /// Overall light multiplier.
    pub brightness: f32,
    /// Horizontal stretch of the source's glows (`1` = round).
    pub aspect: f32,
    /// Strength of the elements at the source (glow, streaks, star, rays).
    pub source_gain: f32,
    /// Strength of the lens artifacts (ghosts and off-source rings).
    pub artifacts: f32,
    /// Rotation of streaks and stars, in degrees.
    pub rotation: f32,
    /// Colour wheel: hue in degrees and how far the flare's colours move towards it (`0` = the
    /// preset's own colours).
    pub hue: f32,
    pub tint: f32,
    #[serde(default)]
    pub texture: FlareTexture,
    pub elements: Vec<FlareElement>,
}

impl LensFlare {
    /// A flare with the default controls around `elements`.
    pub fn new(category: FlareCategory, preset: impl Into<String>, elements: Vec<FlareElement>) -> LensFlare {
        LensFlare {
            category,
            preset: preset.into(),
            source: (0.3, 0.3),
            scale: 1.0,
            brightness: 1.0,
            aspect: 1.0,
            source_gain: 1.0,
            artifacts: 1.0,
            rotation: 0.0,
            hue: 0.0,
            tint: 0.0,
            texture: FlareTexture::default(),
            elements,
        }
    }

    /// The flare with every control clamped into its range and non-finite values replaced by the
    /// defaults, so hand-written or corrupt files render safely.
    pub fn sanitized(mut self) -> LensFlare {
        fn fix(v: f32, (lo, hi): (f32, f32), d: f32) -> f32 {
            if v.is_finite() { v.clamp(lo, hi) } else { d }
        }
        self.source = (fix(self.source.0, range::POSITION, 0.5), fix(self.source.1, range::POSITION, 0.5));
        self.scale = fix(self.scale, range::SCALE, 1.0);
        self.brightness = fix(self.brightness, range::BRIGHTNESS, 1.0);
        self.aspect = fix(self.aspect, range::ASPECT, 1.0);
        self.source_gain = fix(self.source_gain, range::SOURCE, 1.0);
        self.artifacts = fix(self.artifacts, range::ARTIFACTS, 1.0);
        self.rotation = fix(self.rotation, range::ROTATION, 0.0);
        self.hue = fix(self.hue.rem_euclid(360.0), range::HUE, 0.0);
        self.tint = fix(self.tint, range::TINT, 0.0);
        self.texture.opacity = fix(self.texture.opacity, (0.0, 1.0), 0.6);
        self.elements.truncate(range::MAX_ELEMENTS);
        self
    }
}

/// A named preset of the library.
#[derive(Clone, Copy, Debug)]
pub struct FlarePreset {
    pub category: FlareCategory,
    pub name: &'static str,
    build: fn() -> Vec<FlareElement>,
}

impl FlarePreset {
    pub fn elements(&self) -> Vec<FlareElement> {
        (self.build)()
    }

    /// A flare of this preset with the default controls.
    pub fn flare(&self) -> LensFlare {
        LensFlare::new(self.category, self.name, self.elements())
    }
}

/// The preset `name` (case-insensitive), if the library has it.
pub fn preset(name: &str) -> Option<FlarePreset> {
    PRESETS.iter().copied().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// The presets of `category`, in the library's order.
pub fn presets(category: FlareCategory) -> impl Iterator<Item = FlarePreset> {
    PRESETS.iter().copied().filter(move |p| p.category == category)
}

// ---- Element helpers (presets read as a list of optics) ----

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

fn glow(radius: f32, intensity: f32, color: [f32; 3]) -> FlareElement {
    FlareElement::Glow { radius, intensity, color, falloff: 1.0 }
}
fn core(radius: f32, intensity: f32, color: [f32; 3]) -> FlareElement {
    FlareElement::Glow { radius, intensity, color, falloff: 2.0 }
}
fn streak(length: f32, thickness: f32, intensity: f32, color: [f32; 3]) -> FlareElement {
    FlareElement::Streak { length, thickness, intensity, color, angle: 0.0 }
}
fn star(rays: u32, length: f32, thickness: f32, intensity: f32, color: [f32; 3], angle: f32) -> FlareElement {
    FlareElement::Star { rays, length, thickness, intensity, color, angle }
}
fn rays(count: u32, length: f32, intensity: f32, color: [f32; 3], seed: u32) -> FlareElement {
    FlareElement::Rays { count, length, intensity, color, seed }
}
fn halo(radius: f32, width: f32, intensity: f32, color: [f32; 3], chroma: f32) -> FlareElement {
    FlareElement::Ring { t: 0.0, radius, width, intensity, color, chroma, arc: 0.0 }
}
fn arc(t: f32, radius: f32, width: f32, intensity: f32, color: [f32; 3], chroma: f32) -> FlareElement {
    FlareElement::Ring { t, radius, width, intensity, color, chroma, arc: 1.0 }
}
fn ghost(t: f32, radius: f32, intensity: f32, color: [f32; 3]) -> FlareElement {
    FlareElement::Ghost { t, radius, intensity, color, sides: 0, rim: 0.4, softness: 0.25, chroma: 0.0 }
}
fn soft_ghost(t: f32, radius: f32, intensity: f32, color: [f32; 3]) -> FlareElement {
    FlareElement::Ghost { t, radius, intensity, color, sides: 0, rim: 0.0, softness: 0.9, chroma: 0.0 }
}
fn hex(t: f32, radius: f32, intensity: f32, color: [f32; 3]) -> FlareElement {
    FlareElement::Ghost { t, radius, intensity, color, sides: 6, rim: 0.5, softness: 0.15, chroma: 0.3 }
}
fn ring_ghost(t: f32, radius: f32, intensity: f32, color: [f32; 3]) -> FlareElement {
    FlareElement::Ghost { t, radius, intensity, color, sides: 0, rim: 1.0, softness: 0.3, chroma: 0.6 }
}

/// A chain of `n` small ghosts between `t0` and `t1`, sizes and tints alternating.
fn chain(n: u32, t0: f32, t1: f32, radius: f32, intensity: f32, a: [f32; 3], b: [f32; 3]) -> Vec<FlareElement> {
    (0..n)
        .map(|i| {
            let k = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
            let r = radius * (0.55 + 0.9 * ((i * 7 % 5) as f32 / 4.0));
            ghost(t0 + (t1 - t0) * k, r, intensity * (0.85 + 0.55 * ((i * 3 % 4) as f32 / 3.0)), if i % 2 == 0 { a } else { b })
        })
        .collect()
}

/// Anamorphic ghosts along the axis in the streak's colour (the streak carries the anamorphic
/// look; the ghosts are soft discs and one faint ring).
fn oval_ghosts(color: [f32; 3]) -> Vec<FlareElement> {
    vec![
        soft_ghost(0.45, 22.0, 0.16, color),
        ghost(0.8, 9.0, 0.25, color),
        soft_ghost(1.25, 48.0, 0.1, color),
        ghost(1.6, 14.0, 0.2, color),
        ring_ghost(1.95, 70.0, 0.09, color),
    ]
}

fn with(mut v: Vec<FlareElement>, more: Vec<FlareElement>) -> Vec<FlareElement> {
    v.extend(more);
    v
}

// ---- The library (original presets) ----

const BLUE: [f32; 3] = [0.35, 0.6, 1.0];
const DEEP_BLUE: [f32; 3] = [0.15, 0.3, 1.0];
const CYAN: [f32; 3] = [0.3, 0.9, 1.0];
const GOLD: [f32; 3] = [1.0, 0.7, 0.3];
const ORANGE: [f32; 3] = [1.0, 0.5, 0.15];
const RED: [f32; 3] = [1.0, 0.18, 0.12];
const VIOLET: [f32; 3] = [0.65, 0.35, 1.0];
const MAGENTA: [f32; 3] = [1.0, 0.25, 0.75];
const GREEN: [f32; 3] = [0.35, 1.0, 0.45];
const WARM: [f32; 3] = [1.0, 0.92, 0.8];
const COOL: [f32; 3] = [0.85, 0.92, 1.0];

macro_rules! p {
    ($cat:ident, $name:literal, $body:expr) => {
        FlarePreset { category: FlareCategory::$cat, name: $name, build: || $body }
    };
}

pub static PRESETS: &[FlarePreset] = &[
    // Anamorphic
    p!(Anamorphic, "Blue Horizon", with(vec![core(10.0, 1.0, WHITE), glow(60.0, 0.45, BLUE), streak(900.0, 3.0, 0.9, BLUE), streak(400.0, 10.0, 0.25, DEEP_BLUE)], oval_ghosts(BLUE))),
    p!(Anamorphic, "Gold Line", with(vec![core(9.0, 1.0, WARM), glow(55.0, 0.4, GOLD), streak(800.0, 2.5, 0.85, GOLD), streak(300.0, 9.0, 0.2, ORANGE)], oval_ghosts(GOLD))),
    p!(Anamorphic, "Cyan Beam", vec![core(8.0, 1.0, WHITE), glow(40.0, 0.35, CYAN), streak(1100.0, 2.0, 1.0, CYAN), streak(1100.0, 14.0, 0.12, CYAN), ghost(1.3, 30.0, 0.06, CYAN)]),
    p!(Anamorphic, "Violet Edge", with(vec![core(10.0, 1.0, WHITE), glow(70.0, 0.4, VIOLET), streak(850.0, 3.0, 0.8, VIOLET), streak(350.0, 12.0, 0.2, MAGENTA)], oval_ghosts(VIOLET))),
    p!(Anamorphic, "Crimson Bar", vec![core(9.0, 1.0, [1.0, 0.8, 0.7]), glow(60.0, 0.5, RED), streak(900.0, 3.0, 0.9, RED), streak(250.0, 18.0, 0.25, RED), soft_ghost(0.9, 60.0, 0.06, RED), soft_ghost(1.7, 30.0, 0.08, ORANGE)]),
    p!(Anamorphic, "Ice Bar", vec![core(7.0, 1.0, WHITE), glow(35.0, 0.3, COOL), streak(1000.0, 1.5, 1.0, COOL), streak(500.0, 5.0, 0.3, BLUE), ghost(0.6, 6.0, 0.15, COOL), ghost(1.4, 10.0, 0.1, BLUE)]),
    p!(Anamorphic, "Ember Strip", with(vec![core(11.0, 1.0, WARM), glow(80.0, 0.45, ORANGE), streak(700.0, 4.0, 0.75, ORANGE), rays(24, 90.0, 0.15, GOLD, 7)], oval_ghosts(ORANGE))),
    p!(Anamorphic, "Neon Night", vec![core(8.0, 1.0, WHITE), glow(50.0, 0.35, MAGENTA), streak(950.0, 2.5, 0.9, MAGENTA), streak(950.0, 2.0, 0.5, BLUE), ghost(0.7, 12.0, 0.12, BLUE), soft_ghost(1.5, 50.0, 0.06, MAGENTA)]),
    p!(Anamorphic, "Twin Line", vec![core(9.0, 1.0, WHITE), glow(45.0, 0.35, BLUE), streak(900.0, 2.0, 0.8, BLUE), FlareElement::Streak { length: 600.0, thickness: 1.5, intensity: 0.35, color: CYAN, angle: 90.0 }, ghost(1.2, 16.0, 0.1, BLUE)]),
    p!(Anamorphic, "Electric Blue", with(vec![core(12.0, 1.1, WHITE), glow(90.0, 0.5, DEEP_BLUE), streak(1200.0, 4.0, 1.0, BLUE), streak(1200.0, 20.0, 0.12, DEEP_BLUE), halo(110.0, 6.0, 0.1, BLUE, 0.4)], oval_ghosts(DEEP_BLUE))),
    // Spherical
    p!(Spherical, "Prime Classic", with(vec![core(12.0, 1.0, WARM), glow(90.0, 0.35, WARM), halo(140.0, 8.0, 0.14, WHITE, 0.6), star(6, 120.0, 1.2, 0.25, WARM, 15.0)], chain(6, 0.3, 1.9, 18.0, 0.12, [0.6, 0.85, 1.0], [1.0, 0.75, 0.5]))),
    p!(Spherical, "Six Blades", with(vec![core(10.0, 1.0, WHITE), glow(70.0, 0.3, COOL)], vec![hex(0.35, 14.0, 0.14, CYAN), hex(0.7, 26.0, 0.1, GREEN), hex(1.15, 40.0, 0.08, BLUE), hex(1.5, 18.0, 0.12, VIOLET), hex(1.9, 60.0, 0.06, CYAN)])),
    p!(Spherical, "Prism Halo", vec![core(9.0, 1.0, WHITE), glow(60.0, 0.3, WHITE), halo(180.0, 10.0, 0.25, WHITE, 1.0), ghost(1.4, 12.0, 0.1, VIOLET)]),
    p!(Spherical, "Long Glass", with(vec![core(14.0, 1.0, WHITE), glow(110.0, 0.45, BLUE)], vec![soft_ghost(0.5, 70.0, 0.06, BLUE), soft_ghost(1.2, 110.0, 0.05, DEEP_BLUE), ghost(1.6, 20.0, 0.1, CYAN)])),
    p!(Spherical, "Emerald Point", vec![core(6.0, 1.2, [0.7, 1.0, 0.7]), glow(40.0, 0.4, GREEN), star(4, 200.0, 1.0, 0.4, GREEN, 0.0), ghost(0.6, 8.0, 0.18, GREEN), ghost(1.1, 14.0, 0.12, GREEN), ghost(1.7, 5.0, 0.2, GREEN)]),
    p!(Spherical, "Warm Desert", with(vec![core(14.0, 1.0, WARM), glow(130.0, 0.4, GOLD), rays(40, 160.0, 0.12, GOLD, 3)], chain(5, 0.4, 1.8, 22.0, 0.08, GOLD, ORANGE))),
    p!(Spherical, "Orbital", with(vec![core(10.0, 1.0, WHITE), glow(80.0, 0.35, BLUE), halo(90.0, 5.0, 0.12, BLUE, 0.5)], vec![hex(0.8, 50.0, 0.06, BLUE), ring_ghost(1.3, 80.0, 0.08, WHITE), ghost(1.7, 10.0, 0.15, CYAN)])),
    p!(Spherical, "Ruby Point", vec![core(6.0, 1.2, [1.0, 0.7, 0.6]), glow(45.0, 0.45, RED), star(4, 220.0, 1.0, 0.35, RED, 0.0), ghost(0.5, 6.0, 0.2, RED), ghost(1.0, 18.0, 0.1, MAGENTA), ghost(1.6, 9.0, 0.16, ORANGE)]),
    p!(Spherical, "Halo Wide", vec![core(10.0, 1.0, WHITE), glow(80.0, 0.35, COOL), halo(260.0, 30.0, 0.08, COOL, 0.8), arc(1.0, 300.0, 14.0, 0.07, WHITE, 1.0)]),
    p!(Spherical, "Eight Spoke", vec![core(9.0, 1.1, WHITE), glow(60.0, 0.35, BLUE), star(8, 260.0, 1.0, 0.4, WHITE, 22.5), star(8, 120.0, 0.8, 0.2, CYAN, 0.0), ghost(1.3, 16.0, 0.1, BLUE)]),
    // Sun flares
    p!(SunFlare, "Sun Burst", vec![core(18.0, 1.1, WARM), glow(160.0, 0.55, WARM), star(6, 300.0, 2.0, 0.45, WARM, 0.0), rays(60, 260.0, 0.18, WARM, 11), halo(200.0, 25.0, 0.06, GOLD, 0.6)]),
    p!(SunFlare, "Sun Glint", vec![core(12.0, 1.0, WHITE), glow(90.0, 0.4, COOL), star(4, 260.0, 1.5, 0.5, WHITE, 45.0), star(4, 160.0, 1.0, 0.3, WHITE, 0.0)]),
    p!(SunFlare, "Sun Star", vec![core(14.0, 1.0, WHITE), glow(120.0, 0.45, WARM), star(12, 280.0, 1.4, 0.35, WARM, 0.0), rays(30, 200.0, 0.12, WHITE, 5)]),
    p!(SunFlare, "Needle Burst", vec![core(8.0, 1.0, WHITE), glow(60.0, 0.3, WHITE), rays(120, 320.0, 0.3, WHITE, 21)]),
    p!(SunFlare, "Afternoon", with(vec![core(20.0, 1.0, WARM), glow(220.0, 0.6, GOLD), rays(40, 280.0, 0.12, GOLD, 9)], vec![soft_ghost(0.6, 90.0, 0.05, ORANGE), arc(1.0, 420.0, 18.0, 0.05, GOLD, 1.0)])),
    p!(SunFlare, "Pink Sun", vec![core(16.0, 1.0, [1.0, 0.85, 0.9]), glow(150.0, 0.5, MAGENTA), star(6, 240.0, 1.8, 0.35, [1.0, 0.7, 0.85], 10.0), rays(50, 220.0, 0.12, [1.0, 0.7, 0.85], 13)]),
    p!(SunFlare, "Bare Bulb", vec![core(10.0, 1.2, WHITE), glow(70.0, 0.5, WHITE)]),
    p!(SunFlare, "Pinprick", vec![core(6.0, 1.0, WHITE), glow(30.0, 0.35, WARM)]),
    p!(SunFlare, "Amber Arc", vec![core(16.0, 1.0, WARM), glow(140.0, 0.5, GOLD), arc(1.2, 380.0, 16.0, 0.12, ORANGE, 0.8)]),
    p!(SunFlare, "Rainbow Arc", vec![core(14.0, 1.0, WHITE), glow(120.0, 0.45, WARM), arc(1.0, 350.0, 22.0, 0.14, WHITE, 1.0), star(6, 200.0, 1.4, 0.25, WHITE, 30.0)]),
    // Streaks
    p!(Streak, "Silver Thread", vec![core(5.0, 1.0, WHITE), streak(1000.0, 1.5, 0.9, WHITE)]),
    p!(Streak, "Blue Line", vec![core(5.0, 1.0, WHITE), streak(1000.0, 1.5, 0.9, BLUE), streak(1000.0, 8.0, 0.15, BLUE)]),
    p!(Streak, "Gold Thread", vec![core(5.0, 1.0, WARM), streak(900.0, 1.2, 0.9, GOLD)]),
    p!(Streak, "Red Thread", vec![core(5.0, 1.0, [1.0, 0.8, 0.8]), streak(900.0, 1.5, 0.9, RED), streak(300.0, 10.0, 0.2, RED)]),
    p!(Streak, "Cross Beam", vec![core(6.0, 1.0, WHITE), streak(900.0, 1.5, 0.8, COOL), FlareElement::Streak { length: 500.0, thickness: 1.2, intensity: 0.5, color: COOL, angle: 90.0 }]),
    p!(Streak, "Wide Beam", vec![core(8.0, 1.0, WHITE), glow(40.0, 0.3, CYAN), streak(1200.0, 25.0, 0.25, CYAN), streak(1200.0, 2.0, 0.8, WHITE)]),
    p!(Streak, "Violet Slash", vec![core(5.0, 1.0, WHITE), FlareElement::Streak { length: 900.0, thickness: 2.0, intensity: 0.85, color: VIOLET, angle: -12.0 }]),
    p!(Streak, "Short Spark", vec![core(6.0, 1.1, WHITE), streak(220.0, 2.0, 0.9, WARM), glow(25.0, 0.4, ORANGE)]),
    p!(Streak, "Laser Bar", vec![core(4.0, 1.2, WHITE), streak(1400.0, 1.0, 1.0, GREEN), streak(1400.0, 6.0, 0.2, GREEN)]),
    p!(Streak, "Plasma", vec![core(7.0, 1.1, WHITE), streak(800.0, 3.0, 0.8, MAGENTA), streak(500.0, 2.0, 0.6, BLUE), glow(50.0, 0.3, VIOLET)]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_has_presets_and_names_are_unique() {
        for c in FlareCategory::ALL {
            assert!(presets(c).count() >= 8, "{c:?}");
        }
        let mut names: Vec<_> = PRESETS.iter().map(|p| p.name.to_lowercase()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), PRESETS.len());
        for p in PRESETS {
            assert!(!p.elements().is_empty() && p.elements().len() <= range::MAX_ELEMENTS, "{}", p.name);
        }
    }

    #[test]
    fn flare_round_trips_through_json() {
        let mut f = preset("blue horizon").map(|p| p.flare()).unwrap_or_else(|| LensFlare::new(FlareCategory::Streak, "x", vec![]));
        f.texture.kind = TextureKind::HexDust;
        let back: LensFlare = serde_json::from_value(serde_json::to_value(&f).unwrap()).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn wash_round_trips_and_older_json_still_loads() {
        let mut f = preset("forked light").map(|p| p.flare()).unwrap_or_else(|| LensFlare::new(FlareCategory::SunFlare, "x", vec![]));
        f.elements.push(FlareElement::Wash { reach: 300.0, spread: 40.0, intensity: 0.5, color: [1.0, 0.5, 0.2], chroma: 1.2, angle: -15.0 });
        let v = serde_json::to_value(&f).unwrap();
        assert!(v["elements"].as_array().unwrap().iter().any(|e| e["kind"] == "wash" && e["reach"] == 300.0));
        assert_eq!(serde_json::from_value::<LensFlare>(v).unwrap(), f);
        // A wash written without the optional fields.
        let w: FlareElement = serde_json::from_str(r#"{"kind":"wash","reach":200,"spread":30,"intensity":0.4,"color":[1,1,1]}"#).unwrap();
        assert_eq!(w, FlareElement::Wash { reach: 200.0, spread: 30.0, intensity: 0.4, color: [1.0; 3], chroma: 0.0, angle: 0.0 });
        assert!(w.at_source());
        // A flare saved before washes and the newer textures existed.
        let old = r#"{"category":"spherical","preset":"Prime Classic","source":[0.3,0.3],"scale":1.0,"brightness":1.0,
            "aspect":1.0,"sourceGain":1.0,"artifacts":1.0,"rotation":0.0,"hue":0.0,"tint":0.0,
            "texture":{"kind":"hexDust","opacity":0.6},
            "elements":[{"kind":"glow","radius":10,"intensity":1,"color":[1,1,1],"falloff":2},
                        {"kind":"ring","t":0,"radius":90,"width":5,"intensity":0.1,"color":[1,1,1],"chroma":0.5},
                        {"kind":"ghost","t":1.2,"radius":20,"intensity":0.1,"color":[1,1,1],"sides":6,"rim":0.5,"softness":0.2,"chroma":0.3}]}"#;
        let f: LensFlare = serde_json::from_str(old).unwrap();
        assert_eq!((f.elements.len(), f.texture.kind), (3, TextureKind::HexDust));
    }

    #[test]
    fn texture_ids_are_their_serde_names_and_labels_are_unique() {
        let mut labels = std::collections::HashSet::new();
        for t in TextureKind::ALL {
            assert_eq!(serde_json::to_value(t).unwrap(), t.id(), "{t:?}");
            assert_eq!(TextureKind::from_id(t.id()), Some(t));
            assert_eq!(TextureKind::from_id(t.label()), Some(t));
            assert!(labels.insert(t.label()), "{t:?}");
        }
        assert_eq!(TextureKind::ALL.len(), 16);
    }

    #[test]
    fn sanitized_clamps_hostile_values() {
        let mut f = LensFlare::new(FlareCategory::Anamorphic, "x", vec![]);
        f.scale = f32::NAN;
        f.brightness = 1e9;
        f.source = (f32::INFINITY, -50.0);
        f.hue = -30.0;
        let f = f.sanitized();
        assert_eq!(f.scale, 1.0);
        assert_eq!(f.brightness, range::BRIGHTNESS.1);
        assert_eq!(f.source, (0.5, range::POSITION.0));
        assert_eq!(f.hue, 330.0);
    }
}
