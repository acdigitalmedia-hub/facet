//! Balance data. Loaded from `data/balance.json` at runtime if present (so it can be
//! tweaked without recompiling), otherwise falls back to the copy embedded at build time.

use serde::{Deserialize, Serialize};

const EMBEDDED: &str = include_str!("../../data/balance.json");
const EMBEDDED_MAPS: &str = include_str!("../../data/maps.json");

/// A map: grid size, checkpoints, fixed walls and a visual theme.
#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
pub struct MapDef {
    pub name: String,
    pub desc: String,
    /// Ground palette: meadow, autumn, ash, marsh or snow.
    pub theme: String,
    pub w: i32,
    pub h: i32,
    pub waypoints: Vec<(i32, i32)>,
    /// Impassable, unbuildable rectangles: [x, y, width, height].
    #[serde(default)]
    pub walls: Vec<[i32; 4]>,
    /// How walls look: "rock" (cliffs) or "water" (lakes).
    #[serde(default)]
    pub wall_style: String,
    /// Made in the map editor (not part of the daily challenge rotation).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub custom: bool,
}

impl MapDef {
    /// Writes all maps to `data/maps.json`, keeping the previous file as `maps.json.bak`.
    pub fn save_all(maps: &[MapDef]) -> Result<(), String> {
        Self::save_all_in(std::path::Path::new("data"), maps)
    }

    /// Like `save_all`, into another folder.
    pub fn save_all_in(dir: &std::path::Path, maps: &[MapDef]) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join("maps.json");
        if path.exists() {
            let _ = std::fs::copy(&path, dir.join("maps.json.bak"));
        }
        // One field per line in a fixed order, arrays inline: easy to read and hand-edit.
        let j = |v: &dyn erased::Json| v.json();
        let mut out = String::from("[\n");
        for (i, m) in maps.iter().enumerate() {
            let mut fields = vec![
                format!("\"name\": {}", j(&m.name)),
                format!("\"desc\": {}", j(&m.desc)),
                format!("\"theme\": {}", j(&m.theme)),
                format!("\"w\": {}, \"h\": {}", m.w, m.h),
                format!("\"waypoints\": {}", j(&m.waypoints)),
                format!("\"walls\": {}", j(&m.walls)),
                format!("\"wall_style\": {}", j(&m.wall_style)),
            ];
            if m.custom {
                fields.push("\"custom\": true".into());
            }
            out.push_str("  {\n    ");
            out.push_str(&fields.join(",\n    "));
            out.push_str(if i + 1 < maps.len() { "\n  },\n" } else { "\n  }\n" });
        }
        out.push_str("]\n");
        std::fs::write(&path, out).map_err(|e| e.to_string())
    }

    /// Checks that a map is playable: returns a reason if not.
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("Give the map a name".into());
        }
        if self.waypoints.len() < 2 {
            return Some("Place at least a start and an exit checkpoint".into());
        }
        for &(x, y) in &self.waypoints {
            if x < 0 || y < 0 || x >= self.w || y >= self.h {
                return Some("A checkpoint is outside the map".into());
            }
            if self.is_wall(x, y) {
                return Some("A checkpoint is inside a wall".into());
            }
        }
        if crate::path::full_route(self.w, self.h, &self.waypoints, &|x, y| self.is_wall(x, y)).is_none() {
            return Some("The checkpoints aren't connected: open a path between them".into());
        }
        None
    }

    /// Loads `data/maps.json` (falling back to the embedded copy).
    pub fn load_all() -> (Vec<MapDef>, Option<String>) {
        let embedded = || serde_json::from_str::<Vec<MapDef>>(EMBEDDED_MAPS).expect("embedded maps.json is invalid");
        match std::fs::read_to_string("data/maps.json") {
            Ok(text) => match serde_json::from_str::<Vec<MapDef>>(&text) {
                Ok(m) if !m.is_empty() => (m, None),
                Ok(_) => (embedded(), Some("maps.json is empty, using defaults".into())),
                Err(e) => (embedded(), Some(format!("maps.json error, using defaults: {e}"))),
            },
            Err(_) => (embedded(), None),
        }
    }

    pub fn is_wall(&self, x: i32, y: i32) -> bool {
        self.walls.iter().any(|r| x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3])
    }
}

fn one() -> f32 {
    1.0
}

/// Stats shared by basic gems and special towers. Every effect field is optional in
/// the JSON; zero/false means the tower doesn't have that effect.
#[derive(Deserialize, Clone)]
pub struct TowerDef {
    pub name: String,
    pub color: [u8; 3],
    pub damage: f32,
    pub cooldown: f32,
    pub range: f32,
    /// Splash radius (cells). Splash hits take a fraction of the damage.
    #[serde(default)]
    pub splash: f32,
    /// Slow fraction (0.25 = 25% slower) for a couple of seconds.
    #[serde(default)]
    pub slow: f32,
    /// Poison damage per second (ignores armor).
    #[serde(default)]
    pub poison: f32,
    /// Number of extra targets lightning jumps to.
    #[serde(default)]
    pub chain: f32,
    /// Critical strike chance.
    #[serde(default)]
    pub crit: f32,
    /// Ignores armor.
    #[serde(default)]
    pub pierce: bool,
    /// Temporarily reduces target armor by this much.
    #[serde(default)]
    pub shred: f32,
    /// Damage bonus granted to other towers nearby (0.25 = +25%).
    #[serde(default)]
    pub aura: f32,
    /// Damage multiplier against flying enemies.
    #[serde(default = "one")]
    pub air_mult: f32,
    /// Can't attack flying enemies.
    #[serde(default)]
    pub ground_only: bool,
    /// Reveals invisible enemies within its range (for every tower).
    #[serde(default)]
    pub detect: bool,
    /// Sets the ground on fire where it hits: damage per second to ground enemies standing in it.
    #[serde(default)]
    pub burn: f32,
    /// Extra gold for each enemy this tower finishes off.
    #[serde(default)]
    pub bounty: f32,
    pub detail: String,
    /// Specials only: ingredients as (gem or special name, tier index; ignored for specials).
    #[serde(default)]
    pub recipe: Vec<(String, usize)>,
    /// Specials made from another special.
    #[serde(skip)]
    pub ascended: bool,
}

/// One recipe ingredient.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ing {
    /// A gem of this type and tier.
    Gem(usize, usize),
    /// A special tower of this kind (any level).
    Special(usize),
}

#[derive(Deserialize, Clone)]
pub struct WaveKind {
    pub name: String,
    /// Which creature drawing to use (beetle, runner, brute, armored, swarm, bat, golem,
    /// wraith, hydra, hydraling, leviathan).
    #[serde(default)]
    pub look: String,
    pub color: [u8; 3],
    pub count: u32,
    pub hp_mult: f32,
    pub speed_mult: f32,
    pub armor: f32,
    /// Visual radius in cells.
    pub size: f32,
    #[serde(default)]
    pub air: bool,
    #[serde(default)]
    pub boss: bool,
    /// Lives lost if one gets through (bosses use the difficulty's boss_lives instead).
    #[serde(default = "one_life")]
    pub lives: i32,
    /// Short description of special abilities, shown in the wave preview.
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub unslowable: bool,
    /// Fraction of max HP regenerated per second.
    #[serde(default)]
    pub regen: f32,
    /// On death, splits into `split_count` enemies of this kind.
    #[serde(default)]
    pub split_into: Option<String>,
    #[serde(default)]
    pub split_count: u32,
    #[serde(skip)]
    pub split_kind: Option<usize>,
    /// Can only be targeted when revealed by a detector tower (or right next to a tower).
    #[serde(default)]
    pub invisible: bool,
    /// Heals other enemies within `HEAL_RANGE` by this fraction of their max HP per second.
    #[serde(default)]
    pub heal: f32,
    /// Shield worth this fraction of max HP; absorbs damage first and recharges when unhit.
    #[serde(default)]
    pub shield: f32,
    /// Speed multiplier during periodic sprints (0 = never sprints).
    #[serde(default)]
    pub sprint: f32,
    /// Immune to slows, poison and armor shred.
    #[serde(default)]
    pub immune: bool,
}

#[derive(Deserialize, Clone)]
pub struct Difficulty {
    pub name: String,
    pub desc: String,
    pub hp_growth: f32,
    pub lives: i32,
    pub boss_lives: i32,
    pub gold_mult: f32,
}

fn one_life() -> i32 {
    1
}

#[derive(Deserialize, Clone)]
pub struct WaveDefs {
    pub base_hp: f32,
    pub armor_per_waves: u32,
    pub base_speed: f32,
    pub base_gold: f32,
    pub gold_per_wave: f32,
    pub wave_bonus: u32,
    pub spawn_interval: f32,
    pub kinds: Vec<WaveKind>,
    /// Which kind (by name) each wave uses. Its length is the number of waves.
    pub sequence: Vec<String>,
    /// Endless mode: waves after the main sequence cycle through this list.
    pub endless_sequence: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub struct Balance {
    pub difficulties: Vec<Difficulty>,
    pub start_gold: u32,
    pub gems_per_round: usize,
    pub rock_removal_cost: u32,
    pub tier_names: Vec<String>,
    pub tier_damage_mult: Vec<f32>,
    pub range_per_rank: f32,
    pub luck_costs: Vec<u32>,
    pub luck_odds: Vec<Vec<u32>>,
    pub gems: Vec<TowerDef>,
    pub specials: Vec<TowerDef>,
    pub special_level_mult: Vec<f32>,
    pub special_upgrade_costs: Vec<u32>,
    pub waves: WaveDefs,

    /// Resolved from the JSON names at load time.
    #[serde(skip)]
    pub recipes: Vec<Vec<Ing>>,
    #[serde(skip)]
    pub wave_kinds: Vec<usize>,
    #[serde(skip)]
    pub endless_kinds: Vec<usize>,
}

impl Balance {
    pub fn load() -> (Balance, Option<String>) {
        match std::fs::read_to_string("data/balance.json") {
            Ok(text) => match Self::parse(&text) {
                Ok(b) => (b, None),
                Err(e) => (Self::embedded(), Some(format!("balance.json error, using defaults: {e}"))),
            },
            Err(_) => (Self::embedded(), None),
        }
    }

    fn embedded() -> Balance {
        Self::parse(EMBEDDED).expect("embedded balance.json is invalid")
    }

    fn parse(text: &str) -> Result<Balance, String> {
        let mut b: Balance = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let gem_idx = |n: &str| b.gems.iter().position(|g| g.name == n);
        let special_idx = |n: &str| b.specials.iter().position(|g| g.name == n);
        let mut recipes = Vec::new();
        for (si, s) in b.specials.iter().enumerate() {
            let mut r = Vec::new();
            for (name, tier) in &s.recipe {
                if let Some(g) = gem_idx(name) {
                    if *tier >= b.tier_names.len() {
                        return Err(format!("{}: tier {tier} out of range", s.name));
                    }
                    r.push(Ing::Gem(g, *tier));
                } else if let Some(sp) = special_idx(name).filter(|&sp| sp != si) {
                    r.push(Ing::Special(sp));
                } else {
                    return Err(format!("{}: unknown ingredient '{name}'", s.name));
                }
            }
            if r.is_empty() {
                return Err(format!("{}: empty recipe", s.name));
            }
            recipes.push(r);
        }
        for (s, r) in b.specials.iter_mut().zip(&recipes) {
            s.ascended = r.iter().any(|i| matches!(i, Ing::Special(_)));
        }
        b.recipes = recipes;
        let kinds = &b.waves.kinds;
        let kind_idx = |name: &str| kinds.iter().position(|k| k.name == name).ok_or(format!("unknown wave kind '{name}'"));
        b.wave_kinds = b.waves.sequence.iter().map(|n| kind_idx(n)).collect::<Result<_, _>>()?;
        b.endless_kinds = b.waves.endless_sequence.iter().map(|n| kind_idx(n)).collect::<Result<_, _>>()?;
        let splits: Vec<Option<usize>> =
            kinds.iter().map(|k| k.split_into.as_deref().map(kind_idx).transpose()).collect::<Result<_, _>>()?;
        for (k, s) in b.waves.kinds.iter_mut().zip(splits) {
            k.split_kind = s;
        }
        if b.difficulties.is_empty() || b.wave_kinds.is_empty() || b.endless_kinds.is_empty() {
            return Err("need at least one difficulty, wave and endless wave".into());
        }
        Ok(b)
    }

    pub fn max_tier(&self) -> usize {
        self.tier_damage_mult.len() - 1
    }

    /// Number of waves in the main (non-endless) game.
    pub fn wave_count(&self) -> u32 {
        self.wave_kinds.len() as u32
    }

    /// Wave kind for 0-based wave `w`, continuing into the endless sequence.
    pub fn kind_for_wave(&self, w: u32) -> usize {
        let w = w as usize;
        if w < self.wave_kinds.len() {
            self.wave_kinds[w]
        } else {
            self.endless_kinds[(w - self.wave_kinds.len()) % self.endless_kinds.len()]
        }
    }
}

/// Tiny helper so `save_all` can format different field types the same way.
mod erased {
    pub trait Json {
        fn json(&self) -> String;
    }
    impl<T: serde::Serialize> Json for T {
        fn json(&self) -> String {
            serde_json::to_string(self).unwrap_or_default()
        }
    }
}
