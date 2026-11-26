# Facet (working title)

An original gem-themed maze-building tower defense, inspired by the classic
Warcraft III / StarCraft II custom-map genre.

## Run

    cargo run --release

Or install a desktop launcher (adds "Facet" with a gem icon to your app menu):

    scripts/install-desktop.sh            # install or update
    scripts/install-desktop.sh --remove   # uninstall

## How to play

Pick a map (Crossroads, The Coil, Hourglass, Shattered Lakes, Serpent Pass), then a difficulty
on the title screen: **Normal** (learn the game), **Hard** (faster scaling,
15 lives, escaped bosses cost 10), or **Nightmare** (brutal scaling, 10 lives, any escaped boss
ends the run). Beat all 20 waves, then press `E` to continue in **Endless** mode.

- **Build:** each round you place 5 gems. You don't know what a gem is until it lands.
  Every gem is also a wall, so use them to stretch the enemies' path through the
  checkpoints (S -> 1 -> 2 -> 3 -> 4 -> E). Hovering an empty cell previews the new path.
- **Choose:** click one of the glowing new gems and **Keep** it. The other four turn to rock
  (rocks still count as walls). If two of the new gems match exactly, **Combine** them into
  the next quality tier.
  Four of a kind combine for +2 tiers.
- **Recipes:** special towers are forged from 3 specific gems (type + tier). Ingredients can be
  any mix of this round's new gems and towers you kept earlier. Open the **Recipe Book** (`B`) to
  see them all; forgeable ingredients glow pink on the board. If a forge uses a new gem, it counts
  as your keep for the round.
- **Wave:** enemies walk the maze. **Flyers** ignore it and go straight from checkpoint to
  checkpoint (dashed lines show their path). **Bosses** every 5 waves have abilities: the
  Juggernaut is immune to slows, the Wraith flies, the Hydra splits into 6 when killed, and the
  Leviathan (endless) regenerates.
- **Planning:** the side panel lists recipes you're one ingredient away from.
- **Gold:** raise **Gem Quality** for better odds, upgrade special towers (up to Lv 3), or
  remove rocks to rework your maze (click a rock between waves).

Keys: `1`/`2`/`3` game speed, `Space` pause, `U` upgrade quality, `K` keep, `C` combine,
`B` recipe book, `L` upgrade selected special, `X` remove selected rock,
`M` music on/off, `N` sound on/off, `Esc` deselect, `R` back to title (after a game ends), `E` continue endless (after victory).
The game pauses while the recipe book is open.

## Sound, records and settings

All music and sound effects are synthesized in code at startup (`src/audio.rs`); there are no
audio files. Records per difficulty (wins, best lives left, furthest wave) and the music/sound
settings are saved to `~/.local/share/facet/save.json`.

The app icon is drawn in code too (`src/icon.rs`); `cargo run --release --bin make-icon -- out.png 256`
writes it as a PNG.

## Art

Everything is drawn procedurally (`src/art.rs`): each map's terrain is baked into a texture at
startup (ground noise, grass/leaves/embers, cliffs or lakes, vignette), the dirt road is redrawn
along the current maze, and gems, pedestals, special towers, creatures and checkpoints are built
from shaded polygons. Fonts are Cinzel (headings) and Liberation Sans (body), both bundled under
the SIL Open Font License in `assets/fonts/`.

## Maps

Maps live in `data/maps.json`: grid size, checkpoints, wall rectangles (`[x, y, w, h]`), a
`theme` (meadow, autumn, ash, marsh, snow) and `wall_style` (rock or water). Records are kept
per map and difficulty. `cargo test --release maps_are_valid` checks every map is connected.

## Tuning

All balance numbers (gems, tiers, odds, waves, special recipes, wave order) live in
`data/balance.json`. The game reads it at startup, so edit and relaunch, no recompile needed.

`cargo test --release -- --nocapture` runs a headless bot through 6 games as a
balance sanity check. Env vars: `TRACE=1` prints lives per wave, `DIFF=0|1|2` picks the
difficulty, `MAP=n` picks the map, `ENDLESS=1` keeps playing past wave 20.

## 3D version

The game now plays fully in 3D with the Bevy engine (launcher: "Facet 3D").

- `core/` – the game rules (data, pathfinding, simulation, saves, the test bot). No graphics.
  Run the balance tests here: `cd core && cargo test --release -- --nocapture`.
- `facet3d/` – the Bevy client: 3D scene (`view.rs`, `terrain.rs`, `mesh.rs`, `env.rs`),
  egui interface (`ui.rs`: title, side panel, recipe book, overlays), synthesized audio (`audio.rs`).
  Build: `cd facet3d && cargo build --release`; run from the project root so `data/` edits apply:
  `facet3d/target/release/facet3d`.
- Records are shared with the 2D game (same save file).
- Assets in `facet3d/assets/` (all CC0, see `facet3d/assets/CREDITS.md`): animated creature
  models (Quaternius), sound effects (Kenney), music (Juhani Junkala). The game finds this
  folder next to the executable or two levels up from it (the build directory).
- Music follows the game: title theme, a building track per map setting, battle tracks during
  waves, a boss track during boss waves; tracks crossfade.
- The title screen shows the selected map with the bot playing it.
- Debug helpers: `FACET3D_MAP=n`, `FACET3D_PLAY=difficulty` (skip the title), `FACET3D_WAVE=n`
  (bot fast-forwards to wave n; add `FACET3D_INTO=1` to stop mid-wave), `FACET3D_BOOK=1`.
- The original 2D game (`src/`) still builds and plays; it keeps its own copy of the rules and
  will be retired once the 3D version has been playtested.

Controls (3D): click to place / select · K keep · C combine · U quality · X remove rock ·
L upgrade special · B recipe book · 1/2/3 speed · Space pause · M/N music/sound ·
WASD pan · Q/E or right-drag rotate · scroll zoom · middle-drag pan · right-click deselect ·
Home reset view · V toggle top-down view.

The dirt road always shows the default route (checkpoint order). While placing gems, glowing
dots flow along the route enemies will actually take (previewing a hovered placement); they
hide during waves.

## Map editor

Title screen → **Map Editor**. Paint walls (cliffs or water), place checkpoints (Start → 1 → … → Exit,
reorder them in the list), pick the setting and size, and watch the enemy route update live. **Playtest**
plays the map without saving (quitting returns to the editor; no records). **Save** writes
`data/maps.json` and keeps the previous file as `data/maps.json.bak`. Maps made in the editor are
marked `"custom": true` and never appear in the daily challenge.
