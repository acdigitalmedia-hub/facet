//! Procedural art: terrain textures, the road, boulders, pedestals, faceted gems, special
//! towers, creatures and checkpoints. Everything is drawn from code; no image assets.
//!
//! Lighting convention: light comes from the top-left, shadows fall to the bottom-right.

use std::f32::consts::{PI, TAU};

use macroquad::prelude::*;

use crate::data::MapDef;

// ---------- Small helpers ----------

pub fn rgbf(r: f32, g: f32, b: f32) -> Color {
    Color::new(r, g, b, 1.0)
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

pub fn scale(c: Color, k: f32) -> Color {
    Color::new((c.r * k).min(1.0), (c.g * k).min(1.0), (c.b * k).min(1.0), c.a)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

/// Deterministic pseudo-random number in 0..1 for an integer key.
pub fn rnd(key: usize, k: u32) -> f32 {
    hash(key as i32, k as i32, 9_127)
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (xf, yf) = (x - x.floor(), y - y.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (s(xf), s(yf));
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut f = 1.0;
    for o in 0..4 {
        sum += value_noise(x * f, y * f, seed + o) * amp;
        f *= 2.0;
        amp *= 0.5;
    }
    sum / 0.9375
}

/// Filled polygon as a triangle fan around `c`.
pub fn fan(c: Vec2, pts: &[Vec2], col: Color) {
    for i in 0..pts.len() {
        draw_triangle(c, pts[i], pts[(i + 1) % pts.len()], col);
    }
}

pub fn outline(pts: &[Vec2], thick: f32, col: Color) {
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        draw_line(a.x, a.y, b.x, b.y, thick, col);
    }
}

fn light_dir() -> Vec2 {
    vec2(-0.6, -0.8)
}

/// A faceted solid: each triangle of the fan is shaded by how much it faces the light.
/// `table` (0..1) adds a brighter flat top face scaled by that factor.
pub fn faceted(c: Vec2, pts: &[Vec2], base: Color, contrast: f32, table: Option<f32>) {
    let l = light_dir();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        let mid = ((a + b) * 0.5 - c).normalize_or_zero();
        let shade = 1.0 - contrast + contrast * (0.5 + 0.5 * mid.dot(l)) * 1.4;
        draw_triangle(c, a, b, scale(base, shade));
    }
    if let Some(t) = table {
        let inner: Vec<Vec2> = pts.iter().map(|p| c + (*p - c) * t).collect();
        let top = mix(base, WHITE, 0.3);
        for i in 0..inner.len() {
            let (a, b) = (inner[i], inner[(i + 1) % inner.len()]);
            let mid = ((a + b) * 0.5 - c).normalize_or_zero();
            let shade = 0.9 + 0.2 * (0.5 + 0.5 * mid.dot(l));
            draw_triangle(c, a, b, scale(top, shade));
        }
        outline(&inner, 1.0, with_alpha(WHITE, 0.25));
    }
}

fn ngon(c: Vec2, n: usize, rx: f32, ry: f32, rot: f32) -> Vec<Vec2> {
    (0..n).map(|k| {
        let a = rot + k as f32 / n as f32 * TAU;
        c + vec2(a.cos() * rx, a.sin() * ry)
    }).collect()
}

pub fn ellipse(c: Vec2, rx: f32, ry: f32, col: Color) {
    fan(c, &ngon(c, 20, rx, ry, 0.0), col);
}

/// Soft glow: stacked translucent circles.
pub fn glow(p: Vec2, r: f32, col: Color, strength: f32) {
    for k in 0..4 {
        let f = 1.0 - k as f32 * 0.22;
        draw_circle(p.x, p.y, r * f, with_alpha(col, 0.07 * strength * (k as f32 + 1.0)));
    }
}

// ---------- Themes and terrain ----------

pub struct Theme {
    pub ground: Color,
    pub ground_alt: Color,
    pub speck: Color,
    pub speck2: Color,
    pub dirt: Color,
    pub dirt_edge: Color,
    pub rock: Color,
}

pub fn theme(name: &str) -> Theme {
    match name {
        "autumn" => Theme {
            ground: rgbf(0.29, 0.22, 0.12),
            ground_alt: rgbf(0.22, 0.17, 0.10),
            speck: rgbf(0.75, 0.38, 0.12),
            speck2: rgbf(0.62, 0.20, 0.10),
            dirt: rgbf(0.40, 0.32, 0.22),
            dirt_edge: rgbf(0.24, 0.18, 0.11),
            rock: rgbf(0.44, 0.38, 0.33),
        },
        "ash" => Theme {
            ground: rgbf(0.18, 0.16, 0.16),
            ground_alt: rgbf(0.12, 0.11, 0.11),
            speck: rgbf(0.30, 0.27, 0.26),
            speck2: rgbf(0.95, 0.42, 0.12),
            dirt: rgbf(0.30, 0.26, 0.24),
            dirt_edge: rgbf(0.13, 0.11, 0.11),
            rock: rgbf(0.45, 0.39, 0.37),
        },
        "marsh" => Theme {
            ground: rgbf(0.16, 0.23, 0.17),
            ground_alt: rgbf(0.11, 0.17, 0.13),
            speck: rgbf(0.30, 0.40, 0.18),
            speck2: rgbf(0.55, 0.55, 0.35),
            dirt: rgbf(0.32, 0.29, 0.21),
            dirt_edge: rgbf(0.15, 0.17, 0.12),
            rock: rgbf(0.38, 0.40, 0.37),
        },
        "snow" => Theme {
            ground: rgbf(0.74, 0.78, 0.84),
            ground_alt: rgbf(0.62, 0.67, 0.75),
            speck: rgbf(0.50, 0.58, 0.70),
            speck2: rgbf(0.92, 0.95, 1.0),
            dirt: rgbf(0.52, 0.50, 0.50),
            dirt_edge: rgbf(0.60, 0.63, 0.70),
            rock: rgbf(0.45, 0.47, 0.52),
        },
        _ => Theme {
            ground: rgbf(0.21, 0.29, 0.14),
            ground_alt: rgbf(0.15, 0.22, 0.10),
            speck: rgbf(0.35, 0.46, 0.20),
            speck2: rgbf(0.85, 0.80, 0.45),
            dirt: rgbf(0.42, 0.34, 0.22),
            dirt_edge: rgbf(0.20, 0.22, 0.12),
            rock: rgbf(0.42, 0.41, 0.39),
        },
    }
}

const TS: usize = 20;

/// Bakes the ground, walls (cliffs or lakes), decorations and a vignette into one texture.
pub fn terrain_texture(map: &MapDef) -> Texture2D {
    let th = theme(&map.theme);
    let (w, h) = (map.w as usize * TS, map.h as usize * TS);
    let mut px = vec![0u8; w * h * 4];
    let water = map.wall_style == "water";
    let seed = map.name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let wall_at = |x: f32, y: f32| {
        let (cx, cy) = ((x / TS as f32).floor() as i32, (y / TS as f32).floor() as i32);
        cx >= 0 && cy >= 0 && cx < map.w && cy < map.h && map.is_wall(cx, cy)
    };
    // Distance (in px, capped) to the nearest wall cell edge, for shores and shadows.
    let near_wall = |x: f32, y: f32, r: f32| {
        let mut best = r;
        let steps = 6;
        for k in 0..steps * 2 {
            let a = k as f32 / (steps * 2) as f32 * TAU;
            for d in [r * 0.33, r * 0.66, r] {
                if d < best && wall_at(x + a.cos() * d, y + a.sin() * d) {
                    best = d;
                }
            }
        }
        best
    };

    // Fraction of wall within ~1.3 cells of a point: a blurred wall mask that gives lakes
    // rounded, organic shores while still covering the wall cells.
    let coverage = |x: f32, y: f32| {
        let (cx, cy) = ((x / TS as f32).floor() as i32, (y / TS as f32).floor() as i32);
        let mut any = false;
        let mut all = true;
        for dy in -2..=2 {
            for dx in -2..=2 {
                let (nx, ny) = (cx + dx, cy + dy);
                let w = nx >= 0 && ny >= 0 && nx < map.w && ny < map.h && map.is_wall(nx, ny);
                any |= w;
                all &= w;
            }
        }
        if !any {
            return 0.0;
        }
        if all {
            return 1.0;
        }
        let r = TS as f32 * 1.3;
        let mut n = 0;
        let mut hit = 0;
        for ring in 0..4 {
            let rr = r * ring as f32 / 3.0;
            let steps = if ring == 0 { 1 } else { 10 };
            for k in 0..steps {
                let a = k as f32 / steps as f32 * TAU + ring as f32;
                n += 1;
                if wall_at(x + a.cos() * rr, y + a.sin() * rr) {
                    hit += 1;
                }
            }
        }
        hit as f32 / n as f32
    };

    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32, y as f32);
            let n = fbm(fx / 90.0, fy / 90.0, seed);
            let grain = hash(x as i32, y as i32, seed + 77) - 0.5;
            let mut c = mix(th.ground_alt, th.ground, (n * 1.6 - 0.3).clamp(0.0, 1.0));
            c = scale(c, 1.0 + grain * 0.08);
            let detail = fbm(fx / 14.0, fy / 14.0, seed + 5);
            c = scale(c, 0.92 + detail * 0.16);

            if wall_at(fx, fy) {
                if water {
                    c = water_color(c, coverage(fx, fy), fx, fy, seed);
                } else {
                    let r = fbm(fx / 18.0, fy / 18.0, seed + 21);
                    c = scale(th.rock, 0.55 + r * 0.6);
                    // Lit top edge, dark bottom edge
                    if !wall_at(fx, fy - 6.0) {
                        c = mix(c, scale(th.rock, 1.5), 0.6);
                    } else if !wall_at(fx, fy + 6.0) {
                        c = scale(c, 0.55);
                    }
                    if (fbm(fx / 9.0, fy / 9.0, seed + 31) * 9.0).fract() < 0.07 {
                        c = scale(c, 0.6);
                    }
                }
            } else {
                if water {
                    let cov = coverage(fx, fy);
                    if cov > 0.0 {
                        c = water_color(c, cov, fx, fy, seed);
                    }
                } else {
                    let d = near_wall(fx, fy, 14.0);
                    if d < 14.0 && wall_at(fx - 6.0, fy - 8.0) {
                        c = scale(c, 1.0 - (1.0 - d / 14.0) * 0.45);
                    }
                }
            }

            if map.theme == "ash" && !wall_at(fx, fy) {
                // Glowing cracks in the ash
                let crack = fbm(fx / 40.0, fy / 40.0, seed + 91);
                let line = (crack * 14.0).fract();
                if (line - 0.5).abs() < 0.025 {
                    let heat = fbm(fx / 25.0, fy / 25.0, seed + 93);
                    c = mix(c, rgbf(1.0, 0.45, 0.1), (heat - 0.35).clamp(0.0, 0.5) * 1.4);
                }
            }

            // Vignette
            let (u, v) = (fx / w as f32 - 0.5, fy / h as f32 - 0.5);
            let vig = 1.0 - (u * u + v * v).sqrt().powf(2.2) * 0.9;
            c = scale(c, vig.clamp(0.4, 1.0));

            let i = (y * w + x) * 4;
            px[i] = (c.r.clamp(0.0, 1.0) * 255.0) as u8;
            px[i + 1] = (c.g.clamp(0.0, 1.0) * 255.0) as u8;
            px[i + 2] = (c.b.clamp(0.0, 1.0) * 255.0) as u8;
            px[i + 3] = 255;
        }
    }

    // Decorations: tufts, leaves, embers, reeds or snow drifts.
    let mut put = |x: i32, y: i32, c: Color, a: f32| {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return;
        }
        let i = (y as usize * w + x as usize) * 4;
        for (k, v) in [c.r, c.g, c.b].iter().enumerate() {
            px[i + k] = (px[i + k] as f32 * (1.0 - a) + v.clamp(0.0, 1.0) * 255.0 * a) as u8;
        }
    };
    let count = map.w as usize * map.h as usize * 2;
    for k in 0..count {
        let x = (hash(k as i32, 1, seed) * w as f32) as i32;
        let y = (hash(k as i32, 2, seed) * h as f32) as i32;
        if wall_at(x as f32, y as f32) {
            continue;
        }
        let pick = hash(k as i32, 3, seed);
        let col = if pick < 0.85 { th.speck } else { th.speck2 };
        match map.theme.as_str() {
            "meadow" | "marsh" if pick < 0.85 => {
                // A soft grass tuft: a few short blades of varying height
                for b in 0..3 {
                    let dx = b as i32 - 1;
                    let len = 2 + ((hash(k as i32, 10 + b, seed) * 3.0) as i32);
                    for t in 0..len {
                        put(x + dx * (1 + t / 2), y - t, col, 0.28);
                    }
                }
            }
            "meadow" | "marsh" => {
                // Tiny flowers
                put(x, y, col, 0.7);
                put(x + 1, y, col, 0.4);
            }
            "snow" => {
                for dx in -2..=2 {
                    put(x + dx, y, col, 0.25);
                    put(x + dx, y + 1, col, 0.15);
                }
            }
            _ => {
                // Leaves / embers: small blobs
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx * dy == 0 {
                            put(x + dx, y + dy, col, if map.theme == "ash" { 0.5 } else { 0.7 });
                        }
                    }
                }
            }
        }
    }

    let img = Image { bytes: px, width: w as u16, height: h as u16 };
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

/// Ground/bank/water color for a point with the given wall coverage.
fn water_color(ground: Color, cov: f32, x: f32, y: f32, seed: u32) -> Color {
    let smooth = |e0: f32, e1: f32, v: f32| {
        let t = ((v - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let wobble = fbm(x / 18.0, y / 18.0, seed + 41);
    let thresh = 0.5 + (wobble - 0.5) * 0.3;
    // Wet ground -> muddy bank -> foam -> shallow -> deep water, blended smoothly.
    let wet = mix(ground, rgbf(0.20, 0.19, 0.12), (cov / (thresh - 0.1)).clamp(0.0, 1.0) * 0.6);
    let bank = mix(rgbf(0.20, 0.18, 0.12), rgbf(0.29, 0.26, 0.17), wobble);
    let depth = ((cov - thresh) / (1.0 - thresh)).clamp(0.0, 1.0).powf(0.7);
    let mut water = mix(rgbf(0.16, 0.34, 0.38), rgbf(0.05, 0.12, 0.21), depth);
    let ripple = fbm(x / 30.0, y / 12.0, seed + 11);
    let band = ((y * 0.15 + ripple * 9.0 + x * 0.02).sin() * 0.5 + 0.5).powf(12.0);
    water = mix(water, rgbf(0.55, 0.72, 0.80), band * 0.14 * depth);
    water = mix(water, rgbf(0.62, 0.72, 0.70), (1.0 - smooth(thresh + 0.06, thresh + 0.14, cov)) * 0.35);
    let c = mix(wet, bank, smooth(thresh - 0.16, thresh - 0.1, cov));
    mix(c, water, smooth(thresh + 0.02, thresh + 0.07, cov))
}

/// Distance (px, capped at 30) from a water pixel to the nearest non-wall ground.
#[allow(dead_code)]
fn near_ground(map: &MapDef, x: f32, y: f32) -> f32 {
    let mut best: f32 = 30.0;
    for k in 0..16 {
        let a = k as f32 / 16.0 * TAU;
        for d in [2.0, 4.0, 6.0, 9.0, 12.0, 16.0, 22.0, 30.0] {
            if d >= best {
                break;
            }
            let (cx, cy) = (((x + a.cos() * d) / TS as f32).floor() as i32, ((y + a.sin() * d) / TS as f32).floor() as i32);
            if cx >= 0 && cy >= 0 && cx < map.w && cy < map.h && !map.is_wall(cx, cy) {
                best = best.min(d);
            }
        }
    }
    best
}

// ---------- Road ----------

/// The worn dirt road along the current route. `pts` are cell centers in screen space.
pub fn road(pts: &[Vec2], cs: f32, th: &Theme, alpha: f32) {
    let passes = [(0.5, scale(th.dirt_edge, 0.95)), (0.38, th.dirt), (0.16, scale(th.dirt, 1.12))];
    for (r, col) in passes {
        let col = with_alpha(col, alpha);
        for p in pts {
            draw_circle(p.x, p.y, cs * r, col);
        }
        for pair in pts.windows(2) {
            draw_line(pair[0].x, pair[0].y, pair[1].x, pair[1].y, cs * r * 2.0, col);
        }
    }
    // Pebbles
    for (k, p) in pts.iter().enumerate() {
        for j in 0..2 {
            let o = vec2(rnd(k, j * 2) - 0.5, rnd(k, j * 2 + 1) - 0.5) * cs * 0.6;
            draw_circle(p.x + o.x, p.y + o.y, cs * 0.04, with_alpha(scale(th.dirt, 0.7), alpha));
        }
    }
}

// ---------- Boulders, pedestals, gems ----------

pub fn shadow(p: Vec2, rx: f32, ry: f32, a: f32) {
    ellipse(p, rx, ry, Color::new(0.0, 0.0, 0.0, a));
}

/// A rough boulder filling roughly one cell. `seed` keeps its shape stable.
pub fn boulder(c: Vec2, cs: f32, seed: usize, base: Color) {
    shadow(c + vec2(cs * 0.1, cs * 0.16), cs * 0.44, cs * 0.3, 0.35);
    let n = 9;
    let pts: Vec<Vec2> = (0..n)
        .map(|k| {
            let a = k as f32 / n as f32 * TAU + rnd(seed, 50) * 0.6;
            let r = cs * (0.42 + rnd(seed, k as u32) * 0.12);
            c + vec2(a.cos() * r, a.sin() * r * 0.88)
        })
        .collect();
    let body = scale(base, 0.85 + rnd(seed, 60) * 0.3);
    faceted(c + vec2(-cs * 0.06, -cs * 0.08), &pts, body, 0.55, None);
    outline(&pts, 1.0, with_alpha(BLACK, 0.35));
    // A crack
    let a = c + vec2(-cs * 0.12, -cs * 0.05);
    let b = a + vec2(cs * 0.15, cs * (rnd(seed, 70) * 0.2));
    draw_line(a.x, a.y, b.x, b.y, 1.0, with_alpha(BLACK, 0.3));
}

/// Pedestal rim colors by gem tier.
pub fn tier_rim(tier: usize) -> Color {
    match tier {
        0 => rgbf(0.50, 0.49, 0.47),
        1 => rgbf(0.75, 0.48, 0.22),
        2 => rgbf(0.78, 0.82, 0.88),
        3 => rgbf(0.98, 0.80, 0.28),
        _ => rgbf(0.70, 0.95, 1.0),
    }
}

/// Octagonal stone pedestal with a metal rim showing the tier.
pub fn pedestal(c: Vec2, cs: f32, rim: Color, time: f32, radiant: bool) {
    shadow(c + vec2(cs * 0.09, cs * 0.14), cs * 0.42, cs * 0.3, 0.38);
    let base = ngon(c, 8, cs * 0.42, cs * 0.42, PI / 8.0);
    faceted(c, &base, rgbf(0.26, 0.25, 0.24), 0.5, None);
    let top = ngon(c - vec2(0.0, cs * 0.03), 8, cs * 0.34, cs * 0.34, PI / 8.0);
    faceted(c - vec2(0.0, cs * 0.03), &top, rgbf(0.36, 0.35, 0.33), 0.35, None);
    outline(&top, (cs * 0.05).max(1.5), rim);
    if radiant {
        let a = 0.4 + 0.3 * (time * 3.0).sin();
        outline(&ngon(c - vec2(0.0, cs * 0.03), 8, cs * 0.41, cs * 0.41, PI / 8.0 + time * 0.5), 1.5, with_alpha(rim, a));
    }
}

/// The outline of each gem's cut, in unit coordinates (y down).
fn gem_cut(gem: usize) -> Vec<Vec2> {
    match gem {
        // Ruby: cushion (octagon)
        0 => (0..8).map(|k| { let a = PI / 8.0 + k as f32 * TAU / 8.0; vec2(a.cos(), a.sin()) * 0.95 }).collect(),
        // Sapphire: oval
        1 => (0..14).map(|k| { let a = k as f32 * TAU / 14.0; vec2(a.cos() * 0.78, a.sin() * 1.05) }).collect(),
        // Emerald: step-cut rectangle
        2 => vec![
            vec2(-0.55, -0.95), vec2(0.55, -0.95), vec2(0.78, -0.72), vec2(0.78, 0.72),
            vec2(0.55, 0.95), vec2(-0.55, 0.95), vec2(-0.78, 0.72), vec2(-0.78, -0.72),
        ],
        // Topaz: pear
        3 => vec![
            vec2(0.0, -1.1), vec2(0.42, -0.55), vec2(0.72, 0.05), vec2(0.75, 0.45), vec2(0.45, 0.85),
            vec2(0.0, 0.98), vec2(-0.45, 0.85), vec2(-0.75, 0.45), vec2(-0.72, 0.05), vec2(-0.42, -0.55),
        ],
        // Diamond (and anything else): round brilliant
        _ => (0..16).map(|k| { let a = k as f32 * TAU / 16.0; vec2(a.cos(), a.sin()) }).collect(),
    }
}

/// A single pointed crystal prism (for amethyst clusters and specials).
fn prism(c: Vec2, r: f32, angle: f32, col: Color) {
    let local = [vec2(-0.28, 0.55), vec2(-0.28, -0.45), vec2(0.0, -0.95), vec2(0.28, -0.45), vec2(0.28, 0.55)];
    let (s, co) = angle.sin_cos();
    let pts: Vec<Vec2> = local.iter().map(|p| c + vec2(p.x * co - p.y * s, p.x * s + p.y * co) * r).collect();
    let center = c + vec2(-s, co) * -0.1 * r;
    faceted(center, &pts, col, 0.6, None);
    outline(&pts, 1.0, with_alpha(scale(col, 0.4), 0.9));
    // Bright ridge
    let (top, mid) = (pts[2], (pts[0] + pts[4]) * 0.5);
    draw_line(top.x, top.y, mid.x, mid.y, 1.0, with_alpha(WHITE, 0.35));
}

/// A faceted gem of the given type at `c` with radius `r`.
pub fn gem(c: Vec2, r: f32, gem: usize, col: Color, tier: usize, time: f32, key: usize) {
    glow(c, r * (1.6 + tier as f32 * 0.15), col, 0.6 + tier as f32 * 0.35 + 0.15 * (time * 2.0 + key as f32).sin());
    if gem == 4 {
        // Amethyst: a cluster of crystals
        prism(c + vec2(-r * 0.4, r * 0.15), r * 0.75, -0.45, scale(col, 0.85));
        prism(c + vec2(r * 0.42, r * 0.18), r * 0.7, 0.5, scale(col, 0.9));
        prism(c + vec2(0.0, r * 0.05), r * 1.05, 0.0, col);
    } else {
        let pts: Vec<Vec2> = gem_cut(gem).iter().map(|p| c + *p * r).collect();
        let table = if gem == 2 { 0.62 } else { 0.5 };
        faceted(c, &pts, col, 0.55, Some(table));
        outline(&pts, 1.0, with_alpha(scale(col, 0.35), 0.9));
        if gem == 5 {
            // Brilliant cut star facets
            for (k, p) in pts.iter().enumerate().step_by(2) {
                let inner = c + (pts[k] - c) * 0.5;
                draw_line(inner.x, inner.y, p.x, p.y, 1.0, with_alpha(WHITE, 0.25));
            }
        }
    }
    // Specular highlight
    draw_circle(c.x - r * 0.3, c.y - r * 0.38, r * 0.16, with_alpha(WHITE, 0.75));
    // Occasional twinkle on higher tiers
    if tier >= 2 {
        let phase = (time * 0.8 + key as f32 * 0.37).fract();
        if phase < 0.12 {
            let k = 1.0 - (phase / 0.12 * 2.0 - 1.0).abs();
            let s = r * 0.7 * k;
            let p = c + vec2(r * 0.35, -r * 0.45);
            draw_line(p.x - s, p.y, p.x + s, p.y, 1.5, with_alpha(WHITE, k));
            draw_line(p.x, p.y - s, p.x, p.y + s, 1.5, with_alpha(WHITE, k));
        }
    }
}

/// A special tower: ornate pedestal with a floating, slowly spinning star crystal.
pub fn special(c: Vec2, cs: f32, col: Color, level: usize, time: f32, key: usize) {
    shadow(c + vec2(cs * 0.09, cs * 0.14), cs * 0.47, cs * 0.34, 0.4);
    let base = ngon(c, 8, cs * 0.47, cs * 0.47, PI / 8.0);
    faceted(c, &base, rgbf(0.20, 0.19, 0.22), 0.5, None);
    outline(&base, 2.0, rgbf(0.85, 0.70, 0.35));
    // Rotating rune ring
    let ring_r = cs * 0.36;
    for k in 0..8 {
        let a = time * 0.8 + k as f32 * TAU / 8.0;
        let p = c + vec2(a.cos(), a.sin()) * ring_r;
        draw_circle(p.x, p.y, cs * 0.035, with_alpha(col, 0.8));
    }
    let bob = (time * 2.0 + key as f32).sin() * cs * 0.05;
    let cc = c - vec2(0.0, cs * 0.12) + vec2(0.0, bob);
    glow(cc, cs * 0.75, col, 1.4);
    let spin = time * 0.7 + key as f32;
    let pts: Vec<Vec2> = (0..10)
        .map(|k| {
            let a = spin + k as f32 * TAU / 10.0;
            let r = if k % 2 == 0 { cs * 0.32 } else { cs * 0.17 };
            cc + vec2(a.cos() * r, a.sin() * r)
        })
        .collect();
    faceted(cc, &pts, col, 0.6, Some(0.45));
    outline(&pts, 1.0, with_alpha(scale(col, 0.4), 0.9));
    draw_circle(cc.x - cs * 0.06, cc.y - cs * 0.08, cs * 0.05, with_alpha(WHITE, 0.85));
    // Orbiting motes, one more per level
    for k in 0..(level + 1) {
        let a = -time * 1.6 + k as f32 * TAU / (level + 1) as f32;
        let p = cc + vec2(a.cos() * cs * 0.45, a.sin() * cs * 0.18);
        draw_circle(p.x, p.y, cs * 0.045, rgbf(1.0, 0.9, 0.55));
    }
}

// ---------- Checkpoints ----------

/// A stone circle with a rotating rune ring. kind: 0 = start portal, 1 = checkpoint, 2 = exit.
pub fn checkpoint(c: Vec2, cs: f32, kind: u8, col: Color, time: f32) {
    let r = cs * 0.85;
    shadow(c + vec2(cs * 0.06, cs * 0.1), r, r * 0.8, 0.3);
    let ring = ngon(c, 12, r, r, 0.0);
    faceted(c, &ring, rgbf(0.33, 0.32, 0.31), 0.4, None);
    let inner = ngon(c, 12, r * 0.72, r * 0.72, 0.0);
    fan(c, &inner, rgbf(0.14, 0.13, 0.14));
    for k in 0..12 {
        let a = time * if kind == 2 { -0.6 } else { 0.4 } + k as f32 * TAU / 12.0;
        let p = c + vec2(a.cos(), a.sin()) * r * 0.86;
        draw_circle(p.x, p.y, cs * 0.05, with_alpha(col, 0.55 + 0.45 * ((time * 3.0 + k as f32).sin() * 0.5 + 0.5)));
    }
    if kind != 1 {
        // Swirling portal
        for k in 0..3 {
            let rr = r * (0.62 - k as f32 * 0.17);
            let a = time * (1.5 + k as f32) * if kind == 2 { -1.0 } else { 1.0 };
            let pts = ngon(c, 6, rr, rr, a);
            outline(&pts, 2.0, with_alpha(col, 0.5 + k as f32 * 0.15));
        }
        glow(c, r, col, 1.2);
    } else {
        glow(c, r * 0.7, col, 0.6);
    }
}

// ---------- Creatures ----------

/// Local frame for drawing a creature facing `dir`, with unit size `s` (px).
struct Frame {
    p: Vec2,
    f: Vec2,
    side: Vec2,
    s: f32,
}

impl Frame {
    fn pt(&self, x: f32, y: f32) -> Vec2 {
        self.p + self.f * x * self.s + self.side * y * self.s
    }
    fn oval(&self, x: f32, y: f32, rx: f32, ry: f32, col: Color) {
        let c = self.pt(x, y);
        let pts: Vec<Vec2> = (0..14)
            .map(|k| {
                let a = k as f32 / 14.0 * TAU;
                self.pt(x + a.cos() * rx, y + a.sin() * ry)
            })
            .collect();
        fan(c, &pts, col);
    }
    fn shaded_oval(&self, x: f32, y: f32, rx: f32, ry: f32, col: Color) {
        let c = self.pt(x, y);
        let pts: Vec<Vec2> = (0..14)
            .map(|k| {
                let a = k as f32 / 14.0 * TAU;
                self.pt(x + a.cos() * rx, y + a.sin() * ry)
            })
            .collect();
        faceted(c, &pts, col, 0.45, None);
    }
    fn line(&self, a: (f32, f32), b: (f32, f32), w: f32, col: Color) {
        let (p, q) = (self.pt(a.0, a.1), self.pt(b.0, b.1));
        draw_line(p.x, p.y, q.x, q.y, w, col);
    }
    fn dot(&self, x: f32, y: f32, r: f32, col: Color) {
        let c = self.pt(x, y);
        draw_circle(c.x, c.y, r * self.s, col);
    }
    fn tri(&self, a: (f32, f32), b: (f32, f32), c: (f32, f32), col: Color) {
        draw_triangle(self.pt(a.0, a.1), self.pt(b.0, b.1), self.pt(c.0, c.1), col);
    }
}

/// Draws a creature. `size` is its radius in px, `age` drives animation.
pub fn creature(look: &str, p: Vec2, dir: Vec2, size: f32, col: Color, age: f32, air: bool) {
    let dir = if dir.length_squared() > 0.01 { dir.normalize() } else { vec2(1.0, 0.0) };
    let hover = if air { (age * 3.0).sin() * size * 0.12 - size * 0.5 } else { 0.0 };
    // Shadow
    if air {
        shadow(p + vec2(size * 0.5, size * 0.9), size * 0.6, size * 0.35, 0.25);
    } else {
        shadow(p + vec2(size * 0.12, size * 0.3), size * 0.85, size * 0.55, 0.32);
    }
    let fr = Frame { p: p + vec2(0.0, hover), f: dir, side: vec2(-dir.y, dir.x), s: size };
    let dark = scale(col, 0.55);
    let eye = rgbf(1.0, 0.95, 0.6);
    match look {
        "runner" => {
            for k in 0..4 {
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                let x = if k < 2 { 0.3 } else { -0.3 };
                let sw = (age * 22.0 + k as f32 * 1.7).sin() * 0.35;
                fr.line((x, 0.2 * side), (x + sw, 0.85 * side), size * 0.1, dark);
            }
            fr.line((-0.7, 0.0), (-1.25, (age * 10.0).sin() * 0.3), size * 0.12, dark);
            fr.shaded_oval(0.0, 0.0, 0.85, 0.38, col);
            fr.tri((0.65, -0.28), (1.2, 0.0), (0.65, 0.28), scale(col, 0.8));
            fr.dot(0.85, 0.1, 0.07, eye);
        }
        "brute" => {
            fr.shaded_oval(-0.1, 0.0, 0.85, 0.8, col);
            for k in -1..=1 {
                fr.tri((-0.3 + k as f32 * 0.25, -0.5), (-0.4 + k as f32 * 0.25, -1.0), (-0.1 + k as f32 * 0.25, -0.55), dark);
            }
            fr.shaded_oval(0.6, 0.0, 0.38, 0.42, scale(col, 0.8));
            fr.tri((0.8, 0.2), (1.25, 0.4), (0.85, 0.35), rgbf(0.95, 0.92, 0.8));
            fr.tri((0.8, -0.2), (1.25, -0.4), (0.85, -0.35), rgbf(0.95, 0.92, 0.8));
            fr.dot(0.8, 0.15, 0.07, eye);
            fr.dot(0.8, -0.15, 0.07, eye);
        }
        "armored" => {
            for k in 0..3 {
                let side = [-1.0, 0.0, 1.0][k];
                let sw = (age * 12.0 + k as f32).sin() * 0.2;
                fr.line((side * 0.3 + sw, 0.5), (side * 0.3 - sw, 0.9), size * 0.1, dark);
                fr.line((side * 0.3 - sw, -0.5), (side * 0.3 + sw, -0.9), size * 0.1, dark);
            }
            fr.shaded_oval(0.0, 0.0, 0.85, 0.65, scale(col, 0.7));
            for k in 0..3 {
                let x = -0.45 + k as f32 * 0.35;
                fr.shaded_oval(x, 0.0, 0.32, 0.6, scale(col, 0.9 + k as f32 * 0.1));
            }
            fr.shaded_oval(0.75, 0.0, 0.25, 0.3, dark);
            fr.dot(0.85, 0.1, 0.06, eye);
            fr.dot(0.85, -0.1, 0.06, eye);
        }
        "swarm" => {
            let flick = 0.35 + 0.35 * (age * 45.0).sin().abs();
            fr.oval(-0.1, 0.55, 0.45, 0.3, with_alpha(WHITE, flick));
            fr.oval(-0.1, -0.55, 0.45, 0.3, with_alpha(WHITE, flick));
            fr.shaded_oval(0.0, 0.0, 0.65, 0.42, col);
            fr.dot(0.5, 0.0, 0.12, eye);
        }
        "bat" => {
            let flap = (age * 11.0).sin();
            let span = 1.1 + flap * 0.35;
            let tip = -0.2 + flap * 0.3;
            for side in [-1.0f32, 1.0] {
                fr.tri((0.3, 0.0), (tip, span * side), (-0.4, 0.25 * side), dark);
                fr.tri((-0.4, 0.25 * side), (tip, span * side), (-0.5, span * 0.6 * side), scale(col, 0.75));
            }
            fr.shaded_oval(0.0, 0.0, 0.5, 0.32, col);
            fr.dot(0.35, 0.1, 0.06, rgbf(1.0, 0.3, 0.3));
            fr.dot(0.35, -0.1, 0.06, rgbf(1.0, 0.3, 0.3));
        }
        "golem" => {
            let stomp = (age * 4.0).sin();
            fr.shaded_oval(0.15 + stomp * 0.1, 0.75, 0.3, 0.22, scale(col, 0.7));
            fr.shaded_oval(0.15 - stomp * 0.1, -0.75, 0.3, 0.22, scale(col, 0.7));
            let pts: Vec<Vec2> = (0..7)
                .map(|k| {
                    let a = k as f32 / 7.0 * TAU;
                    let r = 0.75 + 0.12 * ((k * 3) % 4) as f32 / 3.0;
                    fr.pt(a.cos() * r * 0.9, a.sin() * r)
                })
                .collect();
            faceted(fr.pt(-0.1, 0.0), &pts, col, 0.6, None);
            outline(&pts, 1.5, with_alpha(BLACK, 0.4));
            let glow_c = rgbf(1.0, 0.55, 0.15);
            fr.dot(0.5, 0.22, 0.1, glow_c);
            fr.dot(0.5, -0.22, 0.1, glow_c);
            glow(fr.pt(0.5, 0.0), size * 0.5, glow_c, 0.5);
        }
        "wraith" => {
            // Tattered cloak streaming behind
            for k in 0..5 {
                let y = -0.6 + k as f32 * 0.3;
                let wave = (age * 6.0 + k as f32).sin() * 0.2;
                fr.tri((0.2, y * 0.6), (-1.3 + wave, y * 1.3), (0.2, y * 0.6 + 0.3), with_alpha(col, 0.5 - k as f32 * 0.04));
            }
            fr.shaded_oval(0.0, 0.0, 0.65, 0.6, scale(col, 0.6));
            fr.shaded_oval(0.35, 0.0, 0.4, 0.42, rgbf(0.08, 0.1, 0.16));
            let e = rgbf(0.6, 0.95, 1.0);
            fr.dot(0.55, 0.14, 0.08, e);
            fr.dot(0.55, -0.14, 0.08, e);
            glow(fr.pt(0.5, 0.0), size * 0.6, e, 0.6);
        }
        "hydra" => {
            fr.shaded_oval(-0.2, 0.0, 0.8, 0.7, col);
            for k in 0..3 {
                let y = (k as f32 - 1.0) * 0.45;
                let sway = (age * 2.5 + k as f32 * 2.0).sin() * 0.25;
                let head = (1.05, y * 1.4 + sway);
                fr.line((0.3, y * 0.6), head, size * 0.22, scale(col, 0.85));
                fr.shaded_oval(head.0, head.1, 0.25, 0.2, scale(col, 0.95));
                fr.dot(head.0 + 0.12, head.1, 0.06, eye);
            }
        }
        "hydraling" => {
            for k in (0..4).rev() {
                let x = -k as f32 * 0.4;
                let y = (age * 9.0 - k as f32 * 0.9).sin() * 0.3;
                fr.shaded_oval(x, y, 0.4 - k as f32 * 0.05, 0.35 - k as f32 * 0.05, scale(col, 1.0 - k as f32 * 0.08));
            }
            fr.dot(0.2, 0.1, 0.07, eye);
            fr.dot(0.2, -0.1, 0.07, eye);
        }
        "leviathan" => {
            for k in (0..6).rev() {
                let x = 0.2 - k as f32 * 0.38;
                let y = (age * 3.0 - k as f32 * 0.8).sin() * 0.3;
                let r = 0.62 - k as f32 * 0.07;
                fr.tri((x, y + r), (x - 0.3, y + r + 0.35), (x - 0.25, y + r * 0.5), scale(col, 0.6));
                fr.tri((x, y - r), (x - 0.3, y - r - 0.35), (x - 0.25, y - r * 0.5), scale(col, 0.6));
                fr.shaded_oval(x, y, r, r, scale(col, 1.0 - k as f32 * 0.06));
            }
            fr.shaded_oval(0.55, 0.0, 0.5, 0.45, col);
            let e = rgbf(1.0, 0.9, 0.4);
            fr.dot(0.8, 0.2, 0.08, e);
            fr.dot(0.8, -0.2, 0.08, e);
        }
        _ => {
            // Beetle (default)
            for k in 0..3 {
                let x = -0.35 + k as f32 * 0.35;
                let sw = (age * 14.0 + k as f32 * 2.1).sin() * 0.22;
                fr.line((x, 0.4), (x + sw, 0.95), size * 0.1, dark);
                fr.line((x, -0.4), (x - sw, -0.95), size * 0.1, dark);
            }
            fr.shaded_oval(-0.05, 0.0, 0.72, 0.6, col);
            fr.line((-0.75, 0.0), (0.45, 0.0), 1.0, with_alpha(BLACK, 0.4));
            fr.shaded_oval(0.7, 0.0, 0.3, 0.32, dark);
            fr.dot(0.85, 0.13, 0.06, eye);
            fr.dot(0.85, -0.13, 0.06, eye);
        }
    }
}
