//! Persistent records and settings, stored as JSON in the user's data directory
//! (`$XDG_DATA_HOME/facet/save.json`, usually `~/.local/share/facet/save.json`).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Record {
    pub runs: u32,
    pub wins: u32,
    /// Most waves cleared in a single run (including endless waves).
    pub best_cleared: u32,
    /// Most lives left when winning the main 20 waves.
    pub best_win_lives: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Quality {
    Low,
    Medium,
    #[default]
    High,
}

/// Player preferences (all optional in the file, so older saves still load).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// 0..1
    pub music_volume: f32,
    /// 0..1
    pub sfx_volume: f32,
    pub fullscreen: bool,
    pub quality: Quality,
    pub health_bars: bool,
    pub floating_text: bool,
    /// Show first-game tips.
    pub tips: bool,
    /// Tips already dismissed.
    pub seen_tips: Vec<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            music_volume: 0.8,
            sfx_volume: 0.8,
            fullscreen: false,
            quality: Quality::High,
            health_bars: true,
            floating_text: true,
            tips: true,
            seen_tips: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Save {
    pub music: bool,
    pub sfx: bool,
    /// Keyed by "map / difficulty".
    pub records: HashMap<String, Record>,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for Save {
    fn default() -> Save {
        Save { music: true, sfx: true, records: HashMap::new(), settings: Settings::default() }
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("facet").join("save.json"))
}

impl Save {
    pub fn load() -> Save {
        let mut save: Save =
            path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        // Records from before maps existed were all played on Crossroads.
        let old: Vec<String> = save.records.keys().filter(|k| !k.contains(" / ")).cloned().collect();
        for k in old {
            if let Some(r) = save.records.remove(&k) {
                save.records.insert(format!("Crossroads / {k}"), r);
            }
        }
        save
    }

    pub fn store(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, text);
        }
    }

    pub fn record(&self, difficulty: &str) -> Record {
        self.records.get(difficulty).cloned().unwrap_or_default()
    }

    /// Records the main game being won. Returns true if it's a new best.
    pub fn record_win(&mut self, difficulty: &str, lives: i32, cleared: u32) -> bool {
        let r = self.records.entry(difficulty.to_string()).or_default();
        r.runs += 1;
        r.wins += 1;
        let better = r.best_win_lives.is_none_or(|b| lives > b);
        if better {
            r.best_win_lives = Some(lives);
        }
        r.best_cleared = r.best_cleared.max(cleared);
        self.store();
        better
    }

    /// Records a run ending in defeat. `counted` is false when the run was already
    /// counted by a win (an endless run after victory). Returns true if it's a new best.
    pub fn record_end(&mut self, difficulty: &str, cleared: u32, counted: bool) -> bool {
        let r = self.records.entry(difficulty.to_string()).or_default();
        if !counted {
            r.runs += 1;
        }
        let better = cleared > r.best_cleared;
        r.best_cleared = r.best_cleared.max(cleared);
        self.store();
        better
    }
}
