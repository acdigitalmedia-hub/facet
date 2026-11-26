//! Controller support (Steam Deck, Xbox, PlayStation, …).
//!
//! While playing, the left stick moves a cursor over the board cell by cell and the face
//! buttons act on it; the right stick and triggers drive the camera. With a menu open (or on
//! the title screen) the stick / D-pad move a focus highlight between buttons instead (see
//! `nav`). Using the mouse switches the hints back to keyboard and mouse.
//!
//! | Button        | Playing                                   | Menus             |
//! |---------------|-------------------------------------------|-------------------|
//! | Left stick    | move the cursor                           | move focus        |
//! | A             | place / select                            | press             |
//! | B             | deselect, dismiss a tip                   | back              |
//! | X             | keep · remove rock · upgrade special      |                   |
//! | Y             | forge (if ready) · combine                |                   |
//! | LB / RB       | previous / next tower (new gems first)    | map (title)       |
//! | D-pad ↑ / ↓   | gem quality / top-down view               | move focus        |
//! | D-pad ← / →   | slower / faster                           | move focus        |
//! | Right stick   | rotate and tilt the camera                | scroll            |
//! | LT / RT       | zoom out / in                             |                   |
//! | L3 / R3       | reset view / pause                        |                   |
//! | Start         | menu                                      | close menu        |
//! | Select        | recipe book                               | close book        |

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::input::gamepad::{Gamepad, GamepadButton as B};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use facet_core::game::{Occ, Phase};
use facet_core::path::Cell;

use crate::audio::Audio;
use crate::menus::Page;
use crate::nav::{Dir, NavInput};
use crate::ui::UiState;
use crate::{view_preset, CamRig, Screen, Sim};

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// The controller was used more recently than the mouse: show controller hints.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Debug: show controller hints and the cursor without a controller (`FACET3D_PAD=1`).
pub fn force_active() {
    ACTIVE.store(true, Ordering::Relaxed);
}

/// A key hint for a label: `[K]` with keyboard and mouse, `(X)` with a controller.
pub fn hint(key: &str, pad: &str) -> String {
    if active() { format!("({pad})") } else { format!("[{key}]") }
}

/// Repeats a held direction: once at once, then steadily after a short delay.
#[derive(Default)]
struct Repeat {
    held: Option<IVec2>,
    timer: f32,
}

impl Repeat {
    fn step(&mut self, v: Option<IVec2>, dt: f32, rate: f32) -> Option<IVec2> {
        match v {
            None => {
                self.held = None;
                None
            }
            Some(v) if self.held != Some(v) => {
                self.held = Some(v);
                self.timer = 0.3;
                Some(v)
            }
            Some(v) => {
                self.timer -= dt;
                if self.timer <= 0.0 {
                    self.timer += rate;
                    Some(v)
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Resource, Default)]
pub struct Pad {
    /// The board cell under the controller cursor.
    pub cursor: Option<Cell>,
    /// Menu input for the interface this frame.
    pub nav: NavInput,
    /// Dismiss the current tip (B with nothing selected).
    pub dismiss_tip: bool,
    stick: Repeat,
    dpad: Repeat,
}

/// A direction from the left stick, or None inside the dead zone. Also returns how far it's pushed.
fn stick_dir(v: Vec2) -> Option<(IVec2, f32)> {
    let len = v.length();
    if len < 0.45 {
        return None;
    }
    let d = if v.x.abs() > v.y.abs() { IVec2::new(v.x.signum() as i32, 0) } else { IVec2::new(0, v.y.signum() as i32) };
    Some((d, len.min(1.0)))
}

fn dpad_dir(g: &Gamepad) -> Option<IVec2> {
    let mut d = IVec2::ZERO;
    if g.pressed(B::DPadUp) {
        d.y += 1;
    }
    if g.pressed(B::DPadDown) {
        d.y -= 1;
    }
    if g.pressed(B::DPadRight) {
        d.x += 1;
    }
    if g.pressed(B::DPadLeft) {
        d.x -= 1;
    }
    (d != IVec2::ZERO).then(|| if d.x != 0 && d.y != 0 { IVec2::new(0, d.y) } else { d })
}

fn to_dir(v: IVec2) -> Dir {
    match (v.x, v.y) {
        (1, _) => Dir::Right,
        (-1, _) => Dir::Left,
        (_, 1) => Dir::Up,
        _ => Dir::Down,
    }
}

const BUTTONS: [B; 17] = [
    B::South,
    B::East,
    B::North,
    B::West,
    B::LeftTrigger,
    B::RightTrigger,
    B::LeftTrigger2,
    B::RightTrigger2,
    B::Select,
    B::Start,
    B::LeftThumb,
    B::RightThumb,
    B::DPadUp,
    B::DPadDown,
    B::DPadLeft,
    B::DPadRight,
    B::Mode,
];

/// Whether the board is the focus (no menu, book, confirmation or end-of-run summary).
pub fn board_mode(sim: &Sim, st: &UiState) -> bool {
    sim.screen == Screen::Playing
        && st.page.is_none()
        && !st.book
        && st.confirm_rock.is_none()
        && !matches!(sim.game.phase, Phase::GameOver | Phase::Victory)
}

#[allow(clippy::too_many_arguments)]
pub fn gamepad(
    pads: Query<&Gamepad>,
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut sim: ResMut<Sim>,
    mut st: ResMut<UiState>,
    mut rig: ResMut<CamRig>,
    mut pad: ResMut<Pad>,
    mut audio: ResMut<Audio>,
) {
    pad.nav = NavInput::default();
    // The mouse takes over again as soon as it moves or clicks.
    if motion.delta.length() > 2.0 || mouse.get_just_pressed().next().is_some() {
        ACTIVE.store(false, Ordering::Relaxed);
    }
    let dt = time.delta_secs().min(0.1);
    // Combine every connected controller.
    let pressed = |b: B| pads.iter().any(|g| g.just_pressed(b));
    let left = pads.iter().map(|g| g.left_stick()).fold(Vec2::ZERO, |a, v| if v.length() > a.length() { v } else { a });
    let right = pads.iter().map(|g| g.right_stick()).fold(Vec2::ZERO, |a, v| if v.length() > a.length() { v } else { a });
    let trig = |b: B| pads.iter().filter_map(|g| g.get(b)).fold(0.0f32, f32::max);
    let dpad = pads.iter().find_map(dpad_dir);
    let used = pads.iter().any(|g| BUTTONS.iter().any(|&b| g.just_pressed(b))) || left.length() > 0.45 || right.length() > 0.3;
    if used {
        ACTIVE.store(true, Ordering::Relaxed);
    }
    if !active() || sim.screen == Screen::Editor || pads.is_empty() {
        pad.stick = Repeat::default();
        pad.dpad = Repeat::default();
        return;
    }

    if !board_mode(&sim, &st) {
        menus(&mut sim, &mut st, &mut pad, pressed, left, right, dpad, dt);
        return;
    }

    // ---- Playing: the board cursor and actions.
    let g = &mut sim.game;
    let (w, h) = (g.map.w, g.map.h);
    let cur = *pad.cursor.get_or_insert((rig.focus.x.floor() as i32, rig.focus.z.floor() as i32));
    let mut cur = (cur.0.clamp(0, w - 1), cur.1.clamp(0, h - 1));

    // Left stick: step the cursor, relative to the camera (up = away from the camera).
    let (sd, push) = stick_dir(left).map_or((None, 0.0), |(d, p)| (Some(d), p));
    let rate = 0.16 - 0.09 * ((push - 0.45) / 0.55).clamp(0.0, 1.0);
    if let Some(v) = pad.stick.step(sd, dt, rate) {
        let fwd = Vec2::new(-rig.yaw.sin(), -rig.yaw.cos());
        let rgt = Vec2::new(rig.yaw.cos(), -rig.yaw.sin());
        let world = rgt * v.x as f32 + fwd * v.y as f32;
        let step = if world.x.abs() > world.y.abs() { (world.x.signum() as i32, 0) } else { (0, world.y.signum() as i32) };
        cur = ((cur.0 + step.0).clamp(0, w - 1), (cur.1 + step.1).clamp(0, h - 1));
    }

    // D-pad: quality, view, speed.
    if let Some(v) = pad.dpad.step(dpad, dt, 0.35) {
        match (v.x, v.y) {
            (0, 1) => {
                g.buy_luck();
            }
            (0, -1) => {
                rig.top = !rig.top;
                rig.target = Some(view_preset(w as f32, h as f32, rig.top));
            }
            (-1, 0) => st.speed = match st.speed { 4 => 2, _ => 1 },
            (1, 0) => st.speed = match st.speed { 1 => 2, _ => 4 },
            _ => {}
        }
    }

    // LB / RB: jump between towers (this round's new gems while choosing).
    let cycle = if pressed(B::RightTrigger) { 1 } else if pressed(B::LeftTrigger) { -1 } else { 0 };
    if cycle != 0 {
        let list: Vec<usize> = if g.phase == Phase::Choose {
            g.placed.iter().copied().filter(|&i| g.tower(i).is_some()).collect()
        } else {
            (0..g.grid.len()).filter(|&i| matches!(g.grid[i], Occ::Tower(_))).collect()
        };
        if !list.is_empty() {
            let at = g.selected.and_then(|s| list.iter().position(|&i| i == s));
            let n = list.len() as i32;
            let k = match at {
                Some(a) => (a as i32 + cycle).rem_euclid(n),
                None if cycle > 0 => 0,
                None => n - 1,
            } as usize;
            let i = list[k];
            g.selected = Some(i);
            cur = (i as i32 % w, i as i32 / w);
            audio.click();
        }
    }

    if pressed(B::South) {
        crate::click_cell(g, cur, &mut audio);
    }
    if pressed(B::East) {
        if g.selected.is_some() {
            g.selected = None;
        } else {
            pad.dismiss_tip = true;
        }
    }
    let ci = g.idx(cur);
    let cursor_fresh = g.tower(ci).is_some_and(|t| t.fresh);
    if pressed(B::West) {
        if g.phase == Phase::Choose {
            if cursor_fresh {
                g.selected = Some(ci);
            }
            g.keep(false);
        } else if let Some(sel) = g.selected {
            if g.is_rock(sel) {
                st.confirm_rock = Some(sel);
            } else {
                g.upgrade_special(sel);
            }
        }
    }
    if pressed(B::North) {
        let forge = g.selected.and_then(|sel| g.recipes_for(sel).into_iter().find(|(_, ings)| ings.is_some()).map(|(r, _)| (r, sel)));
        if let Some((r, sel)) = forge {
            g.forge(r, sel);
        } else if g.phase == Phase::Choose {
            if cursor_fresh {
                g.selected = Some(ci);
            }
            g.keep(true);
        }
    }
    if pressed(B::Start) {
        st.page = Some(Page::Pause);
    }
    if pressed(B::Select) {
        st.book = true;
    }
    if pressed(B::RightThumb) {
        st.paused = !st.paused;
    }
    if pressed(B::LeftThumb) {
        rig.top = false;
        rig.target = Some(view_preset(w as f32, h as f32, false));
    }
    pad.cursor = Some(cur);

    // Camera: right stick rotates and tilts, triggers zoom; the view follows the cursor.
    if right.length() > 0.15 {
        rig.yaw -= right.x * 2.2 * dt;
        rig.pitch = (rig.pitch - right.y * 1.2 * dt).clamp(0.35, 1.53);
        rig.target = None;
    }
    let zoom = trig(B::LeftTrigger2) - trig(B::RightTrigger2);
    if zoom.abs() > 0.05 {
        rig.dist = (rig.dist * (1.0 + zoom * 1.4 * dt)).clamp(6.0, 90.0);
        rig.target = None;
    }
    let target = Vec3::new(cur.0 as f32 + 0.5, 0.0, cur.1 as f32 + 0.5);
    let off = Vec3::new(target.x - rig.focus.x, 0.0, target.z - rig.focus.z);
    let slack = rig.dist * 0.22;
    // (Not while easing to a preset view: that would cancel it.)
    if rig.target.is_none() && off.length() > slack {
        let pull = off.normalize() * (off.length() - slack) * (1.0 - (-dt * 6.0).exp());
        rig.focus += pull;
        rig.target = None;
    }
}

/// Menus and the title screen: focus moves, press, back.
#[allow(clippy::too_many_arguments)]
fn menus(sim: &mut Sim, st: &mut UiState, pad: &mut Pad, pressed: impl Fn(B) -> bool, left: Vec2, right: Vec2, dpad: Option<IVec2>, dt: f32) {
    let stick = stick_dir(left).map(|(d, _)| d);
    let dir = pad.stick.step(stick, dt, 0.12).or(pad.dpad.step(dpad, dt, 0.12));
    pad.nav.dir = dir.map(to_dir);
    pad.nav.press = pressed(B::South);
    if right.y.abs() > 0.2 {
        pad.nav.scroll = right.y * 900.0 * dt;
    }
    let playing = sim.screen == Screen::Playing;
    if pressed(B::East) {
        if !playing {
            st.page = None;
        } else if st.confirm_rock.is_some() {
            st.confirm_rock = None;
        } else if st.book {
            st.book = false;
        } else if let Some(p) = st.page {
            st.page = if p == Page::Pause { None } else { Some(Page::Pause) };
        }
    }
    if playing && pressed(B::Start) {
        st.page = match st.page {
            None if !matches!(sim.game.phase, Phase::GameOver | Phase::Victory) => Some(Page::Pause),
            _ => None,
        };
        st.book = false;
    }
    if playing && pressed(B::Select) && st.page.is_none() {
        st.book = !st.book;
    }
    if !playing && st.page.is_none() {
        let n = sim.maps.len();
        if pressed(B::RightTrigger) {
            sim.select_map((sim.map + 1) % n);
        }
        if pressed(B::LeftTrigger) {
            sim.select_map((sim.map + n - 1) % n);
        }
    }
}

/// `FACET3D_PADLOG=1`: prints each controller and every button it reports, for checking mappings.
pub fn pad_log(pads: Query<(Entity, &Gamepad, Option<&Name>), Added<Gamepad>>, all: Query<(Entity, &Gamepad)>) {
    if std::env::var_os("FACET3D_PADLOG").is_none() {
        return;
    }
    for (e, g, n) in &pads {
        eprintln!("PADLOG connected {e:?} name={:?} vendor={:?} product={:?}", n.map(|n| n.as_str()), g.vendor_id(), g.product_id());
    }
    for (e, g) in &all {
        for b in g.get_just_pressed() {
            eprintln!("PADLOG {e:?} pressed {b:?}");
        }
    }
}

/// Controllers missing from the bundled SDL database. Without an entry the X and Y buttons come
/// through swapped (the Linux driver reports X as BTN_X, which the fallback reads as "north").
/// Same layout as the Xbox 360 pad, since they use the same `xpad` driver.
const EXTRA_MAPPINGS: &[&str] = &[
    "03000000c82d00000b31000014010000,8BitDo Ultimate 2 Wireless,a:b0,b:b1,back:b6,dpdown:h0.4,dpleft:h0.8,dpright:h0.2,dpup:h0.1,guide:b8,leftshoulder:b4,leftstick:b9,lefttrigger:a2,leftx:a0,lefty:a1,rightshoulder:b5,rightstick:b10,righttrigger:a5,rightx:a3,righty:a4,start:b7,x:b2,y:b3,platform:Linux,",
];

/// Adds [`EXTRA_MAPPINGS`] to `SDL_GAMECONTROLLERCONFIG` (keeping any Steam set), which gilrs
/// reads when it starts. Call first thing in `main`, before any threads exist.
pub fn add_mappings() {
    let mut cfg = std::env::var("SDL_GAMECONTROLLERCONFIG").unwrap_or_default();
    for m in EXTRA_MAPPINGS {
        let guid = &m[..32];
        if !cfg.contains(guid) {
            if !cfg.is_empty() && !cfg.ends_with('\n') {
                cfg.push('\n');
            }
            cfg.push_str(m);
            cfg.push('\n');
        }
    }
    // SAFETY: called at the very start of main, while the program is still single-threaded.
    unsafe { std::env::set_var("SDL_GAMECONTROLLERCONFIG", cfg) };
}
