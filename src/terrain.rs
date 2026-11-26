//! Terrain: a height-mapped ground mesh covering the board plus a margin of hills.
//! Grid cell (x, y) occupies world [x, x+1] × [y, y+1] on the XZ plane.

use bevy::prelude::*;
use facet_core::data::MapDef;
use facet_core::path::Cell;

use crate::mesh::{grid_mesh, hash, lin, mixc, shade};
use crate::theme::Theme;

/// World units of scenery beyond each board edge.
pub const MARGIN: f32 = 16.0;
/// Vertices per grid cell.
const RES: usize = 4;
/// Height of the water surface (below the board's ground level).
pub const WATER_Y: f32 = -0.14;

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let r = |a, b| (hash(a, b, seed) & 0xffff) as f32 / 65535.0;
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = r(xi, yi) + (r(xi + 1, yi) - r(xi, yi)) * sx;
    let bot = r(xi, yi + 1) + (r(xi + 1, yi + 1) - r(xi, yi + 1)) * sx;
    top + (bot - top) * sy
}

/// Fractal noise in [0, 1].
pub fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..4 {
        sum += value_noise(x * freq, y * freq, seed + o) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

pub struct Terrain {
    nx: usize,
    nz: usize,
    heights: Vec<f32>,
    colors: Vec<[f32; 4]>,
    w: f32,
    h: f32,
    dirt: [f32; 4],
    dirt_edge: [f32; 4],
    pub water: bool,
}

impl Terrain {
    fn pos(&self, i: usize, j: usize) -> (f32, f32) {
        (-MARGIN + i as f32 / RES as f32, -MARGIN + j as f32 / RES as f32)
    }

    pub fn new(map: &MapDef, th: &Theme) -> Terrain {
        let (w, h) = (map.w as f32, map.h as f32);
        let nx = ((w + 2.0 * MARGIN) as usize) * RES + 1;
        let nz = ((h + 2.0 * MARGIN) as usize) * RES + 1;
        let water = map.wall_style == "water";
        let mut t = Terrain {
            nx,
            nz,
            heights: vec![0.0; nx * nz],
            colors: vec![[0.0; 4]; nx * nz],
            w,
            h,
            dirt: lin(th.dirt),
            dirt_edge: lin(th.dirt_edge),
            water,
        };
        let seed = map.name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
        let wall = |x: f32, z: f32| x >= 0.0 && z >= 0.0 && x < w && z < h && map.is_wall(x.floor() as i32, z.floor() as i32);
        let (ground, alt, hill, rock, mud) = (lin(th.ground), lin(th.ground_alt), lin(th.hill), lin(th.rock), lin(th.mud));

        for j in 0..nz {
            for i in 0..nx {
                let (x, z) = t.pos(i, j);
                // Distance outside the board rectangle.
                let dx = (-x).max(x - w).max(0.0);
                let dz = (-z).max(z - h).max(0.0);
                let out = (dx * dx + dz * dz).sqrt();
                let n = fbm(x * 0.12, z * 0.12, seed);
                let detail = fbm(x * 0.9, z * 0.9, seed + 10);
                let mut y = (n - 0.5) * 0.1 + (detail - 0.5) * 0.03;
                let hill_t = smoothstep(0.8, 9.0, out);
                y += hill_t * (0.6 + n * 3.8 + fbm(x * 0.3, z * 0.3, seed + 20) * 1.2);

                // Wall coverage, softened over about a cell.
                let mut cov = 0.0;
                let spread = if water { 0.38 } else { 0.22 };
                if !map.walls.is_empty() {
                    let mut hits = 0;
                    for a in -2..=2 {
                        for b in -2..=2 {
                            if wall(x + a as f32 * spread, z + b as f32 * spread) {
                                hits += 1;
                            }
                        }
                    }
                    cov = hits as f32 / 25.0;
                }

                let grain = 0.9 + 0.18 * detail;
                let mut col = shade(mixc(ground, alt, smoothstep(0.35, 0.7, fbm(x * 0.25, z * 0.25, seed + 5))), grain);
                col = mixc(col, hill, hill_t);
                if water {
                    let deep = smoothstep(0.25, 0.8, cov);
                    y -= deep * 0.65;
                    col = mixc(col, mud, smoothstep(0.0, 0.35, cov));
                } else if cov > 0.0 {
                    let up = smoothstep(0.4, 0.8, cov);
                    y += up * (0.9 + fbm(x * 0.7, z * 0.7, seed + 30) * 0.9);
                    col = mixc(col, shade(rock, 0.85 + 0.3 * detail), smoothstep(0.3, 0.6, cov));
                }
                let k = j * nx + i;
                t.heights[k] = y;
                t.colors[k] = col;
            }
        }
        // Steep slopes show bare rock.
        for j in 1..nz - 1 {
            for i in 1..nx - 1 {
                let k = j * nx + i;
                let gx = (t.heights[k + 1] - t.heights[k - 1]) * RES as f32 / 2.0;
                let gz = (t.heights[k + nx] - t.heights[k - nx]) * RES as f32 / 2.0;
                let steep = smoothstep(0.7, 1.6, (gx * gx + gz * gz).sqrt());
                t.colors[k] = mixc(t.colors[k], shade(rock, 0.9), steep);
            }
        }
        t
    }

    /// Ground height at a world position (ignoring the road's slight dip).
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let fx = ((x + MARGIN) * RES as f32).clamp(0.0, (self.nx - 1) as f32 - 0.001);
        let fz = ((z + MARGIN) * RES as f32).clamp(0.0, (self.nz - 1) as f32 - 0.001);
        let (i, j) = (fx as usize, fz as usize);
        let (u, v) = (fx - i as f32, fz - j as f32);
        let hgt = |i: usize, j: usize| self.heights[j * self.nx + i];
        let a = hgt(i, j) + (hgt(i + 1, j) - hgt(i, j)) * u;
        let b = hgt(i, j + 1) + (hgt(i + 1, j + 1) - hgt(i, j + 1)) * u;
        a + (b - a) * v
    }

    /// The ground mesh with a dirt road worn along `route`.
    pub fn mesh(&self, road: &[Vec2]) -> Mesh {
        let (nx, nz) = (self.nx, self.nz);
        let mut heights = self.heights.clone();
        let mut colors = self.colors.clone();
        let pts = road;
        for j in 0..nz {
            for i in 0..nx {
                let (x, z) = self.pos(i, j);
                if x < -1.0 || z < -1.0 || x > self.w + 1.0 || z > self.h + 1.0 {
                    continue;
                }
                let p = Vec2::new(x, z);
                let mut d = f32::MAX;
                for s in pts.windows(2) {
                    let (a, b) = (s[0], s[1]);
                    if (p - a).length_squared() > 4.0 {
                        continue;
                    }
                    let ab = b - a;
                    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                    d = d.min((p - (a + ab * t)).length());
                }
                if d > 1.0 {
                    continue;
                }
                // The crisp road is a separate strip (`road_mesh`); the ground underneath just
                // gets a soft worn band and a slight dip.
                let wobble = (fbm(x * 1.7, z * 1.7, 99) - 0.5) * 0.14;
                let road = 1.0 - smoothstep(0.2, 0.5, d + wobble);
                let edge = 1.0 - smoothstep(0.35, 0.8, d + wobble);
                let k = j * nx + i;
                let grain = 0.88 + 0.2 * fbm(x * 3.0, z * 3.0, 7);
                colors[k] = mixc(colors[k], self.dirt_edge, edge * 0.55);
                colors[k] = mixc(colors[k], shade(self.dirt, grain), road * 0.6);
                heights[k] -= road * 0.035;
            }
        }
        let mut positions = Vec::with_capacity(nx * nz);
        let mut normals = Vec::with_capacity(nx * nz);
        let s = 1.0 / RES as f32;
        for j in 0..nz {
            for i in 0..nx {
                let (x, z) = self.pos(i, j);
                let k = j * nx + i;
                positions.push([x, heights[k], z]);
                let l = heights[j * nx + i.saturating_sub(1)];
                let r = heights[j * nx + (i + 1).min(nx - 1)];
                let d = heights[j.saturating_sub(1) * nx + i];
                let u = heights[(j + 1).min(nz - 1) * nx + i];
                normals.push(Vec3::new(l - r, 2.0 * s, d - u).normalize().to_array());
            }
        }
        grid_mesh(positions, normals, colors, nx, nz)
    }
}

/// The road as a smooth curve: the cell route is straightened where nothing is in the way
/// (never cutting past a checkpoint or through walls), its corners are rounded, and it is
/// resampled evenly.
pub fn smooth_road(map: &MapDef, cells: &[Cell]) -> Vec<Vec2> {
    smooth_path(map, cells, &|x, y| map.is_wall(x, y))
}

/// Like `smooth_road`, avoiding any cell `solid` reports (walls, towers, rocks).
pub fn smooth_path(map: &MapDef, cells: &[Cell], solid: &dyn Fn(i32, i32) -> bool) -> Vec<Vec2> {
    if cells.len() < 2 {
        return Vec::new();
    }
    let center = |c: Cell| Vec2::new(c.0 as f32 + 0.5, c.1 as f32 + 0.5);
    let blocked = |p: Vec2| {
        let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
        x < 0 || y < 0 || x >= map.w || y >= map.h || solid(x, y)
    };
    let clear = |a: Vec2, b: Vec2| {
        let d = b - a;
        let len = d.length();
        let n = Vec2::new(-d.y, d.x) / len.max(1e-4) * 0.45;
        let steps = (len / 0.2).ceil() as i32;
        (0..=steps).all(|k| {
            let p = a + d * (k as f32 / steps.max(1) as f32);
            !blocked(p + n) && !blocked(p - n) && !blocked(p)
        })
    };
    let is_checkpoint = |c: Cell| map.waypoints.contains(&c);

    // String pulling between checkpoints.
    let mut pts = vec![center(cells[0])];
    let mut i = 0;
    while i < cells.len() - 1 {
        let mut j = i + 1;
        let mut k = i + 1;
        while k < cells.len() && k - i <= 14 {
            if clear(center(cells[i]), center(cells[k])) {
                j = k;
            }
            if is_checkpoint(cells[k]) {
                break;
            }
            k += 1;
        }
        pts.push(center(cells[j]));
        i = j;
    }

    // Keep the rounding local to each corner: add points near both ends of long straights so
    // the curve hugs the corner (and its checkpoint) instead of sweeping wide.
    let mut tight = vec![pts[0]];
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        let r = 0.7f32.min(len * 0.45);
        if len > 2.0 * r + 0.05 {
            tight.push(a.lerp(b, r / len));
            tight.push(a.lerp(b, 1.0 - r / len));
        }
        tight.push(b);
    }
    pts = tight;

    // Chaikin corner cutting.
    for _ in 0..4 {
        let mut out = vec![pts[0]];
        for w in pts.windows(2) {
            out.push(w[0].lerp(w[1], 0.25));
            out.push(w[0].lerp(w[1], 0.75));
        }
        out.push(*pts.last().unwrap());
        pts = out;
    }

    // Even spacing.
    let mut out = vec![pts[0]];
    let step = 0.18;
    let mut carry = 0.0;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        let mut t = step - carry;
        while t < len {
            out.push(a.lerp(b, t / len));
            t += step;
        }
        carry = len - (t - step);
    }
    out.push(*pts.last().unwrap());
    out
}

impl Terrain {
    /// A strip mesh for the road surface: soft (transparent) edges, gently wobbling width,
    /// lying just above the ground.
    pub fn road_mesh(&self, road: &[Vec2]) -> Mesh {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology};
        let across: [(f32, f32); 6] = [(-0.5, 0.0), (-0.36, 0.85), (-0.12, 1.0), (0.12, 1.0), (0.36, 0.85), (0.5, 0.0)];
        let (mut pos, mut nrm, mut col, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let n = road.len();
        for i in 0..n {
            let prev = road[i.saturating_sub(1)];
            let next = road[(i + 1).min(n - 1)];
            let t = (next - prev).normalize_or(Vec2::X);
            let side = Vec2::new(-t.y, t.x);
            let p = road[i];
            let width = 1.0 + 0.16 * (fbm(p.x * 0.9, p.y * 0.9, 41) - 0.5) * 2.0;
            for (k, &(o, a)) in across.iter().enumerate() {
                let q = p + side * o * width;
                let y = self.height(q.x, q.y) + 0.03;
                pos.push([q.x, y, q.y]);
                nrm.push([0.0, 1.0, 0.0]);
                uv.push([q.x * 0.35, q.y * 0.35]);
                let grain = 0.86 + 0.24 * fbm(q.x * 2.5, q.y * 2.5, 17);
                // Worn, lighter tracks a little off-center.
                let rut = if k == 2 || k == 3 { 1.06 } else { 1.0 };
                let edge_mix = if a < 0.5 { 0.6 } else { 0.0 };
                let c = shade(mixc(self.dirt, self.dirt_edge, edge_mix), grain * rut);
                col.push([c[0], c[1], c[2], a]);
            }
        }
        let w = across.len() as u32;
        for i in 0..n.saturating_sub(1) as u32 {
            for k in 0..w - 1 {
                let a = i * w + k;
                let b = a + w;
                // Wind so the strip faces up whichever way the road turns.
                let (pa, pb, pc) = (Vec3::from(pos[a as usize]), Vec3::from(pos[b as usize]), Vec3::from(pos[(a + 1) as usize]));
                if (pb - pa).cross(pc - pa).y > 0.0 {
                    idx.extend([a, b, a + 1, a + 1, b, b + 1]);
                } else {
                    idx.extend([a, a + 1, b, a + 1, b + 1, b]);
                }
            }
        }
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_indices(Indices::U32(idx))
    }
}
