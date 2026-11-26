//! Menus and helpers layered over the game: pause menu, settings, how to play, first-game
//! tips, the rock-removal confirmation, board tooltips and the end-of-run summary.

use bevy_egui::egui::{self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Stroke, Vec2 as V2};
use facet_core::game::{Game, Kind, Occ, Phase};
use facet_core::save::{Quality, Settings};

use crate::nav::{self, Group, NavExt};
use crate::pad::{self, hint};
use crate::ui::{alpha, bold, c32, diff_color, effects_text, heading, lerp32, small, tower_icon, BAD, DIM, GOLD, GOOD, INFO, RECIPE, TEXT};

/// Which menu page is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Pause,
    Settings,
    Help,
}

/// Something the menus ask the game to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Request {
    Restart,
    Quit,
    Endless,
}

fn frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(14, 12, 10))
        .inner_margin(Margin::same(22))
        .stroke(Stroke::new(1.5, alpha(GOLD, 0.6)))
        .corner_radius(CornerRadius::same(4))
}

fn dim_behind(ctx: &egui::Context, area: Rect, id: &'static str) {
    ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, id.into())).rect_filled(area, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(0, 0, 0, 140));
}

fn big_button(ui: &mut egui::Ui, label: &str, w: f32, clicked: &mut bool) -> bool {
    let b = egui::Button::new(RichText::new(label).family(heading()).size(19.0)).min_size(V2::new(w, 38.0));
    let r = ui.add(b).nav_clicked();
    if r {
        *clicked = true;
    }
    r
}

/// The pause menu. Returns a request when a button asks for one.
pub fn pause(ctx: &egui::Context, view: Rect, page: &mut Option<Page>, clicked: &mut bool) -> Option<Request> {
    nav::group(ctx, Group::Modal);
    dim_behind(ctx, view, "pause_dim");
    let mut req = None;
    egui::Area::new("pause".into()).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(view.center()).show(ctx, |ui| {
        frame().show(ui, |ui| {
            ui.set_width(300.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Paused").family(heading()).size(34.0).color(GOLD));
                ui.add_space(10.0);
                let w = 260.0;
                if big_button(ui, "Resume", w, clicked) {
                    *page = None;
                }
                if big_button(ui, "Restart", w, clicked) {
                    req = Some(Request::Restart);
                }
                if big_button(ui, "Settings", w, clicked) {
                    *page = Some(Page::Settings);
                }
                if big_button(ui, "How to Play", w, clicked) {
                    *page = Some(Page::Help);
                }
                if big_button(ui, "Quit to Title", w, clicked) {
                    req = Some(Request::Quit);
                }
                ui.add_space(6.0);
                small(ui, if pad::active() { "B to resume" } else { "Esc to resume" }, DIM);
            });
        });
    });
    req
}

/// The settings page. Returns true when the player closes it.
pub fn settings(ctx: &egui::Context, area: Rect, s: &mut Settings, music: &mut bool, sfx: &mut bool, clicked: &mut bool) -> bool {
    nav::group(ctx, Group::Modal);
    dim_behind(ctx, area, "settings_dim");
    let mut close = false;
    egui::Area::new("settings".into()).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(area.center()).show(ctx, |ui| {
        frame().show(ui, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new("Settings").family(heading()).size(30.0).color(GOLD));
            ui.add_space(6.0);
            let label = |ui: &mut egui::Ui, t: &str| {
                ui.add_sized(V2::new(150.0, 22.0), egui::Label::new(RichText::new(t).size(16.0)));
            };
            egui::Grid::new("settings_grid").num_columns(2).spacing(V2::new(12.0, 10.0)).show(ui, |ui| {
                label(ui, "Music");
                ui.horizontal(|ui| {
                    let r = ui.checkbox(music, "");
                    nav::toggle(&r, music);
                    let r = ui.add_enabled(*music, egui::Slider::new(&mut s.music_volume, 0.0..=1.0).show_value(false));
                    nav::slider(&r, &mut s.music_volume);
                });
                ui.end_row();
                label(ui, "Sound effects");
                ui.horizontal(|ui| {
                    let r = ui.checkbox(sfx, "");
                    nav::toggle(&r, sfx);
                    let r = ui.add_enabled(*sfx, egui::Slider::new(&mut s.sfx_volume, 0.0..=1.0).show_value(false));
                    nav::slider(&r, &mut s.sfx_volume);
                });
                ui.end_row();
                label(ui, if pad::active() { "Fullscreen" } else { "Fullscreen  [F11]" });
                let r = ui.checkbox(&mut s.fullscreen, "");
                nav::toggle(&r, &mut s.fullscreen);
                ui.end_row();
                label(ui, "Graphics");
                ui.horizontal(|ui| {
                    for (q, name) in [(Quality::Low, "Low"), (Quality::Medium, "Medium"), (Quality::High, "High")] {
                        if ui.selectable_label(s.quality == q, RichText::new(name).size(15.0)).nav_clicked() {
                            s.quality = q;
                            *clicked = true;
                        }
                    }
                })
                .response
                .on_hover_text("Low: no shadows or glow. Medium: no anti-aliasing. High: everything.");
                ui.end_row();
                label(ui, "Interface size");
                ui.horizontal(|ui| {
                    for (k, name) in [(1.0, "100%"), (1.15, "115%"), (1.3, "130%")] {
                        if ui.selectable_label((s.ui_scale - k).abs() < 0.01, RichText::new(name).size(15.0)).nav_clicked() {
                            s.ui_scale = k;
                            *clicked = true;
                        }
                    }
                })
                .response
                .on_hover_text("Bigger text and buttons, e.g. for a handheld screen.");
                ui.end_row();
                label(ui, "Health bars");
                let r = ui.checkbox(&mut s.health_bars, "");
                nav::toggle(&r, &mut s.health_bars);
                ui.end_row();
                label(ui, "Floating gold / crits");
                let r = ui.checkbox(&mut s.floating_text, "");
                nav::toggle(&r, &mut s.floating_text);
                ui.end_row();
                label(ui, "Tips");
                ui.horizontal(|ui| {
                    let r = ui.checkbox(&mut s.tips, "");
                    nav::toggle(&r, &mut s.tips);
                    if ui.button(RichText::new("Show all tips again").size(13.0)).nav_clicked() {
                        s.seen_tips.clear();
                        s.tips = true;
                        *clicked = true;
                    }
                });
                ui.end_row();
            });
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                if big_button(ui, "Back", 200.0, clicked) {
                    close = true;
                }
            });
        });
    });
    close
}

const HELP: &[(&str, &str)] = &[
    (
        "The goal",
        "Enemies walk from the Start portal through every numbered checkpoint in order, then to the Exit. Each one that gets through costs a life (bosses cost more). Survive all 20 waves.",
    ),
    (
        "Building the maze",
        "Each round you place 5 gems on empty ground. Every gem is also a wall, and enemies always take the shortest open path, so use your gems to make that path as long as possible. You can never fully block the path. While building, glowing dots show the route enemies will take; hover a cell to preview it with a gem there.",
    ),
    (
        "Keeping a gem",
        "After placing 5 gems, pick ONE to keep: click it, then Keep (K). The other four turn into rocks: still walls, but they don't attack. Rocks can be removed for gold between waves.",
    ),
    (
        "Combining",
        "If two of the new gems match (same type and quality), Combine (C) keeps one of them a quality higher. Four of a kind jumps two qualities. Qualities: Rough, Cut, Faceted, Brilliant, Radiant.",
    ),
    (
        "Special towers",
        "Some gem combinations form recipes for powerful special towers. When you own all the ingredients, they get a pink ring: click one and press Forge. Open the Recipe Book (B) to plan ahead. Specials can be upgraded with gold.",
    ),
    (
        "Special enemies",
        "Shades are invisible: only towers near an Amethyst (or Obsidian Eye) can see them, plus towers they pass right next to. Menders heal enemies around them. Guardians carry a recharging shield (the blue bar). Imps sprint in bursts. Broodmothers split into hatchlings. Warded enemies ignore slows, poison and armor shred.",
    ),
    (
        "Ascended specials",
        "The strongest towers are made from another special plus more ingredients, e.g. Frostfire + Ember Crown + Brilliant Ruby makes Inferno. Check the Recipe Book.",
    ),
    (
        "Daily challenge and mutators",
        "Mutators on the title screen add optional rules to a run (their records are kept separately). The Daily Challenge picks the map, difficulty and mutators for the day, and everyone gets the same gem rolls.",
    ),
    (
        "Gold",
        "Kills and cleared waves earn gold. Spend it on Gem Quality (better odds of high-quality gems), upgrading special towers, and removing rocks.",
    ),
    (
        "Flyers and bosses",
        "Flyers ignore the maze and fly straight between checkpoints (shown as a dashed line), so keep some damage near that line. Bosses are slow and very tough, and some have special abilities: check the Next Wave panel.",
    ),
    (
        "Gems",
        "Ruby: splash damage (ground only). Sapphire: slows. Emerald: poison. Topaz: chain lightning. Amethyst: long range, ignores armor, strong vs flyers, reveals invisible enemies. Diamond: high damage with critical hits.",
    ),
    (
        "Controls",
        "Click to place or select. K keep, C combine, U gem quality, X remove rock, L upgrade special, B recipe book. 1/2/3 game speed, Space pause, Esc menu. Camera: WASD/arrows pan, Q/E or right-drag rotate, scroll zoom, middle-drag pan, Home reset view, V top-down. M/N music/sound, F11 fullscreen.",
    ),
    (
        "Controller",
        "Left stick moves the cursor. A places or selects, B deselects (or dismisses a tip). X keeps a new gem, removes a selected rock or upgrades a selected special. Y forges when a recipe is ready, otherwise combines. LB/RB jump between towers (the new gems while choosing). D-pad: up gem quality, down top-down view, left/right game speed. Right stick turns the camera, LT/RT zoom, L3 resets the view, R3 pauses. Start opens the menu, Select the recipe book. In menus, move with the stick or D-pad, A to choose, B to go back.",
    ),
];

/// The how-to-play page. Returns true when closed.
pub fn help(ctx: &egui::Context, area: Rect, clicked: &mut bool) -> bool {
    nav::group(ctx, Group::Modal);
    dim_behind(ctx, area, "help_dim");
    let mut close = false;
    egui::Area::new("help".into()).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(area.center()).show(ctx, |ui| {
        frame().show(ui, |ui| {
            ui.set_width(640.0);
            ui.label(RichText::new("How to Play").family(heading()).size(30.0).color(GOLD));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height((area.height() - 220.0).max(200.0)).show(ui, |ui| {
                nav::scroll(ui);
                for (title, body) in HELP {
                    ui.add_space(6.0);
                    ui.label(RichText::new(*title).family(heading()).size(18.0).color(lerp32(GOLD, TEXT, 0.3)));
                    ui.label(RichText::new(*body).size(15.0).color(TEXT));
                }
            });
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                if big_button(ui, "Back", 200.0, clicked) {
                    close = true;
                }
            });
        });
    });
    close
}

/// The rock-removal confirmation. Returns Some(true) to remove, Some(false) to cancel.
pub fn confirm_rock(ctx: &egui::Context, view: Rect, cost: u32, can: bool, clicked: &mut bool) -> Option<bool> {
    nav::group(ctx, Group::Modal);
    let mut out = None;
    egui::Area::new("confirm_rock".into())
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(view.center().x - 170.0, view.center().y - 60.0))
        .show(ctx, |ui| {
            frame().show(ui, |ui| {
                ui.set_width(300.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Remove this rock?").family(heading()).size(22.0).color(GOLD));
                    small(ui, format!("Costs {cost} gold. This reshapes your maze."), TEXT);
                    if !can {
                        small(ui, "Not enough gold, or a wave is running.", BAD);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let yes = egui::Button::new(RichText::new(format!("Remove  {}", hint("Enter", "A"))).size(15.0)).min_size(V2::new(140.0, 30.0));
                        if ui.add_enabled(can, yes).nav_clicked() {
                            out = Some(true);
                            *clicked = true;
                        }
                        let no = egui::Button::new(RichText::new(format!("Cancel  {}", hint("Esc", "B"))).size(15.0)).min_size(V2::new(140.0, 30.0));
                        if ui.add(no).nav_clicked() {
                            out = Some(false);
                            *clicked = true;
                        }
                    });
                });
            });
        });
    out
}

// ---------------------------------------------------------------------------------------------
// First-game tips

struct Tip {
    id: &'static str,
    text: &'static str,
    when: fn(&Game) -> bool,
}

const TIPS: &[Tip] = &[
    Tip {
        id: "place",
        text: "Click empty ground to place a gem. You place 5 each round, and every gem is also a wall: make the enemies' path as long as you can. The glowing dots show the path they will take.",
        when: |g| g.phase == Phase::Build && g.wave == 0,
    },
    Tip {
        id: "choose",
        text: "Now keep ONE of the new gems: click it, then Keep (K). The others turn into rocks. If two new gems match, Combine (C) upgrades one instead.",
        when: |g| g.phase == Phase::Choose,
    },
    Tip {
        id: "wave",
        text: "Enemies walk from Start through each numbered checkpoint to the Exit. Each one that escapes costs a life. Press 2 or 3 to speed the game up.",
        when: |g| g.phase == Phase::Wave && g.wave == 0,
    },
    Tip {
        id: "recipe",
        text: "A recipe is ready! Pink-ringed gems can be forged into a special tower: click one and press Forge. The Recipe Book (B) lists them all.",
        when: |g| matches!(g.phase, Phase::Build | Phase::Choose) && !g.available_recipes().is_empty(),
    },
    Tip {
        id: "quality",
        text: "You have gold to spare. Upgrading Gem Quality (U) improves the odds of rolling higher-quality gems.",
        when: |g| g.phase == Phase::Build && g.wave >= 1 && g.luck_cost().is_some_and(|c| g.gold >= c),
    },
    Tip {
        id: "flyers",
        text: "Flyers are next. They ignore your maze and fly straight between checkpoints (the dashed line), so keep some damage near it.",
        when: |g| g.phase != Phase::Wave && g.wave_info(g.wave).air,
    },
    Tip {
        id: "invisible",
        text: "Invisible Shades are next! Only towers near an Amethyst (or Obsidian Eye) can see them, so make sure one covers the path.",
        when: |g| g.phase != Phase::Wave && g.bal.waves.kinds[g.bal.kind_for_wave(g.wave)].invisible,
    },
    Tip {
        id: "boss",
        text: "A boss is next! Bosses are very tough and cost several lives if they escape. Check the Next Wave panel for its abilities.",
        when: |g| g.phase != Phase::Wave && g.wave_info(g.wave).boss,
    },
    Tip {
        id: "rocks",
        text: "Rocks can be removed between waves: click one, then Remove (X). Handy for fixing a maze or making room.",
        when: |g| g.phase == Phase::Build && g.wave >= 3 && g.grid.iter().any(|o| matches!(o, Occ::Rock)),
    },
];

/// Shows the first relevant tip the player hasn't dismissed. Returns true if settings changed.
/// `dismiss` (B on a controller) acts like "Got it".
pub fn tips(ctx: &egui::Context, view: Rect, g: &Game, s: &mut Settings, dismiss: bool, clicked: &mut bool) -> bool {
    if !s.tips || matches!(g.phase, Phase::GameOver | Phase::Victory) {
        return false;
    }
    let Some(tip) = TIPS.iter().find(|t| !s.seen_tips.iter().any(|x| x == t.id) && (t.when)(g)) else { return false };
    nav::group(ctx, Group::None);
    if dismiss {
        s.seen_tips.push(tip.id.to_string());
        return true;
    }
    let mut changed = false;
    egui::Area::new("tip".into()).order(egui::Order::Foreground).fixed_pos(view.min + V2::new(16.0, 16.0)).show(ctx, |ui| {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(14, 12, 10, 235))
            .inner_margin(Margin::same(12))
            .stroke(Stroke::new(1.2, alpha(INFO, 0.7)))
            .corner_radius(CornerRadius::same(4))
            .show(ui, |ui| {
                ui.set_width(330.0);
                ui.label(RichText::new("Tip").family(heading()).size(17.0).color(INFO));
                ui.label(RichText::new(tip.text).size(14.5).color(TEXT));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let got = if pad::active() { "Got it  (B)" } else { "Got it" };
                    if ui.button(RichText::new(got).size(14.0)).nav_clicked() {
                        s.seen_tips.push(tip.id.to_string());
                        changed = true;
                        *clicked = true;
                    }
                    if ui.button(RichText::new("Hide all tips").size(13.0)).nav_clicked() {
                        s.tips = false;
                        changed = true;
                        *clicked = true;
                    }
                });
            });
    });
    changed
}

// ---------------------------------------------------------------------------------------------
// Board tooltip

/// A small info box next to the cursor for the tower or rock under it.
pub fn board_tooltip(ctx: &egui::Context, g: &Game, cell: (i32, i32), pos: Pos2) {
    let i = g.idx(cell);
    let mut lines: Vec<(String, Color32, bool)> = Vec::new();
    match &g.grid[i] {
        Occ::Rock => {
            lines.push(("Rock".into(), TEXT, true));
            lines.push(("A wall. Select it to remove it for gold between waves.".into(), DIM, false));
        }
        Occ::Tower(t) => {
            let s = g.stats(t);
            lines.push((g.tower_name(t), lerp32(c32(g.color(t.kind)), Color32::WHITE, 0.3), true));
            let bonus = g.aura_bonus(i);
            let dmg = if bonus > 0.0 { format!("Dmg {:.0} (+{:.0}%)", s.damage, bonus * 100.0) } else { format!("Dmg {:.0}", s.damage) };
            lines.push((format!("{dmg}  ·  Range {:.1}  ·  {:.1}s", s.range, s.cooldown), TEXT, false));
            let fx = effects_text(&s);
            if !fx.is_empty() {
                lines.push((fx, INFO, false));
            }
            if t.fresh {
                lines.push(("New this round".into(), GOLD, false));
            } else if t.kills > 0 {
                lines.push((format!("{} kills", t.kills), DIM, false));
            }
            if let Kind::Special(_) = t.kind {
                lines.push((format!("Special · level {}", t.rank + 1), RECIPE, false));
            }
        }
        _ => return,
    }
    egui::Area::new("board_tip".into()).order(egui::Order::Tooltip).fixed_pos(pos + V2::new(18.0, 14.0)).interactable(false).show(ctx, |ui| {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(10, 9, 8, 230))
            .inner_margin(Margin::symmetric(10, 7))
            .stroke(Stroke::new(1.0, alpha(GOLD, 0.4)))
            .corner_radius(CornerRadius::same(3))
            .show(ui, |ui| {
                ui.set_max_width(280.0);
                for (text, col, is_bold) in lines {
                    let r = RichText::new(text).size(if is_bold { 15.0 } else { 13.5 }).color(col);
                    ui.label(if is_bold { r.family(bold()) } else { r });
                }
            });
    });
}

// ---------------------------------------------------------------------------------------------
// End of run

/// The victory / defeat summary with the run's best towers. Returns a request from its buttons.
pub fn summary(ctx: &egui::Context, view: Rect, g: &Game, new_best: bool, t: f32, clicked: &mut bool) -> Option<Request> {
    nav::group(ctx, Group::Modal);
    dim_behind(ctx, view, "summary_dim");
    let mut req = None;
    let won = g.phase == Phase::Victory;
    let (title, col) = match (won, g.endless) {
        (true, _) => ("Victory", GOLD),
        (false, true) => ("Endless Run Over", INFO),
        _ => ("Defeat", BAD),
    };
    let mut towers: Vec<(usize, &facet_core::game::Tower)> =
        (0..g.grid.len()).filter_map(|i| g.tower(i).map(|tw| (i, tw))).filter(|(_, tw)| tw.damage_dealt > 0.0).collect();
    towers.sort_by(|a, b| b.1.damage_dealt.total_cmp(&a.1.damage_dealt));
    let total: f32 = towers.iter().map(|(_, tw)| tw.damage_dealt).sum();
    egui::Area::new("summary".into())
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(view.center().x - 250.0, view.min.y + view.height() * 0.12))
        .show(ctx, |ui| {
            frame().show(ui, |ui| {
                ui.set_width(456.0);
                ui.vertical_centered(|ui| {
                    if new_best {
                        let pulse = 0.7 + 0.3 * (t * 4.0).sin();
                        ui.label(RichText::new("New Personal Best!").family(heading()).size(22.0).color(alpha(GOLD, pulse)));
                    }
                    ui.label(RichText::new(title).family(heading()).size(52.0).color(col));
                    let diff = &g.bal.difficulties[g.diff];
                    ui.label(RichText::new(format!("{}  ·  {}", g.map.name, diff.name)).size(16.0).color(diff_color(g.diff)));
                });
                ui.add_space(8.0);
                let reached = if won { format!("Cleared all {} waves", g.bal.wave_count()) } else { format!("Reached wave {}", g.wave + 1) };
                egui::Grid::new("summary_stats").num_columns(2).spacing(V2::new(24.0, 4.0)).show(ui, |ui| {
                    let row = |ui: &mut egui::Ui, k: &str, v: String| {
                        ui.label(RichText::new(k).size(15.0).color(DIM));
                        ui.label(RichText::new(v).family(bold()).size(15.0));
                        ui.end_row();
                    };
                    row(ui, "Result", reached);
                    row(ui, "Lives left", g.lives.max(0).to_string());
                    row(ui, "Enemies defeated", g.total_kills.to_string());
                    row(ui, "Total damage", format!("{:.0}", total));
                    row(ui, "Towers standing", towers.len().to_string());
                });
                if !towers.is_empty() {
                    ui.add_space(10.0);
                    ui.label(RichText::new("Top towers").family(heading()).size(18.0).color(GOLD));
                    for (rank, (_, tw)) in towers.iter().take(5).enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}.", rank + 1)).size(15.0).color(DIM));
                            tower_icon(ui, g, tw, 22.0);
                            ui.label(RichText::new(g.tower_name(tw)).size(15.0).color(lerp32(c32(g.color(tw.kind)), Color32::WHITE, 0.3)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let share = if total > 0.0 { tw.damage_dealt / total * 100.0 } else { 0.0 };
                                ui.label(RichText::new(format!("{:.0} dmg ({share:.0}%) · {} kills", tw.damage_dealt, tw.kills)).size(14.0).color(TEXT));
                            });
                        });
                    }
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let n = if won { 3.0 } else { 2.0 };
                    let w = (456.0 - (n - 1.0) * 8.0) / n;
                    let mut btn = |ui: &mut egui::Ui, label: &str| {
                        let b = egui::Button::new(RichText::new(label).family(heading()).size(16.0)).min_size(V2::new(w, 36.0));
                        let r = ui.add(b).nav_clicked();
                        if r {
                            *clicked = true;
                        }
                        r
                    };
                    let (endless, title) = if pad::active() { ("Endless", "Title") } else { ("Endless  [E]", "Title  [R]") };
                    if won && btn(ui, endless) {
                        req = Some(Request::Endless);
                    }
                    if btn(ui, "Play Again") {
                        req = Some(Request::Restart);
                    }
                    if btn(ui, title) {
                        req = Some(Request::Quit);
                    }
                });
            });
        });
    let _ = (GOOD, FontId::default());
    req
}
