//! Game state and simulation. Everything here runs in fixed time steps and knows
//! nothing about drawing; positions are in grid units (cell (x,y) spans x..x+1).

pub use glam::Vec2 as GVec2;
use glam::{vec2, Vec2};

use crate::color::{Color, GOLD, GRAY};
use crate::rng as rand;

use crate::data::{Balance, Ing, MapDef, TowerDef};
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
/// Menders heal allies within this distance.
pub const HEAL_RANGE: f32 = 2.5;
/// Towers always see invisible enemies this close to them.
pub const SEE_NEAR: f32 = 1.6;
/// How long an invisible enemy stays visible after being revealed.
const REVEAL_LINGER: f32 = 0.6;
const BURN_DURATION: f32 = 3.0;
pub const BURN_RADIUS: f32 = 0.8;
/// Seconds without being hit before a shield starts recharging.
const SHIELD_DELAY: f32 = 2.5;
/// Regenerating bosses only heal after this long without taking damage (poison counts).
pub const REGEN_DELAY: f32 = 2.0;
/// Sprint cycle: seconds between sprints, and sprint length.
const SPRINT_PERIOD: f32 = 4.0;
const SPRINT_TIME: f32 = 1.2;

/// Optional rules that change a run.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Serialize, serde::Deserialize)]
pub enum Mutator {
    /// Flying waves have twice as many enemies.
    DoubleFlyers,
    /// 50% more enemies, each with 25% less HP.
    Horde,
    /// Enemies have +3 armor.
    Fortified,
    /// Enemies move 20% faster.
    Haste,
    /// Rocks can't be removed.
    Bedrock,
    /// Half the lives, 30% more gold.
    Fragile,
}

impl Mutator {
    pub const ALL: [Mutator; 6] = [Mutator::DoubleFlyers, Mutator::Horde, Mutator::Fortified, Mutator::Haste, Mutator::Bedrock, Mutator::Fragile];

    pub fn name(self) -> &'static str {
        match self {
            Mutator::DoubleFlyers => "Double Flyers",
            Mutator::Horde => "Horde",
            Mutator::Fortified => "Fortified",
            Mutator::Haste => "Haste",
            Mutator::Bedrock => "Bedrock",
            Mutator::Fragile => "Fragile",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Mutator::DoubleFlyers => "Flying waves have twice as many enemies.",
            Mutator::Horde => "50% more enemies, each with 25% less HP.",
            Mutator::Fortified => "Every enemy has +3 armor.",
            Mutator::Haste => "Enemies move 20% faster.",
            Mutator::Bedrock => "Rocks can't be removed.",
            Mutator::Fragile => "Half the lives, but 30% more gold.",
        }
    }
}

/// A patch of burning ground.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Patch {
    pub pos: Vec2,
    pub dps: f32,
    pub t: f32,
    pub src: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
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
    /// A projectile from tower cell `src` struck its target.
    Impact { pos: Vec2, src: usize },
    Death { pos: Vec2, color: Color, boss: bool },
    Leak { lives: i32 },
    WaveStart { boss: bool, name: String, number: u32 },
    WaveClear,
    Victory,
    Defeat,
    Error,
    RockRemoved { pos: Vec2 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Gem(usize),
    Special(usize),
}

/// Which enemy in range a tower shoots at.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum Target {
    /// Furthest along the path (closest to escaping).
    #[default]
    First,
    /// Least far along.
    Last,
    /// Most HP left (bosses among their escorts, tough enemies).
    Strongest,
    /// Least HP left (finishes kills, thins out swarms).
    Weakest,
}

impl Target {
    pub const ALL: [Target; 4] = [Target::First, Target::Last, Target::Strongest, Target::Weakest];

    pub fn name(self) -> &'static str {
        match self {
            Target::First => "First",
            Target::Last => "Last",
            Target::Strongest => "Strongest",
            Target::Weakest => "Weakest",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Target::First => "The enemy furthest along the path, closest to escaping.",
            Target::Last => "The enemy least far along the path.",
            Target::Strongest => "The enemy with the most HP left: bosses, tough enemies.",
            Target::Weakest => "The enemy with the least HP left: finishes kills, thins swarms.",
        }
    }

    pub fn next(self) -> Target {
        let k = Target::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Target::ALL[(k + 1) % Target::ALL.len()]
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Tower {
    pub kind: Kind,
    #[serde(default)]
    pub target: Target,
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
        Tower { kind, target: Target::First, rank, cooldown: 0.0, fresh, damage_dealt: 0.0, kills: 0, flash: 0.0 }
    }
    pub fn gem(&self) -> Option<usize> {
        match self.kind {
            Kind::Gem(g) => Some(g),
            Kind::Special(_) => None,
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
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
    pub detect: bool,
    pub burn: f32,
    pub bounty: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
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
    pub invisible: bool,
    /// Seconds left of being visible (invisible enemies only).
    pub revealed: f32,
    pub heal: f32,
    pub shield: f32,
    pub max_shield: f32,
    /// Seconds since last hit (for shield recharge).
    pub since_hit: f32,
    pub sprint: f32,
    pub immune: bool,
}

impl Creep {
    /// Whether towers can target this enemy right now.
    pub fn visible(&self) -> bool {
        !self.invisible || self.revealed > 0.0
    }

    /// Whether it is mid-sprint.
    pub fn sprinting(&self) -> bool {
        self.sprint > 0.0 && self.age % SPRINT_PERIOD > SPRINT_PERIOD - SPRINT_TIME
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
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

/// Serializable for saved runs. The balance isn't saved: [`Game::rebind`] puts the current one
/// back (and checks the saved run still fits it). Effects and pending events aren't saved.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Game {
    #[serde(skip, default = "placeholder_balance")]
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
    #[serde(skip)]
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
    #[serde(skip)]
    pub message: Option<(String, f32)>,
    /// Bumped whenever the board layout changes.
    pub rev: u64,
    /// Events since the last drain (sound/visual effects).
    #[serde(skip)]
    pub events: Vec<Event>,
    /// Burning ground.
    pub patches: Vec<Patch>,
    pub mutators: Vec<Mutator>,
    /// Gem rolls come from this stream (part of a saved run, so quitting and resuming can't
    /// re-roll anything). Seeded runs (daily challenge) give every player the same gems.
    roll_state: Option<u64>,
    spawn_left: u32,
    spawn_t: f32,
    next_id: u64,
    /// Build phase only: no empty cell can take a gem (see [`Game::skip_round`]).
    #[serde(default)]
    pub no_room: bool,
}

fn placeholder_balance() -> Balance {
    Balance::load().0
}

pub fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], 255)
}

impl Game {
    pub fn new(bal: Balance, map: MapDef, diff: usize) -> Game {
        Game::with_options(bal, map, diff, Vec::new(), None)
    }

    /// A game with mutators and (for seeded runs) a fixed gem-roll seed.
    pub fn with_options(bal: Balance, map: MapDef, diff: usize, mutators: Vec<Mutator>, seed: Option<u64>) -> Game {
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
            lives: if mutators.contains(&Mutator::Fragile) { (bal.difficulties[diff].lives / 2).max(1) } else { bal.difficulties[diff].lives },
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
            patches: Vec::new(),
            mutators,
            roll_state: Some(seed.unwrap_or_else(|| rand::gen_range(1u32, u32::MAX) as u64 * 0x1_0000_0001).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            spawn_left: 0,
            spawn_t: 0.0,
            next_id: 1,
            no_room: false,
            bal,
        };
        g.route = g.route_with(None).expect("waypoints must be connected on an empty map");
        g
    }

    pub fn has(&self, m: Mutator) -> bool {
        self.mutators.contains(&m)
    }

    /// A gem roll in [0, n): from the seeded stream if there is one.
    fn roll(&mut self, n: u32) -> u32 {
        match &mut self.roll_state {
            Some(x) => {
                *x ^= *x >> 12;
                *x ^= *x << 25;
                *x ^= *x >> 27;
                ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) % n.max(1) as u64) as u32
            }
            None => rand::gen_range(0, n.max(1)),
        }
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

    /// After loading a saved run: attach the current balance and check that everything in the
    /// run still exists in it (a newer version might have removed something).
    pub fn rebind(&mut self, bal: Balance) -> Result<(), String> {
        let max = bal.max_tier();
        for o in &self.grid {
            if let Occ::Tower(t) = o {
                let ok = match t.kind {
                    Kind::Gem(g) => g < bal.gems.len() && t.rank <= max,
                    Kind::Special(s) => s < bal.specials.len(),
                };
                if !ok {
                    return Err("a tower in the saved run no longer exists".into());
                }
            }
        }
        if self.creeps.iter().any(|c| c.kind >= bal.waves.kinds.len()) || self.diff >= bal.difficulties.len() {
            return Err("the saved run doesn't fit this version".into());
        }
        if self.grid.len() != (self.map.w * self.map.h) as usize {
            return Err("the saved map is damaged".into());
        }
        self.bal = bal;
        self.route = self.route_with(None).ok_or("the saved maze is blocked")?;
        self.rev += 1;
        Ok(())
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
            detect: d.detect,
            burn: d.burn * mult,
            bounty: d.bounty * mult.sqrt(),
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
        self.update_room();
    }

    /// Sets a tower's targeting.
    pub fn set_target(&mut self, i: usize, target: Target) {
        if let Occ::Tower(t) = &mut self.grid[i] {
            t.target = target;
        }
    }

    /// Selects the next (`dir` = 1) or previous (-1) tower: this round's new gems while
    /// choosing, otherwise every tower. Returns the new selection.
    pub fn cycle_selection(&mut self, dir: i32) -> Option<usize> {
        let list: Vec<usize> = if self.phase == Phase::Choose {
            self.placed.iter().copied().filter(|&i| self.tower(i).is_some()).collect()
        } else {
            (0..self.grid.len()).filter(|&i| matches!(self.grid[i], Occ::Tower(_))).collect()
        };
        if list.is_empty() {
            return None;
        }
        let n = list.len() as i32;
        let k = match self.selected.and_then(|s| list.iter().position(|&i| i == s)) {
            Some(a) => (a as i32 + dir).rem_euclid(n),
            None if dir > 0 => 0,
            None => n - 1,
        } as usize;
        self.selected = Some(list[k]);
        self.selected
    }

    /// Enemies of the current wave still to come.
    pub fn spawn_left(&self) -> u32 {
        self.spawn_left
    }

    /// Is there any cell left that can take a gem?
    pub fn any_placeable(&self) -> bool {
        (0..self.grid.len()).any(|i| matches!(self.grid[i], Occ::Empty) && self.can_place(self.cell_of(i)).is_ok())
    }

    /// Re-checks [`Game::no_room`]. A round that runs out of room part way goes straight to
    /// choosing from the gems already placed.
    fn update_room(&mut self) {
        self.no_room = self.phase == Phase::Build && !self.any_placeable();
        if self.no_room && !self.placed.is_empty() {
            self.no_room = false;
            self.phase = Phase::Choose;
            self.selected = None;
            self.flash("No room left: choose from the gems you placed");
        }
    }

    /// With no room for any gem this round: skip placing and start the wave.
    pub fn skip_round(&mut self) {
        if self.phase == Phase::Build && self.no_room && self.placed.is_empty() {
            self.no_room = false;
            self.start_wave();
        }
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

    fn roll_tier(&mut self) -> usize {
        let odds = self.bal.luck_odds[self.luck.min(self.bal.luck_odds.len() - 1)].clone();
        let total: u32 = odds.iter().sum();
        let mut r = self.roll(total);
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
        let gem = self.roll(self.bal.gems.len() as u32) as usize;
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

    /// What a tower counts as in recipes.
    pub fn ing_of(&self, t: &Tower) -> Ing {
        match t.kind {
            Kind::Gem(g) => Ing::Gem(g, t.rank),
            Kind::Special(s) => Ing::Special(s),
        }
    }

    /// All towers usable as recipe ingredients right now: (cell, ingredient, fresh).
    fn ingredient_pool(&self) -> Vec<(usize, Ing, bool)> {
        let fresh_ok = self.phase == Phase::Choose;
        self.grid
            .iter()
            .enumerate()
            .filter_map(|(i, o)| match o {
                Occ::Tower(t) if fresh_ok || !t.fresh => Some((i, self.ing_of(t), t.fresh)),
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
        let &(_, sel_ing, _) = pool.iter().find(|p| p.0 == sel)?;
        let recipe = &self.bal.recipes[r];
        let slot = recipe.iter().position(|&i| i == sel_ing)?;
        let mut used = vec![sel];
        for (k, &ing) in recipe.iter().enumerate() {
            if k == slot {
                continue;
            }
            let pick = |want_fresh: bool| pool.iter().find(|p| p.1 == ing && p.2 == want_fresh && !used.contains(&p.0)).map(|p| p.0);
            let found = pick(true).or_else(|| pick(false))?;
            used.push(found);
        }
        Some(used)
    }

    /// Recipes that use the selected tower, with ingredients if forgeable now.
    pub fn recipes_for(&self, sel: usize) -> Vec<(usize, Option<Vec<usize>>)> {
        let Some(t) = self.tower(sel) else { return Vec::new() };
        let ing = self.ing_of(t);
        (0..self.bal.recipes.len())
            .filter(|&r| self.bal.recipes[r].contains(&ing))
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
            let first = self.bal.recipes[r][0];
            if let Some(ings) = pool.iter().filter(|p| p.1 == first).find_map(|p| self.recipe_ingredients(r, p.0)) {
                out.push((r, ings));
            }
        }
        out
    }

    /// Recipes where the player owns all but one ingredient: (recipe, missing ingredient).
    pub fn near_recipes(&self) -> Vec<(usize, Ing)> {
        let owned: Vec<Ing> = self.grid.iter().filter_map(|o| if let Occ::Tower(t) = o { Some(self.ing_of(t)) } else { None }).collect();
        let mut out = Vec::new();
        for (r, recipe) in self.bal.recipes.iter().enumerate() {
            let mut pool = owned.clone();
            let missing: Vec<Ing> = recipe
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

    /// Whether the player currently owns this ingredient (for the recipe book).
    pub fn owns(&self, ing: Ing) -> bool {
        self.grid.iter().any(|o| matches!(o, Occ::Tower(t) if self.ing_of(t) == ing))
    }

    /// Display name of an ingredient ("Faceted Ruby", "Frostfire").
    pub fn ing_name(&self, ing: Ing) -> String {
        match ing {
            Ing::Gem(g, t) => format!("{} {}", self.bal.tier_names[t], self.bal.gems[g].name),
            Ing::Special(s) => self.bal.specials[s].name.clone(),
        }
    }

    /// Forge recipe `r` at the selected cell. If any fresh gem is used, this counts as the
    /// round's keep: leftover fresh gems turn to rock and the wave starts.
    /// A recipe that can be forged right now and the tower to forge it at: the selected
    /// tower's if it has one, else the first ready recipe's.
    pub fn ready_forge(&self) -> Option<(usize, usize)> {
        if let Some(sel) = self.selected
            && let Some((r, _)) = self.recipes_for(sel).into_iter().find(|(_, ings)| ings.is_some())
        {
            return Some((r, sel));
        }
        self.available_recipes().into_iter().next().map(|(r, ings)| (r, ings[0]))
    }

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
        self.is_rock(i) && self.between_waves() && self.gold >= self.bal.rock_removal_cost && !self.has(Mutator::Bedrock)
    }

    pub fn remove_rock(&mut self, i: usize) {
        if !self.is_rock(i) || !self.between_waves() {
            return;
        }
        if self.has(Mutator::Bedrock) {
            self.flash("Bedrock: rocks can't be removed");
            self.events.push(Event::Error);
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
        let fragile = if self.has(Mutator::Fragile) { 1.3 } else { 1.0 };
        let gold = (wd.base_gold + wd.gold_per_wave * w as f32) * if k.boss { BOSS_GOLD_MULT } else { 1.0 } * d.gold_mult * fragile;
        let horde = self.has(Mutator::Horde) && !k.boss;
        let mut count = k.count;
        if horde {
            count = (count as f32 * 1.5).ceil() as u32;
        }
        if k.air && !k.boss && self.has(Mutator::DoubleFlyers) {
            count *= 2;
        }
        let hp = wd.base_hp * d.hp_growth.powi(w as i32) * k.hp_mult * if horde { 0.75 } else { 1.0 };
        WaveInfo {
            name: k.name.clone(),
            count,
            hp: hp.round(),
            speed: wd.base_speed * k.speed_mult * if self.has(Mutator::Haste) { 1.2 } else { 1.0 },
            armor: k.armor + (w / wd.armor_per_waves.max(1)) as f32 + if self.has(Mutator::Fortified) { 3.0 } else { 0.0 },
            gold: (gold * if horde { 0.7 } else { 1.0 }).round().max(1.0) as u32,
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
            invisible: k.invisible,
            revealed: 0.0,
            heal: k.heal,
            shield: k.shield * info.hp,
            max_shield: k.shield * info.hp,
            since_hit: 0.0,
            sprint: k.sprint,
            immune: k.immune,
        });
        self.next_id += 1;
    }

    /// After winning, keep playing endless waves.
    pub fn continue_endless(&mut self) {
        if self.phase == Phase::Victory {
            self.endless = true;
            self.phase = Phase::Build;
            self.flash("Endless mode: how far can you go?");
            self.update_room();
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

        // Detection: invisible enemies are seen within a detector's range, or right next to
        // any tower.
        let mut detectors: Vec<(Vec2, f32)> = Vec::new();
        let mut tower_spots: Vec<Vec2> = Vec::new();
        for (i, o) in self.grid.iter().enumerate() {
            if let Occ::Tower(t) = o {
                let p = self.center(i);
                tower_spots.push(p);
                let st = self.stats(t);
                if st.detect {
                    detectors.push((p, st.range));
                }
            }
        }
        for c in &mut self.creeps {
            if !c.invisible {
                continue;
            }
            let seen = detectors.iter().any(|(p, r)| p.distance(c.pos) <= *r) || tower_spots.iter().any(|p| p.distance(c.pos) <= SEE_NEAR);
            c.revealed = if seen { REVEAL_LINGER } else { (c.revealed - dt).max(0.0) };
        }

        // Menders heal their neighbours.
        let healers: Vec<(u64, Vec2, f32)> = self.creeps.iter().filter(|c| c.heal > 0.0 && c.hp > 0.0).map(|c| (c.id, c.pos, c.heal)).collect();
        if !healers.is_empty() {
            for c in &mut self.creeps {
                for &(id, p, h) in &healers {
                    if id != c.id && c.hp > 0.0 && p.distance(c.pos) <= HEAL_RANGE {
                        c.hp = (c.hp + h * c.max_hp * dt).min(c.max_hp);
                    }
                }
            }
        }

        // Burning ground.
        for patch in &mut self.patches {
            patch.t += dt;
        }
        self.patches.retain(|p| p.t < BURN_DURATION);
        let burns: Vec<(Vec2, f32, usize)> = self.patches.iter().map(|p| (p.pos, p.dps, p.src)).collect();
        for (pos, dps, src) in burns {
            for ci in 0..self.creeps.len() {
                let c = &self.creeps[ci];
                if !c.air && c.hp > 0.0 && c.pos.distance(pos) <= BURN_RADIUS {
                    self.damage_creep(ci, dps * dt, src, true);
                }
            }
        }

        // Creep movement, status effects, leaking
        let mut leaked = 0;
        for c in &mut self.creeps {
            c.since_hit += dt;
            if c.max_shield > 0.0 && c.since_hit > SHIELD_DELAY {
                c.shield = (c.shield + c.max_shield * 0.25 * dt).min(c.max_shield);
            }
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
            if c.regen > 0.0 && c.since_hit > REGEN_DELAY && c.poison_t <= 0.0 {
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
            let sprint = if c.sprinting() { c.sprint } else { 1.0 };
            let mut budget = c.speed * sprint * (1.0 - c.slow) * dt;
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
            let mode = t.target;
            let in_range = self.creeps.iter().filter(|cr| cr.hp > 0.0 && cr.visible() && !(cr.air && s.ground_only) && cr.pos.distance(tp) <= s.range);
            // Ties (e.g. equal HP) go to the enemy furthest along.
            let target = match mode {
                Target::First => in_range.max_by(|a, b| a.traveled.total_cmp(&b.traveled)),
                Target::Last => in_range.min_by(|a, b| a.traveled.total_cmp(&b.traveled)),
                Target::Strongest => in_range.max_by(|a, b| a.hp.total_cmp(&b.hp).then(a.traveled.total_cmp(&b.traveled))),
                Target::Weakest => in_range.min_by(|a, b| a.hp.total_cmp(&b.hp).then(b.traveled.total_cmp(&a.traveled))),
            };
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
            let bounty = src.and_then(|s| self.tower(s)).map_or(0.0, |t| self.stats(t).bounty).round() as u32;
            self.gold += bounty;
            let gold = gold + bounty;
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
            self.patches.clear();
            self.gold += self.bal.waves.wave_bonus;
            self.wave += 1;
            if self.wave == self.bal.wave_count() && !self.endless {
                self.phase = Phase::Victory;
                self.events.push(Event::Victory);
            } else {
                self.events.push(Event::WaveClear);
                self.phase = Phase::Build;
                self.flash(format!("Wave cleared! +{} gold", self.bal.waves.wave_bonus));
                self.update_room();
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
        let absorbed = d.min(c.shield);
        c.shield -= absorbed;
        c.hp -= d - absorbed;
        c.since_hit = 0.0;
        c.flash = 0.08;
        c.last_hit = Some(src);
        if let Occ::Tower(t) = &mut self.grid[src] {
            t.damage_dealt += d;
        }
    }

    fn apply_slow(&mut self, ci: usize, slow: f32) {
        let c = &mut self.creeps[ci];
        if c.unslowable || c.immune {
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
        self.events.push(Event::Impact { pos, src });

        if s.crit > 0.0 && rand::gen_range(0.0, 1.0) < s.crit {
            d *= CRIT_MULT;
            self.events.push(Event::Crit { pos, color });
            self.fx.push(Fx { kind: FxKind::Text(pos, "CRIT".into()), color, t: 0.0, life: 0.5 });
        }
        self.damage_creep(ci, d, src, s.pierce);

        if s.slow > 0.0 {
            self.apply_slow(ci, s.slow);
        }
        let immune = self.creeps[ci].immune;
        if s.poison > 0.0 && !immune {
            let c = &mut self.creeps[ci];
            if s.poison >= c.poison_dps || c.poison_t <= 0.0 {
                c.poison_dps = s.poison;
                c.poison_src = src;
            }
            c.poison_t = POISON_DURATION;
        }
        if s.shred > 0.0 && !immune {
            let c = &mut self.creeps[ci];
            c.shred = c.shred.max(s.shred);
            c.shred_t = SHRED_DURATION;
        }
        let hittable = |c: &Creep| c.hp > 0.0 && c.visible() && !(c.air && s.ground_only);
        if s.burn > 0.0 && !air {
            match self.patches.iter_mut().find(|p| p.src == src && p.pos.distance(pos) < 0.6) {
                Some(p) => {
                    p.t = 0.0;
                    p.dps = p.dps.max(s.burn);
                }
                None => self.patches.push(Patch { pos, dps: s.burn, t: 0.0, src }),
            }
        }
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
                if s.slow > 0.0 {
                    self.apply_slow(j, s.slow);
                }
                hit.push(j);
                last = np;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that seed the global RNG take turns.
    static SEEDED: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Each targeting mode picks the right enemy in range.
    #[test]
    fn targeting() {
        let (bal, _) = Balance::load();
        let map = MapDef::load_all().0.swap_remove(0);
        let mut g = Game::new(bal, map, 0);
        // A tower on the first free cell next to the route, enemies on the route near it.
        let ti = (0..g.grid.len()).find(|&i| g.can_place(g.cell_of(i)).is_ok()).unwrap();
        g.grid[ti] = Occ::Tower(Tower::new(Kind::Gem(0), 2, false));
        let tp = g.center(ti);
        g.phase = Phase::Wave;
        for (hp, traveled) in [(50.0, 3.0), (80.0, 1.0), (20.0, 2.0)] {
            g.spawn_creep(0, tp + vec2(0.4, 0.0), 1, traveled);
            let c = g.creeps.last_mut().unwrap();
            c.hp = hp;
            c.max_hp = 100.0;
            c.speed = 0.0;
        }
        let ids: Vec<u64> = g.creeps.iter().map(|c| c.id).collect();
        for (mode, want) in [(Target::First, ids[0]), (Target::Last, ids[1]), (Target::Strongest, ids[1]), (Target::Weakest, ids[2])] {
            g.projectiles.clear();
            g.set_target(ti, mode);
            if let Occ::Tower(t) = &mut g.grid[ti] {
                t.cooldown = 0.0;
            }
            g.step();
            let shot = g.projectiles.iter().find(|p| p.src == ti).map(|p| p.target);
            assert_eq!(shot, Some(want), "{mode:?}");
        }
    }

    /// A board with no room left: a round in progress goes to choosing; with nothing placed
    /// the round can be skipped.
    #[test]
    fn full_board() {
        let (bal, _) = Balance::load();
        let map = MapDef::load_all().0.swap_remove(0);
        let mut g = Game::new(bal, map, 0);
        // Fill every cell that can take a wall with rock, keeping the route open.
        for i in 0..g.grid.len() {
            if g.can_place(g.cell_of(i)).is_ok() {
                g.grid[i] = Occ::Rock;
            }
        }
        g.board_changed();
        assert!(g.no_room && g.phase == Phase::Build);
        g.skip_round();
        assert_eq!(g.phase, Phase::Wave);

        // Room for exactly one gem: placing it moves on to choosing.
        let (bal, _) = Balance::load();
        let mut g = Game::new(bal, MapDef::load_all().0.swap_remove(0), 0);
        let mut last = None;
        for i in 0..g.grid.len() {
            if g.can_place(g.cell_of(i)).is_ok() {
                if let Some(j) = last {
                    g.grid[j] = Occ::Rock;
                }
                last = Some(i);
            }
        }
        g.board_changed();
        assert!(!g.no_room);
        let c = g.cell_of(last.unwrap());
        g.place(c);
        assert_eq!(g.phase, Phase::Choose);
        assert_eq!(g.placed.len(), 1);
    }

    /// A run saved mid-wave and loaded again plays on exactly like the original.
    #[test]
    fn save_and_resume() {
        let _turn = SEEDED.lock().unwrap_or_else(|e| e.into_inner());
        let (bal, _) = Balance::load();
        let map = MapDef::load_all().0.swap_remove(0);
        let mut g = Game::with_options(bal.clone(), map, 0, Vec::new(), Some(42));
        // Play with the bot until a few waves in and mid-wave.
        while !(g.wave >= 3 && g.phase == Phase::Wave && g.creeps.len() > 2) {
            if g.phase == Phase::Wave {
                g.step();
            } else {
                crate::bot::act(&mut g);
            }
            g.events.clear();
        }
        let text = serde_json::to_string(&g).unwrap();
        let mut h: Game = serde_json::from_str(&text).unwrap();
        h.rebind(bal).unwrap();
        // The simulation is deterministic apart from the global RNG (crits etc.): seed both alike.
        for _ in 0..600 {
            rand::srand(7);
            g.step();
            rand::srand(7);
            h.step();
        }
        assert_eq!((g.lives, g.gold, g.creeps.len(), g.wave), (h.lives, h.gold, h.creeps.len(), h.wave));
        // The gem rolls continue from the same point too.
        assert_eq!(g.roll(1000), h.roll(1000));
    }

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
            if g.phase == Phase::Build && g.placed.is_empty() && trace {
                print!("{}:{} ", g.wave + 1, g.lives);
            }
            if g.phase == Phase::Choose && !g.available_recipes().is_empty() {
                forged += 1;
            }
            if !crate::bot::act(&mut g) {
                assert!(g.phase != Phase::Build, "no valid placement");
                g.step();
            }
        }
        if trace {
            println!();
        }
        (g.wave, g.lives, forged)
    }

    #[test]
    fn seeded_rolls_match() {
        // Two players on the same daily seed get the same gems, whatever else the global RNG does.
        let (bal, _) = Balance::load();
        let map = MapDef::load_all().0.swap_remove(0);
        let gems = |noise: u64| {
            rand::srand(noise);
            let mut g = Game::with_options(bal.clone(), map.clone(), 1, vec![Mutator::Haste], Some(20_000));
            let mut out = Vec::new();
            for x in 5..10 {
                g.place((x, 8));
                let t = g.tower(g.idx((x, 8))).unwrap();
                out.push((t.kind, t.rank));
                let _ = rand::gen_range(0u32, 100);
            }
            out
        };
        assert_eq!(gems(1), gems(999));
    }

    #[test]
    fn maps_round_trip() {
        // Saving and re-reading produces the same maps (writes to a temp dir).
        let (maps, _) = MapDef::load_all();
        let dir = std::env::temp_dir().join(format!("facet-maps-{}", std::process::id()));
        MapDef::save_all_in(&dir, &maps).unwrap();
        let text = std::fs::read_to_string(dir.join("maps.json")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let again: Vec<MapDef> = serde_json::from_str(&text).unwrap();
        assert!(again == maps);
        for m in &maps {
            assert!(m.problem().is_none(), "{}: {:?}", m.name, m.problem());
        }
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
        let _turn = SEEDED.lock().unwrap_or_else(|e| e.into_inner());
        for seed in 1..=6 {
            let (w, l, f) = play(seed);
            println!("seed {seed}: reached wave {} with {l} lives, forged {f}", w + 1);
        }
    }
}
