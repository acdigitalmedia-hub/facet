# Facet

An original 3D gem-themed maze-building tower defense, inspired by the classic
Warcraft III / StarCraft II custom-map genre. Built in Rust with the Bevy engine.

## Play

Download a release from the GitHub **Releases** page:

- **Linux:** `Facet-x86_64.AppImage` – make it executable
  (`chmod +x Facet-*.AppImage`, or Properties → "Allow executing") and run it.
- **Windows:** unzip `Facet-windows.zip` and run `Facet.exe` (keep the `assets`
  folder next to it).

- **Steam Deck:** see [Steam Deck](#steam-deck) below.

**Updates:** the game checks GitHub for a new release when it starts. If there is one, the
title screen offers to download and install it (the AppImage replaces itself; on Windows the
files are swapped and the old ones cleaned up next start), then restart. Your saves are kept.
Turn it off under Settings → Check for updates.

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

**Saved runs:** leaving a run (Save & Quit, closing the window) saves it, and so does the start
of every round. Next time the title screen asks whether to continue it.

**Targeting:** select a tower to set what it shoots at: First, Last, Strongest or Weakest.

**Misclicks:** turn on Settings → Click twice to place: the first click marks a cell, the second
places the gem there.

Difficulties: Normal, Hard, Nightmare; Endless after a win; a daily challenge (same gem rolls for
everyone that day) and run mutators on the title screen.

Controls: click to place / select · `Tab`/`Shift+Tab` next / previous tower · `T` targeting · `K` keep · `C` combine · `U` quality · `X` remove rock ·
`L` upgrade special · `B` recipe book · `1`/`2`/`3` speed · `Space` pause · `M`/`N` music/sound ·
`WASD` or arrows pan · `Q`/`E` or right-drag rotate · scroll zoom · middle-drag pan ·
`Home` reset view · `V` top-down view · `F11` fullscreen · `Esc` menu.

Controller (Xbox, PlayStation or Steam Deck): left stick moves a cursor over the board ·
`A` place / select · `B` deselect / back · `X` keep, remove a selected rock or upgrade a selected
special · `Y` forge (when a recipe is ready), combine, or change targeting · `LB`/`RB` jump between towers (the new
gems while choosing) · D-pad ↑ gem quality, ↓ top-down view, ←/→ game speed · right stick camera ·
`LT`/`RT` zoom · `L3` reset view · `R3` pause · `Start` menu · `Select` recipe book. In menus the
stick or D-pad moves the highlight, `A` chooses, `B` goes back. Touching the mouse or trackpad
switches the on-screen hints back to keyboard and mouse.

## Steam Deck

1. In Desktop Mode, download the AppImage (e.g. into `~/Applications`). Right-click it →
   Properties → Permissions → tick "Is executable".
2. Open Steam → **Games → Add a Non-Steam Game to My Library…** → **Browse…**, set the file type
   to "All Files", pick the AppImage and click **Add Selected Programs**.
3. Optional artwork: `packaging/steam/` has a library capsule (`capsule.png`, 600×900), a wide
   capsule (`capsule_wide.png`), a hero banner (`hero.png`) and a logo (`logo.png`). In the Steam
   library, right-click Facet → **Manage → Set custom artwork** for the capsules; right-click the
   banner on its page for the hero and logo.
4. Back in Game Mode, launch Facet from the library. It starts fullscreen on the Deck. Steam's
   default gamepad layout works as is; the right trackpad acts as a mouse.
5. If the text feels small, open **Settings → Interface size** and pick 115% or 130%.

The Deck's own performance menu (… button → battery icon) can cap the frame rate to save battery;
the Low and Medium graphics settings also help.

## Build from source

Needs Rust (stable) and, on Linux, the ALSA and udev development packages
(Fedora: `sudo dnf install alsa-lib-devel systemd-devel`; Ubuntu: `sudo apt install libasound2-dev libudev-dev`).

    cargo run --release

`scripts/install-desktop.sh` builds it and adds a "Facet" launcher to your app menu
(`--remove` to take it out). `scripts/build-appimage.sh` builds `dist/Facet-x86_64.AppImage`.

### Layout

- `src/` – the Bevy client: 3D scene (`view.rs`, `terrain.rs`, `mesh.rs`, `env.rs`, `atmosphere.rs`,
  `creatures.rs`), egui interface (`ui.rs`, `menus.rs`, `editor.rs`), controller (`pad.rs`, `nav.rs`),
  audio (`audio.rs`).
- `core/` – the game rules (data, pathfinding, simulation, saves, the test bot). No graphics.
- `data/` – `balance.json` (all numbers: gems, odds, waves, recipes) and `maps.json` (built-in maps).
  Both are built into the game; when run from the project folder the files are read at startup,
  so edits apply on relaunch without recompiling.
- `assets/` – creature models, sound effects, music and fonts (see `assets/CREDITS.md`).
- `packaging/` – icon, desktop entry and Steam library artwork. `.github/workflows/release.yml` – release builds.

### Tests and balance

    cargo test --release -p facet-core -- --nocapture

runs the rules tests and a headless bot through 6 games as a balance check
(`MAP=n`, `DIFF=0|1|2`, `TRACE=1`, `ENDLESS=1`).

Debug helpers (debug runs never write records): `FACET3D_MAP=n`, `FACET3D_PLAY=difficulty`
(skip the title), `FACET3D_WAVE=n` (bot fast-forwards to wave n; `FACET3D_INTO=1` stops mid-wave, `FACET3D_PLACE=n` places n gems),
`FACET3D_BOOK=1`, `FACET3D_EDITOR=1`, `FACET_ASSETS=path` (assets folder),
`FACET3D_SIZE=1280x800` (window size), `FACET3D_UISCALE=1.3`, `FACET3D_PADLOG=1` (print controller buttons), `FACET3D_FAKE_VERSION=0.0.1` (pretend to be old, to test
updating; `FACET3D_UPDATE=1` also installs without asking), `FACET3D_NOUPDATE=1` (skip the check), `FACET3D_RUNSAVE=1` (debug runs normally leave the saved
run alone; this lets them save and load it), `FACET3D_RESUME=1` (continue the saved run at once), `FACET3D_UPDATE_API=url` (read the release JSON from elsewhere), `FACET3D_PAD=1` (controller hints
without a controller), `FACET3D_NOUI=1` (hide the interface), `FACET3D_SHOT=out.png` (save a
screenshot of the game window after `FACET3D_SHOT_AT` seconds, default 6, then quit).

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

Write the release notes in the GitHub release afterwards if you like: the game's update box
shows them under "What's new". GitHub Actions builds the AppImage (on Ubuntu 22.04, so it runs on older distros too) and the
Windows zip, plus a `SHA256SUMS` file the updater checks downloads against, and attaches them
to a new release. The workflow can also be run by hand from the
Actions tab to get test builds without a release.

## Credits

Creature models by Quaternius, sound effects by Kenney, music by Juhani Junkala (all CC0).
Fonts: Cinzel and Liberation Sans (SIL Open Font License). Details in `assets/CREDITS.md`.
