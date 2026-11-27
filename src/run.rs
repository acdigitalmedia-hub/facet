//! Saved runs: leaving a run (quitting to the title, quitting the game, closing the window)
//! saves it to `run.json` in the user folder, and so does the start of every round (in case the
//! game is killed, e.g. a Steam Deck going to sleep and being shut down). The title screen then
//! offers to continue it. Finishing, restarting or starting another run removes the save.

use std::path::PathBuf;

use facet_core::game::{Game, Phase};
use facet_core::save::user_dir;
use serde::{Deserialize, Serialize};

use crate::Daily;

#[derive(Serialize, Deserialize)]
pub struct RunSave {
    /// The game version that wrote it.
    pub version: String,
    /// Record key ("map / difficulty …" or the daily's), for counting an abandoned run.
    pub key: String,
    /// The main game was already won (an endless run).
    pub won: bool,
    pub daily: Option<Daily>,
    pub game: Game,
}

/// What the title screen shows about a saved run.
#[derive(Clone, Debug)]
pub struct Summary {
    pub title: String,
    pub detail: String,
}

fn path() -> Option<PathBuf> {
    Some(user_dir()?.join("run.json"))
}

/// Debug runs (FACET3D_*) leave the real save alone unless FACET3D_RUNSAVE is set.
pub fn enabled(debug: bool) -> bool {
    !debug || std::env::var_os("FACET3D_RUNSAVE").is_some()
}

/// Is this run worth saving (in progress, not a finished game)?
pub fn resumable(g: &Game) -> bool {
    match g.phase {
        Phase::GameOver => false,
        Phase::Victory => g.endless,
        // Nothing done yet: nothing worth continuing.
        Phase::Build => g.wave > 0 || !g.placed.is_empty(),
        _ => true,
    }
}

/// Same fields as [`RunSave`], borrowed, for writing.
#[derive(Serialize)]
struct RunSaveRef<'a> {
    version: &'a str,
    key: &'a str,
    won: bool,
    daily: &'a Option<Daily>,
    game: &'a Game,
}

pub fn store(version: &str, key: &str, won: bool, daily: &Option<Daily>, game: &Game) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(text) = serde_json::to_string(&RunSaveRef { version, key, won, daily, game }) else { return };
    // Write then rename, so a crash mid-write never leaves a half-written save.
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}

pub fn load() -> Option<RunSave> {
    let text = std::fs::read_to_string(path()?).ok()?;
    match serde_json::from_str(&text) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("saved run: {e}");
            None
        }
    }
}

pub fn delete() {
    if let Some(p) = path() {
        let _ = std::fs::remove_file(p);
    }
}

pub fn summary(s: &RunSave, diff_names: &[String]) -> Summary {
    let g = &s.game;
    let diff = diff_names.get(g.diff).cloned().unwrap_or_default();
    let title = match &s.daily {
        Some(d) => format!("Daily challenge ({})", d.date),
        None => format!("{} · {diff}", g.map.name),
    };
    let wave = if g.endless { format!("Endless wave {}", g.wave + 1) } else { format!("Wave {}", g.wave + 1) };
    Summary { title, detail: format!("{wave} · {} lives · {} gold", g.lives, g.gold) }
}
