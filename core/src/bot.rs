//! A simple greedy player, used by the balance tests and the attract-mode demo.
//! Each gem goes where it lengthens the route most (while staying near the path); it forges
//! any recipe it can, otherwise keeps its strongest gem. Gold goes to quality, then specials.

use crate::game::{Game, Phase};
use crate::path::Cell;

/// Makes one decision if the game is waiting on the player (Build or Choose phase).
/// Returns true if it did something; in Wave phase the caller should keep stepping.
pub fn act(g: &mut Game) -> bool {
    match g.phase {
        Phase::Build => {
            if g.placed.is_empty() {
                spend(g);
            }
            match best_spot(g) {
                Some(c) => g.place(c),
                None => return false,
            }
            true
        }
        Phase::Choose => {
            if let Some((r, ings)) = g.available_recipes().into_iter().next() {
                // Put the special where the freshest ingredient was.
                let sel = ings.iter().copied().find(|&i| g.tower(i).is_some_and(|t| t.fresh)).unwrap_or(ings[0]);
                g.forge(r, sel);
                return true;
            }
            // Like a careful player: make sure something can see invisible enemies before they come.
            let has_detector = g.grid.iter().any(|o| matches!(o, crate::game::Occ::Tower(t) if !t.fresh && g.stats(t).detect));
            let need_detector = !has_detector && g.wave >= 5;
            let pick = g.placed.iter().copied().max_by(|&a, &b| {
                let s = |i| {
                    let st = g.stats(g.tower(i).unwrap());
                    st.damage + 1e6 * g.combine_steps(i) as f32 + if need_detector && st.detect { 1e9 } else { 0.0 }
                };
                s(a).total_cmp(&s(b))
            });
            g.selected = pick;
            let combine = pick.is_some_and(|p| g.combine_steps(p) > 0);
            g.keep(combine);
            true
        }
        _ => false,
    }
}

fn spend(g: &mut Game) {
    loop {
        if g.luck_cost().is_some_and(|c| g.gold >= c) {
            g.buy_luck();
            continue;
        }
        let up = (0..g.grid.len()).find(|&i| g.special_upgrade_cost(i).is_some_and(|c| g.gold >= c));
        match up {
            Some(i) if g.luck_cost().is_none() => g.upgrade_special(i),
            _ => break,
        }
    }
}

fn best_spot(g: &Game) -> Option<Cell> {
    let mut best: Option<(usize, Cell)> = None;
    for i in 0..g.grid.len() {
        let c = g.cell_of(i);
        if g.can_place(c).is_err() {
            continue;
        }
        if let Some(r) = g.route_with(Some(i)) {
            // Like a person would: only build where the path still passes nearby.
            let near = r.iter().any(|p| (p.0 - c.0).abs() <= 2 && (p.1 - c.1).abs() <= 2);
            if near && best.is_none_or(|(l, _)| r.len() > l) {
                best = Some((r.len(), c));
            }
        }
    }
    best.map(|b| b.1)
}
