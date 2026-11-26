# Facet

An original 3D gem-themed maze-building tower defense, inspired by the classic
Warcraft III / StarCraft II custom-map genre. Built in Rust with the Bevy engine.

## Play

Download a release from the GitHub **Releases** page:

- **Linux:** `Facet-<version>-x86_64.AppImage` – make it executable
  (`chmod +x Facet-*.AppImage`, or Properties → "Allow executing") and run it.
- **Windows:** unzip `Facet-<version>-windows.zip` and run `Facet.exe` (keep the `assets`
  folder next to it).

Saves, settings and your own maps live in `~/.local/share/facet/` on Linux and
`%APPDATA%\facet\` on Windows.

## How to play

The in-game **How to Play** page (title screen, or `Esc` → How to Play) explains everything. In short:

- **Build:** each round you place 5 random gems. Every gem is also a wall, so use them to stretch
  the enemies' path through the checkpoints (Start → 1 → 2 → … → Exit).
- **Choose:** **Keep** one of the new gems; the rest turn to rock. Two matching gems **Combine**
  into a better one, and recipes of 3 specific gems forge **special towers** (Recipe Book, `B`).
- **Wave:** enemies walk your maze. Flyers ignore it; bosses every 5 waves have abilities; some
  enemies are invisible, heal, shield, sprint or split.
- **Gold:** raise gem quality, upgrade specials, or remove rocks to rework the maze.

Difficulties: Normal, Hard, Nightmare; Endless after a win; a daily challenge (same gem rolls for
everyone that day) and run mutators on the title screen.

Controls: click to place / select · `K` keep · `C` combine · `U` quality · `X` remove rock ·
`L` upgrade special · `B` recipe book · `1`/`2`/`3` speed · `Space` pause · `M`/`N` music/sound ·
`WASD` or arrows pan · `Q`/`E` or right-drag rotate · scroll zoom · middle-drag pan ·
`Home` reset view · `V` top-down view · `F11` fullscreen · `Esc` menu.

## Build from source

Needs Rust (stable) and, on Linux, the ALSA and udev development packages
(Fedora: `sudo dnf install alsa-lib-devel systemd-devel`; Ubuntu: `sudo apt install libasound2-dev libudev-dev`).

    cargo run --release

`scripts/install-desktop.sh` builds it and adds a "Facet" launcher to your app menu
(`--remove` to take it out). `scripts/build-appimage.sh` builds `dist/Facet-<version>-x86_64.AppImage`.

### Layout

- `src/` – the Bevy client: 3D scene (`view.rs`, `terrain.rs`, `mesh.rs`, `env.rs`, `atmosphere.rs`,
  `creatures.rs`), egui interface (`ui.rs`, `menus.rs`, `editor.rs`), audio (`audio.rs`).
- `core/` – the game rules (data, pathfinding, simulation, saves, the test bot). No graphics.
- `data/` – `balance.json` (all numbers: gems, odds, waves, recipes) and `maps.json` (built-in maps).
  Both are built into the game; when run from the project folder the files are read at startup,
  so edits apply on relaunch without recompiling.
- `assets/` – creature models, sound effects, music and fonts (see `assets/CREDITS.md`).
- `packaging/` – icon and desktop entry. `.github/workflows/release.yml` – release builds.

### Tests and balance

    cargo test --release -p facet-core -- --nocapture

runs the rules tests and a headless bot through 6 games as a balance check
(`MAP=n`, `DIFF=0|1|2`, `TRACE=1`, `ENDLESS=1`).

Debug helpers (debug runs never write records): `FACET3D_MAP=n`, `FACET3D_PLAY=difficulty`
(skip the title), `FACET3D_WAVE=n` (bot fast-forwards to wave n; `FACET3D_INTO=1` stops mid-wave),
`FACET3D_BOOK=1`, `FACET3D_EDITOR=1`, `FACET_ASSETS=path` (assets folder).

## Map editor

Title screen → **Map Editor**. Paint walls (cliffs or water), place and reorder checkpoints, pick
the setting and size, and watch the enemy route update live. **Playtest** plays it without saving.
**Save** stores your maps in the user folder (`maps.json`, previous version kept as `maps.json.bak`).
Your maps never appear in the daily challenge.

Built-in maps can be opened and saved as your own copy under a new name. When the game runs from
the project folder (`cargo run`, or the `install-desktop.sh` launcher) the editor can also change
the built-in maps themselves, writing `data/maps.json`.

## Releases

Bump `version` in `Cargo.toml`, commit, then tag and push:

    git tag v0.1.0 && git push origin v0.1.0

GitHub Actions builds the AppImage (on Ubuntu 22.04, so it runs on older distros too) and the
Windows zip and attaches both to a new release. The workflow can also be run by hand from the
Actions tab to get test builds without a release.

## Credits

Creature models by Quaternius, sound effects by Kenney, music by Juhani Junkala (all CC0).
Fonts: Cinzel and Liberation Sans (SIL Open Font License). Details in `assets/CREDITS.md`.
