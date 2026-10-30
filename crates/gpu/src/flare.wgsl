
// ---- Light flare fills (photocraft_compose::lens_flare) --------------------------------------
//
// Appended to compose.wgsl (same bindings). `fs_flare` evaluates a Light Flare fill per pixel,
// operation for operation like `lens_flare::render_quantized`. The flare is laid out on the CPU
// (`lens_flare::prepare`: positions, sizes, colours, per-element constants, each ray's angle,
// length and strength) and packed into `lut_tex` as a flat float array by `plan/flare.rs`, whose
// `LF_*` constants give the layout; keep the two in sync. `lut_tex` holds it as RGBA32F texels
// (1024 a row), so a record's 20 floats are five loads.

const LF_HEADER: i32 = 32;
const LF_STRIDE: i32 = 20;
// std::f32::consts::PI / TAU (these literals round to the same f32).
const LF_PI: f32 = 3.14159265358979;
const LF_TAU: f32 = 6.28318530717959;

// Element kinds (plan/flare.rs).
const LF_GLOW: i32 = 0;
const LF_GLOW_GAUSS: i32 = 1;
const LF_STREAK: i32 = 2;
const LF_STAR: i32 = 3;
const LF_RAYS: i32 = 4;
const LF_RING: i32 = 5;
const LF_GHOST: i32 = 6;
const LF_WASH: i32 = 7;

// TextureKind::ALL indices.
const LT_FILM_GRAIN: i32 = 1;
const LT_DUST: i32 = 2;
const LT_BIG_SPOTS: i32 = 3;
const LT_HEX_DUST: i32 = 4;
const LT_RAINDROPS: i32 = 5;
const LT_SCRATCHES: i32 = 6;
const LT_SMUDGE: i32 = 7;
const LT_COARSE_GRAIN: i32 = 8;
const LT_FINE_GRAIN: i32 = 9;
const LT_GLASS_ARCS: i32 = 10;
const LT_SOFT_ORBS: i32 = 11;
const LT_HAZY_GLASS: i32 = 12;
const LT_BRUSHED_STEEL: i32 = 13;
const LT_STRETCHED_SPECKS: i32 = 14;
const LT_BOKEH_BUBBLES: i32 = 15;

// Texel `t` of the packed flare (floats 4t..4t+3), and float `i`.
fn lf4(t: i32) -> vec4<f32> { return textureLoad(lut_tex, vec2(t % 1024, t / 1024), 0); }
fn lf(i: i32) -> f32 { return lf4(i >> 2u)[i & 3]; }

fn lf_hypot(x: f32, y: f32) -> f32 { return sqrt(x * x + y * y); }

// f32::rem_euclid.
fn lf_rem(x: f32, m: f32) -> f32 {
    let r = x - m * trunc(x / m);
    return select(r, r + abs(m), r < 0.0);
}

// f32::round: halves away from zero (WGSL's round() goes to even).
fn lf_round(x: f32) -> f32 { return sign(x) * floor(abs(x) + 0.5); }

fn lf_max3(l: vec3<f32>) -> f32 { return max(max(l.x, l.y), l.z); }

// lens_flare::smoothstep (not WGSL's: equal edges are a step).
fn lf_smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if (abs(e1 - e0) < 1.1920929e-7) {
        return select(1.0, 0.0, x < e0);
    }
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn lf_hash2(x: i32, y: i32, seed: i32) -> u32 {
    var h = (bitcast<u32>(x) * 0x8da6b343u) ^ (bitcast<u32>(y) * 0xd8163841u) ^ (bitcast<u32>(seed) * 0xcb1ab31fu);
    h = h ^ (h >> 13u);
    h = h * 0x5bd1e995u;
    return h ^ (h >> 15u);
}

// The low 24 bits over 2^24 (exact).
fn lf_hash01(h: u32) -> f32 { return f32(h & 0x00ffffffu) * 5.9604644775390625e-8; }

fn lf_sub(h: u32, k: i32) -> f32 { return lf_hash01(lf_hash2(bitcast<i32>(h), k, 0x2c1b)); }

fn lf_vnoise(x: f32, y: f32, seed: i32) -> f32 {
    let x0 = floor(x);
    let y0 = floor(y);
    let fx = x - x0;
    let fy = y - y0;
    let ix = i32(x0);
    let iy = i32(y0);
    let v00 = lf_hash01(lf_hash2(ix, iy, seed));
    let v10 = lf_hash01(lf_hash2(ix + 1, iy, seed));
    let v01 = lf_hash01(lf_hash2(ix, iy + 1, seed));
    let v11 = lf_hash01(lf_hash2(ix + 1, iy + 1, seed));
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let top = v00 + (v10 - v00) * sx;
    let bottom = v01 + (v11 - v01) * sx;
    return top + (bottom - top) * sy;
}

// The spotted kinds of lens_flare::dirt: the brightest spot of the 3×3 cells around (x, y).
fn lf_spots(kind: i32, x: f32, y: f32) -> f32 {
    var cell = 30.0;
    var prob = 0.45;
    var rmin = 1.0;
    var rmax = 4.0;
    var seed = 21;
    switch kind {
        case 3: { cell = 140.0; prob = 0.55; rmin = 25.0; rmax = 60.0; seed = 22; }
        case 4: { cell = 90.0; prob = 0.5; rmin = 14.0; rmax = 38.0; seed = 23; }
        case 5: { cell = 70.0; prob = 0.5; rmin = 8.0; rmax = 26.0; seed = 24; }
        case 6: { cell = 200.0; prob = 0.8; rmin = 50.0; rmax = 95.0; seed = 25; }
        case 11: { cell = 45.0; prob = 0.6; rmin = 6.0; rmax = 20.0; seed = 26; }
        case 14: { cell = 40.0; prob = 0.5; rmin = 1.0; rmax = 3.0; seed = 27; }
        case 15: { cell = 230.0; prob = 0.5; rmin = 45.0; rmax = 105.0; seed = 28; }
        default: {}
    }
    let cx = i32(floor(x / cell));
    let cy = i32(floor(y / cell));
    var best = 0.0;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let h = lf_hash2(cx + i, cy + j, seed);
            if (lf_hash01(h) > prob) {
                continue;
            }
            let px = f32(cx + i) * cell + lf_sub(h, 0x68e3) * cell;
            let py = f32(cy + j) * cell + lf_sub(h, 0x2b79) * cell;
            let r = rmin + (rmax - rmin) * lf_sub(h, 0x4cf5);
            let b = 0.35 + 0.65 * lf_sub(h, 0x1d87);
            let dx = x - px;
            let dy = y - py;
            var v = 0.0;
            switch kind {
                case 2: { v = b * (1.0 - lf_smoothstep(r * 0.4, r, lf_hypot(dx, dy))); }
                case 3: {
                    let d = lf_hypot(dx, dy) / r;
                    let e = (d - 0.95) / 0.08;
                    v = b * 0.55 * ((1.0 - lf_smoothstep(0.55, 1.0, d)) + 0.5 * exp(-(e * e)));
                }
                case 4: {
                    let step = LF_TAU / 6.0;
                    let a = lf_rem(atan2(dy, dx), step) - step / 2.0;
                    let d = lf_hypot(dx, dy) / r * cos(step / 2.0) / cos(a);
                    v = b * 0.6 * (1.0 - lf_smoothstep(0.75, 1.0, d));
                }
                case 5: {
                    let d = lf_hypot(dx, dy) / r;
                    if (d <= 1.2) {
                        let e = (d - 0.88) / 0.1;
                        v = b * (0.18 * (1.0 - lf_smoothstep(0.8, 1.0, d)) + 0.9 * exp(-(e * e)));
                    }
                }
                case 6: {
                    let ang = lf_sub(h, 0x7a1c) * LF_PI;
                    let s = sin(ang);
                    let c = cos(ang);
                    let along = abs(dx * c + dy * s);
                    if (along <= r) {
                        let perp = abs(-dx * s + dy * c);
                        let e = perp / 0.7;
                        v = b * 0.8 * exp(-(e * e)) * (1.0 - along / r);
                    }
                }
                case 11: {
                    let d = lf_hypot(dx, dy) / r;
                    v = b * 0.4 * (1.0 - lf_smoothstep(0.2, 1.0, d));
                }
                case 14: {
                    let d = lf_hypot(dx / (r * 6.0), dy / r);
                    v = b * (1.0 - lf_smoothstep(0.3, 1.0, d));
                }
                case 15: {
                    let d = lf_hypot(dx, dy) / r;
                    if (d <= 1.1) {
                        let body = (1.0 - lf_smoothstep(0.96, 1.02, d)) * (0.22 + 0.18 * d * d);
                        let er = (d - 0.97) / 0.035;
                        let rim = exp(-(er * er));
                        let ei = (d - 0.7) / 0.05;
                        let inner = 0.12 * exp(-(ei * ei));
                        v = b * 0.7 * (body + 0.55 * rim + inner);
                    }
                }
                default: {}
            }
            best = max(best, v);
        }
    }
    return best;
}

// lens_flare::glass_arcs.
fn lf_glass_arcs(x: f32, y: f32) -> f32 {
    var best = 0.0;
    for (var k = 0; k < 2; k++) {
        let cx = select(1250.0, -250.0, k == 0);
        let cy = select(-420.0, 950.0, k == 0);
        let seed = 41 + k;
        let dx = x - cx;
        let dy = y - cy;
        let r = lf_hypot(dx, dy);
        let s = atan2(dy, dx) * r;
        let band = lf_vnoise(r / 4.0, s / 160.0, seed);
        let streak = lf_smoothstep(0.7, 0.97, band);
        let patchy = lf_smoothstep(0.45, 0.85, lf_vnoise(r / 140.0, s / 220.0, seed + 7));
        best = max(best, 0.4 * streak * patchy);
    }
    return best;
}

// lens_flare::dirt.
fn lf_dirt(kind: i32, x: f32, y: f32) -> f32 {
    switch kind {
        case 10: { return lf_glass_arcs(x, y); }
        case 12: {
            let rx = 0.8 * x - 0.6 * y;
            let ry = 0.6 * x + 0.8 * y;
            let fog = 0.5 * lf_vnoise(x / 150.0, y / 150.0, 31) + 0.3 * lf_vnoise(rx / 60.0, ry / 60.0, 32) + 0.2 * lf_vnoise(ry / 23.0, -rx / 23.0, 36);
            return 0.3 * lf_smoothstep(0.25, 0.85, fog) + 0.6 * lf_spots(LT_DUST, x, y) * (0.4 + fog);
        }
        case 13: {
            let lines = 0.6 * lf_vnoise(x / 260.0, y / 0.9, 33) + 0.4 * lf_vnoise(x / 70.0, y / 0.45, 34);
            let sheen = lf_vnoise(x / 300.0, y / 300.0, 35);
            return lf_smoothstep(0.35, 0.95, lines) * (0.15 + 0.3 * sheen);
        }
        case 7: {
            let n = 0.6 * lf_vnoise(x / 140.0, y / 140.0, 11) + 0.3 * lf_vnoise(x / 55.0, y / 55.0, 12) + 0.1 * lf_vnoise(x / 18.0, y / 18.0, 13);
            return lf_smoothstep(0.5, 0.85, n) * 0.6;
        }
        case 2, 3, 4, 5, 6, 11, 14, 15: { return lf_spots(kind, x, y); }
        default: { return 0.0; }
    }
}

// Prepared::texture_light. Header: 6 kind, 7 opacity, 8/9 flips, 10..13 canvas, 14 unit,
// 15 radius, 16..18 colour, 1/2 source.
fn lf_texture(x: f32, y: f32, l: vec3<f32>) -> vec3<f32> {
    let kind = i32(lf(6));
    let opacity = lf(7);
    var tx = x - lf(10);
    var ty = y - lf(11);
    if (lf(8) > 0.5) { tx = lf(12) - tx; }
    if (lf(9) > 0.5) { ty = lf(13) - ty; }
    let unit = lf(14);
    let qx = tx * unit;
    let qy = ty * unit;
    let radius = lf(15);
    let tc = vec3(lf(16), lf(17), lf(18));
    let d = lf_hypot(x - lf(1), y - lf(2));
    var n = 0.0;
    var amount = -1.0;
    if (kind == LT_FILM_GRAIN) {
        n = lf_hash01(lf_hash2(i32(qx * 1.5), i32(qy * 1.5), 77)) - 0.5;
        amount = 0.9;
    } else if (kind == LT_COARSE_GRAIN) {
        let clump = lf_vnoise(qx / 4.0, qy / 4.0, 78) + 0.6 * lf_vnoise(qx / 1.6, qy / 1.6, 79);
        let fine = lf_hash01(lf_hash2(i32(qx * 1.5), i32(qy * 1.5), 80));
        n = (clump / 1.6 - 0.5) * 1.6 + 0.25 * (fine - 0.5);
        amount = 1.1;
    } else if (kind == LT_FINE_GRAIN) {
        n = lf_hash01(lf_hash2(i32(floor(tx)), i32(floor(ty)), 81)) - 0.5;
        amount = 0.6;
    }
    if (amount >= 0.0) {
        let lum = lf_max3(l);
        var o = l * (1.0 + opacity * amount * n);
        let broad = exp(-d / radius) * (1.0 - min(lum, 1.0));
        return o + opacity * 0.08 * max(n + 0.5, 0.0) * broad * tc;
    }
    let dirt = lf_dirt(kind, qx, qy);
    if (dirt <= 0.0) {
        return l;
    }
    let lum = min(lf_max3(l), 2.0);
    let lit = 0.55 * exp(-d / radius) + 0.5 * lum;
    return l + opacity * dirt * lit * tc;
}

// lens_flare::add: only light above the cutoff.
fn lf_add(l: vec3<f32>, col: vec3<f32>, v: f32, cutoff: f32) -> vec3<f32> {
    if (v > cutoff) { return l + col * v; }
    return l;
}

// Elem::add for the element at texel `t` (LF_STRIDE floats: kind, centre, colour, bbox, ten
// shape constants P).
fn lf_element(t: i32, x: f32, y: f32, l_in: vec3<f32>, cutoff: f32) -> vec3<f32> {
    var l = l_in;
    let v1 = lf4(t + 1);
    let v2 = lf4(t + 2);
    if (x < v1.z || x > v2.x || y < v1.w || y > v2.y) {
        return l;
    }
    let v0 = lf4(t);
    let v3 = lf4(t + 3);
    let v4 = lf4(t + 4);
    let P = array<f32, 10>(v2.z, v2.w, v3.x, v3.y, v3.z, v3.w, v4.x, v4.y, v4.z, v4.w);
    let kind = i32(v0.x);
    let dx = x - v0.y;
    let dy = y - v0.z;
    let col = vec3(v0.w, v1.x, v1.y);
    switch kind {
        case 0, 1: {
            // Glow: radius; in the source's frame (header 3/4 cos/sin, 5 1/aspect).
            let cs = lf(3);
            let sn = lf(4);
            let lx = (dx * cs + dy * sn) * lf(5);
            let ly = -dx * sn + dy * cs;
            let d = lf_hypot(lx, ly) / P[0];
            let v = select(exp(-d), exp(-d * d), kind == LF_GLOW_GAUSS);
            l = lf_add(l, col, v, cutoff);
        }
        case 2: {
            // Streak: cos, sin, len, th, halo.
            let cs = P[0];
            let sn = P[1];
            let len = P[2];
            let along = abs(dx * cs + dy * sn);
            if (along < len) {
                let perp = abs(-dx * sn + dy * cs);
                let core = perp / P[3];
                let k = along / len;
                let v = (exp(-core * core) * 0.75 + exp(-perp / P[4]) * 0.25) * exp(-2.2 * k) * (1.0 - k);
                l = lf_add(l, col, v, cutoff);
            }
        }
        case 3: {
            // Star: rot, step, len, th.
            let d = lf_hypot(dx, dy);
            let len = P[2];
            if (d < len) {
                let step = P[1];
                let a = atan2(dy, dx) - P[0];
                let delta = a - lf_round(a / step) * step;
                let along = d * cos(delta);
                if (along > 0.0) {
                    let perp = d * abs(sin(delta));
                    let k = along / len;
                    let w = P[3] * (1.0 + 2.0 * k);
                    let e = perp / w;
                    let f = 1.0 - k;
                    l = lf_add(l, col, exp(-(e * e)) * (f * f), cutoff);
                }
            }
        }
        case 4: {
            // Rays: rot, step, len, th, count, table offset (phi, reach, strength per ray).
            let d = lf_hypot(dx, dy);
            let len = P[2];
            if (d < len && d >= 0.5) {
                let step = P[1];
                let th = P[3];
                let count = max(i32(P[4]), 1);
                let table = i32(P[5]);
                let a = lf_rem(atan2(dy, dx) - P[0], LF_TAU);
                let bin = i32(a / step);
                var v = 0.0;
                for (var j = -1; j <= 1; j++) {
                    let k = ((bin + j) % count + count) % count;
                    let r = table + 3 * k;
                    let reach = lf(r + 1);
                    let raw = a - lf(r);
                    let delta = raw - lf_round(raw / LF_TAU) * LF_TAU;
                    let along = d * cos(delta);
                    if (along <= 0.0 || along >= reach) {
                        continue;
                    }
                    let perp = d * abs(sin(delta));
                    let e = perp / th;
                    let t = 1.0 - along / reach;
                    v += lf(r + 2) * exp(-(e * e)) * (t * sqrt(t));
                }
                l = lf_add(l, col, v, cutoff);
            }
        }
        case 5: {
            // Ring: r, w, chroma, arc, dir.
            let d = lf_hypot(dx, dy);
            let r = P[0];
            let w = P[1];
            let chroma = P[2];
            let arc = P[3];
            let dir = vec2(P[4], P[5]);
            var facing = 1.0;
            if (arc > 0.0 && d > 1e-3 && (dir.x != 0.0 || dir.y != 0.0)) {
                let c = (dx * dir.x + dy * dir.y) / d;
                facing = 1.0 - arc + arc * lf_smoothstep(-0.1, 0.7, c);
            }
            for (var k = 0; k < 3; k++) {
                let rk = r * (1.0 + chroma * 0.03 * (1.0 - f32(k)));
                let e = (d - rk) / w;
                l[k] += col[k] * exp(-e * e) * facing;
            }
        }
        case 6: {
            // Ghost: r, sides, rot, rim, soft, chroma, step, cos(step / 2).
            var dd = lf_hypot(dx, dy) / P[0];
            let sides = P[1];
            let rim = P[3];
            let soft = P[4];
            let chroma = P[5];
            if (sides > 0.0) {
                let step = P[6];
                let a = lf_rem(atan2(dy, dx) - P[2], step) - step / 2.0;
                dd *= P[7] / cos(a);
            }
            if (dd < 1.25 + 0.06 * chroma) {
                for (var k = 0; k < 3; k++) {
                    let e = dd / (1.0 + chroma * 0.04 * (1.0 - f32(k)));
                    let body = 1.0 - lf_smoothstep(1.0 - soft, 1.0, e);
                    let q = (e - 0.97) / 0.05;
                    let edge = exp(-(q * q));
                    let shade = body * (1.0 - 0.5 * rim + 0.5 * rim * e * e * e) + 0.6 * rim * edge;
                    l[k] += col[k] * shade;
                }
            }
        }
        case 7: {
            // Wash: dir, reach, spread, chroma, peak, reach × WASH_REACH.
            let d = lf_hypot(dx, dy);
            if (d < P[6]) {
                let dir = vec2(P[0], P[1]);
                let reach = P[2];
                let chroma = P[4];
                let along = dx * dir.x + dy * dir.y;
                let across = abs(-dx * dir.y + dy * dir.x);
                let theta = atan2(across, along);
                let open = P[3] + LF_PI * exp(-d / (0.45 * reach));
                let t = theta / open;
                let cone = exp(-(t * t));
                let fade = exp(-d / reach);
                l = lf_add(l, col, fade * (0.05 + 0.95 * cone), cutoff);
                if (chroma > 0.0 && cone > cutoff) {
                    let r0 = reach * 0.9;
                    let w = reach * 0.1;
                    for (var k = 0; k < 3; k++) {
                        let e = (d - r0 * (1.0 + 0.06 * chroma * (1.0 - f32(k)))) / w;
                        l[k] += P[5] * 0.18 * chroma * cone * exp(-e * e);
                    }
                }
            }
        }
        default: {}
    }
    return l;
}

// lens_flare::burn.
fn lf_burn(l_in: vec3<f32>) -> vec3<f32> {
    var l = l_in;
    for (var k = 0; k < 3; k++) {
        let v = l[k];
        // Not finite (NaN fails every comparison): no light.
        l[k] = select(0.0, max(v, 0.0), abs(v) <= 3.4e38);
    }
    let m = lf_max3(l);
    if (m > 1.0) {
        l = min(l + (m - 1.0) * 0.6, vec3(1.0));
    }
    return l;
}

// The flare at document pixel `d` as straight RGBA (lens_flare::render_quantized). Header:
// 0 element count, 20 has texture, 21 quantum (0: float), 22/23 the dither's second draw offset.
fn lf_pixel(d: vec2<i32>) -> vec4<f32> {
    let x = f32(d.x) + 0.5;
    let y = f32(d.y) + 0.5;
    var l = vec3(0.0);
    let n = i32(lf(0));
    let cutoff = lf(19);
    for (var i = 0; i < n; i++) {
        l = lf_element((LF_HEADER + i * LF_STRIDE) / 4, x, y, l, cutoff);
    }
    if (lf(20) > 0.5) {
        l = lf_texture(x, y, l);
    }
    l = lf_burn(l);
    let q = lf(21);
    if (q > 0.0 && lf_max3(l) > 0.0) {
        // Triangular dither, the same in every channel.
        let off = vec2(i32(lf(22)), i32(lf(23)));
        let nz = dither_noise(d) + dither_noise(d + off) - 1.0;
        l = clamp(floor(l * q + nz + 0.5) / q, vec3(0.0), vec3(1.0));
    }
    // lens_flare::to_rgba.
    let a = min(lf_max3(l), 1.0);
    if (a < lf(19)) {
        return vec4(0.0);
    }
    return vec4(min(l / a, vec3(1.0)), a);
}

// A Light Flare fill layer's pixels, times its mask.
@fragment
fn fs_flare(in: VOut) -> @location(0) vec4<f32> {
    let d = doc_px(local(in.pos));
    var c = lf_pixel(d);
    c.a = c.a * mask_value(d);
    return c;
}
