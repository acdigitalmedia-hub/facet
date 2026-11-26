//! Drawing and input: the title screen (map and difficulty selection) and the in-game
//! board and side panel. Shapes come from `art`, text from `text`.

use std::cell::Cell as Flag;

use macroquad::prelude::*;

use crate::art::{self, mix, with_alpha, Theme};
use crate::data::{Balance, MapDef};
use crate::game::{rgb, Event, FxKind, Game, Kind, Occ, Phase, Stats, Tower, AURA_RANGE};
use crate::path::{full_route, Cell};
use crate::save::Save;
use crate::text::{self, Style};

pub const PANEL_W: f32 = 350.0;
const MARGIN: f32 = 18.0;

const BG: Color = Color::new(0.05, 0.05, 0.06, 1.0);
const TEXT: Color = Color::new(0.93, 0.91, 0.86, 1.0);
const DIM: Color = Color::new(0.63, 0.61, 0.56, 1.0);
const GOLD: Color = Color::new(0.93, 0.77, 0.42, 1.0);
const INFO: Color = Color::new(0.55, 0.80, 1.0, 1.0);
const BAD: Color = Color::new(1.0, 0.38, 0.33, 1.0);
const GOOD: Color = Color::new(0.50, 0.90, 0.55, 1.0);
const RECIPE: Color = Color::new(1.0, 0.58, 0.95, 1.0);

fn diff_color(d: usize) -> Color {
    [GOOD, GOLD, BAD][d.min(2)]
}

/// Records are kept per map and difficulty.
pub fn record_key(map: &str, diff: &str) -> String {
    format!("{map} / {diff}")
}

// ---------- Shared widgets ----------

/// Stone button with a gold (or tinted) border. Returns true when clicked.
fn button(r: Rect, label: &str, enabled: bool, tint: Color, mouse: Vec2, clicked: bool, flag: &Flag<bool>) -> bool {
    let hover = enabled && r.contains(mouse);
    let (bg, border) = if !enabled {
        (Color::new(0.11, 0.11, 0.11, 1.0), with_alpha(GRAY, 0.25))
    } else if hover {
        (mix(Color::new(0.22, 0.20, 0.18, 1.0), tint, 0.18), tint)
    } else {
        (mix(Color::new(0.16, 0.15, 0.14, 1.0), tint, 0.08), with_alpha(tint, 0.55))
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle(r.x, r.y, r.w, r.h * 0.45, with_alpha(WHITE, 0.03));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, border);
    text::centered(label, r.x + r.w / 2.0, r.y + r.h / 2.0, 15.0, if enabled { TEXT } else { DIM }, Style::Bold);
    let hit = hover && clicked;
    if hit {
        flag.set(true);
    }
    hit
}

/// Gold section heading with an ornamental rule.
fn section(x: f32, y: &mut f32, w: f32, title: &str) {
    *y += 10.0;
    let size = 15.0;
    text::heading(title, x, *y + 12.0, size, GOLD);
    let tw = text::measure(title, size, Style::Heading).width;
    let ly = *y + 7.0;
    draw_line(x + tw + 10.0, ly, x + w, ly, 1.0, with_alpha(GOLD, 0.35));
    draw_poly(x + tw + 10.0, ly, 4, 3.0, 45.0, GOLD);
    *y += 22.0;
}

fn panel_background(x: f32, w: f32, h: f32) {
    let bands = 24;
    for k in 0..bands {
        let t = k as f32 / bands as f32;
        let c = mix(Color::new(0.12, 0.11, 0.11, 1.0), Color::new(0.07, 0.07, 0.08, 1.0), t);
        draw_rectangle(x, h * t, w, h / bands as f32 + 1.0, c);
    }
    draw_line(x + 1.0, 0.0, x + 1.0, h, 2.0, with_alpha(GOLD, 0.55));
    draw_line(x + 5.0, 0.0, x + 5.0, h, 1.0, with_alpha(GOLD, 0.18));
}

/// A carved frame around a rectangle.
fn frame(r: Rect) {
    draw_rectangle_lines(r.x - 7.0, r.y - 7.0, r.w + 14.0, r.h + 14.0, 6.0, Color::new(0.16, 0.14, 0.12, 1.0));
    draw_rectangle_lines(r.x - 4.0, r.y - 4.0, r.w + 8.0, r.h + 8.0, 1.5, with_alpha(GOLD, 0.6));
    draw_rectangle_lines(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, 1.0, with_alpha(BLACK, 0.8));
    for (cx, cy) in [(r.x, r.y), (r.x + r.w, r.y), (r.x, r.y + r.h), (r.x + r.w, r.y + r.h)] {
        draw_poly(cx, cy, 4, 6.0, 45.0, GOLD);
        draw_poly(cx, cy, 4, 3.0, 45.0, Color::new(0.3, 0.2, 0.1, 1.0));
    }
}

/// Draws a tower (pedestal + gem, or a special) centered at `p` in a cell of size `cs`.
fn tower_art(g: &Game, t: &Tower, p: Vec2, cs: f32, time: f32, key: usize) {
    let col = g.color(t.kind);
    let pulse = 1.0 + 0.15 * t.flash;
    match t.kind {
        Kind::Gem(gem) => {
            art::pedestal(p, cs, art::tier_rim(t.rank), time, t.rank >= 4);
            let r = cs * (0.31 + 0.02 * t.rank as f32) * pulse;
            art::gem(p - vec2(0.0, cs * 0.06), r, gem, col, t.rank, time, key);
        }
        Kind::Special(_) => art::special(p, cs * pulse, col, t.rank, time, key),
    }
    if t.flash > 0.0 {
        art::glow(p, cs * 0.6, col, t.flash * 1.5);
    }
}

fn lightning(a: Vec2, b: Vec2, col: Color, width: f32) {
    let n = 6;
    let d = b - a;
    let perp = vec2(-d.y, d.x).normalize_or_zero();
    let mut prev = a;
    for k in 1..=n {
        let t = k as f32 / n as f32;
        let off = if k == n { 0.0 } else { rand::gen_range(-1.0, 1.0) * d.length() * 0.08 };
        let p = a + d * t + perp * off;
        draw_line(prev.x, prev.y, p.x, p.y, width * 2.5, with_alpha(col, col.a * 0.3));
        draw_line(prev.x, prev.y, p.x, p.y, width, mix(col, WHITE, 0.5));
        prev = p;
    }
}

fn dashed_line(a: Vec2, b: Vec2, dash: f32, offset: f32, col: Color) {
    let len = a.distance(b);
    if len < 1.0 {
        return;
    }
    let dir = (b - a) / len;
    let mut d = -(offset % (dash * 2.0));
    while d < len {
        let s = d.max(0.0);
        let e = (d + dash).min(len);
        if e > s {
            let (p0, p1) = (a + dir * s, a + dir * e);
            draw_line(p0.x, p0.y, p1.x, p1.y, 2.0, col);
        }
        d += dash * 2.0;
    }
}

fn effects_text(s: &Stats) -> String {
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
        v.push(format!("x{:.1} vs flyers", s.air_mult));
    }
    if s.ground_only {
        v.push("Can't hit flyers".into());
    }
    v.join(", ")
}

// ---------- Title screen ----------

pub struct Title {
    pub map: usize,
    terrains: Vec<Texture2D>,
    routes: Vec<Vec<Cell>>,
    pub button_clicked: Flag<bool>,
}

impl Title {
    pub fn new() -> Title {
        Title { map: 0, terrains: Vec::new(), routes: Vec::new(), button_clicked: Flag::new(false) }
    }

    fn ensure(&mut self, maps: &[MapDef]) {
        if self.terrains.len() != maps.len() {
            self.terrains = maps.iter().map(art::terrain_texture).collect();
            self.routes = maps
                .iter()
                .map(|m| full_route(m.w, m.h, &m.waypoints, &|x, y| m.is_wall(x, y)).unwrap_or_default())
                .collect();
        }
        self.map = self.map.min(maps.len() - 1);
    }

    /// Draws a map's terrain, road and checkpoints scaled into `r` (keeping aspect ratio).
    fn minimap(&self, maps: &[MapDef], k: usize, r: Rect, time: f32) -> Rect {
        let m = &maps[k];
        let cs = (r.w / m.w as f32).min(r.h / m.h as f32);
        let (w, h) = (cs * m.w as f32, cs * m.h as f32);
        let o = vec2(r.x + (r.w - w) / 2.0, r.y + (r.h - h) / 2.0);
        draw_texture_ex(&self.terrains[k], o.x, o.y, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
        let pts: Vec<Vec2> = self.routes[k].iter().map(|c| o + vec2(c.0 as f32 + 0.5, c.1 as f32 + 0.5) * cs).collect();
        art::road(&pts, cs, &art::theme(&m.theme), 0.9);
        let n = m.waypoints.len();
        for (j, wp) in m.waypoints.iter().enumerate() {
            let p = o + vec2(wp.0 as f32 + 0.5, wp.1 as f32 + 0.5) * cs;
            let col = if j == 0 { GOOD } else if j == n - 1 { BAD } else { INFO };
            art::glow(p, cs * 2.0, col, 0.8 + 0.3 * (time * 2.0 + j as f32).sin());
            draw_circle(p.x, p.y, cs * 0.9, col);
        }
        Rect::new(o.x, o.y, w, h)
    }

    /// Returns (map, difficulty) when the player starts a game.
    pub fn draw(&mut self, bal: &Balance, maps: &[MapDef], save: &Save, err: Option<&str>) -> Option<(usize, usize)> {
        self.ensure(maps);
        let mouse = Vec2::from(mouse_position());
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        let (sw, sh) = (screen_width(), screen_height());
        let cx = sw / 2.0;
        let time = get_time() as f32;

        if is_key_pressed(KeyCode::Left) {
            self.map = (self.map + maps.len() - 1) % maps.len();
        }
        if is_key_pressed(KeyCode::Right) {
            self.map = (self.map + 1) % maps.len();
        }

        // Backdrop: the selected map, darkened
        clear_background(BG);
        let tex = &self.terrains[self.map];
        let s = (sw / tex.width()).max(sh / tex.height());
        draw_texture_ex(tex, cx - tex.width() * s / 2.0, sh / 2.0 - tex.height() * s / 2.0, Color::new(0.35, 0.35, 0.35, 1.0), DrawTextureParams {
            dest_size: Some(vec2(tex.width() * s, tex.height() * s)),
            ..Default::default()
        });
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.35));

        // Title with orbiting gems
        let ty = sh * 0.12;
        for (k, gem) in bal.gems.iter().enumerate() {
            let a = time * 0.3 + k as f32 / bal.gems.len() as f32 * std::f32::consts::TAU;
            let p = vec2(cx + a.cos() * 260.0, ty + a.sin() * 55.0);
            if a.sin() < 0.0 {
                art::gem(p, 13.0, k, rgb(gem.color), 2, time, k);
            }
        }
        text::title("FACET", cx, ty, 84.0, GOLD);
        for (k, gem) in bal.gems.iter().enumerate() {
            let a = time * 0.3 + k as f32 / bal.gems.len() as f32 * std::f32::consts::TAU;
            let p = vec2(cx + a.cos() * 260.0, ty + a.sin() * 55.0);
            if a.sin() >= 0.0 {
                art::gem(p, 15.0, k, rgb(gem.color), 2, time, k);
            }
        }
        text::centered("a gem maze defense", cx, ty + 70.0, 20.0, DIM, Style::Heading);

        // Map picker
        let n = maps.len() as f32;
        let card_w = 190.0f32.min((sw - 80.0) / n - 14.0);
        let card_h = card_w * 0.8 + 32.0;
        let total = n * card_w + (n - 1.0) * 14.0;
        let my = ty + 110.0;
        for (k, m) in maps.iter().enumerate() {
            let r = Rect::new(cx - total / 2.0 + k as f32 * (card_w + 14.0), my, card_w, card_h);
            let hover = r.contains(mouse);
            let sel = k == self.map;
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.08, 0.08, 0.09, 0.9));
            let mm = self.minimap(maps, k, Rect::new(r.x + 8.0, r.y + 8.0, r.w - 16.0, r.h - 40.0), time);
            if !sel {
                draw_rectangle(mm.x, mm.y, mm.w, mm.h, Color::new(0.0, 0.0, 0.0, if hover { 0.15 } else { 0.4 }));
            }
            let border = if sel { GOLD } else if hover { with_alpha(GOLD, 0.6) } else { with_alpha(GOLD, 0.2) };
            draw_rectangle_lines(r.x, r.y, r.w, r.h, if sel { 3.0 } else { 1.5 }, border);
            text::centered(&m.name, r.x + r.w / 2.0, r.y + r.h - 16.0, 16.0, if sel { GOLD } else { TEXT }, Style::Heading);
            if hover && clicked {
                self.map = k;
                self.button_clicked.set(true);
            }
        }
        let m = &maps[self.map];
        text::centered(&m.desc, cx, my + card_h + 24.0, 16.0, TEXT, Style::Body);
        text::centered("Left/Right or click to choose a map", cx, my + card_h + 46.0, 13.0, DIM, Style::Body);

        // Difficulty cards
        let nd = bal.difficulties.len() as f32;
        let dw = 290.0f32.min((sw - 80.0) / nd - 18.0);
        let dh = 150.0;
        let dtotal = nd * dw + (nd - 1.0) * 18.0;
        let dy = my + card_h + 74.0;
        let mut chosen = None;
        for (k, d) in bal.difficulties.iter().enumerate() {
            let r = Rect::new(cx - dtotal / 2.0 + k as f32 * (dw + 18.0), dy, dw, dh);
            let hover = r.contains(mouse);
            let tint = diff_color(k);
            draw_rectangle(r.x, r.y, r.w, r.h, if hover { Color::new(0.15, 0.14, 0.13, 0.95) } else { Color::new(0.09, 0.09, 0.10, 0.92) });
            draw_rectangle_lines(r.x, r.y, r.w, r.h, if hover { 2.5 } else { 1.2 }, with_alpha(tint, if hover { 1.0 } else { 0.5 }));
            text::heading(&format!("{}", d.name), r.x + 14.0, r.y + 30.0, 22.0, tint);
            let kw = text::width(&format!("[{}]", k + 1), 14.0);
            text::text(&format!("[{}]", k + 1), r.x + r.w - kw - 14.0, r.y + 28.0, 14.0, DIM);
            let y = text::wrapped(&d.desc, r.x + 14.0, r.y + 36.0, r.w - 28.0, 14.0, TEXT);
            let boss = if d.boss_lives >= d.lives { "defeat".to_string() } else { format!("-{} lives", d.boss_lives) };
            text::text(&format!("{} lives  -  boss escape: {boss}", d.lives), r.x + 14.0, y + 14.0, 13.0, DIM);
            let rec = save.record(&record_key(&m.name, &d.name));
            let best = if rec.runs == 0 {
                "No runs yet".to_string()
            } else {
                let mut b = match rec.best_win_lives {
                    Some(l) => format!("Won {}/{}  -  best {l} lives left", rec.wins, rec.runs),
                    None => format!("Best: cleared {} of {} waves", rec.best_cleared.min(bal.wave_count()), bal.wave_count()),
                };
                if rec.best_cleared > bal.wave_count() {
                    b.push_str(&format!("  -  endless {}", rec.best_cleared + 1));
                }
                b
            };
            text::text(&best, r.x + 14.0, r.y + r.h - 12.0, 13.0, if rec.wins > 0 { GOLD } else { DIM });
            if hover && clicked {
                chosen = Some((self.map, k));
                self.button_clicked.set(true);
            }
        }
        for (k, key) in [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3].iter().enumerate() {
            if k < bal.difficulties.len() && is_key_pressed(*key) {
                chosen = Some((self.map, k));
            }
        }

        let onoff = |b: bool| if b { "on" } else { "off" };
        text::centered(&format!("[M] Music: {}      [N] Sound: {}", onoff(save.music), onoff(save.sfx)), cx, sh - 46.0, 14.0, DIM, Style::Body);
        if let Some(e) = err {
            text::centered(e, cx, sh - 20.0, 14.0, BAD, Style::Body);
        }
        chosen
    }
}

// ---------- In-game UI ----------

#[derive(PartialEq, Eq)]
pub enum Action {
    None,
    ToTitle,
}

struct Particle {
    pos: Vec2,
    vel: Vec2,
    t: f32,
    life: f32,
    color: Color,
    size: f32,
}

pub struct Ui {
    pub speed: u32,
    pub paused: bool,
    pub book: bool,
    /// Set by main when the finished run beat a saved record.
    pub new_best: bool,
    /// Set whenever a button is clicked this frame (for the click sound).
    pub button_clicked: Flag<bool>,
    terrain: Texture2D,
    theme: Theme,
    particles: Vec<Particle>,
    shake: f32,
    vignette: f32,
    banner: Option<(String, Color, f32)>,
    hover: Option<Cell>,
    /// Cached preview route for the hovered cell: (cell, board rev, route or None if blocking).
    preview: Option<(Cell, u64, Option<Vec<Cell>>)>,
    clicked: bool,
    mouse: Vec2,
}

pub struct Layout {
    pub origin: Vec2,
    pub cell: f32,
    pub panel_x: f32,
}

impl Layout {
    pub fn compute(g: &Game) -> Layout {
        let (sw, sh) = (screen_width(), screen_height());
        let avail_w = sw - PANEL_W - MARGIN * 2.0;
        let avail_h = sh - MARGIN * 2.0;
        let cell = (avail_w / g.w() as f32).min(avail_h / g.h() as f32).floor().max(4.0);
        let (bw, bh) = (cell * g.w() as f32, cell * g.h() as f32);
        let origin = vec2(MARGIN + (avail_w - bw) / 2.0, MARGIN + (avail_h - bh) / 2.0);
        Layout { origin, cell, panel_x: sw - PANEL_W }
    }
    fn to_screen(&self, p: Vec2) -> Vec2 {
        self.origin + p * self.cell
    }
    fn cell_center(&self, c: Cell) -> Vec2 {
        self.to_screen(vec2(c.0 as f32 + 0.5, c.1 as f32 + 0.5))
    }
    fn cell_rect(&self, c: Cell) -> Rect {
        Rect::new(self.origin.x + c.0 as f32 * self.cell, self.origin.y + c.1 as f32 * self.cell, self.cell, self.cell)
    }
    fn cell_at(&self, s: Vec2, g: &Game) -> Option<Cell> {
        let p = (s - self.origin) / self.cell;
        let c = (p.x.floor() as i32, p.y.floor() as i32);
        g.in_bounds(c).then_some(c)
    }
    fn board(&self, g: &Game) -> Rect {
        Rect::new(self.origin.x, self.origin.y, self.cell * g.w() as f32, self.cell * g.h() as f32)
    }
}

impl Ui {
    pub fn new(g: &Game) -> Ui {
        Ui {
            speed: 1,
            paused: false,
            book: false,
            new_best: false,
            button_clicked: Flag::new(false),
            terrain: art::terrain_texture(&g.map),
            theme: art::theme(&g.map.theme),
            particles: Vec::new(),
            shake: 0.0,
            vignette: 0.0,
            banner: None,
            hover: None,
            preview: None,
            clicked: false,
            mouse: Vec2::ZERO,
        }
    }

    fn button(&self, r: Rect, label: &str, enabled: bool) -> bool {
        button(r, label, enabled, GOLD, self.mouse, self.clicked, &self.button_clicked)
    }

    fn button_tinted(&self, r: Rect, label: &str, enabled: bool, tint: Color) -> bool {
        button(r, label, enabled, tint, self.mouse, self.clicked, &self.button_clicked)
    }

    fn burst(&mut self, pos: Vec2, color: Color, n: usize, speed: f32, size: f32, life: f32) {
        for _ in 0..n {
            let a = rand::gen_range(0.0, std::f32::consts::TAU);
            let v = rand::gen_range(0.3, 1.0) * speed;
            self.particles.push(Particle {
                pos,
                vel: vec2(a.cos(), a.sin()) * v,
                t: 0.0,
                life: life * rand::gen_range(0.6, 1.2),
                color,
                size: size * rand::gen_range(0.6, 1.3),
            });
        }
    }

    /// Visual reaction to a game event.
    pub fn effect(&mut self, e: &Event) {
        match e {
            Event::Place { pos, gem: _ } => self.burst(*pos, WHITE, 8, 2.5, 0.07, 0.4),
            Event::Keep { pos, color } => self.burst(*pos, *color, 14, 3.0, 0.08, 0.6),
            Event::Combine { pos, color } => {
                self.burst(*pos, *color, 30, 5.0, 0.1, 0.8);
                self.burst(*pos, WHITE, 10, 3.0, 0.06, 0.6);
                self.shake = self.shake.max(3.0);
            }
            Event::Forge { pos, color } => {
                self.burst(*pos, *color, 50, 7.0, 0.12, 1.0);
                self.burst(*pos, GOLD, 20, 4.0, 0.08, 0.9);
                self.burst(*pos, WHITE, 20, 2.0, 0.06, 0.8);
                self.shake = self.shake.max(6.0);
            }
            Event::Upgrade { pos: Some(pos), color } => {
                self.burst(*pos, *color, 24, 4.0, 0.1, 0.7);
                self.burst(*pos, GOLD, 12, 2.5, 0.07, 0.7);
            }
            Event::Splash { pos, color, radius } => self.burst(*pos, *color, 6, radius * 3.0, 0.06, 0.35),
            Event::Crit { pos, color } => {
                self.burst(*pos, WHITE, 4, 4.0, 0.06, 0.3);
                self.burst(*pos, *color, 4, 3.0, 0.06, 0.3);
            }
            Event::Death { pos, color, boss } => {
                if *boss {
                    self.burst(*pos, *color, 70, 8.0, 0.16, 1.4);
                    self.burst(*pos, GOLD, 30, 5.0, 0.1, 1.2);
                    self.shake = self.shake.max(12.0);
                } else {
                    self.burst(*pos, *color, 10, 3.0, 0.08, 0.5);
                }
            }
            Event::Leak { lives } => {
                self.shake = self.shake.max(4.0 + *lives as f32);
                self.vignette = 1.0;
            }
            Event::WaveStart { boss, name, number } => {
                let (t, col) = if *boss { (format!("Wave {number}  -  {}", name.to_uppercase()), BAD) } else { (format!("Wave {number}  -  {name}"), GOLD) };
                self.banner = Some((t, col, 0.0));
                if *boss {
                    self.shake = self.shake.max(5.0);
                }
            }
            Event::RockRemoved { pos } => self.burst(*pos, self.theme.rock, 14, 3.0, 0.1, 0.5),
            _ => {}
        }
    }

    /// Handles input for a game in progress.
    pub fn input(&mut self, g: &mut Game) -> Action {
        let lay = Layout::compute(g);
        let (mx, my) = mouse_position();
        self.mouse = vec2(mx, my);
        self.clicked = is_mouse_button_pressed(MouseButton::Left);
        self.hover = if mx < lay.panel_x && !self.book { lay.cell_at(self.mouse, g) } else { None };

        for (key, sp) in [(KeyCode::Key1, 1), (KeyCode::Key2, 2), (KeyCode::Key3, 4)] {
            if is_key_pressed(key) {
                self.speed = sp;
            }
        }
        if is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::P) {
            self.paused = !self.paused;
        }
        if is_key_pressed(KeyCode::B) {
            self.book = !self.book;
        }
        if is_key_pressed(KeyCode::U) {
            g.buy_luck();
        }
        if is_key_pressed(KeyCode::Escape) {
            if self.book {
                self.book = false;
            } else {
                g.selected = None;
            }
        }
        if let Some(sel) = g.selected {
            if is_key_pressed(KeyCode::X) && g.is_rock(sel) {
                g.remove_rock(sel);
            }
            if is_key_pressed(KeyCode::L) {
                g.upgrade_special(sel);
            }
        }
        if g.phase == Phase::Choose {
            if is_key_pressed(KeyCode::K) {
                g.keep(false);
            }
            if is_key_pressed(KeyCode::C) {
                g.keep(true);
            }
        }
        if matches!(g.phase, Phase::GameOver | Phase::Victory) && is_key_pressed(KeyCode::R) {
            return Action::ToTitle;
        }
        if g.phase == Phase::Victory && is_key_pressed(KeyCode::E) {
            g.continue_endless();
        }

        if self.clicked && mx < lay.panel_x {
            if self.book {
                self.book = false;
                self.clicked = false;
            } else if let Some(c) = self.hover {
                let i = g.idx(c);
                match (&g.grid[i], g.phase) {
                    (Occ::Empty, Phase::Build) => g.place(c),
                    (Occ::Tower(_) | Occ::Rock, _) => g.selected = Some(i),
                    _ => g.selected = None,
                }
            }
        }
        if is_mouse_button_pressed(MouseButton::Right) {
            g.selected = None;
        }

        // Refresh the hover path preview only when the hovered cell or board changes.
        self.preview = match (g.phase, self.hover) {
            (Phase::Build, Some(c)) => match self.preview.take() {
                Some(p) if p.0 == c && p.1 == g.rev => Some(p),
                _ => {
                    let r = if g.can_place(c).is_ok() { g.route_with(Some(g.idx(c))) } else { None };
                    Some((c, g.rev, r))
                }
            },
            _ => None,
        };
        Action::None
    }

    pub fn draw(&mut self, g: &mut Game) {
        clear_background(BG);
        let real_dt = get_frame_time().min(0.1);
        let dt = if self.paused || self.book { 0.0 } else { real_dt * self.speed as f32 };
        for p in &mut self.particles {
            p.t += dt;
            p.pos += p.vel * dt;
            p.vel *= (1.0 - 3.0 * dt).max(0.0);
        }
        self.particles.retain(|p| p.t < p.life);
        self.shake *= (1.0 - 10.0 * real_dt).max(0.0);
        self.vignette = (self.vignette - real_dt * 1.5).max(0.0);
        if let Some((_, _, t)) = &mut self.banner {
            *t += real_dt;
        }
        self.banner = self.banner.take().filter(|b| b.2 < 2.4);

        let mut lay = Layout::compute(g);
        frame(lay.board(g));
        if self.shake > 0.3 {
            lay.origin += vec2(rand::gen_range(-1.0, 1.0), rand::gen_range(-1.0, 1.0)) * self.shake;
        }
        self.draw_board(g, &lay);
        self.draw_particles(&lay);
        self.draw_boss_bar(g, &lay);
        self.draw_panel(g, &lay);
        if self.book {
            self.draw_book(g, &lay);
        }
        self.draw_overlay(g, &lay);
    }

    fn draw_particles(&self, lay: &Layout) {
        for p in &self.particles {
            let k = p.t / p.life;
            let s = lay.to_screen(p.pos);
            let col = with_alpha(p.color, (1.0 - k).powf(0.7));
            let r = (p.size * lay.cell * (1.0 - k * 0.5)).max(1.0);
            draw_poly(s.x, s.y, 4, r, 45.0 + p.t * 400.0, col);
        }
    }

    fn wave_label(g: &Game) -> String {
        if g.endless || g.wave >= g.bal.wave_count() {
            format!("Wave {}", g.wave + 1)
        } else {
            format!("Wave {}/{}", g.wave + 1, g.bal.wave_count())
        }
    }

    fn draw_board(&self, g: &Game, lay: &Layout) {
        let cs = lay.cell;
        let b = lay.board(g);
        let time = get_time() as f32;
        draw_texture_ex(&self.terrain, b.x, b.y, WHITE, DrawTextureParams { dest_size: Some(vec2(b.w, b.h)), ..Default::default() });

        // The road follows the current maze
        let road: Vec<Vec2> = g.route.iter().map(|c| lay.cell_center(*c)).collect();
        art::road(&road, cs, &self.theme, 1.0);

        // Grid lines while building
        if matches!(g.phase, Phase::Build | Phase::Choose) {
            let col = Color::new(0.0, 0.0, 0.0, 0.12);
            for x in 0..=g.w() {
                let px = b.x + x as f32 * cs;
                draw_line(px, b.y, px, b.y + b.h, 1.0, col);
            }
            for y in 0..=g.h() {
                let py = b.y + y as f32 * cs;
                draw_line(b.x, py, b.x + b.w, py, 1.0, col);
            }
        }

        // Flyer path when an air wave is next/active
        if g.wave_info(g.wave).air {
            for pair in g.map.waypoints.windows(2) {
                dashed_line(lay.cell_center(pair[0]), lay.cell_center(pair[1]), cs * 0.6, time * cs, Color::new(0.6, 0.9, 1.0, 0.55));
            }
        }

        // Hover preview
        if let (Some(c), Phase::Build) = (self.hover, g.phase) {
            let r = lay.cell_rect(c);
            match &self.preview {
                Some((pc, rev, Some(route))) if *pc == c && *rev == g.rev => {
                    for pair in route.windows(2) {
                        let (a, b) = (lay.cell_center(pair[0]), lay.cell_center(pair[1]));
                        draw_line(a.x, a.y, b.x, b.y, cs * 0.1, Color::new(1.0, 0.88, 0.45, 0.55));
                    }
                    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 1.0, 1.0, 0.2));
                    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, with_alpha(GOLD, 0.9));
                }
                _ => {
                    if matches!(g.grid[g.idx(c)], Occ::Empty) {
                        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 0.2, 0.2, 0.35));
                    }
                }
            }
        }

        // Checkpoints
        let n = g.map.waypoints.len();
        for (k, &wp) in g.map.waypoints.iter().enumerate() {
            let p = lay.cell_center(wp);
            let (kind, col) = if k == 0 {
                (0, Color::new(0.4, 1.0, 0.55, 1.0))
            } else if k == n - 1 {
                (2, Color::new(1.0, 0.35, 0.3, 1.0))
            } else {
                (1, INFO)
            };
            art::checkpoint(p, cs, kind, col, time);
            if kind == 1 {
                text::centered(&k.to_string(), p.x, p.y, (cs * 0.8).max(11.0), TEXT, Style::Heading);
            }
        }

        // Recipe ingredients that can be forged right now get a highlight
        let ready: Vec<usize> = g.available_recipes().into_iter().flat_map(|(_, ings)| ings).collect();

        // Rocks and towers
        let pulse = (time * 5.0).sin() * 0.5 + 0.5;
        for i in 0..g.grid.len() {
            let c = g.cell_of(i);
            let p = lay.cell_center(c);
            match &g.grid[i] {
                Occ::Empty => {}
                Occ::Wall => {
                    if g.map.wall_style != "water" {
                        let jitter = vec2(art::rnd(i, 80) - 0.5, art::rnd(i, 81) - 0.5) * cs * 0.3;
                        let size = cs * (1.05 + art::rnd(i, 82) * 0.35);
                        art::boulder(p + jitter, size, i, art::scale(self.theme.rock, 0.7 + art::rnd(i, 83) * 0.25));
                    }
                }
                Occ::Rock => art::boulder(p, cs, i, self.theme.rock),
                Occ::Tower(t) => {
                    tower_art(g, t, p, cs, time, i);
                    let ring = |col: Color, r: f32| {
                        let pts: Vec<Vec2> = (0..8)
                            .map(|k| {
                                let a = std::f32::consts::PI / 8.0 + k as f32 * std::f32::consts::TAU / 8.0;
                                p + vec2(a.cos(), a.sin()) * r
                            })
                            .collect();
                        art::outline(&pts, 2.0, col);
                    };
                    if t.fresh {
                        ring(with_alpha(WHITE, 0.35 + 0.55 * pulse), cs * 0.52);
                    }
                    if ready.contains(&i) {
                        ring(with_alpha(RECIPE, 0.5 + 0.5 * pulse), cs * 0.58);
                    }
                }
            }
        }

        // Selection
        if let Some(i) = g.selected {
            let c = g.cell_of(i);
            let r = lay.cell_rect(c);
            if let Some(t) = g.tower(i) {
                let p = lay.cell_center(c);
                let s = g.stats(t);
                let col = g.color(t.kind);
                draw_circle(p.x, p.y, s.range * cs, with_alpha(col, 0.08));
                draw_circle_lines(p.x, p.y, s.range * cs, 1.5, with_alpha(col, 0.7));
                if s.aura > 0.0 {
                    draw_circle_lines(p.x, p.y, AURA_RANGE * cs, 1.5, with_alpha(WHITE, 0.4));
                }
            }
            let k = cs * 0.3;
            for (cx, cy, dx, dy) in [(r.x, r.y, 1.0, 1.0), (r.x + r.w, r.y, -1.0, 1.0), (r.x, r.y + r.h, 1.0, -1.0), (r.x + r.w, r.y + r.h, -1.0, -1.0)] {
                draw_line(cx, cy, cx + k * dx, cy, 2.5, GOLD);
                draw_line(cx, cy, cx, cy + k * dy, 2.5, GOLD);
            }
        }

        // Creeps (ground first, flyers on top)
        for air in [false, true] {
            for c in g.creeps.iter().filter(|c| c.air == air) {
                let p = lay.to_screen(c.pos);
                let kind = &g.bal.waves.kinds[c.kind];
                let size = cs * kind.size * 1.4;
                let mut col = rgb(kind.color);
                if c.slow_t > 0.0 {
                    col = mix(col, Color::new(0.6, 0.85, 1.0, 1.0), 0.45);
                }
                art::creature(&kind.look, p, c.dir, size, col, c.age, c.air);
                let body = p - vec2(0.0, if c.air { size * 0.5 } else { 0.0 });
                if c.flash > 0.0 {
                    draw_circle(body.x, body.y, size * 0.8, with_alpha(WHITE, (c.flash / 0.08) * 0.55));
                }
                if c.slow_t > 0.0 {
                    draw_circle_lines(body.x, body.y, size * 1.05, 1.5, Color::new(0.7, 0.9, 1.0, 0.6));
                }
                if c.poison_t > 0.0 {
                    for k in 0..3 {
                        let t = (c.age * 1.5 + k as f32 * 0.33).fract();
                        let bp = body + vec2((k as f32 - 1.0) * size * 0.4, -t * size * 1.2);
                        draw_circle(bp.x, bp.y, size * 0.12 * (1.0 - t), Color::new(0.4, 1.0, 0.4, 0.8 * (1.0 - t)));
                    }
                }
                if c.regen > 0.0 {
                    let a = 0.35 + 0.3 * (time * 4.0).sin();
                    draw_circle_lines(body.x, body.y, size * 1.2, 2.0, Color::new(0.3, 1.0, 0.5, a));
                }
                if c.armor - c.shred >= 3.0 && !c.boss {
                    draw_poly_lines(body.x, body.y, 6, size * 1.05, 30.0, 1.5, Color::new(0.85, 0.88, 0.95, 0.5));
                }
                if !c.boss && c.hp < c.max_hp {
                    let frac = (c.hp / c.max_hp).clamp(0.0, 1.0);
                    let bw = cs * 0.8;
                    let by = body.y - size - 6.0;
                    draw_rectangle(p.x - bw / 2.0 - 1.0, by - 1.0, bw + 2.0, 5.0, Color::new(0.0, 0.0, 0.0, 0.75));
                    draw_rectangle(p.x - bw / 2.0, by, bw * frac, 3.0, mix(BAD, GOOD, frac));
                }
            }
        }

        // Projectiles
        for p in &g.projectiles {
            let s = lay.to_screen(p.pos);
            let tail = lay.to_screen(p.pos - p.vel * 0.05);
            let r = cs * p.size;
            draw_line(tail.x, tail.y, s.x, s.y, r * 1.5, with_alpha(p.color, 0.3));
            art::glow(s, r * 2.5, p.color, 1.0);
            draw_circle(s.x, s.y, r, p.color);
            draw_circle(s.x, s.y, r * 0.5, with_alpha(WHITE, 0.9));
        }

        // Effects
        for f in &g.fx {
            let k = f.t / f.life;
            let col = with_alpha(f.color, 1.0 - k);
            match &f.kind {
                FxKind::Beam(a0, b0) => lightning(lay.to_screen(*a0), lay.to_screen(*b0), col, 2.0),
                FxKind::Ring(p, r) => {
                    let s = lay.to_screen(*p);
                    let rr = r * cs * (0.4 + 0.6 * k);
                    draw_circle(s.x, s.y, rr, with_alpha(f.color, (1.0 - k) * 0.12));
                    draw_circle_lines(s.x, s.y, rr, 2.0, col);
                }
                FxKind::Text(p, t) => {
                    let s = lay.to_screen(*p) - vec2(0.0, k * cs * 1.2 + cs * 0.5);
                    text::centered(t, s.x + 1.0, s.y + 1.0, 15.0, with_alpha(BLACK, 1.0 - k), Style::Bold);
                    text::centered(t, s.x, s.y, 15.0, col, Style::Bold);
                }
            }
        }
    }

    fn draw_boss_bar(&self, g: &Game, lay: &Layout) {
        let Some(c) = g.creeps.iter().find(|c| c.boss && c.hp > 0.0) else { return };
        let b = lay.board(g);
        let w = b.w * 0.5;
        let r = Rect::new(b.x + (b.w - w) / 2.0, b.y + 16.0, w, 14.0);
        let kind = &g.bal.waves.kinds[c.kind];
        text::centered(&kind.name.to_uppercase(), r.x + r.w / 2.0, r.y - 2.0, 16.0, BAD, Style::Heading);
        draw_rectangle(r.x - 2.0, r.y + 6.0, r.w + 4.0, r.h + 4.0, Color::new(0.0, 0.0, 0.0, 0.8));
        let frac = (c.hp / c.max_hp).clamp(0.0, 1.0);
        draw_rectangle(r.x, r.y + 8.0, r.w * frac, r.h, Color::new(0.75, 0.12, 0.1, 1.0));
        draw_rectangle(r.x, r.y + 8.0, r.w * frac, r.h * 0.4, Color::new(1.0, 0.4, 0.35, 0.6));
        draw_rectangle_lines(r.x - 2.0, r.y + 6.0, r.w + 4.0, r.h + 4.0, 1.5, GOLD);
    }

    fn draw_panel(&mut self, g: &mut Game, lay: &Layout) {
        let x0 = lay.panel_x;
        let pad = 22.0;
        let x = x0 + pad;
        let inner = PANEL_W - pad * 1.6;
        panel_background(x0, PANEL_W, screen_height());
        let mut y = 44.0;

        text::heading("FACET", x, y, 34.0, GOLD);
        let mode = format!("{}{}", g.bal.difficulties[g.diff].name, if g.endless { "  -  Endless" } else { "" });
        let mw = text::width(&mode, 14.0);
        text::text(&mode, x + inner - mw, y - 14.0, 14.0, diff_color(g.diff));
        let mapw = text::width(&g.map.name, 13.0);
        text::text(&g.map.name, x + inner - mapw, y + 2.0, 13.0, DIM);
        y += 30.0;

        // Stats row
        let lives_col = if g.lives <= 5 { BAD } else { TEXT };
        let stats = [(Self::wave_label(g), TEXT), (format!("{} lives", g.lives), lives_col), (format!("{} gold", g.gold), GOLD)];
        let mut sx = x;
        for (s, col) in stats {
            text::bold(&s, sx, y, 17.0, col);
            sx += text::measure(&s, 17.0, Style::Bold).width + 20.0;
        }
        y += 8.0;

        // Phase guidance
        let hint = match g.phase {
            Phase::Build => format!("Place gems ({} left). Each gem is also a wall: shape the maze!", g.bal.gems_per_round - g.placed.len()),
            Phase::Choose => "Click one of the new (glowing) gems, then keep it. The rest turn to rock.".into(),
            Phase::Wave => format!("Wave in progress: {} enemies left", g.creeps.len()),
            Phase::GameOver => "Defeated. Press R to return to the title.".into(),
            Phase::Victory => "Victory! Press E for endless or R for the title.".into(),
        };
        y = text::wrapped(&hint, x, y, inner, 14.0, DIM);
        if !g.available_recipes().is_empty() {
            y = text::wrapped("A recipe is ready! Click a pink-ringed gem to forge it.", x, y, inner, 14.0, RECIPE);
        }
        if matches!(g.phase, Phase::Build | Phase::Choose) {
            for (r, (gem, tier)) in g.near_recipes().into_iter().take(2) {
                let line = format!("Close: {} (need {} {})", g.bal.specials[r].name, g.bal.tier_names[tier], g.bal.gems[gem].name);
                y = text::wrapped(&line, x, y, inner, 13.0, with_alpha(RECIPE, 0.8));
            }
        }
        y += 6.0;

        // This round's gems
        let time = get_time() as f32;
        if matches!(g.phase, Phase::Build | Phase::Choose) {
            let slot = 44.0;
            for k in 0..g.bal.gems_per_round {
                let sx = x + k as f32 * (slot + 9.0);
                draw_rectangle(sx, y, slot, slot, Color::new(0.06, 0.06, 0.07, 1.0));
                draw_rectangle_lines(sx, y, slot, slot, 1.0, with_alpha(GOLD, 0.3));
                match g.placed.get(k).and_then(|&i| g.tower(i).map(|t| (i, t))) {
                    Some((i, t)) => tower_art(g, t, vec2(sx + slot / 2.0, y + slot / 2.0), slot * 0.95, time, i),
                    None => text::centered("?", sx + slot / 2.0, y + slot / 2.0, 20.0, DIM, Style::Heading),
                }
            }
            y += slot + 12.0;
        }

        // Keep / combine
        if g.phase == Phase::Choose {
            let sel_fresh = g.selected.filter(|&i| g.tower(i).is_some_and(|t| t.fresh));
            if self.button(Rect::new(x, y, inner, 30.0), "Keep selected  [K]", sel_fresh.is_some()) {
                g.keep(false);
            }
            y += 36.0;
            let steps = sel_fresh.map_or(0, |i| g.combine_steps(i));
            let label = match sel_fresh {
                Some(i) if steps > 0 => {
                    let t = g.tower(i).unwrap();
                    let what = if steps == 2 { "Four of a kind" } else { "Combine" };
                    format!("{what} -> {}  [C]", g.bal.tier_names[t.rank + steps])
                }
                _ => "Combine (needs a matching pair)".into(),
            };
            if self.button(Rect::new(x, y, inner, 30.0), &label, steps > 0) {
                g.keep(true);
            }
            y += 38.0;
        }

        if let Some(sel) = g.selected {
            y = self.draw_selected(g, sel, x, y, inner);
        }

        // Gem quality
        section(x, &mut y, inner, "Gem Quality");
        let odds = g.bal.luck_odds[g.luck.min(g.bal.luck_odds.len() - 1)].clone();
        let total: u32 = odds.iter().sum::<u32>().max(1);
        text::bold(&format!("Level {}", g.luck + 1), x, y + 10.0, 15.0, TEXT);
        let mut bx = x + 70.0;
        let bar_w = inner - 70.0;
        for (t, &o) in odds.iter().enumerate() {
            let w = bar_w * o as f32 / total as f32;
            if w > 0.0 {
                draw_rectangle(bx, y + 1.0, w, 10.0, art::tier_rim(t));
                draw_rectangle(bx, y + 1.0, w, 4.0, with_alpha(WHITE, 0.2));
            }
            bx += w;
        }
        y += 14.0;
        let odds_text: Vec<String> =
            odds.iter().enumerate().filter(|(_, o)| **o > 0).map(|(t, o)| format!("{} {}%", g.bal.tier_names[t], o * 100 / total)).collect();
        y = text::wrapped(&odds_text.join("   "), x, y, inner, 13.0, DIM) + 2.0;
        let label = match g.luck_cost() {
            Some(c) => format!("Upgrade quality ({c} gold)  [U]"),
            None => "Quality maxed".into(),
        };
        let can = g.luck_cost().is_some_and(|c| g.gold >= c) && !matches!(g.phase, Phase::GameOver | Phase::Victory);
        if self.button(Rect::new(x, y, inner, 30.0), &label, can) {
            g.buy_luck();
        }
        y += 36.0;

        // Next wave
        if !matches!(g.phase, Phase::GameOver | Phase::Victory) {
            let wi = g.wave_info(g.wave);
            section(x, &mut y, inner, if g.phase == Phase::Wave { "Current Wave" } else { "Next Wave" });
            let d = &g.bal.difficulties[g.diff];
            let tag = if wi.boss {
                if d.boss_lives >= g.lives { "BOSS: escape = defeat".to_string() } else { format!("BOSS: costs {} lives", d.boss_lives) }
            } else if wi.air {
                "FLYING: ignores the maze".to_string()
            } else {
                String::new()
            };
            let head = format!("{} x{}", wi.name, wi.count);
            text::bold(&head, x, y + 12.0, 15.0, TEXT);
            text::text(&tag, x + text::measure(&head, 15.0, Style::Bold).width + 10.0, y + 12.0, 13.0, if wi.boss { BAD } else { INFO });
            y += 18.0;
            text::text(&format!("HP {}    Armor {}    Speed {:.1}", wi.hp, wi.armor, wi.speed), x, y + 10.0, 13.0, DIM);
            y += 10.0;
            if !wi.desc.is_empty() {
                text::wrapped(&wi.desc, x, y, inner, 13.0, if wi.boss { BAD } else { INFO });
            }
        }

        // Bottom: recipe book + speed controls
        let by = screen_height() - 104.0;
        let book_label = if self.book { "Close Recipe Book  [B]" } else { "Recipe Book  [B]" };
        if self.button_tinted(Rect::new(x, by, inner, 30.0), book_label, true, RECIPE) {
            self.book = !self.book;
        }
        let by = by + 38.0;
        let bw = (inner - 3.0 * 8.0) / 4.0;
        for (k, (label, sp)) in [("1x", 1u32), ("2x", 2), ("4x", 4)].iter().enumerate() {
            let r = Rect::new(x + k as f32 * (bw + 8.0), by, bw, 28.0);
            if self.speed == *sp {
                draw_rectangle(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, with_alpha(GOLD, 0.35));
            }
            if self.button(r, label, true) {
                self.speed = *sp;
            }
        }
        let r = Rect::new(x + 3.0 * (bw + 8.0), by, bw, 28.0);
        if self.button(r, if self.paused { "Play" } else { "Pause" }, true) {
            self.paused = !self.paused;
        }
        text::text("1/2/3 speed   Space pause   M/N music/sound", x, screen_height() - 18.0, 12.0, DIM);
    }

    /// Info and actions for the selected cell. Returns the new y.
    fn draw_selected(&mut self, g: &mut Game, sel: usize, x: f32, mut y: f32, inner: f32) -> f32 {
        if g.is_rock(sel) {
            section(x, &mut y, inner, "Rock");
            y = text::wrapped("Rocks are walls. Remove one to reshape your maze (between waves only).", x, y - 6.0, inner, 13.0, DIM);
            let label = format!("Remove rock ({} gold)  [X]", g.bal.rock_removal_cost);
            if self.button(Rect::new(x, y, inner, 30.0), &label, g.can_remove_rock(sel)) {
                g.remove_rock(sel);
            }
            return y + 38.0;
        }
        let Some(t) = g.tower(sel).cloned() else { return y };
        let def = g.def(t.kind).clone();
        let col = g.color(t.kind);
        let time = get_time() as f32;
        section(x, &mut y, inner, if matches!(t.kind, Kind::Special(_)) { "Special Tower" } else { "Selected" });
        tower_art(g, &t, vec2(x + 16.0, y + 8.0), 34.0, time, sel);
        text::bold(&g.tower_name(&t), x + 40.0, y + 6.0, 17.0, mix(col, WHITE, 0.25));
        let s = g.stats(&t);
        let bonus = g.aura_bonus(sel);
        let dmg = if bonus > 0.0 { format!("Dmg {:.0} (+{:.0}%)", s.damage, bonus * 100.0) } else { format!("Dmg {:.0}", s.damage) };
        text::text(&format!("{dmg}    Range {:.1}    {:.1}s", s.range, s.cooldown), x + 40.0, y + 24.0, 13.0, TEXT);
        y += 30.0;
        y = text::wrapped(&effects_text(&s), x, y, inner, 13.0, INFO);
        y = text::wrapped(&def.detail, x, y - 2.0, inner, 13.0, DIM);
        if !t.fresh {
            text::text(&format!("Damage dealt {:.0}    Kills {}", t.damage_dealt, t.kills), x, y + 10.0, 13.0, DIM);
            y += 16.0;
        }
        y += 6.0;

        if let Kind::Special(_) = t.kind {
            match g.special_upgrade_cost(sel) {
                Some(cost) => {
                    let label = format!("Upgrade to Lv {} ({cost} gold)  [L]", t.rank + 2);
                    let can = g.gold >= cost && !matches!(g.phase, Phase::GameOver | Phase::Victory);
                    if self.button(Rect::new(x, y, inner, 30.0), &label, can) {
                        g.upgrade_special(sel);
                    }
                    y += 36.0;
                }
                None => {
                    text::bold("Max level", x, y + 10.0, 14.0, GOLD);
                    y += 18.0;
                }
            }
        }

        for (r, ings) in g.recipes_for(sel) {
            let name = g.bal.specials[r].name.clone();
            let label = if ings.is_some() { format!("Forge {name}!") } else { format!("Part of: {name}") };
            if self.button_tinted(Rect::new(x, y, inner, 28.0), &label, ings.is_some(), RECIPE) {
                g.forge(r, sel);
            }
            y += 34.0;
        }
        y + 4.0
    }

    fn draw_book(&self, g: &Game, lay: &Layout) {
        let b = lay.board(g);
        draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.04, 0.035, 0.03, 0.94));
        let x = b.x + 26.0;
        let mut y = b.y + 44.0;
        text::heading("Recipe Book", x, y, 30.0, GOLD);
        text::text("Ingredients you own are green. Forge between waves.", x + 230.0, y - 4.0, 14.0, DIM);
        y += 16.0;
        let time = get_time() as f32;
        let row_h = ((b.h - 70.0) / g.bal.specials.len() as f32).min(104.0);
        for (r, sp) in g.bal.specials.iter().enumerate() {
            let col = rgb(sp.color);
            let probe = Tower::new(Kind::Special(r), 0, false);
            draw_line(x, y + 4.0, b.x + b.w - 26.0, y + 4.0, 1.0, with_alpha(GOLD, 0.15));
            tower_art(g, &probe, vec2(x + 22.0, y + 38.0), 44.0, time, r);
            text::heading(&sp.name, x + 54.0, y + 30.0, 20.0, mix(col, WHITE, 0.2));
            let mut ix = x + 230.0;
            for &(gem, tier) in &g.bal.recipes[r] {
                let own = g.owns(gem, tier);
                let gprobe = Tower::new(Kind::Gem(gem), tier, false);
                tower_art(g, &gprobe, vec2(ix + 12.0, y + 24.0), 26.0, time, gem);
                let label = format!("{} {}", g.bal.tier_names[tier], g.bal.gems[gem].name);
                text::text(&label, ix + 30.0, y + 29.0, 14.0, if own { GOOD } else { DIM });
                ix += text::width(&label, 14.0) + 52.0;
            }
            let s = g.stats(&probe);
            text::text(&format!("Dmg {:.0}, Range {:.1}, {:.1}s.  {}", s.damage, s.range, s.cooldown, effects_text(&s)), x + 54.0, y + 54.0, 13.0, TEXT);
            text::text(&sp.detail, x + 54.0, y + 72.0, 13.0, DIM);
            y += row_h;
        }
    }

    fn draw_overlay(&self, g: &Game, lay: &Layout) {
        let b = lay.board(g);
        let center_x = b.x + b.w / 2.0;
        if self.vignette > 0.0 {
            let a = self.vignette * 0.5;
            for k in 0..6 {
                let inset = k as f32 * 5.0;
                let col = Color::new(1.0, 0.1, 0.1, a * (1.0 - k as f32 / 6.0));
                draw_rectangle_lines(b.x + inset, b.y + inset, b.w - inset * 2.0, b.h - inset * 2.0, 5.0, col);
            }
        }
        if let Some((t, col, time)) = &self.banner {
            let a = (time / 0.3).min(1.0) * ((2.4 - time) / 0.6).clamp(0.0, 1.0);
            let y = b.y + b.h * 0.3 - (1.0 - (time / 0.3).min(1.0)) * 20.0;
            let d = text::measure(t, 40.0, Style::Heading);
            let w = d.width + 80.0;
            draw_rectangle(center_x - w / 2.0, y - 34.0, w, 64.0, Color::new(0.0, 0.0, 0.0, 0.6 * a));
            draw_line(center_x - w / 2.0, y - 34.0, center_x + w / 2.0, y - 34.0, 1.5, with_alpha(GOLD, a * 0.7));
            draw_line(center_x - w / 2.0, y + 30.0, center_x + w / 2.0, y + 30.0, 1.5, with_alpha(GOLD, a * 0.7));
            text::title(t, center_x, y - 2.0, 40.0, with_alpha(*col, a));
        }
        if let Some((msg, t)) = &g.message {
            let a = t.min(1.0);
            let d = text::measure(msg, 17.0, Style::Bold);
            let y = b.y + b.h - 40.0;
            draw_rectangle(center_x - d.width / 2.0 - 16.0, y - 16.0, d.width + 32.0, 32.0, Color::new(0.0, 0.0, 0.0, 0.7 * a));
            text::centered(msg, center_x, y, 17.0, with_alpha(TEXT, a), Style::Bold);
        }
        if self.paused {
            text::title("Paused", center_x, b.y + 90.0, 44.0, with_alpha(TEXT, 0.85));
        }
        if matches!(g.phase, Phase::GameOver | Phase::Victory) {
            let cy = b.y + b.h / 2.0;
            draw_rectangle(b.x, cy - 90.0, b.w, 185.0, Color::new(0.0, 0.0, 0.0, 0.8));
            draw_line(b.x, cy - 90.0, b.x + b.w, cy - 90.0, 1.5, with_alpha(GOLD, 0.6));
            draw_line(b.x, cy + 95.0, b.x + b.w, cy + 95.0, 1.5, with_alpha(GOLD, 0.6));
            let (title, col) = match (g.phase, g.endless) {
                (Phase::Victory, _) => ("Victory", GOLD),
                (_, true) => ("Endless Run Over", INFO),
                _ => ("Defeat", BAD),
            };
            text::title(title, center_x, cy - 35.0, 60.0, col);
            let (reached, lives) = if g.phase == Phase::Victory {
                (format!("Cleared all {} waves", g.bal.wave_count()), format!("  -  {} lives left", g.lives))
            } else {
                (format!("Reached wave {}", g.wave + 1), String::new())
            };
            let diff = &g.bal.difficulties[g.diff].name;
            text::centered(&format!("{}, {diff}: {reached}  -  {} kills{lives}", g.map.name, g.total_kills), center_x, cy + 20.0, 17.0, TEXT, Style::Body);
            let keys = if g.phase == Phase::Victory {
                "Press E to continue in Endless mode, or R for the title screen"
            } else {
                "Press R for the title screen"
            };
            text::centered(keys, center_x, cy + 55.0, 15.0, DIM, Style::Body);
            if self.new_best {
                let pulse = 0.7 + 0.3 * (get_time() as f32 * 4.0).sin();
                text::title("New Personal Best!", center_x, cy - 120.0, 30.0, with_alpha(GOLD, pulse));
            }
        }
    }
}
