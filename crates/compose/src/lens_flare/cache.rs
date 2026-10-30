//! A memory-bounded cache of rendered flare light, in [`TILE`]² tiles on the canvas grid.
//!
//! A flare is evaluated per pixel from its elements, which costs far more than reading pixels:
//! without a cache every recomposite (a brush stroke on another layer, a pan, an opacity change)
//! evaluated every flare over the whole damaged area again. Tiles are keyed by the flare's
//! settings, the canvas, the depth's levels and the tile's position, so an edit to the flare
//! simply misses and stale tiles age out. Light is stored at the depth's own precision (three
//! bytes a pixel at 8 bits), which round-trips the quantized light exactly: cached and uncached
//! renders are bit-identical.

use super::{LensFlare, Rect, light, to_rgba};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

/// Cache tile side, the compositor's render tile (whose grid render requests follow).
const TILE: i32 = crate::RENDER_TILE;

/// Requests smaller than this many pixels (a dialog thumbnail, a colour probe) are rendered
/// directly rather than as whole tiles.
const SMALL: u64 = 4096;

/// Default budget: 2 GiB (a quarter of the default Memory Usage preference), about 24
/// full-canvas 8-bit flares on a 27-megapixel image. Over budget, a redraw that cycles through
/// more tiles than fit evicts each before it is reused, so the budget should hold every flare
/// of the documents being edited.
pub const DEFAULT_BUDGET: usize = 2 << 30;

/// One tile's light at the depth's precision.
enum Light {
    Dark,
    U8(Vec<[u8; 3]>),
    U16(Vec<[u16; 3]>),
    F32(Vec<[f32; 3]>),
}

impl Light {
    fn pack(l: Option<Vec<[f32; 3]>>, quantum: Option<f32>) -> Light {
        let Some(l) = l else { return Light::Dark };
        let level = |v: f32, q: f32| (v * q).round();
        match quantum {
            Some(q) if q == 255.0 => Light::U8(l.iter().map(|p| p.map(|v| level(v, q) as u8)).collect()),
            Some(q) if q == 32768.0 => Light::U16(l.iter().map(|p| p.map(|v| level(v, q) as u16)).collect()),
            _ => Light::F32(l),
        }
    }

    fn bytes(&self) -> usize {
        64 + match self {
            Light::Dark => 0,
            Light::U8(v) => v.len() * 3,
            Light::U16(v) => v.len() * 6,
            Light::F32(v) => v.len() * 12,
        }
    }

    /// Pixel `i` as straight RGBA; `q` is the quantum the light was packed with.
    fn rgba(&self, i: usize, q: f32) -> [f32; 4] {
        let l = match self {
            Light::Dark => return [0.0; 4],
            Light::U8(v) => v.get(i).map(|p| p.map(|k| f32::from(k) / q)),
            Light::U16(v) => v.get(i).map(|p| p.map(|k| f32::from(k) / q)),
            Light::F32(v) => v.get(i).copied(),
        };
        l.map_or([0.0; 4], to_rgba)
    }
}

struct Entry {
    light: Arc<Light>,
    used: u64,
}

struct Cache {
    map: HashMap<u64, Entry>,
    bytes: usize,
    budget: usize,
    clock: u64,
}

impl Cache {
    fn get(&mut self, key: u64) -> Option<Arc<Light>> {
        self.clock += 1;
        let now = self.clock;
        self.map.get_mut(&key).map(|e| {
            e.used = now;
            e.light.clone()
        })
    }

    fn insert(&mut self, key: u64, light: Arc<Light>) {
        self.clock += 1;
        let bytes = light.bytes();
        if let Some(old) = self.map.insert(key, Entry { light, used: self.clock }) {
            self.bytes = self.bytes.saturating_sub(old.light.bytes());
        }
        self.bytes += bytes;
        if self.bytes > self.budget {
            self.evict(self.budget - self.budget / 8);
        }
    }

    /// Drop the least recently used tiles until at most `target` bytes remain.
    fn evict(&mut self, target: usize) {
        let mut ages: Vec<(u64, u64)> = self.map.iter().map(|(k, e)| (e.used, *k)).collect();
        ages.sort_unstable();
        for (_, k) in ages {
            if self.bytes <= target {
                break;
            }
            if let Some(e) = self.map.remove(&k) {
                self.bytes = self.bytes.saturating_sub(e.light.bytes());
            }
        }
    }
}

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Cache { map: HashMap::new(), bytes: 0, budget: DEFAULT_BUDGET, clock: 0 }))
}

fn lock() -> std::sync::MutexGuard<'static, Cache> {
    cache().lock().unwrap_or_else(|e| e.into_inner())
}

/// Set the cache's memory budget in bytes (at least 1 MiB); evicts down to it at once.
pub fn set_cache_budget(bytes: usize) {
    let mut c = lock();
    c.budget = bytes.max(1 << 20);
    let budget = c.budget;
    c.evict(budget);
}

/// Bytes the cache holds.
pub fn cache_bytes() -> usize {
    lock().bytes
}

/// Drop every cached tile (Edit › Purge › All). Returns the bytes released.
pub fn purge_cache() -> usize {
    let mut c = lock();
    let freed = c.bytes;
    c.map.clear();
    c.bytes = 0;
    freed
}

/// [`super::render_quantized`] through the cache.
pub(super) fn render(flare: &LensFlare, rect: Rect, canvas: Rect, quantum: Option<f32>) -> Vec<[f32; 4]> {
    let (w, h) = (rect.width() as usize, rect.height() as usize);
    if (w as u64) * (h as u64) < SMALL || !canvas.contains_rect(&rect) {
        return super::render_uncached(flare, rect, canvas, quantum);
    }
    let mut base = std::collections::hash_map::DefaultHasher::new();
    format!("{flare:?}").hash(&mut base);
    (canvas.x0, canvas.y0, canvas.x1, canvas.y1).hash(&mut base);
    quantum.map(f32::to_bits).hash(&mut base);
    let q = quantum.unwrap_or(1.0);
    let mut px = vec![[0.0f32; 4]; w * h];
    let grid = |v: i32| v.div_euclid(TILE) * TILE;
    let mut ty = grid(rect.y0);
    while ty < rect.y1 {
        let mut tx = grid(rect.x0);
        while tx < rect.x1 {
            let tile = Rect::new(tx, ty, tx.saturating_add(TILE), ty.saturating_add(TILE)).intersect(&canvas);
            let mut k = base.clone();
            (tx, ty).hash(&mut k);
            let key = k.finish();
            let cached = lock().get(key);
            let light = match cached {
                Some(l) => l,
                None => {
                    let l = Arc::new(Light::pack(light(flare, tile, canvas, quantum), quantum));
                    lock().insert(key, l.clone());
                    l
                }
            };
            if !matches!(*light, Light::Dark) {
                let part = tile.intersect(&rect);
                let tw = tile.width() as usize;
                for y in part.y0..part.y1 {
                    let src = (y - tile.y0) as usize * tw;
                    let dst = (y - rect.y0) as usize * w;
                    for x in part.x0..part.x1 {
                        if let Some(p) = px.get_mut(dst + (x - rect.x0) as usize) {
                            *p = light.rgba(src + (x - tile.x0) as usize, q);
                        }
                    }
                }
            }
            tx = tx.saturating_add(TILE);
        }
        ty = ty.saturating_add(TILE);
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_doc::lens_flare::{FlareCategory, TextureKind, presets};

    fn flare() -> LensFlare {
        let mut f = presets(FlareCategory::default()).next().expect("a preset").flare();
        f.texture.kind = TextureKind::Dust;
        f.texture.opacity = 0.8;
        f
    }

    #[test]
    fn cached_tiles_match_the_direct_render_at_every_depth() {
        let canvas = Rect::new(0, 0, 700, 420);
        let f = flare();
        // Unaligned, spanning tiles, and the whole canvas; twice so the second read hits.
        for rect in [Rect::new(37, 11, 611, 400), canvas, Rect::new(250, 250, 520, 420)] {
            for q in [Some(255.0), Some(32768.0), None] {
                let direct = super::super::render_uncached(&f, rect, canvas, q);
                assert!(render(&f, rect, canvas, q) == direct, "{rect:?} {q:?} first");
                assert!(render(&f, rect, canvas, q) == direct, "{rect:?} {q:?} cached");
            }
        }
    }

    #[test]
    fn eviction_keeps_the_cache_within_budget_and_drops_the_oldest() {
        let mut c = Cache { map: HashMap::new(), bytes: 0, budget: 20_000, clock: 0 };
        let tile = || Arc::new(Light::U8(vec![[1, 2, 3]; 1000]));
        for k in 0..5 {
            c.insert(k, tile());
        }
        // Touch the first so the second is now the oldest.
        assert!(c.get(0).is_some());
        for k in 5..8 {
            c.insert(k, tile());
        }
        assert!(c.bytes <= c.budget, "{} > {}", c.bytes, c.budget);
        assert!(c.map.contains_key(&0), "recently used tile evicted");
        assert!(!c.map.contains_key(&1), "oldest tile kept");
    }
}
