//! The 2D interface drawn over the 3D scene with egui: title screen (map and difficulty),
//! the side panel, the recipe book and in-game overlays (banners, boss bar, end screen).

use std::sync::Arc;

use bevy::prelude::*;
use bevy_egui::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Pos2, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, Vec2 as V2,
};
use bevy_egui::EguiContexts;
use facet_core::color::Color as CColor;
use facet_core::data::{Ing, MapDef};
use facet_core::game::{rgb, Event, FxKind, Game, Kind, Mutator, Phase, Stats, Tower};
use facet_core::path::{full_route, Cell};

use crate::audio::Audio;
use crate::menus::{self, Page, Request};
use crate::nav::{self, Group, NavExt};
use crate::pad::{self, hint, Pad};
use crate::{Screen, Sim};

pub const PANEL_W: f32 = 350.0;

pub(crate) const TEXT: Color32 = Color32::from_rgb(237, 232, 219);
pub(crate) const DIM: Color32 = Color32::from_rgb(161, 156, 143);
pub(crate) const GOLD: Color32 = Color32::from_rgb(237, 196, 107);
pub(crate) const INFO: Color32 = Color32::from_rgb(140, 204, 255);
pub(crate) const BAD: Color32 = Color32::from_rgb(255, 97, 84);
pub(crate) const GOOD: Color32 = Color32::from_rgb(128, 230, 140);
pub(crate) const RECIPE: Color32 = Color32::from_rgb(255, 148, 242);
pub(crate) const STONE: Color32 = Color32::from_rgb(38, 35, 32);
pub(crate) const PANEL_BG: Color32 = Color32::from_rgb(16, 14, 12);

pub(crate) fn heading() -> FontFamily {
    FontFamily::Name("heading".into())
}

pub(crate) fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub(crate) fn c32(c: CColor) -> Color32 {
    Color32::from_rgb((c.r * 255.0) as u8, (c.g * 255.0) as u8, (c.b * 255.0) as u8)
}

pub(crate) fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub(crate) fn lerp32(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub(crate) fn diff_color(d: usize) -> Color32 {
    [GOOD, GOLD, BAD][d.min(2)]
}

fn tier_rim(t: usize) -> Color32 {
    [
        Color32::from_rgb(130, 125, 118),
        Color32::from_rgb(196, 125, 70),
        Color32::from_rgb(215, 222, 232),
        Color32::from_rgb(255, 200, 82),
        Color32::from_rgb(255, 240, 180),
    ][t.min(4)]
}

/// Records are kept per map and difficulty (same keys as the 2D game, so records carry over).
pub fn record_key(map: &str, diff: &str) -> String {
    format!("{map} / {diff}")
}

/// Records with mutators are kept separately: "Map / Difficulty + Haste + Horde".
pub fn record_key_with(map: &str, diff: &str, mutators: &[Mutator]) -> String {
    let mut k = record_key(map, diff);
    for m in Mutator::ALL {
        if mutators.contains(&m) {
            k.push_str(" + ");
            k.push_str(m.name());
        }
    }
    k
}

/// Interface state shared between input, simulation and drawing.
#[derive(Resource)]
pub struct UiState {
    pub speed: u32,
    pub paused: bool,
    pub book: bool,
    pub new_best: bool,
    pub banner: Option<(String, Color32, f32)>,
    pub shake: f32,
    pub vignette: f32,
    /// The pointer was over the interface last frame (so board clicks are ignored).
    pub pointer_over_ui: bool,
    pub hover: Option<Cell>,
    /// With "click twice to place": the cell marked by the first click.
    pub marked: Option<Cell>,
    /// A gem in the bottom tray under the pointer (highlighted on the board).
    pub tray_hover: Option<usize>,
    /// The wave whose start was last autosaved.
    pub autosaved: Option<u32>,
    /// Cached path preview for the hovered cell: (cell, board rev, route or None if blocking).
    pub preview: Option<(Cell, u64, Option<Vec<Cell>>)>,
    pub run_won: bool,
    pub run_ended: bool,
    /// A camera preset requested from the panel (false: angled, true: top-down).
    pub cam_request: Option<bool>,
    /// Open menu page, if any.
    pub page: Option<Page>,
    /// A rock waiting for removal confirmation.
    pub confirm_rock: Option<usize>,
    /// A text field has keyboard focus (so shortcuts are ignored).
    pub typing: bool,
    fonts_set: bool,
    title_routes: Vec<Vec<Cell>>,
}

impl Default for UiState {
    fn default() -> UiState {
        UiState {
            speed: 1,
            paused: false,
            book: false,
            new_best: false,
            banner: None,
            shake: 0.0,
            vignette: 0.0,
            pointer_over_ui: false,
            hover: None,
            marked: None,
            tray_hover: None,
            autosaved: None,
            preview: None,
            run_won: false,
            run_ended: false,
            cam_request: None,
            page: None,
            confirm_rock: None,
            typing: false,
            fonts_set: false,
            title_routes: Vec::new(),
        }
    }
}

impl UiState {
    /// Anything that stops the game clock (menus, book, confirmation).
    pub fn blocking(&self) -> bool {
        self.paused || self.book || self.page.is_some() || self.confirm_rock.is_some()
    }

    pub fn reset_run(&mut self) {
        let keep_speed = self.speed;
        let fonts = self.fonts_set;
        let routes = std::mem::take(&mut self.title_routes);
        *self = UiState { speed: keep_speed, fonts_set: fonts, title_routes: routes, ..default() };
    }

    /// Reacts to simulation events with banners, shake and red flashes.
    pub fn effect(&mut self, e: &Event) {
        match e {
            Event::Combine { .. } => self.shake = self.shake.max(0.08),
            Event::Forge { .. } => self.shake = self.shake.max(0.15),
            Event::Death { boss: true, .. } => self.shake = self.shake.max(0.35),
            Event::Leak { lives } => {
                self.shake = self.shake.max(0.1 + *lives as f32 * 0.03);
                self.vignette = 1.0;
            }
            Event::WaveStart { boss, name, number } => {
                let (t, col) = if *boss { (format!("Wave {number}  ·  {}", name.to_uppercase()), BAD) } else { (format!("Wave {number}  ·  {name}"), GOLD) };
                self.banner = Some((t, col, 0.0));
                if *boss {
                    self.shake = self.shake.max(0.12);
                }
            }
            Event::WaveClear => self.banner = Some(("Wave cleared".into(), GOOD, 0.0)),
            Event::Victory => self.banner = None,
            _ => {}
        }
    }

    pub fn tick(&mut self, dt: f32) {
        if let Some(b) = &mut self.banner {
            b.2 += dt;
            if b.2 > 2.4 {
                self.banner = None;
            }
        }
        self.shake = (self.shake - dt * 0.8).max(0.0);
        self.vignette = (self.vignette - dt * 1.5).max(0.0);
    }
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("cinzel".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Cinzel.ttf"))));
    fonts.font_data.insert("lib".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/LiberationSans-Regular.ttf"))));
    fonts.font_data.insert("libb".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/LiberationSans-Bold.ttf"))));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "lib".into());
    fonts.families.insert(heading(), vec!["cinzel".into(), "lib".into()]);
    fonts.families.insert(bold(), vec!["libb".into(), "lib".into()]);
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
    let v = &mut style.visuals;
    v.override_text_color = Some(TEXT);
    v.widgets.inactive.weak_bg_fill = STONE;
    v.widgets.inactive.bg_fill = STONE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, alpha(GOLD, 0.55));
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(58, 52, 44);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, GOLD);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(76, 66, 50);
    v.widgets.active.bg_stroke = Stroke::new(1.5, GOLD);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, alpha(GOLD, 0.2));
    style.spacing.item_spacing = V2::new(6.0, 5.0);
    style.spacing.button_padding = V2::new(8.0, 5.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Small drawing helpers

/// A gem (or special crystal) icon on a tier-colored ring.
fn gem_icon(p: &egui::Painter, c: Pos2, r: f32, col: Color32, rank: usize, special: bool) {
    p.circle_filled(c, r * 0.95, Color32::from_rgba_unmultiplied(0, 0, 0, 110));
    p.circle_stroke(c, r * 0.92, Stroke::new(r * 0.12, if special { GOLD } else { tier_rim(rank) }));
    let pts: Vec<Pos2> = if special {
        vec![c + V2::new(0.0, -r * 0.8), c + V2::new(r * 0.38, 0.0), c + V2::new(0.0, r * 0.8), c + V2::new(-r * 0.38, 0.0)]
    } else {
        (0..6).map(|k| {
            let a = k as f32 / 6.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            c + V2::new(a.cos(), a.sin()) * r * 0.62
        }).collect()
    };
    let n = pts.len();
    for k in 0..n {
        let a = pts[k];
        let b = pts[(k + 1) % n];
        let mid = ((a + b.to_vec2()).to_vec2() / 2.0 - c.to_vec2()).normalized();
        // Light from the top-left.
        let light = (-mid.x * 0.6 - mid.y * 0.8) * 0.5 + 0.5;
        let shade = if light > 0.5 { lerp32(col, Color32::WHITE, (light - 0.5) * 0.9) } else { lerp32(col, Color32::BLACK, (0.5 - light) * 0.9) };
        p.add(Shape::convex_polygon(vec![c, a, b], shade, Stroke::NONE));
    }
    p.add(Shape::closed_line(pts, Stroke::new(1.0, alpha(Color32::WHITE, 0.35))));
    if rank >= 3 {
        p.circle_filled(c + V2::new(-r * 0.2, -r * 0.25), r * 0.08, Color32::WHITE);
    }
}

/// Quality pips: one small diamond per quality (Rough … Radiant), filled up to `tier` in that
/// quality's color. Read left to right: more filled = better.
pub(crate) fn quality_pips(p: &egui::Painter, center: Pos2, tier: usize, max: usize, size: f32) {
    let n = max + 1;
    let gap = size * 1.15;
    let x0 = center.x - gap * (n as f32 - 1.0) / 2.0;
    for k in 0..n {
        let c = Pos2::new(x0 + gap * k as f32, center.y);
        let pts = vec![c + V2::new(0.0, -size * 0.55), c + V2::new(size * 0.42, 0.0), c + V2::new(0.0, size * 0.55), c + V2::new(-size * 0.42, 0.0)];
        if k <= tier {
            p.add(Shape::convex_polygon(pts, tier_rim(tier), Stroke::new(0.8, alpha(Color32::BLACK, 0.7))));
        } else {
            p.add(Shape::closed_line(pts, Stroke::new(1.0, alpha(DIM, 0.55))));
        }
    }
}

/// Quality pips plus "Faceted · 3 of 5" as a row in a layout.
pub(crate) fn quality_row(ui: &mut egui::Ui, g: &Game, tier: usize) {
    let max = g.bal.max_tier();
    ui.horizontal(|ui| {
        let w = 9.0 * 1.15 * (max + 1) as f32;
        let (r, _) = ui.allocate_exact_size(V2::new(w, 14.0), Sense::hover());
        quality_pips(ui.painter(), r.center(), tier, max, 9.0);
        let name = g.bal.tier_names.get(tier).cloned().unwrap_or_default();
        ui.label(RichText::new(format!("{name} · quality {} of {}", tier + 1, max + 1)).size(13.0).color(tier_rim(tier)));
    });
}

pub(crate) fn tower_icon(ui: &mut egui::Ui, g: &Game, t: &Tower, size: f32) {
    let (rect, _) = ui.allocate_exact_size(V2::splat(size), Sense::hover());
    gem_icon(ui.painter(), rect.center(), size / 2.0, c32(g.color(t.kind)), t.rank, matches!(t.kind, Kind::Special(_)));
}

/// A panel button with a hover explanation (shown even when it's disabled).
fn button_tip(ui: &mut egui::Ui, label: &str, enabled: bool, tint: Color32, clicked: &mut bool, tip: &str) -> bool {
    let text = RichText::new(label).size(15.0).color(if enabled { TEXT } else { DIM });
    let b = egui::Button::new(text).min_size(V2::new(ui.available_width(), 30.0)).stroke(Stroke::new(1.2, alpha(tint, if enabled { 0.85 } else { 0.25 })));
    let mut resp = ui.add_enabled(enabled, b);
    if !tip.is_empty() {
        resp = resp.on_hover_text(tip).on_disabled_hover_text(tip);
    }
    let r = resp.nav_clicked();
    if r {
        *clicked = true;
    }
    r
}

pub(crate) fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).family(heading()).size(17.0).color(GOLD));
    let (r, _) = ui.allocate_exact_size(V2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, alpha(GOLD, 0.3)));
    ui.add_space(2.0);
}

pub(crate) fn small(ui: &mut egui::Ui, text: impl Into<String>, col: Color32) {
    ui.label(RichText::new(text).size(13.5).color(col));
}

pub(crate) fn effects_text(s: &Stats) -> String {
    let mut v: Vec<String> = Vec::new();
    if s.splash > 0.0 {
        v.push(format!("Splash {:.1}", s.splash));
    }
    if s.slow > 0.0 {
        v.push(format!("Slow {:.0}%", s.slow * 100.0));
    }
    if s.poison > 0.0 {
        v.push(format!("Poison {:.0}/s", s.poison));
    }
    if s.chain > 0 {
        v.push(format!("Chains to {} more", s.chain));
    }
    if s.crit > 0.0 {
        v.push(format!("Crit {:.0}%", s.crit * 100.0));
    }
    if s.pierce {
        v.push("Ignores armor".into());
    }
    if s.shred > 0.0 {
        v.push(format!("Shreds {:.0} armor", s.shred));
    }
    if s.aura > 0.0 {
        v.push(format!("Aura: +{:.0}% damage to nearby towers", s.aura * 100.0));
    }
    if s.air_mult != 1.0 {
        v.push(format!("×{:.1} vs flyers", s.air_mult));
    }
    if s.ground_only {
        v.push("Can't hit flyers".into());
    }
    if s.burn > 0.0 {
        v.push(format!("Burning ground {:.0}/s", s.burn));
    }
    if s.bounty > 0.0 {
        v.push(format!("+{:.0} gold per kill", s.bounty.round()));
    }
    if s.detect {
        v.push("Reveals invisible".into());
    }
    v.join(", ")
}

fn wave_label(g: &Game) -> String {
    if g.endless || g.wave >= g.bal.wave_count() {
        format!("Wave {}", g.wave + 1)
    } else {
        format!("Wave {}/{}", g.wave + 1, g.bal.wave_count())
    }
}

/// Theme ground color for minimaps.
fn map_ground(theme: &str) -> Color32 {
    match theme {
        "autumn" => Color32::from_rgb(110, 80, 40),
        "ash" => Color32::from_rgb(52, 46, 46),
        "marsh" => Color32::from_rgb(55, 78, 50),
        "snow" => Color32::from_rgb(205, 212, 225),
        _ => Color32::from_rgb(70, 100, 45),
    }
}

fn minimap(p: &egui::Painter, m: &MapDef, route: &[Cell], r: Rect, time: f32, dim: bool) {
    let cs = (r.width() / m.w as f32).min(r.height() / m.h as f32);
    let size = V2::new(cs * m.w as f32, cs * m.h as f32);
    let o = r.center() - size / 2.0;
    p.rect_filled(Rect::from_min_size(o, size), CornerRadius::same(3), map_ground(&m.theme));
    let water = m.wall_style == "water";
    for w in &m.walls {
        let wr = Rect::from_min_size(o + V2::new(w[0] as f32, w[1] as f32) * cs, V2::new(w[2] as f32, w[3] as f32) * cs);
        p.rect_filled(wr, CornerRadius::same(if water { 4 } else { 1 }), if water { Color32::from_rgb(40, 85, 105) } else { Color32::from_rgb(120, 112, 104) });
    }
    let pts: Vec<Pos2> = route.iter().map(|c| o + V2::new(c.0 as f32 + 0.5, c.1 as f32 + 0.5) * cs).collect();
    p.add(Shape::line(pts, Stroke::new((cs * 0.7).max(1.5), Color32::from_rgb(160, 128, 84))));
    let n = m.waypoints.len();
    for (j, wp) in m.waypoints.iter().enumerate() {
        let c = o + V2::new(wp.0 as f32 + 0.5, wp.1 as f32 + 0.5) * cs;
        let col = if j == 0 { Color32::from_rgb(170, 110, 255) } else if j == n - 1 { BAD } else { GOLD };
        let pulse = 0.8 + 0.2 * (time * 2.0 + j as f32).sin();
        p.circle_filled(c, (cs * 1.1).max(2.5) * pulse, col);
    }
    if dim {
        p.rect_filled(Rect::from_min_size(o, size), CornerRadius::same(3), Color32::from_rgba_unmultiplied(0, 0, 0, 110));
    }
}

// ---------------------------------------------------------------------------------------------
// The interface system

/// What the interface asks main to do.
#[derive(Default)]
pub struct UiOut {
    pub start: Option<(usize, usize)>,
    pub daily: bool,
    pub editor: bool,
    pub select_map: Option<usize>,
    pub resume: bool,
    pub quit: bool,
}

#[allow(clippy::too_many_arguments)]
/// Settings page with the save's music/sound switches; saves when closed.
fn settings_page(ctx: &egui::Context, area: Rect, sim: &mut Sim, clicked: &mut bool) -> bool {
    let save = &mut sim.save;
    let (mut music, mut sfx) = (save.music, save.sfx);
    let close = menus::settings(ctx, area, &mut save.settings, &mut music, &mut sfx, clicked);
    save.music = music;
    save.sfx = sfx;
    if close {
        save.store();
    }
    close
}

/// Records an unfinished run (so quitting still counts), then restarts / quits / goes endless.
pub fn handle_request(sim: &mut Sim, st: &mut UiState, r: Request) {
    // Leaving a run saves it to continue later; restarting ends it (counted as unfinished).
    let saves = crate::run::enabled(sim.debug) && sim.test_map.is_none() && crate::run::resumable(&sim.game);
    let leaving = matches!(r, Request::Quit | Request::Exit);
    if leaving && saves {
        sim.save_run(st.run_won);
    } else if matches!(r, Request::Restart | Request::Quit | Request::Exit) {
        let key = sim.run_key();
        let Sim { game: g, save, .. } = sim;
        if !sim.debug && sim.test_map.is_none() && !st.run_ended && !st.run_won && (g.wave > 0 || g.phase == Phase::Wave) {
            save.record_end(&key, g.wave, false);
        }
        if r == Request::Restart && sim.test_map.is_none() {
            sim.forget_saved_run();
        }
    }
    match r {
        Request::Exit => {
            // Already saved above; just quit.
            sim.screen = Screen::Title;
            sim.quit = true;
        }
        Request::Endless => {
            sim.game.continue_endless();
            st.page = None;
        }
        Request::Restart => {
            sim.restart();
            st.reset_run();
        }
        Request::Quit => {
            sim.to_title();
            st.reset_run();
        }
    }
}

pub fn interface(
    mut contexts: EguiContexts,
    mut sim: ResMut<Sim>,
    mut st: ResMut<UiState>,
    mut audio: ResMut<Audio>,
    mut pad: ResMut<Pad>,
    time: Res<Time>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !st.fonts_set {
        install_fonts(ctx);
        st.fonts_set = true;
        // New fonts take effect next frame.
        return Ok(());
    }
    // Debug: FACET3D_NOUI=1 hides the interface (clean scene shots, e.g. store artwork).
    if crate::no_ui() {
        return Ok(());
    }
    let zoom = sim.save.settings.ui_scale.clamp(0.75, 2.0);
    if (ctx.zoom_factor() - zoom).abs() > 0.001 {
        ctx.set_zoom_factor(zoom);
    }
    nav::begin(ctx, std::mem::take(&mut pad.nav), pad::active() && sim.screen != Screen::Editor);
    let dismiss_tip = std::mem::take(&mut pad.dismiss_tip);
    let t = time.elapsed_secs();
    let mut clicked = false;
    let screen = ctx.content_rect();
    match sim.screen {
        Screen::Title => {
            let out = title(ctx, &mut sim, &mut st, &mut audio, screen, t, &mut clicked);
            if let Some(m) = out.select_map {
                sim.select_map(m);
            }
            if let Some((m, d)) = out.start
                && st.page.is_none()
            {
                sim.start(m, d);
                st.reset_run();
            } else if out.daily && st.page.is_none() {
                sim.start_daily();
                st.reset_run();
            } else if out.editor && st.page.is_none() {
                sim.open_editor();
            } else if out.quit && st.page.is_none() {
                sim.quit = true;
            }
            // Debug: FACET3D_RESUME=1 continues the saved run straight away.
            let mut resume = (out.resume || (sim.resume_prompt && std::env::var_os("FACET3D_RESUME").is_some())) && st.page.is_none();
            // At startup, ask about a run left unfinished.
            if sim.resume_prompt && st.page.is_none() {
                match sim.saved.clone() {
                    Some(s) => match menus::resume_prompt(ctx, screen, &s, &mut clicked) {
                        Some(menus::ResumeChoice::Continue) => resume = true,
                        Some(menus::ResumeChoice::NewRun) => {
                            sim.discard_saved_run();
                            sim.resume_prompt = false;
                        }
                        Some(menus::ResumeChoice::Later) => sim.resume_prompt = false,
                        None => {}
                    },
                    None => sim.resume_prompt = false,
                }
            }
            if resume && let Err(e) = sim.resume_saved(&mut st) {
                sim.data_err = Some(format!("Couldn't continue the saved run: {e}"));
                sim.forget_saved_run();
                sim.resume_prompt = false;
            }
            match st.page {
                Some(Page::Settings) => {
                    if settings_page(ctx, screen, &mut sim, &mut clicked) {
                        st.page = None;
                    }
                }
                Some(Page::Help) => {
                    if menus::help(ctx, screen, &mut clicked) {
                        st.page = None;
                    }
                }
                _ => {}
            }
        }
        Screen::Editor => {}
        Screen::Playing => {
            let view = Rect::from_min_max(screen.min, Pos2::new(screen.max.x - PANEL_W, screen.max.y));
            panel(ctx, &mut sim, &mut st, &mut audio, screen, t, &mut clicked);
            hud(ctx, &sim, &mut st, view, &mut clicked);
            let ended = matches!(sim.game.phase, Phase::GameOver | Phase::Victory);
            if !ended && st.page.is_none() && !st.book {
                tray(ctx, &mut sim.game, &mut st, view, &mut clicked);
            } else {
                st.tray_hover = None;
            }
            if st.book {
                book(ctx, &sim.game, view, t, &mut st.book, &mut clicked);
            }
            let (bars, floats) = (sim.save.settings.health_bars, sim.save.settings.floating_text);
            if let Ok((cam, ct)) = cams.single() {
                overlay(ctx, &sim.game, &st, view, t, cam, ct, &|x, z| sim.scene.ground(x, z), bars, floats);
            }
            let ended = matches!(sim.game.phase, Phase::GameOver | Phase::Victory);
            let mut req = None;
            if st.page.is_none() && st.confirm_rock.is_none() && !st.book {
                let Sim { game, save, .. } = &mut *sim;
                if !ended && menus::tips(ctx, view, game, &mut save.settings, dismiss_tip, &mut clicked) {
                    save.store();
                }
                if let Some(c) = st.hover
                    && (!st.pointer_over_ui || pad::active())
                {
                    // Next to the mouse, or next to the controller cursor's cell.
                    let at = if pad::active() {
                        cams.single().ok().and_then(|(cam, ct)| {
                            let p = bevy::math::Vec3::new(c.0 as f32 + 0.5, sim.scene.ground(c.0 as f32 + 0.5, c.1 as f32 + 0.5), c.1 as f32 + 0.5);
                            cam.world_to_viewport(ct, p).ok().map(|v| Pos2::new(v.x, v.y) / zoom)
                        })
                    } else {
                        ctx.pointer_hover_pos()
                    };
                    if let Some(at) = at {
                        menus::board_tooltip(ctx, &sim.game, c, at);
                    }
                }
                if ended {
                    req = menus::summary(ctx, view, &sim.game, st.new_best, t, &mut clicked);
                }
            }
            if let Some(rock) = st.confirm_rock {
                let g = &mut sim.game;
                if !g.is_rock(rock) {
                    st.confirm_rock = None;
                } else {
                    let can = g.can_remove_rock(rock);
                    match menus::confirm_rock(ctx, view, g.bal.rock_removal_cost, can, &mut clicked) {
                        Some(true) => {
                            g.remove_rock(rock);
                            st.confirm_rock = None;
                        }
                        Some(false) => st.confirm_rock = None,
                        None => {}
                    }
                }
            }
            match st.page {
                Some(Page::Pause) => {
                    if let Some(r) = menus::pause(ctx, view, &mut st.page, crate::run::enabled(sim.debug) && sim.test_map.is_none(), &mut clicked) {
                        req = Some(r);
                    }
                }
                Some(Page::Settings) => {
                    if settings_page(ctx, view, &mut sim, &mut clicked) {
                        st.page = Some(Page::Pause);
                    }
                }
                Some(Page::Help) => {
                    if menus::help(ctx, view, &mut clicked) {
                        st.page = Some(Page::Pause);
                    }
                }
                None => {}
            }
            if let Some(r) = req {
                handle_request(&mut sim, &mut st, r);
            }
        }
    }
    if sim.screen == Screen::Editor {
        let dt = time.delta_secs();
        let maps = sim.maps.clone();
        if let Some(mut ed) = sim.editor.take() {
            let (out, new_maps) = crate::editor::show(ctx, &mut ed, &maps, dt, &mut clicked);
            sim.editor = Some(ed);
            if let Some(m) = new_maps {
                sim.maps = m;
                sim.map = sim.map.min(sim.maps.len() - 1);
            }
            match out {
                Some(crate::editor::EditorOut::Exit) => {
                    sim.editor = None;
                    sim.to_title();
                }
                Some(crate::editor::EditorOut::Playtest(m)) => {
                    sim.start_test(m);
                    st.reset_run();
                }
                None => {}
            }
        }
    }
    if clicked {
        audio.click();
    }
    st.pointer_over_ui = ctx.is_pointer_over_egui();
    st.typing = ctx.egui_wants_keyboard_input();
    Ok(())
}

fn title(ctx: &egui::Context, sim: &mut Sim, st: &mut UiState, audio: &mut Audio, screen: Rect, t: f32, clicked: &mut bool) -> UiOut {
    let mut out = UiOut::default();
    if st.title_routes.len() != sim.maps.len() {
        st.title_routes = sim.maps.iter().map(|m| full_route(m.w, m.h, &m.waypoints, &|x, y| m.is_wall(x, y)).unwrap_or_default()).collect();
    }
    nav::group(ctx, Group::Title);
    let cx = screen.center().x;
    let fg = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, "title_bg".into()));
    // Darken the scene a little so text reads, more at the top and bottom.
    fg.rect_filled(screen, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(0, 0, 0, 70));

    egui::Area::new("title".into()).fixed_pos(Pos2::new(screen.min.x, screen.min.y)).show(ctx, |ui| {
        ui.set_width(screen.width());
        ui.set_height(screen.height());
        let p = ui.painter().clone();
        // Short screens (handhelds, large interface sizes) get a tighter layout.
        let compact = screen.height() < 760.0;
        let pick = |normal: f32, small: f32| if compact { small } else { normal };
        let ty = screen.min.y + screen.height() * pick(0.11, 0.075);
        let big = pick(88.0, 62.0);
        p.text(Pos2::new(cx + 3.0, ty + 4.0), Align2::CENTER_CENTER, "FACET", FontId::new(big, heading()), alpha(Color32::BLACK, 0.6));
        p.text(Pos2::new(cx, ty), Align2::CENTER_CENTER, "FACET", FontId::new(big, heading()), GOLD);
        p.text(Pos2::new(cx, ty + pick(62.0, 44.0)), Align2::CENTER_CENTER, "a gem maze defense", FontId::new(pick(20.0, 17.0), heading()), TEXT);

        // Map cards.
        // Show as many cards as fit (at least ~150 px each), scrolling to keep the selection visible.
        let count = sim.maps.len();
        let fit = (((screen.width() - 120.0) / 164.0).floor() as usize).max(3);
        let shown = count.min(fit);
        let first = sim.map.saturating_sub(shown / 2).min(count - shown);
        let n = shown as f32;
        let card_w = pick(196.0, 140.0).min((screen.width() - 120.0) / n - 14.0);
        let card_h = card_w * 0.8 + 34.0;
        let total = n * card_w + (n - 1.0) * 14.0;
        let my = ty + pick(110.0, 76.0);
        if first > 0 {
            p.text(Pos2::new(cx - total / 2.0 - 22.0, my + card_h / 2.0), Align2::CENTER_CENTER, "‹", FontId::new(40.0, heading()), GOLD);
        }
        if first + shown < count {
            p.text(Pos2::new(cx + total / 2.0 + 22.0, my + card_h / 2.0), Align2::CENTER_CENTER, "›", FontId::new(40.0, heading()), GOLD);
        }
        for k in first..first + shown {
            let slot = (k - first) as f32;
            let r = Rect::from_min_size(Pos2::new(cx - total / 2.0 + slot * (card_w + 14.0), my), V2::new(card_w, card_h));
            let resp = ui.interact(r, ui.id().with(("map", k)), Sense::click());
            let sel = k == sim.map;
            p.rect_filled(r, CornerRadius::same(4), Color32::from_rgba_unmultiplied(14, 12, 10, 225));
            let mm = Rect::from_min_size(r.min + V2::new(8.0, 8.0), V2::new(r.width() - 16.0, r.height() - 44.0));
            minimap(&p, &sim.maps[k], &st.title_routes[k], mm, t, !sel && !resp.hovered());
            let border = if sel { GOLD } else if resp.hovered() { alpha(GOLD, 0.6) } else { alpha(GOLD, 0.2) };
            p.rect_stroke(r, CornerRadius::same(4), Stroke::new(if sel { 2.5 } else { 1.2 }, border), StrokeKind::Inside);
            p.text(Pos2::new(r.center().x, r.max.y - 18.0), Align2::CENTER_CENTER, &sim.maps[k].name, FontId::new(16.0, heading()), if sel { GOLD } else { TEXT });
            if resp.nav_clicked() && !sel {
                out.select_map = Some(k);
                *clicked = true;
            }
        }
        let m = &sim.maps[sim.map];
        p.text(Pos2::new(cx, my + card_h + pick(22.0, 18.0)), Align2::CENTER_CENTER, &m.desc, FontId::proportional(16.0), TEXT);
        if !compact {
            p.text(Pos2::new(cx, my + card_h + 44.0), Align2::CENTER_CENTER, if pad::active() { "LB / RB to change the map, then choose a difficulty" } else { "Click a map or use the Left/Right keys to choose" }, FontId::proportional(13.0), DIM);
        }

        // Difficulty cards.
        let diffs = &sim.bal.difficulties;
        let nd = diffs.len() as f32;
        let dw = 300.0f32.min((screen.width() - 80.0) / nd - 18.0);
        let dh = pick(150.0, 136.0);
        let dtotal = nd * dw + (nd - 1.0) * 18.0;
        // Mutators: optional rules for the next game (records are kept separately).
        {
            let chips: Vec<(Mutator, f32)> = Mutator::ALL
                .iter()
                .map(|&m| (m, ctx.fonts_mut(|f| f.layout_no_wrap(m.name().into(), FontId::proportional(14.0), TEXT).size().x) + 22.0))
                .collect();
            let label_w = 90.0;
            let total: f32 = label_w + chips.iter().map(|c| c.1 + 8.0).sum::<f32>();
            let mut x = cx - total / 2.0;
            let y = my + card_h + pick(62.0, 34.0);
            p.text(Pos2::new(x, y + 12.0), Align2::LEFT_CENTER, "Mutators", FontId::new(15.0, heading()), GOLD);
            x += label_w;
            for (m, w) in chips {
                let on = sim.mutators.contains(&m);
                let r = Rect::from_min_size(Pos2::new(x, y), V2::new(w, 24.0));
                let b = egui::Button::new(RichText::new(m.name()).size(14.0).color(if on { Color32::BLACK } else { TEXT }))
                    .fill(if on { GOLD } else { Color32::from_rgba_unmultiplied(20, 18, 16, 220) })
                    .corner_radius(CornerRadius::same(12));
                if ui.put(r, b).on_hover_text(m.desc()).nav_clicked() {
                    if on {
                        sim.mutators.retain(|x| *x != m);
                    } else {
                        sim.mutators.push(m);
                    }
                    *clicked = true;
                }
                x += w + 8.0;
            }
        }
        let dy = my + card_h + pick(104.0, 68.0);
        let wave_count = sim.bal.wave_count();
        for (k, d) in diffs.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(cx - dtotal / 2.0 + k as f32 * (dw + 18.0), dy), V2::new(dw, dh));
            let resp = ui.interact(r, ui.id().with(("diff", k)), Sense::click());
            let tint = diff_color(k);
            let hov = resp.hovered();
            p.rect_filled(r, CornerRadius::same(4), if hov { Color32::from_rgba_unmultiplied(38, 34, 30, 240) } else { Color32::from_rgba_unmultiplied(16, 14, 12, 230) });
            p.rect_stroke(r, CornerRadius::same(4), Stroke::new(if hov { 2.2 } else { 1.2 }, alpha(tint, if hov { 1.0 } else { 0.5 })), StrokeKind::Inside);
            p.text(r.min + V2::new(14.0, 14.0), Align2::LEFT_TOP, &d.name, FontId::new(22.0, heading()), tint);
            if !pad::active() {
                p.text(Pos2::new(r.max.x - 14.0, r.min.y + 18.0), Align2::RIGHT_TOP, format!("[{}]", k + 1), FontId::proportional(14.0), DIM);
            }
            let galley = ctx.fonts_mut(|f| f.layout(d.desc.clone(), FontId::proportional(14.0), TEXT, dw - 28.0));
            let gh = galley.size().y;
            p.galley(r.min + V2::new(14.0, 46.0), galley, TEXT);
            let boss = if d.boss_lives >= d.lives { "defeat".to_string() } else { format!("−{} lives", d.boss_lives) };
            p.text(r.min + V2::new(14.0, 52.0 + gh), Align2::LEFT_TOP, format!("{} lives  ·  boss escape: {boss}", d.lives), FontId::proportional(13.0), DIM);
            let rec = sim.save.record(&record_key_with(&m.name, &d.name, &sim.mutators));
            let best = if rec.runs == 0 {
                "No runs yet".to_string()
            } else {
                let mut b = match rec.best_win_lives {
                    Some(l) => format!("Won {}/{}  ·  best {l} lives left", rec.wins, rec.runs),
                    None => format!("Best: cleared {} of {} waves", rec.best_cleared.min(wave_count), wave_count),
                };
                if rec.best_cleared > wave_count {
                    b.push_str(&format!("  ·  endless {}", rec.best_cleared + 1));
                }
                b
            };
            // Compact cards put the record right under the lives line so long descriptions fit.
            let best_at = if compact { Pos2::new(r.min.x + 14.0, r.min.y + 72.0 + gh) } else { Pos2::new(r.min.x + 14.0, r.max.y - 12.0) };
            p.text(best_at, if compact { Align2::LEFT_TOP } else { Align2::LEFT_BOTTOM }, best, FontId::proportional(13.0), if rec.wins > 0 { GOLD } else { DIM });
            if resp.nav_clicked() {
                out.start = Some((sim.map, k));
                *clicked = true;
            }
        }
        // Daily challenge.
        {
            let d = crate::Daily::today(&sim.maps, sim.bal.difficulties.len());
            let w = 3.0 * 300.0f32.min((screen.width() - 80.0) / 3.0 - 18.0) + 36.0;
            let r = Rect::from_min_size(Pos2::new(cx - w / 2.0, dy + dh + pick(16.0, 8.0)), V2::new(w, pick(64.0, 56.0)));
            p.rect_filled(r, CornerRadius::same(4), Color32::from_rgba_unmultiplied(16, 14, 12, 230));
            p.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.2, alpha(INFO, 0.6)), StrokeKind::Inside);
            p.text(r.min + V2::new(14.0, 10.0), Align2::LEFT_TOP, "Daily Challenge", FontId::new(20.0, heading()), INFO);
            let muts: Vec<&str> = d.mutators.iter().map(|m| m.name()).collect();
            let info = format!("{}  ·  {}  ·  {}  ·  {}", d.date, sim.maps[d.map].name, sim.bal.difficulties[d.diff].name, muts.join(" + "));
            p.text(r.min + V2::new(14.0, pick(38.0, 33.0)), Align2::LEFT_TOP, info, FontId::proportional(14.0), TEXT);
            let rec = sim.save.record(&d.key());
            let best = if rec.runs == 0 {
                "Same gems for everyone today".to_string()
            } else if let Some(l) = rec.best_win_lives {
                format!("Won today · best {l} lives left")
            } else {
                format!("Best today: wave {}", rec.best_cleared + 1)
            };
            p.text(Pos2::new(r.max.x - 150.0, r.min.y + 14.0), Align2::RIGHT_TOP, best, FontId::proportional(13.0), if rec.wins > 0 { GOLD } else { DIM });
            let br = Rect::from_min_size(Pos2::new(r.max.x - 130.0, r.min.y + pick(14.0, 10.0)), V2::new(116.0, 36.0));
            let b = egui::Button::new(RichText::new("Play Daily").family(heading()).size(16.0));
            if ui.put(br, b).on_hover_text("Today's map, difficulty and mutators, with the same gem rolls for every player.").nav_clicked() {
                out.daily = true;
                *clicked = true;
            }
        }

        // Settings and help.
        let bw = 150.0;
        let by = screen.max.y - pick(92.0, 68.0);
        let mut buttons: Vec<(&str, u8)> = vec![("Settings", 1), ("How to Play", 2), ("Map Editor", 3), ("Quit", 4)];
        if sim.saved.is_some() {
            buttons.insert(0, ("Continue Run", 0));
        }
        let n = buttons.len() as f32;
        let x0 = cx - (n * bw + (n - 1.0) * 12.0) / 2.0;
        for (k, (label, what)) in buttons.into_iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(x0 + k as f32 * (bw + 12.0), by), V2::new(bw, 34.0));
            let text = RichText::new(label).family(heading()).size(17.0);
            let mut resp = ui.put(r, egui::Button::new(if what == 0 { text.color(GOLD) } else { text }));
            if what == 0
                && let Some(s) = &sim.saved
            {
                resp = resp.on_hover_text(format!("{}\n{}", s.title, s.detail));
            }
            if resp.nav_clicked() {
                match what {
                    0 => out.resume = true,
                    1 => st.page = Some(Page::Settings),
                    2 => st.page = Some(Page::Help),
                    3 => out.editor = true,
                    _ => out.quit = true,
                }
                *clicked = true;
            }
        }
        let onoff = |b: bool| if b { "on" } else { "off" };
        p.text(
            Pos2::new(cx, screen.max.y - pick(40.0, 18.0)),
            Align2::CENTER_CENTER,
            if pad::active() {
                "LB / RB map  ·  Stick or D-pad to move  ·  A to choose  ·  B back".to_string()
            } else {
                format!("[M] Music: {}      [N] Sound: {}", onoff(audio.music_on), onoff(audio.sfx_on))
            },
            FontId::proportional(14.0),
            DIM,
        );
        if let Some(e) = &sim.data_err {
            p.text(Pos2::new(cx, screen.max.y - 18.0), Align2::CENTER_CENTER, e, FontId::proportional(14.0), BAD);
        }
        p.text(
            Pos2::new(screen.min.x + 14.0, screen.max.y - 14.0),
            Align2::LEFT_BOTTOM,
            concat!("v", env!("CARGO_PKG_VERSION")),
            FontId::proportional(13.0),
            DIM,
        );
    });

    if st.page.is_some() {
        return out;
    }
    nav::group(ctx, Group::Title);
    menus::update_banner(ctx, screen, clicked);
    ctx.input(|i| {
        let n = sim.maps.len();
        if i.key_pressed(egui::Key::ArrowLeft) {
            out.select_map = Some((sim.map + n - 1) % n);
        }
        if i.key_pressed(egui::Key::ArrowRight) {
            out.select_map = Some((sim.map + 1) % n);
        }
        for (k, key) in [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3].iter().enumerate() {
            if k < sim.bal.difficulties.len() && i.key_pressed(*key) {
                out.start = Some((sim.map, k));
            }
        }
    });
    out
}

/// The side panel: what's selected, recipes, gem quality, the next wave; the recipe book and
/// camera at the bottom. The wave / lives / gold card, speed buttons and this round's gems live
/// on the board view instead (`hud`, `tray`).
fn panel(ctx: &egui::Context, sim: &mut Sim, st: &mut UiState, _audio: &mut Audio, screen: Rect, _t: f32, clicked: &mut bool) {
    nav::group(ctx, Group::None);
    let g = &mut sim.game;
    let frame = egui::Frame::new().fill(PANEL_BG).inner_margin(Margin::symmetric(18, 14)).stroke(Stroke::new(1.0, alpha(GOLD, 0.35)));
    egui::Area::new("panel".into()).fixed_pos(Pos2::new(screen.max.x - PANEL_W, screen.min.y)).show(ctx, |ui| {
        frame.show(ui, |ui| {
            let inner = PANEL_W - 36.0;
            ui.set_width(inner);
            ui.set_min_height(screen.height() - 28.0);
            ui.set_max_width(inner);
            // The body scrolls if it doesn't fit above the bottom block (small screens, big
            // interface sizes).
            egui::ScrollArea::vertical().max_height((screen.height() - 130.0).max(120.0)).auto_shrink([false, true]).show(ui, |ui| {
            ui.set_max_width(inner);

            match g.selected {
                Some(sel) => selected(ui, g, sel, &mut st.confirm_rock, clicked),
                None => {
                    section(ui, "Selected");
                    let t = if pad::active() { "Move the cursor onto a tower or rock and press A to see it here." } else { "Click a tower or rock on the board to see it here." };
                    small(ui, t, DIM);
                }
            }

            // Recipes: ready to forge, or one ingredient away.
            let ready: Vec<usize> = g.available_recipes().into_iter().map(|(r, _)| r).collect();
            let close = if matches!(g.phase, Phase::Build | Phase::Choose) { g.near_recipes().into_iter().take(3).collect() } else { Vec::new() };
            if !ready.is_empty() || !close.is_empty() {
                section(ui, "Recipes");
                for r in &ready {
                    let how = if pad::active() { "select a pink-ringed gem, then Y" } else { "click a pink-ringed gem" };
                    small(ui, format!("Ready: {} ({how})", g.bal.specials[*r].name), RECIPE);
                }
                for (r, missing) in close {
                    small(ui, format!("Close: {} (need {})", g.bal.specials[r].name, g.ing_name(missing)), alpha(RECIPE, 0.75));
                }
            }

            // Gem quality: the odds of each quality, worst to best.
            section(ui, "Gem Quality");
            let odds = g.bal.luck_odds[g.luck.min(g.bal.luck_odds.len() - 1)].clone();
            let total: u32 = odds.iter().sum::<u32>().max(1);
            small(ui, format!("Level {} of {} · odds for each gem you place", g.luck + 1, g.bal.luck_odds.len()), DIM);
            ui.add_space(2.0);
            let tiers = g.bal.tier_names.len();
            let colw = ui.available_width() / tiers as f32;
            let (r, _) = ui.allocate_exact_size(V2::new(ui.available_width(), 54.0), Sense::hover());
            let p = ui.painter();
            for (tier, name) in g.bal.tier_names.iter().enumerate() {
                let x = r.min.x + colw * (tier as f32 + 0.5);
                let o = odds.get(tier).copied().unwrap_or(0);
                let on = o > 0;
                p.text(Pos2::new(x, r.min.y + 7.0), Align2::CENTER_CENTER, name, FontId::proportional(12.5), if on { tier_rim(tier) } else { alpha(tier_rim(tier), 0.4) });
                quality_pips(p, Pos2::new(x, r.min.y + 24.0), tier, tiers - 1, 9.0);
                let pct = format!("{}%", o * 100 / total);
                p.text(Pos2::new(x, r.min.y + 44.0), Align2::CENTER_CENTER, pct, FontId::new(14.0, bold()), if on { TEXT } else { alpha(DIM, 0.5) });
                if tier > 0 {
                    p.text(Pos2::new(r.min.x + colw * tier as f32, r.min.y + 7.0), Align2::CENTER_CENTER, "›", FontId::proportional(12.0), alpha(DIM, 0.6));
                }
            }
            let label = match g.luck_cost() {
                Some(c) => format!("Upgrade quality ({c} gold)  {}", hint("U", "D-pad ↑")),
                None => "Quality maxed".into(),
            };
            let can = g.luck_cost().is_some_and(|c| g.gold >= c) && !matches!(g.phase, Phase::GameOver | Phase::Victory);
            if button_tip(ui, &label, can, GOLD, clicked, "Improves the odds of rolling higher-quality gems from now on.") {
                g.buy_luck();
            }

            // Next wave.
            if !matches!(g.phase, Phase::GameOver | Phase::Victory) {
                let wi = g.wave_info(g.wave);
                section(ui, if g.phase == Phase::Wave { "Current Wave" } else { "Next Wave" });
                let d = &g.bal.difficulties[g.diff];
                let tag = if wi.boss {
                    if d.boss_lives >= g.lives { "BOSS: escape = defeat".to_string() } else { format!("BOSS: costs {} lives", d.boss_lives) }
                } else if wi.air {
                    "FLYING: ignores the maze".to_string()
                } else {
                    String::new()
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} ×{}", wi.name, wi.count)).family(bold()).size(15.0));
                    ui.label(RichText::new(tag).size(13.0).color(if wi.boss { BAD } else { INFO }));
                });
                small(ui, format!("HP {}    Armor {}    Speed {:.1}", wi.hp, wi.armor, wi.speed), DIM);
                if !wi.desc.is_empty() {
                    small(ui, &wi.desc, if wi.boss { BAD } else { INFO });
                }
            }
            });
        });
    });

    // Bottom of the panel: recipe book and camera.
    egui::Area::new("panel_bottom".into())
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(screen.max.x - PANEL_W + 18.0, screen.max.y - 104.0))
        .show(ctx, |ui| {
            let inner = PANEL_W - 36.0;
            ui.set_width(inner);
            let book_label = format!("{}  {}", if st.book { "Close Recipe Book" } else { "Recipe Book" }, hint("B", "Select"));
            if button_tip(ui, &book_label, true, RECIPE, clicked, "Every special tower and its ingredients.") {
                st.book = !st.book;
            }
            ui.horizontal(|ui| {
                let bw = (inner - 6.0) / 2.0;
                for (label, top, tip) in [
                    (format!("Reset view  {}", hint("Home", "L3")), false, "Back to the default angled view of the whole board."),
                    (format!("Top-down  {}", hint("V", "D-pad ↓")), true, "Look straight down at the board."),
                ] {
                    let b = egui::Button::new(RichText::new(label).size(14.0)).min_size(V2::new(bw, 28.0));
                    if ui.add(b).on_hover_text(tip).nav_clicked() {
                        st.cam_request = Some(top);
                        *clicked = true;
                    }
                }
            });
            if pad::active() {
                small(ui, "Right stick camera · LT/RT zoom · Start menu", DIM);
            } else {
                small(ui, "WASD pan · Q/E rotate · scroll zoom · Esc menu", DIM);
            }
        });
}

/// Top of the board view: wave, lives and gold on the left; game speed on the right.
fn hud(ctx: &egui::Context, sim: &Sim, st: &mut UiState, view: Rect, clicked: &mut bool) {
    nav::group(ctx, Group::None);
    let g = &sim.game;
    let card = egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(12, 11, 10, 215))
        .inner_margin(Margin::symmetric(12, 7))
        .stroke(Stroke::new(1.0, alpha(GOLD, 0.35)))
        .corner_radius(CornerRadius::same(4));
    egui::Area::new("hud".into()).fixed_pos(view.min + V2::new(12.0, 12.0)).show(ctx, |ui| {
        card.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                ui.label(RichText::new(wave_label(g)).family(bold()).size(18.0));
                let lives_col = if g.lives <= 5 { BAD } else { TEXT };
                ui.label(RichText::new(format!("{} lives", g.lives)).family(bold()).size(18.0).color(lives_col));
                ui.label(RichText::new(format!("{} gold", g.gold)).family(bold()).size(18.0).color(GOLD));
            });
            let mut mode = format!("{} · {}", g.bal.difficulties[g.diff].name, g.map.name);
            if g.endless {
                mode.push_str(" · Endless");
            }
            if sim.daily.is_some() {
                mode = format!("Daily · {mode}");
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(mode).size(13.0).color(diff_color(g.diff)));
                if !g.mutators.is_empty() {
                    let names: Vec<&str> = g.mutators.iter().map(|m| m.name()).collect();
                    ui.label(RichText::new(names.join(" · ")).size(12.5).color(INFO))
                        .on_hover_text(g.mutators.iter().map(|m| format!("{}: {}", m.name(), m.desc())).collect::<Vec<_>>().join("\n"));
                }
            });
            if g.phase == Phase::Wave {
                let n = g.creeps.len() as u32 + g.spawn_left();
                ui.label(RichText::new(format!("{n} {} left", if n == 1 { "enemy" } else { "enemies" })).size(13.0).color(DIM));
            }
        });
    });

    // Speed and pause.
    egui::Area::new("speed".into()).pivot(Align2::RIGHT_TOP).fixed_pos(Pos2::new(view.max.x - 12.0, view.min.y + 12.0)).show(ctx, |ui| {
        card.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (label, sp) in [("1×", 1u32), ("2×", 2), ("4×", 4)] {
                    let on = st.speed == sp && !st.paused;
                    let b = egui::Button::new(RichText::new(label).size(15.0).color(if on { GOLD } else { TEXT }))
                        .min_size(V2::new(40.0, 28.0))
                        .fill(if on { Color32::from_rgb(70, 58, 36) } else { STONE });
                    if ui.add(b).on_hover_text(format!("Game speed {label}  {}", hint(&sp.min(3).to_string(), "D-pad ←/→"))).nav_clicked() {
                        st.speed = sp;
                        st.paused = false;
                        *clicked = true;
                    }
                }
                let b = egui::Button::new(RichText::new(if st.paused { "Play" } else { "Pause" }).size(15.0).color(if st.paused { GOLD } else { TEXT }))
                    .min_size(V2::new(58.0, 28.0))
                    .fill(if st.paused { Color32::from_rgb(70, 58, 36) } else { STONE });
                if ui.add(b).on_hover_text(format!("Pause  {}", hint("Space", "R3"))).nav_clicked() {
                    st.paused = !st.paused;
                    *clicked = true;
                }
            });
        });
    });
}

/// Bottom of the board view while placing and choosing: what to do, this round's gems
/// (click one to select it) and the keep / combine buttons.
fn tray(ctx: &egui::Context, g: &mut Game, st: &mut UiState, view: Rect, clicked: &mut bool) {
    st.tray_hover = None;
    if !matches!(g.phase, Phase::Build | Phase::Choose) {
        return;
    }
    nav::group(ctx, Group::None);
    let padded = pad::active();
    let card = egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(12, 11, 10, 225))
        .inner_margin(Margin::symmetric(14, 9))
        .stroke(Stroke::new(1.0, alpha(GOLD, 0.4)))
        .corner_radius(CornerRadius::same(5));
    egui::Area::new("tray".into()).pivot(Align2::CENTER_BOTTOM).fixed_pos(Pos2::new(view.center().x, view.max.y - 12.0)).show(ctx, |ui| {
        card.show(ui, |ui| {
            // What to do now.
            let hint_text = match g.phase {
                Phase::Build if g.no_room && g.placed.is_empty() => "No room for new gems. Remove rocks to make space, or start the wave.".to_string(),
                Phase::Build if st.marked.is_some() && !padded => "Click the marked cell again to place the gem there.".to_string(),
                Phase::Build => format!("Place {} more gem{}. Each gem is also a wall: shape the maze!", g.bal.gems_per_round - g.placed.len(), if g.bal.gems_per_round - g.placed.len() == 1 { "" } else { "s" }),
                _ if padded => "Pick a gem (cursor or LB/RB), then X to keep it. The others turn to rock.".to_string(),
                _ => "Pick a gem (here or on the board), then keep it. The others turn to rock.".to_string(),
            };
            let hint_col = if st.marked.is_some() && g.phase == Phase::Build { GOLD } else { TEXT };
            ui.label(RichText::new(hint_text).size(14.5).color(hint_col));
            if g.phase == Phase::Choose && !g.available_recipes().is_empty() {
                let t = if padded { "A recipe is ready: select a pink-ringed gem and press Y to forge it." } else { "A recipe is ready: select a pink-ringed gem to forge it." };
                ui.label(RichText::new(t).size(13.5).color(RECIPE));
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for k in 0..g.bal.gems_per_round {
                    let cell = g.placed.get(k).copied();
                    let tw = cell.and_then(|i| g.tower(i)).cloned();
                    let sense = if tw.is_some() { Sense::click() } else { Sense::hover() };
                    let (r, resp) = ui.allocate_exact_size(V2::new(58.0, 68.0), sense);
                    let sel = cell.is_some() && g.selected == cell;
                    let hovered = resp.hovered() && tw.is_some();
                    let p = ui.painter();
                    p.rect_filled(r, CornerRadius::same(4), if sel { Color32::from_rgb(52, 44, 30) } else { Color32::from_rgb(16, 15, 13) });
                    let border = if sel { Stroke::new(2.0, GOLD) } else if hovered { Stroke::new(1.5, alpha(GOLD, 0.8)) } else { Stroke::new(1.0, alpha(GOLD, 0.3)) };
                    p.rect_stroke(r, CornerRadius::same(4), border, StrokeKind::Inside);
                    match &tw {
                        Some(t) => {
                            gem_icon(p, r.center() - V2::new(0.0, 8.0), 19.0, c32(g.color(t.kind)), t.rank, false);
                            quality_pips(p, Pos2::new(r.center().x, r.max.y - 10.0), t.rank, g.bal.max_tier(), 8.5);
                        }
                        None => {
                            p.text(r.center(), Align2::CENTER_CENTER, "?", FontId::new(20.0, heading()), DIM);
                        }
                    }
                    if let (Some(i), Some(t)) = (cell, &tw) {
                        if hovered {
                            st.tray_hover = Some(i);
                        }
                        let s = g.stats(t);
                        let resp = resp.on_hover_text(format!("{}\nDmg {:.0} · Range {:.1} · {:.1}s{}", g.tower_name(t), s.damage, s.range, s.cooldown, {
                            let fx = effects_text(&s);
                            if fx.is_empty() { String::new() } else { format!("\n{fx}") }
                        }));
                        if resp.clicked() {
                            g.selected = Some(i);
                            *clicked = true;
                        }
                    }
                }
                // Keep / combine (choosing), or skip a round with no room.
                if g.phase == Phase::Choose {
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.set_width(190.0);
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let sel_fresh = g.selected.filter(|&i| g.tower(i).is_some_and(|t| t.fresh));
                        if button_tip(ui, &format!("Keep  {}", hint("K", "X")), sel_fresh.is_some(), GOLD, clicked, "Keep the selected new gem. The other new gems turn into rocks.") {
                            g.keep(false);
                        }
                        let steps = sel_fresh.map_or(0, |i| g.combine_steps(i));
                        let label = match sel_fresh {
                            Some(i) if steps > 0 => {
                                let tw = g.tower(i).unwrap();
                                format!("{} → {}  {}", if steps == 2 { "Four of a kind" } else { "Combine" }, g.bal.tier_names[tw.rank + steps], hint("C", "Y"))
                            }
                            _ => "Combine (needs a pair)".into(),
                        };
                        if button_tip(ui, &label, steps > 0, GOLD, clicked, "Two matching new gems (same type and quality) make one gem a quality higher. Four of a kind jumps two.") {
                            g.keep(true);
                        }
                    });
                } else if g.no_room && g.placed.is_empty() {
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.set_width(190.0);
                        if button_tip(ui, "Start the wave", true, GOLD, clicked, "Skip placing gems this round.") {
                            g.skip_round();
                        }
                    });
                }
            });
        });
    });
}

fn selected(ui: &mut egui::Ui, g: &mut Game, sel: usize, confirm: &mut Option<usize>, clicked: &mut bool) {
    if g.is_rock(sel) {
        section(ui, "Rock");
        small(ui, "Rocks are walls. Remove one to reshape your maze (between waves only).", DIM);
        let label = format!("Remove rock ({} gold)  {}", g.bal.rock_removal_cost, hint("X", "X"));
        if button_tip(ui, &label, g.can_remove_rock(sel), GOLD, clicked, "Rocks can only be removed between waves.") {
            *confirm = Some(sel);
        }
        return;
    }
    let Some(tw) = g.tower(sel).cloned() else { return };
    let def = g.def(tw.kind).clone();
    let col = c32(g.color(tw.kind));
    section(ui, if matches!(tw.kind, Kind::Special(_)) { "Special Tower" } else { "Selected" });
    let s = g.stats(&tw);
    ui.horizontal(|ui| {
        tower_icon(ui, g, &tw, 36.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(g.tower_name(&tw)).family(bold()).size(17.0).color(lerp32(col, Color32::WHITE, 0.3)));
            let bonus = g.aura_bonus(sel);
            let dmg = if bonus > 0.0 { format!("Dmg {:.0} (+{:.0}%)", s.damage, bonus * 100.0) } else { format!("Dmg {:.0}", s.damage) };
            small(ui, format!("{dmg}    Range {:.1}    {:.1}s", s.range, s.cooldown), TEXT);
        });
    });
    if let Kind::Gem(_) = tw.kind {
        quality_row(ui, g, tw.rank);
    }
    let fx = effects_text(&s);
    if !fx.is_empty() {
        small(ui, fx, INFO);
    }
    small(ui, &def.detail, DIM);
    if !tw.fresh {
        small(ui, format!("Damage dealt {:.0}    Kills {}", tw.damage_dealt, tw.kills), DIM);
    }
    if let Kind::Special(_) = tw.kind {
        match g.special_upgrade_cost(sel) {
            Some(cost) => {
                let label = format!("Upgrade to Lv {} ({cost} gold)  {}", tw.rank + 2, hint("L", "X"));
                let can = g.gold >= cost && !matches!(g.phase, Phase::GameOver | Phase::Victory);
                if button_tip(ui, &label, can, GOLD, clicked, "Each level multiplies the special's damage.") {
                    g.upgrade_special(sel);
                }
            }
            None => {
                ui.label(RichText::new("Max level").family(bold()).size(14.0).color(GOLD));
            }
        }
    }
    for (r, ings) in g.recipes_for(sel) {
        let name = g.bal.specials[r].name.clone();
        let label = match ings {
            Some(_) if pad::active() => format!("Forge {name}!  (Y)"),
            Some(_) => format!("Forge {name}!"),
            None => format!("Part of: {name}"),
        };
        let tip = if ings.is_some() { "Combine the ingredients into this special tower here." } else { "You don't own all the ingredients yet. See the Recipe Book (B)." };
        if button_tip(ui, &label, ings.is_some(), RECIPE, clicked, tip) {
            g.forge(r, sel);
        }
    }
}

fn book(ctx: &egui::Context, g: &Game, view: Rect, _t: f32, open: &mut bool, clicked: &mut bool) {
    nav::group(ctx, Group::Modal);
    let w = (view.width() - 60.0).min(900.0);
    // Dim the board behind the book.
    ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, "book_dim".into())).rect_filled(view, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(0, 0, 0, 120));
    let frame = egui::Frame::new().fill(Color32::from_rgb(12, 10, 9)).inner_margin(Margin::same(22)).stroke(Stroke::new(1.5, alpha(GOLD, 0.6)));
    egui::Area::new("book".into())
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(view.center().x - w / 2.0, view.min.y + 30.0))
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(w - 44.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Recipe Book").family(heading()).size(30.0).color(GOLD));
                    ui.add_space(16.0);
                    ui.label(RichText::new("Ingredients you own are green. Forge between waves.").size(14.0).color(DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new(format!("Close  {}", hint("B", "B"))).size(14.0)).nav_clicked() {
                            *open = false;
                            *clicked = true;
                        }
                    });
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(view.height() - 150.0).min_scrolled_height(view.height() - 150.0).show(ui, |ui| {
                    nav::scroll(ui);
                    for (r, sp) in g.bal.specials.iter().enumerate() {
                        let col = c32(rgb(sp.color));
                        let probe = Tower::new(Kind::Special(r), 0, false);
                        ui.separator();
                        ui.horizontal(|ui| {
                            tower_icon(ui, g, &probe, 46.0);
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&sp.name).family(heading()).size(20.0).color(lerp32(col, Color32::WHITE, 0.2)));
                                    ui.add_space(14.0);
                                    for &ing in &g.bal.recipes[r] {
                                        let own = g.owns(ing);
                                        let gp = match ing {
                                            Ing::Gem(gem, tier) => Tower::new(Kind::Gem(gem), tier, false),
                                            Ing::Special(sp) => Tower::new(Kind::Special(sp), 0, false),
                                        };
                                        tower_icon(ui, g, &gp, 22.0);
                                        ui.label(RichText::new(g.ing_name(ing)).size(14.0).color(if own { GOOD } else { DIM }));
                                        ui.add_space(8.0);
                                    }
                                });
                                let s = g.stats(&probe);
                                small(ui, format!("Dmg {:.0}, Range {:.1}, {:.1}s.  {}", s.damage, s.range, s.cooldown, effects_text(&s)), TEXT);
                                small(ui, &sp.detail, DIM);
                            });
                        });
                    }
                });
            });
        });
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn overlay(
    ctx: &egui::Context,
    g: &Game,
    st: &UiState,
    view: Rect,
    t: f32,
    cam: &Camera,
    ct: &GlobalTransform,
    ground: &dyn Fn(f32, f32) -> f32,
    show_bars: bool,
    show_floats: bool,
) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, "overlay".into()));
    let cx = view.center().x;
    let zoom = ctx.zoom_factor();
    let to_screen = |x: f32, y: f32, z: f32| cam.world_to_viewport(ct, bevy::math::Vec3::new(x, y, z)).ok().map(|v| Pos2::new(v.x, v.y) / zoom);

    // Checkpoint markers so the order is easy to follow.
    let n = g.map.waypoints.len();
    for (k, wp) in g.map.waypoints.iter().enumerate() {
        let (x, z) = (wp.0 as f32 + 0.5, wp.1 as f32 + 0.5);
        let Some(sp) = to_screen(x, ground(x, z) + 1.1, z) else { continue };
        if !view.contains(sp) {
            continue;
        }
        let (label, col) = match k {
            0 => ("Start".to_string(), Color32::from_rgb(190, 140, 255)),
            _ if k == n - 1 => ("Exit".to_string(), BAD),
            _ => (k.to_string(), GOLD),
        };
        let font = FontId::new(15.0, heading());
        let w = ctx.fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), col).size().x).max(12.0) + 12.0;
        let r = Rect::from_center_size(sp, V2::new(w, 22.0));
        p.rect_filled(r, CornerRadius::same(11), alpha(Color32::BLACK, 0.65));
        p.rect_stroke(r, CornerRadius::same(11), Stroke::new(1.2, alpha(col, 0.8)), StrokeKind::Inside);
        p.text(sp + V2::new(0.0, 1.0), Align2::CENTER_CENTER, label, font, col);
    }

    // Health bars for damaged enemies, drawn flat on screen so they stay steady.
    for c in g.creeps.iter().filter(|_| show_bars) {
        let shielded = c.max_shield > 0.0 && c.shield < c.max_shield;
        if c.boss || !c.visible() || (c.hp >= c.max_hp && !shielded) {
            continue;
        }
        let size = g.bal.waves.kinds[c.kind].size;
        let y = ground(c.pos.x, c.pos.y) + if c.air { 1.5 } else { 0.0 } + size * 2.75 + 0.25;
        let Some(sp) = to_screen(c.pos.x, y, c.pos.y) else { continue };
        if !view.contains(sp) {
            continue;
        }
        let w = 24.0 + size * 30.0;
        let f = (c.hp / c.max_hp).clamp(0.0, 1.0);
        let r = Rect::from_center_size(sp, V2::new(w, 5.0));
        p.rect_filled(r.expand(1.5), CornerRadius::same(1), alpha(Color32::BLACK, 0.8));
        let col = if f > 0.5 {
            Color32::from_rgb(90, 225, 80)
        } else if f > 0.25 {
            Color32::from_rgb(240, 190, 50)
        } else {
            Color32::from_rgb(240, 65, 50)
        };
        p.rect_filled(Rect::from_min_size(r.min, V2::new(w * f, 5.0)), CornerRadius::ZERO, col);
        if c.max_shield > 0.0 && c.shield > 0.0 {
            let sf = (c.shield / c.max_shield).clamp(0.0, 1.0);
            let sr = Rect::from_min_size(r.min - V2::new(0.0, 4.5), V2::new(w * sf, 3.0));
            p.rect_filled(sr, CornerRadius::ZERO, Color32::from_rgb(110, 180, 255));
        }
    }

    // Floating texts (gold, crits).
    for fx in g.fx.iter().filter(|_| show_floats) {
        if let FxKind::Text(pos, s) = &fx.kind {
            let k = (fx.t / fx.life).clamp(0.0, 1.0);
            if let Some(sp) = to_screen(pos.x, 1.0 + k * 0.8, pos.y) {
                let col = alpha(c32(fx.color), 1.0 - k);
                p.text(sp + V2::new(1.0, 1.0), Align2::CENTER_CENTER, s, FontId::new(15.0, bold()), alpha(Color32::BLACK, 1.0 - k));
                p.text(sp, Align2::CENTER_CENTER, s, FontId::new(15.0, bold()), col);
            }
        }
    }

    // Red flash at the edges when enemies get through.
    if st.vignette > 0.0 {
        for k in 0..6 {
            let inset = k as f32 * 6.0;
            let a = st.vignette * 0.5 * (1.0 - k as f32 / 6.0);
            p.rect_stroke(view.shrink(inset), CornerRadius::ZERO, Stroke::new(6.0, alpha(Color32::from_rgb(255, 30, 30), a)), StrokeKind::Inside);
        }
    }

    // Boss health bar.
    if let Some(c) = g.creeps.iter().find(|c| c.boss && c.hp > 0.0) {
        let w = view.width() * 0.45;
        // Below the wave card on narrow views (handhelds, big interface sizes).
        let top = if view.width() < 900.0 { 135.0 } else { 100.0 };
        let r = Rect::from_min_size(Pos2::new(cx - w / 2.0, view.min.y + top), V2::new(w, 14.0));
        let kind = &g.bal.waves.kinds[c.kind];
        p.text(Pos2::new(cx, r.min.y - 6.0), Align2::CENTER_BOTTOM, kind.name.to_uppercase(), FontId::new(18.0, heading()), BAD);
        p.rect_filled(r.expand(2.0), CornerRadius::same(2), alpha(Color32::BLACK, 0.8));
        let f = (c.hp / c.max_hp).clamp(0.0, 1.0);
        p.rect_filled(Rect::from_min_size(r.min, V2::new(w * f, 14.0)), CornerRadius::ZERO, Color32::from_rgb(190, 30, 25));
        p.rect_filled(Rect::from_min_size(r.min, V2::new(w * f, 5.0)), CornerRadius::ZERO, alpha(Color32::from_rgb(255, 110, 90), 0.6));
        p.rect_stroke(r.expand(2.0), CornerRadius::same(2), Stroke::new(1.5, GOLD), StrokeKind::Outside);
    }

    // Wave banner.
    if let Some((text, col, time)) = &st.banner {
        let a = (time / 0.3).min(1.0) * ((2.4 - time) / 0.6).clamp(0.0, 1.0);
        let y = view.min.y + view.height() * 0.28 - (1.0 - (time / 0.3).min(1.0)) * 20.0;
        let font = FontId::new(40.0, heading());
        let width = ctx.fonts_mut(|f| f.layout_no_wrap(text.clone(), font.clone(), *col).size().x) + 80.0;
        let r = Rect::from_center_size(Pos2::new(cx, y), V2::new(width, 64.0));
        p.rect_filled(r, CornerRadius::ZERO, alpha(Color32::BLACK, 0.6 * a));
        p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.5, alpha(GOLD, 0.7 * a)));
        p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.5, alpha(GOLD, 0.7 * a)));
        p.text(Pos2::new(cx + 2.0, y + 3.0), Align2::CENTER_CENTER, text, font.clone(), alpha(Color32::BLACK, 0.6 * a));
        p.text(Pos2::new(cx, y), Align2::CENTER_CENTER, text, font, alpha(*col, a));
    }

    // Short messages from the game ("Can't block the path!").
    if let Some((msg, mt)) = &g.message {
        let a = mt.min(1.0);
        let font = FontId::new(17.0, bold());
        let width = ctx.fonts_mut(|f| f.layout_no_wrap(msg.clone(), font.clone(), TEXT).size().x);
        // Above the gem tray while it's showing.
        let y = view.max.y - if matches!(g.phase, Phase::Build | Phase::Choose) { 150.0 } else { 60.0 };
        p.rect_filled(Rect::from_center_size(Pos2::new(cx, y), V2::new(width + 32.0, 34.0)), CornerRadius::same(3), alpha(Color32::BLACK, 0.7 * a));
        p.text(Pos2::new(cx, y), Align2::CENTER_CENTER, msg, font, alpha(TEXT, a));
    }

    if st.paused {
        p.text(Pos2::new(cx, view.min.y + 160.0), Align2::CENTER_CENTER, "Paused", FontId::new(44.0, heading()), alpha(TEXT, 0.85));
    }

    let _ = t;
}
