//! Game state and simulation. Everything here runs in fixed time steps and knows
//! nothing about drawing; positions are in grid units (cell (x,y) spans x..x+1).

use macroquad::prelude::*;

use crate::data::{Balance, MapDef, TowerDef};
use crate::path::{full_route, Cell};

pub const STEP: f32 = 1.0 / 60.0;
pub const AURA_RANGE: f32 = 3.0;
const PROJECTILE_SPEED: f32 = 14.0;
const SLOW_DURATION: f32 = 2.0;
const POISON_DURATION: f32 = 4.0;
const SHRED_DURATION: f32 = 5.0;
const CHAIN_RANGE: f32 = 2.5;
const SPLASH_FRACTION: f32 = 0.5;
const CRIT_MULT: f32 = 2.5;
const BOSS_GOLD_MULT: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Placing this round's gems.
    Build,
    /// All gems placed; pick one to keep (the rest become rocks).
    Choose,
    Wave,
    GameOver,
    Victory,
}

/// Things that happened during a step, for sound and visual effects. Positions are in grid units.
#[derive(Clone)]
pub enum Event {
    Place { pos: Vec2, gem: usize },
    Keep { pos: Vec2, color: Color },
    Combine { pos: Vec2, color: Color },
    Forge { pos: Vec2, color: Color },
    Upgrade { pos: Option<Vec2>, color: Color },
    Shot,
    Splash { pos: Vec2, color: Color, radius: f32 },
    Chain,
    Crit { pos: Vec2, color: Color },
    Death { pos: Vec2, color: Color, boss: bool },
    Leak { lives: i32 },
    WaveStart { boss: bool, name: String, number: u32 },
    WaveClear,
    Victory,
    Defeat,
    Error,
    RockRemoved { pos: Vec2 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Gem(usize),
    Special(usize),
}

#[derive(Clone)]
pub struct Tower {
    pub kind: Kind,
    /// Tier for gems, upgrade level for specials (both 0-based).
    pub rank: usize,
    pub cooldown: f32,
    /// Placed this round and not yet kept.
    pub fresh: bool,
    pub damage_dealt: f32,
    pub kills: u32,
    /// 1.0 right after firing, decays to 0 (for the firing pulse).
    pub flash: f32,
}

impl Tower {
    pub fn new(kind: Kind, rank: usize, fresh: bool) -> Tower {
        Tower { kind, rank, cooldown: 0.0, fresh, damage_dealt: 0.0, kills: 0, flash: 0.0 }
    }
    pub fn gem(&self) -> Option<usize> {
        match self.kind {
            Kind::Gem(g) => Some(g),
            Kind::Special(_) => None,
        }
    }
}

#[derive(Clone)]
pub enum Occ {
    Empty,
    Rock,
    /// Permanent map terrain (cliff or lake): impassable and unbuildable.
    Wall,
    Tower(Tower),
}

/// A tower's effective numbers after tier/level scaling.
pub struct Stats {
    pub damage: f32,
    pub cooldown: f32,
    pub range: f32,
    pub splash: f32,
    pub slow: f32,
    pub poison: f32,
    pub chain: usize,
    pub crit: f32,
    pub pierce: bool,
    pub shred: f32,
    pub aura: f32,
    pub air_mult: f32,
    pub ground_only: bool,
}

pub struct Creep {
    pub id: u64,
    pub kind: usize,
    pub air: bool,
    pub boss: bool,
    pub unslowable: bool,
    pub regen: f32,
    pub lives: i32,
    pub pos: Vec2,
    /// Unit vector of the current movement direction.
    pub dir: Vec2,
    /// Seconds alive, for animation.
    pub age: f32,
    /// Index of the cell (ground: route, air: waypoints) currently being walked towards.
    pub next: usize,
    pub hp: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub armor: f32,
    pub slow: f32,
    pub slow_t: f32,
    pub poison_dps: f32,
    pub poison_t: f32,
    pub poison_src: usize,
    pub shred: f32,
    pub shred_t: f32,
    pub gold: u32,
    pub traveled: f32,
    pub last_hit: Option<usize>,
    /// Hit flash timer.
    pub flash: f32,
}

pub struct Projectile {
    pub pos: Vec2,
    pub vel: Vec2,
    pub target: u64,
    pub src: usize,
    pub damage: f32,
    pub color: Color,
    pub size: f32,
}

pub enum FxKind {
    Beam(Vec2, Vec2),
    Ring(Vec2, f32),
    Text(Vec2, String),
}

pub struct Fx {
    pub kind: FxKind,
    pub color: Color,
    pub t: f32,
    pub life: f32,
}

pub struct WaveInfo {
    pub name: String,
    pub count: u32,
    pub hp: f32,
    pub speed: f32,
    pub armor: f32,
    pub gold: u32,
    pub air: bool,
    pub boss: bool,
    pub desc: String,
}

pub struct Game {
    pub bal: Balance,
    pub map: MapDef,
    /// Index into `bal.difficulties`.
    pub diff: usize,
    /// Playing past the main waves.
    pub endless: bool,
    pub phase: Phase,
    pub grid: Vec<Occ>,
    pub route: Vec<Cell>,
    pub creeps: Vec<Creep>,
    pub projectiles: Vec<Projectile>,
    pub fx: Vec<Fx>,
    pub lives: i32,
    pub gold: u32,
    pub luck: usize,
    /// 0-based index of the current (or next) wave.
    pub wave: u32,
    /// Cells holding this round's fresh gems, in placement order.
    pub placed: Vec<usize>,
    /// Selected cell (a tower or a rock).
    pub selected: Option<usize>,
    pub total_kills: u32,
    pub message: Option<(String, f32)>,
    /// Bumped whenever the board layout changes.
    pub rev: u64,
    /// Events since the last drain (sound/visual effects).
    pub events: Vec<Event>,
    spawn_left: u32,
    spawn_t: f32,
    next_id: u64,
}

pub fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], 255)
}

impl Game {
    pub fn new(bal: Balance, map: MapDef, diff: usize) -> Game {
        let diff = diff.min(bal.difficulties.len() - 1);
        let n = (map.w * map.h) as usize;
        let mut grid = vec![Occ::Empty; n];
        for y in 0..map.h {
            for x in 0..map.w {
                if map.is_wall(x, y) && !map.waypoints.contains(&(x, y)) {
                    grid[(y * map.w + x) as usize] = Occ::Wall;
                }
            }
        }
        let mut g = Game {
            map,
            phase: Phase::Build,
            grid,
            route: Vec::new(),
            creeps: Vec::new(),
            projectiles: Vec::new(),
            fx: Vec::new(),
            lives: bal.difficulties[diff].lives,
            diff,
            endless: false,
            gold: bal.start_gold,
            luck: 0,
            wave: 0,
            placed: Vec::new(),
            selected: None,
            total_kills: 0,
            message: None,
            rev: 0,
            events: Vec::new(),
            spawn_left: 0,
            spawn_t: 0.0,
            next_id: 1,
            bal,
        };
        g.route = g.route_with(None).expect("waypoints must be connected on an empty map");
        g
    }

    // ---- Grid helpers ----

    pub fn w(&self) -> i32 {
        self.map.w
    }
    pub fn h(&self) -> i32 {
        self.map.h
    }
    pub fn idx(&self, c: Cell) -> usize {
        (c.1 * self.w() + c.0) as usize
    }
    pub fn cell_of(&self, i: usize) -> Cell {
        (i as i32 % self.w(), i as i32 / self.w())
    }
    pub fn center(&self, i: usize) -> Vec2 {
        let c = self.cell_of(i);
        vec2(c.0 as f32 + 0.5, c.1 as f32 + 0.5)
    }
    pub fn in_bounds(&self, c: Cell) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.0 < self.w() && c.1 < self.h()
    }
    pub fn is_waypoint(&self, c: Cell) -> bool {
        self.map.waypoints.contains(&c)
    }
    pub fn tower(&self, i: usize) -> Option<&Tower> {
        match &self.grid[i] {
            Occ::Tower(t) => Some(t),
            _ => None,
        }
    }
    pub fn is_rock(&self, i: usize) -> bool {
        matches!(self.grid[i], Occ::Rock)
    }
    fn between_waves(&self) -> bool {
        matches!(self.phase, Phase::Build | Phase::Choose)
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), 2.5));
    }

    // ---- Tower info ----

    pub fn def(&self, kind: Kind) -> &TowerDef {
        match kind {
            Kind::Gem(g) => &self.bal.gems[g],
            Kind::Special(s) => &self.bal.specials[s],
        }
    }

    pub fn color(&self, kind: Kind) -> Color {
        rgb(self.def(kind).color)
    }

    pub fn tower_name(&self, t: &Tower) -> String {
        match t.kind {
            Kind::Gem(g) => format!("{} {}", self.bal.tier_names[t.rank], self.bal.gems[g].name),
            Kind::Special(s) => format!("{} Lv {}", self.bal.specials[s].name, t.rank + 1),
        }
    }

    pub fn stats(&self, t: &Tower) -> Stats {
        let d = self.def(t.kind);
        let mult = match t.kind {
            Kind::Gem(_) => self.bal.tier_damage_mult[t.rank],
            Kind::Special(_) => self.bal.special_level_mult[t.rank],
        };
        let r = t.rank as f32;
        let scaled = |base: f32, per: f32| if base > 0.0 { base + per * r } else { 0.0 };
        Stats {
            damage: d.damage * mult,
            cooldown: d.cooldown,
            range: d.range + self.bal.range_per_rank * r,
            splash: scaled(d.splash, 0.15),
            slow: scaled(d.slow, 0.07).min(0.7),
            poison: d.poison * mult,
            chain: scaled(d.chain, 1.0) as usize,
            crit: scaled(d.crit, 0.05).min(0.6),
            pierce: d.pierce,
            shred: scaled(d.shred, 1.0),
            aura: scaled(d.aura, 0.1),
            air_mult: d.air_mult,
            ground_only: d.ground_only,
        }
    }

    /// Total aura damage bonus that tower `i` receives from other towers.
    pub fn aura_bonus(&self, i: usize) -> f32 {
        let p = self.center(i);
        let mut bonus = 0.0;
        for (j, o) in self.grid.iter().enumerate() {
            if let Occ::Tower(t) = o {
                if j != i && self.def(t.kind).aura > 0.0 && self.center(j).distance(p) <= AURA_RANGE {
                    bonus += self.stats(t).aura;
                }
            }
        }
        bonus
    }

    // ---- Paths ----

    /// Route through all waypoints, optionally pretending `extra` is also blocked.
    pub fn route_with(&self, extra: Option<usize>) -> Option<Vec<Cell>> {
        let w = self.w();
        let blocked = |x: i32, y: i32| {
            let i = (y * w + x) as usize;
            Some(i) == extra || !matches!(self.grid[i], Occ::Empty)
        };
        full_route(w, self.h(), &self.map.waypoints, &blocked)
    }

    fn board_changed(&mut self) {
        self.route = self.route_with(None).expect("route must stay open");
        self.rev += 1;
    }

    // ---- Build phase ----

    pub fn can_place(&self, c: Cell) -> Result<(), &'static str> {
        if self.phase != Phase::Build {
            return Err("Not in build phase");
        }
        if !self.in_bounds(c) {
            return Err("Out of bounds");
        }
        if self.is_waypoint(c) {
            return Err("Can't build on a checkpoint");
        }
        let i = self.idx(c);
        if !matches!(self.grid[i], Occ::Empty) {
            return Err("Cell is occupied");
        }
        if self.route_with(Some(i)).is_none() {
            return Err("That would block the path");
        }
        Ok(())
    }

    fn roll_tier(&self) -> usize {
        let odds = &self.bal.luck_odds[self.luck.min(self.bal.luck_odds.len() - 1)];
        let total: u32 = odds.iter().sum();
        let mut r = rand::gen_range(0, total.max(1));
        for (t, &o) in odds.iter().enumerate() {
            if r < o {
                return t.min(self.bal.max_tier());
            }
            r -= o;
        }
        0
    }

    pub fn place(&mut self, c: Cell) {
        if let Err(e) = self.can_place(c) {
            self.flash(e);
            self.events.push(Event::Error);
            return;
        }
        let i = self.idx(c);
        let gem = rand::gen_range(0, self.bal.gems.len());
        let tier = self.roll_tier();
        self.grid[i] = Occ::Tower(Tower::new(Kind::Gem(gem), tier, true));
        self.placed.push(i);
        self.board_changed();
        let col = rgb(self.bal.gems[gem].color);
        self.ring(i, 0.9, col, 0.4);
        self.events.push(Event::Place { pos: self.center(i), gem });
        if self.placed.len() >= self.bal.gems_per_round {
            self.phase = Phase::Choose;
            self.selected = None;
        }
    }

    fn ring(&mut self, i: usize, r: f32, color: Color, life: f32) {
        let p = self.center(i);
        self.fx.push(Fx { kind: FxKind::Ring(p, r), color, t: 0.0, life });
    }

    /// Other fresh gems identical to `i` (same type and tier).
    pub fn combine_partners(&self, i: usize) -> Vec<usize> {
        let Some(t) = self.tower(i).filter(|t| t.fresh) else { return Vec::new() };
        self.placed
            .iter()
            .copied()
            .filter(|&j| j != i && self.tower(j).is_some_and(|o| o.fresh && o.kind == t.kind && o.rank == t.rank))
            .collect()
    }

    /// How many tiers keeping `i` with "combine" would add: +1 for a pair, +2 for four of a kind.
    pub fn combine_steps(&self, i: usize) -> usize {
        let Some(t) = self.tower(i) else { return 0 };
        let n = self.combine_partners(i).len();
        let max = self.bal.max_tier();
        if n >= 3 && t.rank + 2 <= max {
            2
        } else if n >= 1 && t.rank < max {
            1
        } else {
            0
        }
    }

    /// Keep the selected fresh gem (optionally upgrading it by combining identical ones).
    /// Every other fresh gem turns to rock and the wave begins.
    pub fn keep(&mut self, combine: bool) {
        let Some(sel) = self.selected else { return };
        if self.phase != Phase::Choose || !self.tower(sel).is_some_and(|t| t.fresh) {
            return;
        }
        let steps = if combine { self.combine_steps(sel) } else { 0 };
        if combine && steps == 0 {
            return;
        }
        if let Occ::Tower(t) = &mut self.grid[sel] {
            t.rank += steps;
            t.fresh = false;
        }
        self.finish_choose(sel);
        let col = self.color(self.tower(sel).unwrap().kind);
        self.ring(sel, if steps > 0 { 2.0 } else { 1.2 }, col, 0.6);
        let pos = self.center(sel);
        self.events.push(if steps > 0 { Event::Combine { pos, color: col } } else { Event::Keep { pos, color: col } });
        if steps > 0 {
            let name = self.tower_name(self.tower(sel).unwrap());
            self.flash(format!("Combined into {name}!"));
        }
        self.start_wave();
    }

    /// Turns every remaining fresh gem (except `keep`) to rock and ends the choose step.
    fn finish_choose(&mut self, keep: usize) {
        for j in std::mem::take(&mut self.placed) {
            if j != keep && self.tower(j).is_some_and(|t| t.fresh) {
                self.grid[j] = Occ::Rock;
            }
        }
        self.board_changed();
    }

    // ---- Recipes ----

    /// All gem towers usable as recipe ingredients right now: (cell, gem, tier, fresh).
    fn ingredient_pool(&self) -> Vec<(usize, usize, usize, bool)> {
        let fresh_ok = self.phase == Phase::Choose;
        self.grid
            .iter()
            .enumerate()
            .filter_map(|(i, o)| match o {
                Occ::Tower(t) => t.gem().filter(|_| fresh_ok || !t.fresh).map(|g| (i, g, t.rank, t.fresh)),
                _ => None,
            })
            .collect()
    }

    /// Ingredient cells for recipe `r` with `sel` as one of them (sel first), if all are present.
    /// Prefers fresh gems (they'd become rocks anyway) over kept towers.
    pub fn recipe_ingredients(&self, r: usize, sel: usize) -> Option<Vec<usize>> {
        if !self.between_waves() {
            return None;
        }
        let pool = self.ingredient_pool();
        let &(_, sg, st, _) = pool.iter().find(|p| p.0 == sel)?;
        let recipe = &self.bal.recipes[r];
        let slot = recipe.iter().position(|&(g, t)| g == sg && t == st)?;
        let mut used = vec![sel];
        for (k, &(g, t)) in recipe.iter().enumerate() {
            if k == slot {
                continue;
            }
            let pick = |want_fresh: bool| {
                pool.iter().find(|p| p.1 == g && p.2 == t && p.3 == want_fresh && !used.contains(&p.0)).map(|p| p.0)
            };
            let found = pick(true).or_else(|| pick(false))?;
            used.push(found);
        }
        Some(used)
    }

    /// Recipes that use the selected gem's type and tier, with ingredients if forgeable now.
    pub fn recipes_for(&self, sel: usize) -> Vec<(usize, Option<Vec<usize>>)> {
        let Some(t) = self.tower(sel) else { return Vec::new() };
        let Some(g) = t.gem() else { return Vec::new() };
        (0..self.bal.recipes.len())
            .filter(|&r| self.bal.recipes[r].contains(&(g, t.rank)))
            .map(|r| (r, self.recipe_ingredients(r, sel)))
            .collect()
    }

    /// Any recipe that can be forged right now: (recipe, ingredients).
    pub fn available_recipes(&self) -> Vec<(usize, Vec<usize>)> {
        if !self.between_waves() {
            return Vec::new();
        }
        let pool = self.ingredient_pool();
        let mut out = Vec::new();
        for r in 0..self.bal.recipes.len() {
            let (g, t) = self.bal.recipes[r][0];
            if let Some(ings) = pool.iter().filter(|p| p.1 == g && p.2 == t).find_map(|p| self.recipe_ingredients(r, p.0)) {
                out.push((r, ings));
            }
        }
        out
    }

    /// Recipes where the player owns all but one ingredient: (recipe, missing (gem, tier)).
    pub fn near_recipes(&self) -> Vec<(usize, (usize, usize))> {
        let owned: Vec<(usize, usize)> =
            self.grid.iter().filter_map(|o| if let Occ::Tower(t) = o { t.gem().map(|g| (g, t.rank)) } else { None }).collect();
        let mut out = Vec::new();
        for (r, recipe) in self.bal.recipes.iter().enumerate() {
            let mut pool = owned.clone();
            let missing: Vec<(usize, usize)> = recipe
                .iter()
                .copied()
                .filter(|ing| match pool.iter().position(|o| o == ing) {
                    Some(k) => {
                        pool.swap_remove(k);
                        false
                    }
                    None => true,
                })
                .collect();
            if missing.len() == 1 {
                out.push((r, missing[0]));
            }
        }
        out
    }

    /// Whether the player currently owns a gem of this type and tier (for the recipe book).
    pub fn owns(&self, gem: usize, tier: usize) -> bool {
        self.grid.iter().any(|o| matches!(o, Occ::Tower(t) if t.kind == Kind::Gem(gem) && t.rank == tier))
    }

    /// Forge recipe `r` at the selected cell. If any fresh gem is used, this counts as the
    /// round's keep: leftover fresh gems turn to rock and the wave starts.
    pub fn forge(&mut self, r: usize, sel: usize) {
        let Some(ings) = self.recipe_ingredients(r, sel) else { return };
        let used_fresh = ings.iter().any(|&i| self.tower(i).is_some_and(|t| t.fresh));
        for &i in &ings[1..] {
            self.grid[i] = Occ::Rock;
            self.placed.retain(|&p| p != i);
        }
        self.grid[sel] = Occ::Tower(Tower::new(Kind::Special(r), 0, false));
        self.placed.retain(|&p| p != sel);
        let col = rgb(self.bal.specials[r].color);
        self.ring(sel, 3.0, col, 0.9);
        self.events.push(Event::Forge { pos: self.center(sel), color: col });
        self.flash(format!("Forged {}!", self.bal.specials[r].name));
        if used_fresh {
            self.finish_choose(sel);
            self.start_wave();
        } else {
            self.board_changed();
        }
    }

    // ---- Gold sinks ----

    pub fn luck_cost(&self) -> Option<u32> {
        self.bal.luck_costs.get(self.luck).copied()
    }

    pub fn buy_luck(&mut self) {
        let Some(cost) = self.luck_cost() else { return };
        if self.gold < cost {
            self.flash("Not enough gold");
            self.events.push(Event::Error);
            return;
        }
        self.gold -= cost;
        self.luck += 1;
        self.events.push(Event::Upgrade { pos: None, color: GOLD });
        self.flash(format!("Gem quality upgraded to level {}", self.luck + 1));
    }

    pub fn special_upgrade_cost(&self, i: usize) -> Option<u32> {
        let t = self.tower(i)?;
        match t.kind {
            Kind::Special(_) => self.bal.special_upgrade_costs.get(t.rank).copied(),
            Kind::Gem(_) => None,
        }
    }

    pub fn upgrade_special(&mut self, i: usize) {
        let Some(cost) = self.special_upgrade_cost(i) else { return };
        if self.gold < cost {
            self.flash("Not enough gold");
            self.events.push(Event::Error);
            return;
        }
        self.gold -= cost;
        if let Occ::Tower(t) = &mut self.grid[i] {
            t.rank += 1;
        }
        let col = self.color(self.tower(i).unwrap().kind);
        self.ring(i, 2.0, col, 0.6);
        self.events.push(Event::Upgrade { pos: Some(self.center(i)), color: col });
        let name = self.tower_name(self.tower(i).unwrap());
        self.flash(format!("Upgraded to {name}"));
    }

    pub fn can_remove_rock(&self, i: usize) -> bool {
        self.is_rock(i) && self.between_waves() && self.gold >= self.bal.rock_removal_cost
    }

    pub fn remove_rock(&mut self, i: usize) {
        if !self.is_rock(i) || !self.between_waves() {
            return;
        }
        if self.gold < self.bal.rock_removal_cost {
            self.flash("Not enough gold");
            self.events.push(Event::Error);
            return;
        }
        self.gold -= self.bal.rock_removal_cost;
        self.grid[i] = Occ::Empty;
        self.selected = None;
        self.board_changed();
        self.ring(i, 1.0, GRAY, 0.4);
        self.events.push(Event::RockRemoved { pos: self.center(i) });
    }

    // ---- Waves ----

    pub fn wave_info(&self, w: u32) -> WaveInfo {
        self.kind_info(self.bal.kind_for_wave(w), w)
    }

    /// Stats for an enemy of `kind` appearing in wave `w`.
    pub fn kind_info(&self, kind: usize, w: u32) -> WaveInfo {
        let wd = &self.bal.waves;
        let d = &self.bal.difficulties[self.diff];
        let k = &wd.kinds[kind];
        let gold = (wd.base_gold + wd.gold_per_wave * w as f32) * if k.boss { BOSS_GOLD_MULT } else { 1.0 } * d.gold_mult;
        WaveInfo {
            name: k.name.clone(),
            count: k.count,
            hp: (wd.base_hp * d.hp_growth.powi(w as i32) * k.hp_mult).round(),
            speed: wd.base_speed * k.speed_mult,
            armor: k.armor + (w / wd.armor_per_waves.max(1)) as f32,
            gold: gold.round().max(1.0) as u32,
            air: k.air,
            boss: k.boss,
            desc: k.desc.clone(),
        }
    }

    fn start_wave(&mut self) {
        self.phase = Phase::Wave;
        self.selected = self.selected.filter(|&i| self.tower(i).is_some());
        let info = self.wave_info(self.wave);
        self.spawn_left = info.count;
        self.spawn_t = 0.0;
        self.events.push(Event::WaveStart { boss: info.boss, name: info.name, number: self.wave + 1 });
    }

    fn spawn(&mut self) {
        let kind = self.bal.kind_for_wave(self.wave);
        let start = self.map.waypoints[0];
        self.spawn_creep(kind, vec2(start.0 as f32 + 0.5, start.1 as f32 + 0.5), 1, 0.0);
    }

    fn spawn_creep(&mut self, kind: usize, pos: Vec2, next: usize, traveled: f32) {
        let info = self.kind_info(kind, self.wave);
        let k = &self.bal.waves.kinds[kind];
        let lives = if k.boss { self.bal.difficulties[self.diff].boss_lives } else { k.lives };
        self.creeps.push(Creep {
            id: self.next_id,
            kind,
            air: info.air,
            boss: info.boss,
            unslowable: k.unslowable,
            regen: k.regen,
            lives,
            pos,
            dir: vec2(1.0, 0.0),
            age: rand::gen_range(0.0, 10.0),
            next,
            hp: info.hp,
            max_hp: info.hp,
            speed: info.speed,
            armor: info.armor,
            slow: 0.0,
            slow_t: 0.0,
            poison_dps: 0.0,
            poison_t: 0.0,
            poison_src: 0,
            shred: 0.0,
            shred_t: 0.0,
            gold: info.gold,
            traveled,
            last_hit: None,
            flash: 0.0,
        });
        self.next_id += 1;
    }

    /// After winning, keep playing endless waves.
    pub fn continue_endless(&mut self) {
        if self.phase == Phase::Victory {
            self.endless = true;
            self.phase = Phase::Build;
            self.flash("Endless mode: how far can you go?");
        }
    }

    // ---- Simulation ----

    pub fn step(&mut self) {
        let dt = STEP;
        for f in &mut self.fx {
            f.t += dt;
        }
        self.fx.retain(|f| f.t < f.life);
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }
        if self.phase != Phase::Wave {
            return;
        }

        // Spawning
        if self.spawn_left > 0 {
            self.spawn_t -= dt;
            if self.spawn_t <= 0.0 {
                self.spawn();
                self.spawn_left -= 1;
                self.spawn_t = self.bal.waves.spawn_interval / self.wave_info(self.wave).speed * self.bal.waves.base_speed;
            }
        }

        // Creep movement, status effects, leaking
        let mut leaked = 0;
        for c in &mut self.creeps {
            if c.poison_t > 0.0 {
                c.poison_t -= dt;
                let d = c.poison_dps * dt;
                c.hp -= d;
                c.last_hit = Some(c.poison_src);
                if let Occ::Tower(t) = &mut self.grid[c.poison_src] {
                    t.damage_dealt += d;
                }
            }
            c.flash = (c.flash - dt).max(0.0);
            if c.regen > 0.0 {
                c.hp = (c.hp + c.regen * c.max_hp * dt).min(c.max_hp);
            }
            if c.slow_t > 0.0 {
                c.slow_t -= dt;
                if c.slow_t <= 0.0 {
                    c.slow = 0.0;
                }
            }
            if c.shred_t > 0.0 {
                c.shred_t -= dt;
                if c.shred_t <= 0.0 {
                    c.shred = 0.0;
                }
            }
            // Flyers ignore the maze and go straight between checkpoints.
            let path: &[Cell] = if c.air { &self.map.waypoints } else { &self.route };
            c.age += dt;
            let mut budget = c.speed * (1.0 - c.slow) * dt;
            while budget > 0.0 && c.next < path.len() {
                let tc = path[c.next];
                let target = vec2(tc.0 as f32 + 0.5, tc.1 as f32 + 0.5);
                let d = target - c.pos;
                let len = d.length();
                if len > 1e-4 {
                    // Turn smoothly toward the direction of travel.
                    c.dir = (c.dir + (d / len - c.dir) * 0.25).normalize_or(d / len);
                }
                if len <= budget {
                    c.pos = target;
                    budget -= len;
                    c.traveled += len;
                    c.next += 1;
                } else {
                    c.pos += d / len * budget;
                    c.traveled += budget;
                    budget = 0.0;
                }
            }
            if c.next >= path.len() {
                c.hp = f32::NEG_INFINITY; // mark for removal, no reward
                leaked += c.lives;
            }
        }
        if leaked > 0 {
            self.lives -= leaked;
            self.events.push(Event::Leak { lives: leaked });
            self.flash(format!("-{leaked} {}!", if leaked > 1 { "lives" } else { "life" }));
        }

        // Towers acquire targets and fire
        let auras: Vec<(Vec2, f32)> = self
            .grid
            .iter()
            .enumerate()
            .filter_map(|(j, o)| match o {
                Occ::Tower(t) if self.def(t.kind).aura > 0.0 => Some((self.center(j), self.stats(t).aura)),
                _ => None,
            })
            .collect();
        for i in 0..self.grid.len() {
            let Occ::Tower(t) = &self.grid[i] else { continue };
            if t.cooldown > 0.0 {
                if let Occ::Tower(t) = &mut self.grid[i] {
                    t.cooldown -= dt;
                    t.flash = (t.flash - dt * 5.0).max(0.0);
                }
                continue;
            }
            let tp = self.center(i);
            let s = self.stats(t);
            let target = self
                .creeps
                .iter()
                .filter(|cr| cr.hp > 0.0 && !(cr.air && s.ground_only) && cr.pos.distance(tp) <= s.range)
                .max_by(|a, b| a.traveled.partial_cmp(&b.traveled).unwrap());
            if let Some(cr) = target {
                let bonus: f32 = auras.iter().filter(|(p, _)| *p != tp && p.distance(tp) <= AURA_RANGE).map(|a| a.1).sum();
                let size = 0.12 + 0.03 * t.rank as f32 + if matches!(t.kind, Kind::Special(_)) { 0.06 } else { 0.0 };
                self.projectiles.push(Projectile {
                    pos: tp,
                    vel: Vec2::ZERO,
                    target: cr.id,
                    src: i,
                    damage: s.damage * (1.0 + bonus),
                    color: self.color(t.kind),
                    size,
                });
                if let Occ::Tower(t) = &mut self.grid[i] {
                    t.cooldown = s.cooldown;
                    t.flash = 1.0;
                }
                self.events.push(Event::Shot);
            }
        }

        // Projectiles
        let mut hits = Vec::new();
        self.projectiles.retain_mut(|p| {
            let Some(c) = self.creeps.iter().find(|c| c.id == p.target && c.hp > 0.0) else {
                return false;
            };
            let d = c.pos - p.pos;
            let step = PROJECTILE_SPEED * dt;
            if d.length() <= step + 0.15 {
                hits.push((p.target, p.src, p.damage));
                false
            } else {
                p.vel = d.normalize() * PROJECTILE_SPEED;
                p.pos += d.normalize() * step;
                true
            }
        });
        for (target, src, dmg) in hits {
            self.resolve_hit(target, src, dmg);
        }

        // Deaths
        let mut gained = 0;
        let mut kills = Vec::new();
        let mut splits = Vec::new();
        let kinds = &self.bal.waves.kinds;
        self.creeps.retain(|c| {
            if c.hp > 0.0 {
                return true;
            }
            if c.hp > f32::NEG_INFINITY {
                gained += c.gold;
                kills.push((c.pos, c.gold, c.last_hit, rgb(kinds[c.kind].color), c.boss));
                if let Some(sk) = kinds[c.kind].split_kind {
                    splits.push((sk, kinds[c.kind].split_count, c.pos, c.next, c.traveled));
                }
            }
            false
        });
        for (kind, count, pos, next, traveled) in splits {
            for k in 0..count {
                let jitter = vec2(rand::gen_range(-0.3, 0.3), rand::gen_range(-0.3, 0.3));
                // Slight stagger so they don't all share one spot on the path.
                self.spawn_creep(kind, pos + jitter, next, traveled - k as f32 * 0.05);
            }
            let col = rgb(self.bal.waves.kinds[kind].color);
            self.fx.push(Fx { kind: FxKind::Ring(pos, 1.5), color: col, t: 0.0, life: 0.5 });
        }
        self.gold += gained;
        for (pos, gold, src, color, boss) in kills {
            self.total_kills += 1;
            self.events.push(Event::Death { pos, color, boss });
            if let Some(Occ::Tower(t)) = src.map(|s| &mut self.grid[s]) {
                t.kills += 1;
            }
            self.fx.push(Fx {
                kind: FxKind::Text(pos, format!("+{gold}")),
                color: Color::from_rgba(255, 215, 90, 255),
                t: 0.0,
                life: 0.8,
            });
        }

        if self.lives <= 0 {
            self.lives = 0;
            self.phase = Phase::GameOver;
            self.events.push(Event::Defeat);
            return;
        }

        // Wave cleared
        if self.spawn_left == 0 && self.creeps.is_empty() {
            self.projectiles.clear();
            self.gold += self.bal.waves.wave_bonus;
            self.wave += 1;
            if self.wave == self.bal.wave_count() && !self.endless {
                self.phase = Phase::Victory;
                self.events.push(Event::Victory);
            } else {
                self.events.push(Event::WaveClear);
                self.phase = Phase::Build;
                self.flash(format!("Wave cleared! +{} gold", self.bal.waves.wave_bonus));
            }
            for o in &mut self.grid {
                if let Occ::Tower(t) = o {
                    t.cooldown = 0.0;
                }
            }
        }
    }

    fn damage_creep(&mut self, ci: usize, dmg: f32, src: usize, pierce: bool) {
        let c = &mut self.creeps[ci];
        let armor = c.armor - c.shred;
        let mult = if pierce {
            1.0
        } else if armor >= 0.0 {
            1.0 - (0.06 * armor) / (1.0 + 0.06 * armor)
        } else {
            2.0 - 0.94f32.powf(-armor)
        };
        let d = dmg * mult;
        c.hp -= d;
        c.flash = 0.08;
        c.last_hit = Some(src);
        if let Occ::Tower(t) = &mut self.grid[src] {
            t.damage_dealt += d;
        }
    }

    fn apply_slow(&mut self, ci: usize, slow: f32) {
        let c = &mut self.creeps[ci];
        if c.unslowable {
            return;
        }
        c.slow = c.slow.max(slow);
        c.slow_t = SLOW_DURATION;
    }

    fn resolve_hit(&mut self, target: u64, src: usize, dmg: f32) {
        let Some(ci) = self.creeps.iter().position(|c| c.id == target) else { return };
        let Some(t) = self.tower(src) else { return };
        let s = self.stats(t);
        let color = self.color(t.kind);
        let pos = self.creeps[ci].pos;
        let air = self.creeps[ci].air;
        let mut d = dmg * if air { s.air_mult } else { 1.0 };

        if s.crit > 0.0 && rand::gen_range(0.0, 1.0) < s.crit {
            d *= CRIT_MULT;
            self.events.push(Event::Crit { pos, color });
            self.fx.push(Fx { kind: FxKind::Text(pos, "CRIT".into()), color, t: 0.0, life: 0.5 });
        }
        self.damage_creep(ci, d, src, s.pierce);

        if s.slow > 0.0 {
            self.apply_slow(ci, s.slow);
        }
        if s.poison > 0.0 {
            let c = &mut self.creeps[ci];
            if s.poison >= c.poison_dps || c.poison_t <= 0.0 {
                c.poison_dps = s.poison;
                c.poison_src = src;
            }
            c.poison_t = POISON_DURATION;
        }
        if s.shred > 0.0 {
            let c = &mut self.creeps[ci];
            c.shred = c.shred.max(s.shred);
            c.shred_t = SHRED_DURATION;
        }
        let hittable = |c: &Creep| c.hp > 0.0 && !(c.air && s.ground_only);
        if s.splash > 0.0 {
            for j in 0..self.creeps.len() {
                if j != ci && hittable(&self.creeps[j]) && self.creeps[j].pos.distance(pos) <= s.splash {
                    self.damage_creep(j, d * SPLASH_FRACTION, src, s.pierce);
                    if s.slow > 0.0 {
                        self.apply_slow(j, s.slow);
                    }
                }
            }
            self.fx.push(Fx { kind: FxKind::Ring(pos, s.splash), color, t: 0.0, life: 0.3 });
            self.events.push(Event::Splash { pos, color, radius: s.splash });
        }
        if s.chain > 0 {
            self.events.push(Event::Chain);
            let mut hit = vec![ci];
            let mut last = pos;
            let mut cd = d;
            for _ in 0..s.chain {
                cd *= 0.75;
                let next = (0..self.creeps.len())
                    .filter(|j| !hit.contains(j) && hittable(&self.creeps[*j]))
                    .map(|j| (j, self.creeps[j].pos.distance(last)))
                    .filter(|(_, dist)| *dist <= CHAIN_RANGE)
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                let Some((j, _)) = next else { break };
                let np = self.creeps[j].pos;
                self.fx.push(Fx { kind: FxKind::Beam(last, np), color, t: 0.0, life: 0.15 });
                self.damage_creep(j, cd, src, s.pierce);
                hit.push(j);
                last = np;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Greedy bot: each gem goes where it lengthens the route most; forges any recipe it
    /// can, otherwise keeps the strongest gem. Spends gold on quality, then specials.
    fn play(seed: u64) -> (u32, i32, usize) {
        rand::srand(seed);
        let (bal, _) = Balance::load();
        let diff = std::env::var("DIFF").ok().and_then(|d| d.parse().ok()).unwrap_or(0);
        let map_idx = std::env::var("MAP").ok().and_then(|d| d.parse().ok()).unwrap_or(0);
        let map = MapDef::load_all().0.swap_remove(map_idx);
        let mut g = Game::new(bal, map, diff);
        let mut forged = 0;
        let trace = std::env::var("TRACE").is_ok();
        let endless = std::env::var("ENDLESS").is_ok();
        while !matches!(g.phase, Phase::GameOver | Phase::Victory) || (endless && g.phase == Phase::Victory) {
            g.events.clear();
            if g.phase == Phase::Victory {
                g.continue_endless();
            }
            match g.phase {
                Phase::Build => {
                    if g.placed.is_empty() {
                        if trace {
                            print!("{}:{} ", g.wave + 1, g.lives);
                        }
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
                    g.place(best.expect("no valid placement").1);
                }
                Phase::Choose => {
                    if let Some((r, ings)) = g.available_recipes().into_iter().next() {
                        // Put the special where the freshest ingredient was.
                        let sel = ings.iter().copied().find(|&i| g.tower(i).unwrap().fresh).unwrap_or(ings[0]);
                        g.forge(r, sel);
                        forged += 1;
                        continue;
                    }
                    let pick = g.placed.iter().copied().max_by(|&a, &b| {
                        let s = |i| g.stats(g.tower(i).unwrap()).damage + 1e6 * g.combine_steps(i) as f32;
                        s(a).partial_cmp(&s(b)).unwrap()
                    });
                    g.selected = pick;
                    let combine = g.combine_steps(pick.unwrap()) > 0;
                    g.keep(combine);
                }
                _ => g.step(),
            }
        }
        if trace {
            println!();
        }
        (g.wave, g.lives, forged)
    }

    #[test]
    fn maps_are_valid() {
        let (maps, err) = MapDef::load_all();
        assert!(err.is_none(), "{err:?}");
        for m in maps {
            for &(x, y) in &m.waypoints {
                assert!(x >= 0 && y >= 0 && x < m.w && y < m.h, "{}: waypoint ({x},{y}) out of bounds", m.name);
                assert!(!m.is_wall(x, y), "{}: waypoint ({x},{y}) is inside a wall", m.name);
            }
            let (bal, _) = Balance::load();
            let name = m.name.clone();
            let g = Game::new(bal, m, 0);
            println!("{name}: route length {}", g.route.len());
        }
    }

    #[test]
    fn bot_runs() {
        for seed in 1..=6 {
            let (w, l, f) = play(seed);
            println!("seed {seed}: reached wave {} with {l} lives, forged {f}", w + 1);
        }
    }
}
