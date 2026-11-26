//! Procedural low-poly meshes: gems, pedestals, boulders, trees, creatures.
//! Everything is flat shaded (each triangle has its own normal) so facets catch the light,
//! and colored per vertex so one material can serve many props.

use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

pub fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^ (h >> 15)
}

/// Deterministic pseudo-random number in [0, 1).
pub fn rnd(x: i32, y: i32, seed: u32) -> f32 {
    (hash(x, y, seed) & 0xffff) as f32 / 65536.0
}

/// sRGB color to the linear RGBA that vertex colors use.
pub fn lin(c: [f32; 3]) -> [f32; 4] {
    Color::srgb(c[0], c[1], c[2]).to_linear().to_f32_array()
}

pub fn shade(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

pub fn mixc(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

/// Flat-shaded triangle soup builder.
#[derive(Default)]
pub struct Mb {
    p: Vec<[f32; 3]>,
    n: Vec<[f32; 3]>,
    c: Vec<[f32; 4]>,
}

impl Mb {
    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: [f32; 4]) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        for v in [a, b, c] {
            self.p.push(v.to_array());
            self.n.push(n.to_array());
            self.c.push(col);
        }
    }

    /// Triangle wound so it faces away from `center` (for convex or star-shaped solids).
    pub fn tri_out(&mut self, a: Vec3, b: Vec3, c: Vec3, center: Vec3, col: [f32; 4]) {
        let n = (b - a).cross(c - a);
        if n.dot((a + b + c) / 3.0 - center) < 0.0 {
            self.tri(a, c, b, col);
        } else {
            self.tri(a, b, c, col);
        }
    }

    pub fn quad_out(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, center: Vec3, col: [f32; 4]) {
        self.tri_out(a, b, c, center, col);
        self.tri_out(a, c, d, center, col);
    }

    /// Triangle that faces the given direction.
    pub fn tri_facing(&mut self, a: Vec3, b: Vec3, c: Vec3, dir: Vec3, col: [f32; 4]) {
        let n = (b - a).cross(c - a);
        if n.dot(dir) < 0.0 {
            self.tri(a, c, b, col);
        } else {
            self.tri(a, b, c, col);
        }
    }

    pub fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.p)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.n)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.c)
    }

    /// A tapered n-sided prism from `a` to `b` (trunks, crystals, legs). `tip` > 0 adds a
    /// pyramid point beyond `b`.
    #[allow(clippy::too_many_arguments)]
    pub fn prism(&mut self, a: Vec3, b: Vec3, r0: f32, r1: f32, sides: usize, tip: f32, col: [f32; 4], seed: u32) {
        let axis = (b - a).normalize_or(Vec3::Y);
        let side = if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
        let u = axis.cross(side).normalize();
        let v = axis.cross(u);
        let center = (a + b) / 2.0;
        let ring = |c: Vec3, r: f32, i: usize| {
            let t = i as f32 / sides as f32 * TAU;
            c + (u * t.cos() + v * t.sin()) * r
        };
        let apex = b + axis * tip;
        for i in 0..sides {
            let j = (i + 1) % sides;
            let k = 0.8 + 0.35 * rnd(i as i32, 7, seed);
            let (a0, a1, b0, b1) = (ring(a, r0, i), ring(a, r0, j), ring(b, r1, i), ring(b, r1, j));
            self.quad_out(a0, a1, b1, b0, center, shade(col, k));
            if tip > 0.0 {
                self.tri_out(b0, b1, apex, center, shade(col, k * 1.1));
            } else {
                self.tri_out(b0, b1, b, center, shade(col, k * 1.05));
            }
            self.tri_out(a0, a1, a, center, shade(col, 0.6));
        }
    }

    /// A lumpy low-poly blob (boulders, canopies, creature bodies).
    pub fn blob(&mut self, c: Vec3, r: Vec3, lump: f32, col: [f32; 4], seed: u32) {
        let (verts, faces) = icosphere(1);
        let pts: Vec<Vec3> = verts
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let k = 1.0 + lump * (rnd(i as i32, 3, seed) - 0.5) * 2.0;
                c + *v * r * k
            })
            .collect();
        for (fi, f) in faces.iter().enumerate() {
            let k = 0.82 + 0.3 * rnd(fi as i32, 11, seed);
            // Faces pointing up are lit by the sky; darken the undersides a little.
            let up = ((verts[f[0]] + verts[f[1]] + verts[f[2]]).normalize().y * 0.5 + 0.5) * 0.25 + 0.8;
            self.tri_out(pts[f[0]], pts[f[1]], pts[f[2]], c, shade(col, k * up));
        }
    }
}

/// Unit icosphere (vertices, triangle indices).
pub fn icosphere(subdiv: usize) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let mut v: Vec<Vec3> = [
        (-1.0, t, 0.0), (1.0, t, 0.0), (-1.0, -t, 0.0), (1.0, -t, 0.0),
        (0.0, -1.0, t), (0.0, 1.0, t), (0.0, -1.0, -t), (0.0, 1.0, -t),
        (t, 0.0, -1.0), (t, 0.0, 1.0), (-t, 0.0, -1.0), (-t, 0.0, 1.0),
    ]
    .iter()
    .map(|&(x, y, z)| Vec3::new(x, y, z).normalize())
    .collect();
    let mut f: Vec<[usize; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2],
        [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5],
        [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    for _ in 0..subdiv {
        let mut cache = std::collections::HashMap::new();
        let mut mid = |a: usize, b: usize, v: &mut Vec<Vec3>| {
            *cache.entry((a.min(b), a.max(b))).or_insert_with(|| {
                v.push(((v[a] + v[b]) / 2.0).normalize());
                v.len() - 1
            })
        };
        let mut nf = Vec::new();
        for [a, b, c] in f {
            let ab = mid(a, b, &mut v);
            let bc = mid(b, c, &mut v);
            let ca = mid(c, a, &mut v);
            nf.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        f = nf;
    }
    (v, f)
}

// ---------------------------------------------------------------------------------------------
// Gems

/// Gem cut, by gem name. Girdle radius is about 1; callers scale.
pub fn gem(name: &str) -> Mesh {
    let mut mb = Mb::default();
    let white = [1.0, 1.0, 1.0, 1.0];
    let n = |k: usize| (0..k).map(move |i| i as f32 / k as f32 * TAU);
    match name {
        "Ruby" => {
            // Cushion: a rounded square.
            let g: Vec<Vec2> = n(16)
                .map(|t| {
                    let (c, s) = (t.cos(), t.sin());
                    let r = (c.abs().powf(3.0) + s.abs().powf(3.0)).powf(-1.0 / 3.0);
                    Vec2::new(c, s) * r * 0.9
                })
                .collect();
            cut(&mut mb, &g, 0.55, 0.42, 0.95, true, white, 1);
        }
        "Sapphire" => {
            let g: Vec<Vec2> = n(14).map(|t| Vec2::new(t.cos(), t.sin() * 0.72)).collect();
            cut(&mut mb, &g, 0.5, 0.4, 0.9, true, white, 2);
        }
        "Emerald" => {
            // Step cut: a rectangle with clipped corners.
            let (w, h, k) = (1.0, 0.68, 0.24);
            let g = vec![
                Vec2::new(w, h - k), Vec2::new(w - k, h), Vec2::new(-w + k, h), Vec2::new(-w, h - k),
                Vec2::new(-w, -h + k), Vec2::new(-w + k, -h), Vec2::new(w - k, -h), Vec2::new(w, -h + k),
            ];
            cut(&mut mb, &g, 0.62, 0.32, 0.7, false, white, 3);
        }
        "Topaz" => {
            // Pear: narrow at +x.
            let g: Vec<Vec2> = n(16).map(|t| Vec2::new(t.cos(), t.sin() * (0.72 - 0.3 * t.cos()))).collect();
            cut(&mut mb, &g, 0.5, 0.4, 0.95, true, white, 4);
        }
        "Amethyst" => {
            // A cluster of pointed crystals.
            let base = Vec3::new(0.0, -0.7, 0.0);
            let dirs = [(0.0, 0.0, 1.2), (0.55, 0.3, 0.85), (-0.5, 0.45, 0.8), (0.1, -0.6, 0.75), (-0.35, -0.35, 0.6)];
            for (i, &(dx, dz, len)) in dirs.iter().enumerate() {
                let d = Vec3::new(dx, 1.0, dz).normalize();
                let r = if i == 0 { 0.34 } else { 0.24 };
                mb.prism(base + d * 0.1, base + d * len * 1.25, r, r * 0.95, 6, r * 1.3, white, 20 + i as u32);
            }
        }
        _ => {
            // Diamond: round brilliant with a tall crown.
            let g: Vec<Vec2> = n(16).map(|t| Vec2::new(t.cos(), t.sin())).collect();
            cut(&mut mb, &g, 0.55, 0.5, 1.05, true, white, 5);
        }
    }
    mb.build()
}

/// Builds a faceted gem from a convex girdle outline.
#[allow(clippy::too_many_arguments)]
fn cut(mb: &mut Mb, girdle: &[Vec2], table: f32, crown: f32, pavilion: f32, brilliant: bool, col: [f32; 4], seed: u32) {
    let k = girdle.len();
    let at = |p: Vec2, y: f32| Vec3::new(p.x, y, p.y);
    let center = Vec3::new(0.0, 0.0, 0.0);
    let jitter = |i: usize, salt: i32| shade(col, 0.5 + 0.85 * rnd(i as i32, salt, seed));
    let g: Vec<Vec3> = girdle.iter().map(|&p| at(p, 0.0)).collect();
    let top = Vec3::new(0.0, crown, 0.0);
    let culet = Vec3::new(0.0, -pavilion, 0.0);
    // The thin vertical girdle band.
    let band = 0.05;
    let gt: Vec<Vec3> = g.iter().map(|&p| p + Vec3::Y * band).collect();
    for i in 0..k {
        let j = (i + 1) % k;
        mb.quad_out(g[i], g[j], gt[j], gt[i], center, jitter(i, 9));
    }
    if brilliant {
        let t: Vec<Vec3> = (0..k).map(|i| at((girdle[i] + girdle[(i + 1) % k]) / 2.0 * table, crown)).collect();
        for i in 0..k {
            let j = (i + 1) % k;
            let h = (i + k - 1) % k;
            mb.tri_out(gt[i], gt[j], t[i], center, jitter(i, 1));
            mb.tri_out(t[h], gt[i], t[i], center, jitter(i, 2));
            mb.tri_out(t[i], t[j], top, center, shade(jitter(i, 3), 1.1));
        }
        let p: Vec<Vec3> = (0..k).map(|i| at((girdle[i] + girdle[(i + 1) % k]) / 2.0 * 0.55, -pavilion * 0.55)).collect();
        for i in 0..k {
            let j = (i + 1) % k;
            let h = (i + k - 1) % k;
            mb.tri_out(g[i], g[j], p[i], center, jitter(i, 4));
            mb.tri_out(p[h], g[i], p[i], center, jitter(i, 5));
            mb.tri_out(p[i], p[j], culet, center, jitter(i, 6));
        }
    } else {
        let t: Vec<Vec3> = girdle.iter().map(|&q| at(q * table, crown)).collect();
        let m: Vec<Vec3> = girdle.iter().map(|&q| at(q * 0.82, crown * 0.55 + band)).collect();
        let p: Vec<Vec3> = girdle.iter().map(|&q| at(q * 0.6, -pavilion * 0.5)).collect();
        let keel: Vec<Vec3> = girdle.iter().map(|&q| at(Vec2::new(q.x * 0.35, 0.0), -pavilion)).collect();
        for i in 0..k {
            let j = (i + 1) % k;
            mb.quad_out(gt[i], gt[j], m[j], m[i], center, jitter(i, 1));
            mb.quad_out(m[i], m[j], t[j], t[i], center, jitter(i, 2));
            mb.quad_out(g[i], g[j], p[j], p[i], center, jitter(i, 4));
            mb.quad_out(p[i], p[j], keel[j], keel[i], center, jitter(i, 5));
        }
        // Table and keel caps.
        for i in 1..k - 1 {
            mb.tri_facing(t[0], t[i], t[i + 1], Vec3::Y, shade(col, 1.1));
            mb.tri_facing(keel[0], keel[i], keel[i + 1], Vec3::NEG_Y, shade(col, 0.8));
        }
    }
}

/// The floating crystal of a special tower: a long hexagonal bipyramid with shards.
pub fn special_crystal() -> Mesh {
    let mut mb = Mb::default();
    let white = [1.0, 1.0, 1.0, 1.0];
    let c = Vec3::ZERO;
    let top = Vec3::new(0.0, 1.0, 0.0);
    let bot = Vec3::new(0.0, -0.75, 0.0);
    let ring: Vec<Vec3> = (0..6).map(|i| {
        let t = i as f32 / 6.0 * TAU;
        Vec3::new(t.cos() * 0.38, 0.05, t.sin() * 0.38)
    }).collect();
    let ring2: Vec<Vec3> = ring.iter().map(|p| *p + Vec3::Y * 0.35).collect();
    for i in 0..6 {
        let j = (i + 1) % 6;
        let k = 0.75 + 0.45 * rnd(i as i32, 1, 9);
        mb.quad_out(ring[i], ring[j], ring2[j], ring2[i], c, shade(white, k));
        mb.tri_out(ring2[i], ring2[j], top, c, shade(white, k * 1.1));
        mb.tri_out(ring[i], ring[j], bot, c, shade(white, k * 0.85));
    }
    mb.build()
}

pub fn shard() -> Mesh {
    let mut mb = Mb::default();
    mb.prism(Vec3::new(0.0, -0.5, 0.0), Vec3::new(0.0, 0.3, 0.0), 0.16, 0.14, 5, 0.3, [1.0; 4], 77);
    mb.build()
}

// ---------------------------------------------------------------------------------------------
// Stonework

fn ring_pts(r: f32, y: f32, sides: usize, rot: f32) -> Vec<Vec3> {
    (0..sides).map(|i| {
        let t = i as f32 / sides as f32 * TAU + rot;
        Vec3::new(t.cos() * r, y, t.sin() * r)
    }).collect()
}

/// Stacked octagonal frustums: [(radius, y)] from bottom to top, capped.
fn stack(mb: &mut Mb, profile: &[(f32, f32)], sides: usize, col: [f32; 4], seed: u32) {
    let rot = PI / sides as f32;
    let rings: Vec<Vec<Vec3>> = profile.iter().map(|&(r, y)| ring_pts(r, y, sides, rot)).collect();
    let center = Vec3::new(0.0, profile[0].1 - 10.0, 0.0);
    for l in 0..rings.len() - 1 {
        for i in 0..sides {
            let j = (i + 1) % sides;
            let k = 0.78 + 0.3 * rnd(i as i32, l as i32, seed);
            let (a, b, c, d) = (rings[l][i], rings[l][j], rings[l + 1][j], rings[l + 1][i]);
            // Each face gets its own outward direction (the profile isn't convex).
            let mid = (a + b) / 2.0;
            let out = Vec3::new(mid.x, 0.0, mid.z) + Vec3::Y * (profile[l].0 - profile[l + 1].0).max(0.0) * 4.0;
            mb.tri_facing(a, b, c, out, shade(col, k));
            mb.tri_facing(a, c, d, out, shade(col, k));
        }
    }
    let _ = center;
    let top = rings.last().unwrap();
    let y = profile.last().unwrap().1;
    for i in 0..sides {
        let j = (i + 1) % sides;
        mb.tri_facing(top[i], top[j], Vec3::new(0.0, y, 0.0), Vec3::Y, shade(col, 1.02 - 0.06 * rnd(i as i32, 99, seed)));
    }
}

/// A gem tower's stone pedestal. Top surface at y = 0.24.
pub fn pedestal() -> Mesh {
    let mut mb = Mb::default();
    let stone = lin([0.55, 0.52, 0.48]);
    stack(&mut mb, &[(0.47, -0.05), (0.47, 0.05), (0.42, 0.08), (0.36, 0.1), (0.36, 0.2), (0.32, 0.24)], 8, stone, 3);
    mb.build()
}

/// The metal band on a pedestal; its material shows the tier.
pub fn rim() -> Mesh {
    let mut mb = Mb::default();
    let white = [1.0; 4];
    let sides = 8;
    let rot = PI / sides as f32;
    let (lo, hi, ro, ri) = (0.17, 0.215, 0.385, 0.3);
    let ob = ring_pts(ro, lo, sides, rot);
    let ot = ring_pts(ro, hi, sides, rot);
    let it = ring_pts(ri, hi + 0.01, sides, rot);
    for i in 0..sides {
        let j = (i + 1) % sides;
        let out = Vec3::new(ob[i].x + ob[j].x, 0.0, ob[i].z + ob[j].z);
        mb.tri_facing(ob[i], ob[j], ot[j], out, white);
        mb.tri_facing(ob[i], ot[j], ot[i], out, white);
        mb.tri_facing(ot[i], ot[j], it[j], Vec3::Y, white);
        mb.tri_facing(ot[i], it[j], it[i], Vec3::Y, white);
    }
    mb.build()
}

/// A special tower's taller, ornate base. Top at y = 0.42.
pub fn special_base() -> Mesh {
    let mut mb = Mb::default();
    let stone = lin([0.36, 0.34, 0.38]);
    stack(&mut mb, &[(0.49, -0.05), (0.49, 0.07), (0.43, 0.1), (0.3, 0.16), (0.27, 0.34), (0.37, 0.38), (0.37, 0.42), (0.3, 0.44)], 6, stone, 5);
    // Four small horns of stone around the top.
    for i in 0..3 {
        let t = i as f32 / 3.0 * TAU + 0.5;
        let p = Vec3::new(t.cos() * 0.33, 0.4, t.sin() * 0.33);
        mb.prism(p, p + Vec3::new(t.cos() * 0.05, 0.16, t.sin() * 0.05), 0.06, 0.04, 4, 0.08, shade(stone, 0.9), 40 + i);
    }
    mb.build()
}

/// A lumpy boulder (the rocks left by unkept gems, cliff stones).
pub fn boulder(seed: u32, col: [f32; 3]) -> Mesh {
    let mut mb = Mb::default();
    let c = lin(col);
    mb.blob(Vec3::new(0.0, 0.16, 0.0), Vec3::new(0.42, 0.32, 0.4), 0.22, c, seed);
    // A smaller stone leaning on it.
    let a = rnd(1, 1, seed) * TAU;
    mb.blob(Vec3::new(a.cos() * 0.3, 0.08, a.sin() * 0.3), Vec3::new(0.17, 0.14, 0.17), 0.25, shade(c, 0.9), seed + 7);
    mb.build()
}

/// A standing stone for checkpoint circles.
pub fn standing_stone(seed: u32) -> Mesh {
    let mut mb = Mb::default();
    mb.prism(Vec3::ZERO, Vec3::new(0.0, 0.42, 0.0), 0.08, 0.06, 5, 0.06, lin([0.55, 0.53, 0.5]), seed);
    mb.build()
}

/// A flat annulus (portal swirls, rune circles), facing up.
pub fn annulus(r_in: f32, r_out: f32, segments: usize, gaps: bool) -> Mesh {
    let mut mb = Mb::default();
    for i in 0..segments {
        if gaps && i % 3 == 2 {
            continue;
        }
        let a = i as f32 / segments as f32 * TAU;
        let b = (i + 1) as f32 / segments as f32 * TAU;
        let p = |r: f32, t: f32| Vec3::new(t.cos() * r, 0.0, t.sin() * r);
        mb.tri_facing(p(r_in, a), p(r_out, a), p(r_out, b), Vec3::Y, [1.0; 4]);
        mb.tri_facing(p(r_in, a), p(r_out, b), p(r_in, b), Vec3::Y, [1.0; 4]);
    }
    mb.build()
}

// ---------------------------------------------------------------------------------------------
// Vegetation

#[derive(Clone, Copy)]
pub enum TreeKind {
    Broadleaf([f32; 3]),
    Pine { snowy: bool },
    Dead,
    Reeds,
}

pub fn tree(kind: TreeKind, seed: u32) -> Mesh {
    let mut mb = Mb::default();
    let bark = lin([0.33, 0.24, 0.16]);
    let s = 0.8 + 0.5 * rnd(0, 0, seed);
    match kind {
        TreeKind::Broadleaf(leaf) => {
            let h = 1.1 * s;
            mb.prism(Vec3::ZERO, Vec3::new(0.0, h, 0.0), 0.12 * s, 0.08 * s, 5, 0.0, bark, seed);
            let leaf = lin(leaf);
            for i in 0..4 {
                let a = rnd(i, 1, seed) * TAU;
                let d = if i == 0 { 0.0 } else { 0.35 * s };
                let c = Vec3::new(a.cos() * d, h + 0.2 * s + rnd(i, 2, seed) * 0.4 * s, a.sin() * d);
                let r = (0.55 - 0.08 * i as f32) * s;
                let tint = 0.85 + 0.3 * rnd(i, 3, seed);
                mb.blob(c, Vec3::new(r, r * 0.85, r), 0.18, shade(leaf, tint), seed * 7 + i as u32);
            }
        }
        TreeKind::Pine { snowy } => {
            let h = 2.0 * s;
            mb.prism(Vec3::ZERO, Vec3::new(0.0, 0.5 * s, 0.0), 0.1 * s, 0.08 * s, 5, 0.0, bark, seed);
            let needles = lin(if snowy { [0.18, 0.3, 0.26] } else { [0.16, 0.32, 0.18] });
            let snow = lin([0.92, 0.95, 1.0]);
            for l in 0..3 {
                let y0 = 0.35 * s + l as f32 * 0.45 * s;
                let r = (0.75 - l as f32 * 0.18) * s;
                let tip = (h - y0).min(0.9 * s);
                let sides = 7;
                let apex = Vec3::new(0.0, y0 + tip, 0.0);
                let c = Vec3::new(0.0, y0 + tip * 0.3, 0.0);
                for i in 0..sides {
                    let a = i as f32 / sides as f32 * TAU;
                    let b = (i + 1) as f32 / sides as f32 * TAU;
                    let p = |t: f32| Vec3::new(t.cos() * r, y0, t.sin() * r);
                    let k = 0.8 + 0.3 * rnd(i, l, seed);
                    let col = if snowy && (l == 2 || (i + l) % 3 == 0) { mixc(needles, snow, 0.75) } else { needles };
                    mb.tri_out(p(a), p(b), apex, c, shade(col, k));
                    mb.tri_facing(p(a), p(b), Vec3::new(0.0, y0, 0.0), Vec3::NEG_Y, shade(needles, 0.5));
                }
            }
        }
        TreeKind::Dead => {
            let h = 1.4 * s;
            let dark = lin([0.16, 0.13, 0.12]);
            mb.prism(Vec3::ZERO, Vec3::new(0.05, h, 0.0), 0.1 * s, 0.03 * s, 5, 0.0, dark, seed);
            for i in 0..4 {
                let a = rnd(i, 5, seed) * TAU;
                let y = h * (0.45 + 0.13 * i as f32);
                let from = Vec3::new(0.0, y, 0.0);
                let to = from + Vec3::new(a.cos() * 0.5 * s, 0.35 * s, a.sin() * 0.5 * s);
                mb.prism(from, to, 0.04 * s, 0.015 * s, 4, 0.0, dark, seed + i as u32);
            }
        }
        TreeKind::Reeds => {
            let green = lin([0.38, 0.45, 0.22]);
            let brown = lin([0.45, 0.33, 0.18]);
            for i in 0..9 {
                let a = rnd(i, 1, seed) * TAU;
                let d = rnd(i, 2, seed) * 0.3;
                let base = Vec3::new(a.cos() * d, 0.0, a.sin() * d);
                let top = base + Vec3::new(a.cos() * 0.12, (0.6 + rnd(i, 3, seed) * 0.5) * s, a.sin() * 0.12);
                mb.prism(base, top, 0.025, 0.01, 3, 0.0, green, seed + i as u32);
                if i % 3 == 0 {
                    mb.prism(top - Vec3::Y * 0.18, top, 0.04, 0.035, 4, 0.03, brown, seed + 50 + i as u32);
                }
            }
        }
    }
    mb.build()
}

/// A small tuft or bush for ground cover.
pub fn bush(col: [f32; 3], seed: u32) -> Mesh {
    let mut mb = Mb::default();
    let c = lin(col);
    for i in 0..3 {
        let a = rnd(i, 1, seed) * TAU;
        let p = Vec3::new(a.cos() * 0.15, 0.08, a.sin() * 0.15);
        mb.blob(p, Vec3::new(0.2, 0.16, 0.2), 0.2, shade(c, 0.85 + 0.25 * rnd(i, 2, seed)), seed + i as u32);
    }
    mb.build()
}

// ---------------------------------------------------------------------------------------------
// Creatures (placeholders until real models arrive). They face +Z.

pub struct CreatureMeshes {
    pub body: Mesh,
    pub head: Mesh,
    pub eye: Mesh,
    pub wing: Mesh,
    pub leg: Mesh,
    pub horn: Mesh,
}

pub fn creature_parts() -> CreatureMeshes {
    let white = [1.0; 4];
    let mut body = Mb::default();
    body.blob(Vec3::ZERO, Vec3::new(0.5, 0.42, 0.6), 0.12, white, 5);
    let mut head = Mb::default();
    head.blob(Vec3::ZERO, Vec3::new(0.3, 0.27, 0.3), 0.1, white, 9);
    let mut eye = Mb::default();
    eye.blob(Vec3::ZERO, Vec3::splat(0.07), 0.0, white, 1);
    let mut wing = Mb::default();
    // A bat-like wing in the XZ plane, extending to +X.
    let pts = [Vec3::new(0.0, 0.0, 0.15), Vec3::new(0.9, 0.05, 0.25), Vec3::new(0.75, 0.0, -0.2), Vec3::new(0.45, 0.0, -0.05), Vec3::new(0.0, 0.0, -0.2)];
    for i in 1..pts.len() - 1 {
        wing.tri_facing(pts[0], pts[i], pts[i + 1], Vec3::Y, white);
        wing.tri_facing(pts[0], pts[i], pts[i + 1], Vec3::NEG_Y, shade(white, 0.7));
    }
    let mut leg = Mb::default();
    leg.prism(Vec3::ZERO, Vec3::new(0.0, -0.3, 0.0), 0.07, 0.05, 4, 0.0, white, 3);
    let mut horn = Mb::default();
    horn.prism(Vec3::ZERO, Vec3::new(0.0, 0.25, 0.05), 0.06, 0.02, 4, 0.08, white, 8);
    CreatureMeshes { body: body.build(), head: head.build(), eye: eye.build(), wing: wing.build(), leg: leg.build(), horn: horn.build() }
}

/// Smooth indexed mesh from a height grid (terrain).
pub fn grid_mesh(positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, colors: Vec<[f32; 4]>, nx: usize, nz: usize) -> Mesh {
    let mut idx = Vec::with_capacity((nx - 1) * (nz - 1) * 6);
    for z in 0..nz - 1 {
        for x in 0..nx - 1 {
            let i = (z * nx + x) as u32;
            let n = nx as u32;
            // Counter-clockwise when seen from above (+Y), with +Z pointing towards the viewer.
            idx.extend([i, i + n, i + 1, i + 1, i + n, i + n + 1]);
        }
    }
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p[0] * 0.35, p[2] * 0.35]).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(idx))
}
