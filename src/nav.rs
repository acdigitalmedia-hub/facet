//! Controller focus for the egui menus. Buttons register themselves as they're drawn (via
//! [`NavExt::nav_clicked`] and friends); the controller moves a highlight between them by
//! direction and presses the focused one.
//!
//! Each part of the interface sets a [`Group`] before drawing. Only one group is navigable at
//! a time: an open modal (pause, settings, help, recipe book, summary, confirmation) wins over
//! the title screen. Everything else (the side panel, tips) is left to the controller's direct
//! buttons.

use bevy_egui::egui::{self, Color32, CornerRadius, Id, LayerId, Order, Rect, Response, Stroke, StrokeKind, Vec2};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Group {
    /// Not reachable by controller focus.
    #[default]
    None,
    Title,
    Modal,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// What the controller asks of the menus this frame.
#[derive(Clone, Copy, Default, Debug)]
pub struct NavInput {
    pub dir: Option<Dir>,
    pub press: bool,
    /// Scroll for the open scroll area (points, positive shows earlier content).
    pub scroll: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Button,
    Slider,
}

#[derive(Clone, Copy)]
struct Item {
    id: Id,
    rect: Rect,
    group: Group,
    kind: Kind,
}

#[derive(Clone, Default)]
struct Nav {
    on: bool,
    group: Group,
    items: Vec<Item>,
    focus: Option<Id>,
    /// The focus moved this frame: scroll it into view.
    moved: bool,
    press: bool,
    /// Left/right on a focused slider.
    adjust: f32,
    scroll: f32,
}

fn with<R>(ctx: &egui::Context, f: impl FnOnce(&mut Nav) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<Nav>(Id::new("controller_nav"))))
}

/// Starts a frame: applies the controller's input to last frame's items.
pub fn begin(ctx: &egui::Context, input: NavInput, on: bool) {
    with(ctx, |n| {
        let prev = std::mem::take(&mut n.items);
        n.group = Group::None;
        n.on = on;
        n.press = false;
        n.adjust = 0.0;
        n.moved = false;
        n.scroll = input.scroll;
        if !on {
            n.focus = None;
            return;
        }
        let top = if prev.iter().any(|i| i.group == Group::Modal) { Group::Modal } else { Group::Title };
        let cands: Vec<Item> = prev.into_iter().filter(|i| i.group == top).collect();
        let Some(first) = cands.first().copied() else {
            n.focus = None;
            if let Some(d) = input.dir {
                n.scroll += step_scroll(d);
            }
            return;
        };
        let cur = match n.focus.and_then(|f| cands.iter().find(|i| i.id == f)) {
            Some(c) => *c,
            None => {
                n.focus = Some(first.id);
                n.moved = true;
                first
            }
        };
        if let Some(d) = input.dir {
            if cur.kind == Kind::Slider && matches!(d, Dir::Left | Dir::Right) {
                n.adjust = if d == Dir::Right { 1.0 } else { -1.0 };
            } else if let Some(next) = nearest(cur.rect, d, &cands) {
                n.focus = Some(next);
                n.moved = true;
            } else {
                n.scroll += step_scroll(d);
            }
        }
        n.press = input.press;
    });
}

fn step_scroll(d: Dir) -> f32 {
    match d {
        Dir::Up => 140.0,
        Dir::Down => -140.0,
        _ => 0.0,
    }
}

/// The closest item in a direction, preferring ones lined up with the current one.
fn nearest(from: Rect, d: Dir, cands: &[Item]) -> Option<Id> {
    let c = from.center();
    let mut best: Option<(f32, Id)> = None;
    for it in cands {
        if it.rect == from {
            continue;
        }
        let o = it.rect.center();
        // Use the gap between the rectangles along the axis, so tall and wide items work.
        let (along, across) = match d {
            Dir::Right => (it.rect.min.x - from.max.x, o.y - c.y),
            Dir::Left => (from.min.x - it.rect.max.x, o.y - c.y),
            Dir::Down => (it.rect.min.y - from.max.y, o.x - c.x),
            Dir::Up => (from.min.y - it.rect.max.y, o.x - c.x),
        };
        if along < -2.0 {
            continue;
        }
        // Must be more in this direction than sideways (measured centre to centre).
        let (dc_along, dc_across) = match d {
            Dir::Right => (o.x - c.x, o.y - c.y),
            Dir::Left => (c.x - o.x, o.y - c.y),
            Dir::Down => (o.y - c.y, o.x - c.x),
            Dir::Up => (c.y - o.y, o.x - c.x),
        };
        if dc_along <= 0.0 || dc_across.abs() > dc_along * 2.5 + 40.0 {
            continue;
        }
        let score = along.max(0.0) + across.abs() * 2.0;
        if best.is_none_or(|b| score < b.0) {
            best = Some((score, it.id));
        }
    }
    best.map(|b| b.1)
}

/// Sets the group for the widgets drawn next.
pub fn group(ctx: &egui::Context, g: Group) {
    with(ctx, |n| n.group = g);
}

/// Registers a widget; returns (focused, pressed by the controller, left/right adjust).
fn register(r: &Response, kind: Kind) -> (bool, bool, f32) {
    let (focused, hit, adjust, moved, on) = with(&r.ctx, |n| {
        if n.group == Group::None || !r.enabled() {
            return (false, false, 0.0, false, n.on);
        }
        n.items.push(Item { id: r.id, rect: r.rect, group: n.group, kind });
        let f = n.on && n.focus == Some(r.id);
        let hit = f && std::mem::take(&mut n.press);
        let adj = if f { std::mem::take(&mut n.adjust) } else { 0.0 };
        (f, hit, adj, n.moved, n.on)
    });
    if focused && on {
        if moved {
            r.scroll_to_me(None);
        }
        let p = r.ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("nav_focus")));
        let t = r.ctx.input(|i| i.time) as f32;
        let a = 0.75 + 0.25 * (t * 5.0).sin();
        let col = Color32::from_rgba_unmultiplied(255, 255, 255, (a * 255.0) as u8);
        p.rect_stroke(r.rect.expand(4.0), CornerRadius::same(6), Stroke::new(3.0, col), StrokeKind::Outside);
        p.rect_stroke(r.rect.expand(7.0), CornerRadius::same(8), Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 220, 120, (a * 140.0) as u8)), StrokeKind::Outside);
        r.ctx.request_repaint();
    }
    (focused, hit, adjust)
}

pub trait NavExt {
    /// `clicked()`, or pressed with the controller while focused.
    fn nav_clicked(&self) -> bool;
}

impl NavExt for Response {
    fn nav_clicked(&self) -> bool {
        let (_, hit, _) = register(self, Kind::Button);
        hit || self.clicked()
    }
}

/// A checkbox: the controller toggles it when pressed. Returns true if it changed.
pub fn toggle(r: &Response, v: &mut bool) -> bool {
    let (_, hit, _) = register(r, Kind::Button);
    if hit {
        *v = !*v;
    }
    hit || r.changed()
}

/// A 0..1 slider: left/right adjust it while focused.
pub fn slider(r: &Response, v: &mut f32) {
    let (_, _, adj) = register(r, Kind::Slider);
    if adj != 0.0 {
        *v = ((*v + adj * 0.1) * 10.0).round().clamp(0.0, 10.0) / 10.0;
    }
}

/// Inside a scroll area: applies the controller's scrolling.
pub fn scroll(ui: &mut egui::Ui) {
    let s = with(ui.ctx(), |n| std::mem::take(&mut n.scroll));
    if s != 0.0 {
        ui.scroll_with_delta(Vec2::new(0.0, s));
    }
}
