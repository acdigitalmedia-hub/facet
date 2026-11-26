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
- [ ] Retire the 2D version (`src/`, macroquad) once 3D is confirmed complete
- [ ] Linux AppImage or Flatpak
- [ ] Windows build

## Done
- [x] M1–M3: core loop, recipes/specials, bosses, difficulties, endless, synth audio, records (2D)
- [x] Maps + art pass (5 maps)
- [x] 3D: Bevy prototype → full port (egui UI, title, panel, recipe book, overlays)
- [x] Camera reset / top-down views; fixed road + live path dots; steady health bars
- [x] CC0 creature models (Quaternius), Kenney SFX, Junkala music with crossfades, effects pass
- [x] Crossroads 4th checkpoint; smoothed roads and path dots

## Notes
- Map editor: Title → Map Editor. Saves to data/maps.json (backup maps.json.bak); editor-made maps are `custom` and excluded from the daily rotation. Playtests don't record.
- Adding built-in maps changes which map the daily picks (pool size) – avoid mid-day.
- Item 2 content: Shades (invisible; Amethyst/Obsidian Eye/All-Seeing Eye detect, towers see within 1.6), Menders (heal), Guardians (shield), Imps (sprint), Broodmothers (split), Warded (immune). Specials: Ember Crown (burn), Glacier Arc (chain slow), Gilded Idol (bounty); ascended: Inferno, Tempest, Philosopher's Stone, All-Seeing Eye. Mutator records are keyed separately; daily = 'Daily YYYY-MM-DD'.
- The old 2D game can't read the new balance.json (special ingredients) and falls back to its embedded copy.
- Debug runs (any FACET3D_* env var) never write records.
- Playtest feedback so far: Coil Normal lost at wave 20 (Hydra); synth audio replaced; likes 3D.
- Crossroads got easier after the extra checkpoint (bot 6/6 on Normal) – watch in playtests.
