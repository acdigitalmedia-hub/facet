//! Grid pathfinding: A* with 8-directional movement (no corner cutting), chained
//! through every waypoint in order.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

pub type Cell = (i32, i32);

#[derive(Copy, Clone, PartialEq)]
struct Node {
    f: f32,
    idx: usize,
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

fn octile(a: Cell, b: Cell) -> f32 {
    let dx = (a.0 - b.0).abs() as f32;
    let dy = (a.1 - b.1).abs() as f32;
    dx.max(dy) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dy)
}

/// Shortest path from `a` to `b` (inclusive of both). `blocked(x,y)` reports impassable cells.
pub fn astar(w: i32, h: i32, a: Cell, b: Cell, blocked: &dyn Fn(i32, i32) -> bool) -> Option<Vec<Cell>> {
    let n = (w * h) as usize;
    let idx = |c: Cell| (c.1 * w + c.0) as usize;
    let mut g = vec![f32::INFINITY; n];
    let mut came = vec![usize::MAX; n];
    let mut closed = vec![false; n];
    let mut open = BinaryHeap::new();
    g[idx(a)] = 0.0;
    open.push(Node { f: octile(a, b), idx: idx(a) });

    let free = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && !blocked(x, y);

    while let Some(Node { idx: cur, .. }) = open.pop() {
        if closed[cur] {
            continue;
        }
        closed[cur] = true;
        let c = ((cur as i32) % w, (cur as i32) / w);
        if c == b {
            let mut out = vec![c];
            let mut k = cur;
            while came[k] != usize::MAX {
                k = came[k];
                out.push(((k as i32) % w, (k as i32) / w));
            }
            out.reverse();
            return Some(out);
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (c.0 + dx, c.1 + dy);
                if !free(nx, ny) {
                    continue;
                }
                // No squeezing diagonally between two blocked cells / around corners.
                if dx != 0 && dy != 0 && (!free(c.0 + dx, c.1) || !free(c.0, c.1 + dy)) {
                    continue;
                }
                let ni = idx((nx, ny));
                if closed[ni] {
                    continue;
                }
                let step = if dx != 0 && dy != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
                let ng = g[cur] + step;
                if ng < g[ni] {
                    g[ni] = ng;
                    came[ni] = cur;
                    open.push(Node { f: ng + octile((nx, ny), b), idx: ni });
                }
            }
        }
    }
    None
}

/// Full route through all waypoints, or None if any leg is blocked.
pub fn full_route(w: i32, h: i32, waypoints: &[Cell], blocked: &dyn Fn(i32, i32) -> bool) -> Option<Vec<Cell>> {
    let mut route: Vec<Cell> = Vec::new();
    for pair in waypoints.windows(2) {
        let seg = astar(w, h, pair[0], pair[1], blocked)?;
        if route.is_empty() {
            route.extend(seg);
        } else {
            route.extend(seg.into_iter().skip(1));
        }
    }
    Some(route)
}
