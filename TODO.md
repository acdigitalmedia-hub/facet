# Facet – Roadmap / TODO

Check items off as they land. Newest decisions go in "Notes" at the bottom.

## 1. Menus & quality of life  ✓
- [x] Pause menu (Esc): Resume, Restart, Settings, How to play, Quit to title
- [x] Settings screen: music volume, effects volume, fullscreen, graphics quality, show health bars (saved)
- [x] How to play page (rules: place, keep, combine, recipes, maze, flyers, bosses, controls)
- [x] First-game tips (short hints shown during the first run)
- [x] Hover tooltips on panel buttons and towers on the board
- [x] Confirmation before removing a rock
- [x] End-of-run summary: waves cleared, kills, damage by tower, best gem

## 2. Gameplay depth  ✓
- [x] New enemy abilities: invisible (needs detector), healers, shielded / magic-immune, sprinters, splitters
- [x] More specials, incl. a second tier (special + special/gems → stronger special)
- [x] New special effects: chain-slow, burning ground, gold on kill
- [x] Seeded daily challenge (same gem rolls for everyone)
- [x] Run mutators (double flyers, no rock removal, …)
- [x] Rebalance after new content (bot: `cd core && MAP=n DIFF=d cargo test --release bot_runs -- --nocapture`)

## 3. Maps  ✓
- [x] More maps (Riverbend, The Crucible, Frozen Crossing)
- [x] In-game map editor (checkpoints, walls, theme) writing to `data/maps.json`

## 4. Visual polish, part 2  ✓ (awaiting playtest)
- [x] Gem-specific attacks (frost bolts, fireballs, poison spit, beams)
- [x] Creature hit reactions
- [x] Ambient weather per theme (snow, leaves, embers, marsh fog)
- [x] Light shift over the course of a run (day → evening)

## 5. Packaging
- [x] Retire the 2D version (removed; it's in git history). 3D client moved to the project root
- [x] Installed-game paths: built-in maps/balance embedded, custom maps in the user folder
- [x] Linux AppImage (`scripts/build-appimage.sh`)
- [x] Windows build (zip, from CI) – needs testing in a VM
- [x] Automatic releases: GitHub Actions on `v*` tags (`.github/workflows/release.yml`)
- [x] First CI run checked, first release (v0.1.0) tagged

## 6. Steam Deck & controller  ✓ (awaiting Deck playtest)
- [x] Controller support: board cursor, face-button actions, camera on right stick/triggers (`src/pad.rs`)
- [x] Menus navigable by controller (focus highlight, A/B, sliders, scrolling) (`src/nav.rs`)
- [x] Controller button hints that switch with the last input device; Controller section in How to Play
- [x] Small screens: Interface size setting (100/115/130%), compact title layout, scrolling side panel
- [x] Fullscreen by default on the Deck; Steam library artwork (`packaging/steam/`); Deck guide in README
- [x] Controller on the PC: 8BitDo Ultimate 2 had X/Y swapped (missing SDL mapping), fixed
- [ ] Playtest on the Deck (Game Mode, non-Steam game)
- [ ] Map editor stays mouse/touch only (by choice)

## 7. In-game updater  ✓
- [x] Title screen checks GitHub releases; Update / What's new / Not now; progress; restart
- [x] AppImage replaces itself in place; Windows zip swaps files (*.old removed next start)
- [x] Downloads checked against SHA256SUMS (published by CI); fixed release file names
- [x] Tested end to end against a local server (update installs; bad checksum refused)
- [ ] Tag v0.1.1 and install it once by hand on each device
- [ ] Windows update path tested in the VM

## Done
- [x] M1–M3: core loop, recipes/specials, bosses, difficulties, endless, synth audio, records (2D)
- [x] Maps + art pass (5 maps)
- [x] 3D: Bevy prototype → full port (egui UI, title, panel, recipe book, overlays)
- [x] Camera reset / top-down views; fixed road + live path dots; steady health bars
- [x] CC0 creature models (Quaternius), Kenney SFX, Junkala music with crossfades, effects pass
- [x] Crossroads 4th checkpoint; smoothed roads and path dots

## Notes
- Map editor: Title → Map Editor. Custom maps save to ~/.local/share/facet/maps.json (backup .bak) and are excluded from the daily rotation. Built-in maps are only editable when run from the project folder (data/maps.json exists in cwd); otherwise they can be saved as a renamed copy. Playtests don't record.
- Adding built-in maps changes which map the daily picks (pool size) – avoid mid-day.
- Item 2 content: Shades (invisible; Amethyst/Obsidian Eye/All-Seeing Eye detect, towers see within 1.6), Menders (heal), Guardians (shield), Imps (sprint), Broodmothers (split), Warded (immune). Specials: Ember Crown (burn), Glacier Arc (chain slow), Gilded Idol (bounty); ascended: Inferno, Tempest, Philosopher's Stone, All-Seeing Eye. Mutator records are keyed separately; daily = 'Daily YYYY-MM-DD'.
- Debug runs (any FACET3D_* env var) never write records.
- Playtest feedback so far: Coil Normal lost at wave 20 (Hydra); synth audio replaced; likes 3D.
- Crossroads got easier after the extra checkpoint (bot 6/6 on Normal) – watch in playtests.
