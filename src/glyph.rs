//! Button prompts drawn as little pictures of the real thing: keyboard keys as keycaps,
//! controller buttons as coloured face buttons, bumpers, triggers, sticks and a D-pad.
//!
//! In text, `{K|A}` is a prompt: keyboard key `K`, or controller button `A` when the
//! controller is in use. Either side may be empty (`{Tab|}` shows nothing with a controller,
//! `{|LB}` nothing with the keyboard). `{k:Tab}` and `{p:Y}` always show that key or button.
//! Controller names: A B X Y LB RB LT RT Start Select
//! Up Down Left Right LeftRight L3 R3 LS RS.

use bevy_egui::egui::{self, epaint::PathShape, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};

use crate::pad;
use crate::ui::{alpha, bold, DIM, TEXT};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Btn {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    Start,
    Select,
    Up,
    Down,
    Left,
    Right,
    LeftRight,
    L3,
    R3,
    LS,
    RS,
}

impl Btn {
    pub fn parse(s: &str) -> Option<Btn> {
        Some(match s {
            "A" => Btn::A,
            "B" => Btn::B,
            "X" => Btn::X,
            "Y" => Btn::Y,
            "LB" => Btn::LB,
            "RB" => Btn::RB,
            "LT" => Btn::LT,
            "RT" => Btn::RT,
            "Start" => Btn::Start,
            "Select" => Btn::Select,
            "Up" => Btn::Up,
            "Down" => Btn::Down,
            "Left" => Btn::Left,
            "Right" => Btn::Right,
            "LeftRight" => Btn::LeftRight,
            "L3" => Btn::L3,
            "R3" => Btn::R3,
            "LS" => Btn::LS,
            "RS" => Btn::RS,
            _ => return None,
        })
    }
}

/// A keyboard key (its label) or a controller button.
#[derive(Clone, PartialEq, Debug)]
pub enum Glyph {
    Key(String),
    Pad(Btn),
}

/// The prompt for whichever input is in use; `None` if that side is empty.
pub fn pick(key: &str, pad: &str) -> Option<Glyph> {
    if pad::active() {
        Btn::parse(pad).map(Glyph::Pad)
    } else if key.is_empty() {
        None
    } else {
        Some(Glyph::Key(key.to_string()))
    }
}

/// Width of a glyph `h` points tall.
pub fn width(p: &Painter, g: &Glyph, h: f32) -> f32 {
    match g {
        Glyph::Key(k) => key_width(p, k, h),
        Glyph::Pad(b) => match b {
            Btn::LB | Btn::RB => h * 1.55,
            Btn::LT | Btn::RT => h * 1.2,
            Btn::Start | Btn::Select => h * 1.35,
            _ => h,
        },
    }
}

fn key_font(h: f32, label: &str) -> FontId {
    // Long names ("Space", "Enter") a little smaller.
    let k = if label.chars().count() > 2 { 0.5 } else { 0.62 };
    FontId::new(h * k, bold())
}

fn key_width(p: &Painter, k: &str, h: f32) -> f32 {
    let w = p.layout_no_wrap(k.to_string(), key_font(h, k), Color32::WHITE).size().x;
    (w + h * 0.55).max(h)
}

/// Paints a glyph with its left edge at `left` (vertically centred on it). Returns its width.
pub fn paint(p: &Painter, left: Pos2, g: &Glyph, h: f32, faded: bool) -> f32 {
    let w = width(p, g, h);
    let r = Rect::from_min_size(Pos2::new(left.x, left.y - h / 2.0), Vec2::new(w, h));
    let a = if faded { 0.45 } else { 1.0 };
    let f = |c: Color32| alpha(c, a);
    match g {
        Glyph::Key(k) => {
            // A keycap: light top face over a darker base, so it reads as a key.
            let rad = CornerRadius::same((h * 0.18) as u8);
            let base = r.translate(Vec2::new(0.0, 0.0));
            let face = Rect::from_min_max(base.min, Pos2::new(base.max.x, base.max.y - h * 0.14));
            p.rect_filled(base, rad, f(Color32::from_rgb(112, 106, 96)));
            p.rect_filled(face, rad, f(Color32::from_rgb(236, 231, 218)));
            p.rect_stroke(base, rad, Stroke::new(1.0, f(Color32::from_rgb(40, 37, 33))), StrokeKind::Inside);
            p.text(face.center(), Align2::CENTER_CENTER, k, key_font(h, k), f(Color32::from_rgb(28, 26, 23)));
        }
        Glyph::Pad(b) => paint_pad(p, r, *b, h, &f),
    }
    w
}

fn paint_pad(p: &Painter, r: Rect, b: Btn, h: f32, f: &dyn Fn(Color32) -> Color32) {
    let c = r.center();
    let dark = Color32::from_rgb(70, 67, 63);
    let unlit = Color32::from_rgb(138, 133, 124);
    let rim = Color32::from_rgb(20, 19, 18);
    let light = Color32::from_rgb(232, 230, 224);
    match b {
        Btn::A | Btn::B | Btn::X | Btn::Y => {
            let (col, text, letter) = match b {
                Btn::A => (Color32::from_rgb(92, 176, 64), light, "A"),
                Btn::B => (Color32::from_rgb(214, 66, 56), light, "B"),
                Btn::X => (Color32::from_rgb(64, 126, 222), light, "X"),
                _ => (Color32::from_rgb(234, 186, 44), Color32::from_rgb(40, 32, 10), "Y"),
            };
            p.circle_filled(c, h / 2.0, f(rim));
            p.circle_filled(c, h / 2.0 - 1.5, f(col));
            p.text(c + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, letter, FontId::new(h * 0.6, bold()), f(text));
        }
        Btn::LB | Btn::RB => {
            // A bumper: wide, rounded more on the outer top corner.
            let big = (h * 0.45) as u8;
            let small = (h * 0.15) as u8;
            let rad = if b == Btn::LB {
                CornerRadius { nw: big, ne: small, sw: small, se: small }
            } else {
                CornerRadius { nw: small, ne: big, sw: small, se: small }
            };
            let rr = r.shrink2(Vec2::new(0.0, h * 0.1));
            p.rect_filled(rr, rad, f(dark));
            p.rect_stroke(rr, rad, Stroke::new(1.2, f(rim)), StrokeKind::Inside);
            p.text(rr.center(), Align2::CENTER_CENTER, if b == Btn::LB { "LB" } else { "RB" }, FontId::new(h * 0.48, bold()), f(light));
        }
        Btn::LT | Btn::RT => {
            // A trigger: taller, rounded top.
            let rad = CornerRadius { nw: (h * 0.45) as u8, ne: (h * 0.45) as u8, sw: (h * 0.12) as u8, se: (h * 0.12) as u8 };
            p.rect_filled(r, rad, f(dark));
            p.rect_stroke(r, rad, Stroke::new(1.2, f(rim)), StrokeKind::Inside);
            p.text(c + Vec2::new(0.0, h * 0.06), Align2::CENTER_CENTER, if b == Btn::LT { "LT" } else { "RT" }, FontId::new(h * 0.44, bold()), f(light));
        }
        Btn::Start | Btn::Select => {
            let rr = r.shrink2(Vec2::new(0.0, h * 0.14));
            let rad = CornerRadius::same((rr.height() / 2.0) as u8);
            p.rect_filled(rr, rad, f(dark));
            p.rect_stroke(rr, rad, Stroke::new(1.2, f(rim)), StrokeKind::Inside);
            let s = Stroke::new((h * 0.08).max(1.2), f(light));
            if b == Btn::Start {
                // Menu: three lines.
                for k in -1..=1 {
                    let y = c.y + k as f32 * h * 0.15;
                    p.line_segment([Pos2::new(c.x - h * 0.2, y), Pos2::new(c.x + h * 0.2, y)], s);
                }
            } else {
                // View: two overlapping windows.
                let q = h * 0.2;
                let back = Rect::from_center_size(c + Vec2::new(-q * 0.4, -q * 0.35), Vec2::new(q * 1.7, q * 1.3));
                let front = back.translate(Vec2::new(q * 0.8, q * 0.7));
                p.rect_stroke(back, CornerRadius::ZERO, Stroke::new(s.width * 0.8, s.color), StrokeKind::Inside);
                p.rect_filled(front.expand(1.0), CornerRadius::ZERO, f(dark));
                p.rect_filled(front, CornerRadius::ZERO, f(light));
            }
        }
        Btn::Up | Btn::Down | Btn::Left | Btn::Right | Btn::LeftRight => {
            // A D-pad cross with the pressed arm(s) lit.
            let arm = h * 0.34;
            let len = h * 0.5;
            let lit = |d: Btn| d == b || (b == Btn::LeftRight && matches!(d, Btn::Left | Btn::Right));
            let arms = [
                (Btn::Up, Rect::from_min_max(Pos2::new(c.x - arm / 2.0, c.y - len), Pos2::new(c.x + arm / 2.0, c.y))),
                (Btn::Down, Rect::from_min_max(Pos2::new(c.x - arm / 2.0, c.y), Pos2::new(c.x + arm / 2.0, c.y + len))),
                (Btn::Left, Rect::from_min_max(Pos2::new(c.x - len, c.y - arm / 2.0), Pos2::new(c.x, c.y + arm / 2.0))),
                (Btn::Right, Rect::from_min_max(Pos2::new(c.x, c.y - arm / 2.0), Pos2::new(c.x + len, c.y + arm / 2.0))),
            ];
            let rad = CornerRadius::same((h * 0.06) as u8);
            for (_, ar) in &arms {
                p.rect_filled(ar.expand(1.2), rad, f(rim));
            }
            for (d, ar) in &arms {
                p.rect_filled(*ar, rad, f(if lit(*d) { Color32::from_rgb(244, 206, 112) } else { unlit }));
            }
            // Arrow tips on the lit arms.
            for (d, ar) in &arms {
                if !lit(*d) {
                    continue;
                }
                let s = arm * 0.32;
                let (tip, dir) = match d {
                    Btn::Up => (Pos2::new(ar.center().x, ar.min.y + s * 0.9), Vec2::new(0.0, -1.0)),
                    Btn::Down => (Pos2::new(ar.center().x, ar.max.y - s * 0.9), Vec2::new(0.0, 1.0)),
                    Btn::Left => (Pos2::new(ar.min.x + s * 0.9, ar.center().y), Vec2::new(-1.0, 0.0)),
                    _ => (Pos2::new(ar.max.x - s * 0.9, ar.center().y), Vec2::new(1.0, 0.0)),
                };
                let side = Vec2::new(-dir.y, dir.x);
                let pts = vec![tip + dir * s * 0.6, tip - dir * s * 0.5 + side * s, tip - dir * s * 0.5 - side * s];
                p.add(Shape::Path(PathShape::convex_polygon(pts, f(Color32::from_rgb(40, 32, 10)), Stroke::NONE)));
            }
        }
        Btn::L3 | Btn::R3 | Btn::LS | Btn::RS => {
            // A thumbstick seen from above: ring, cap, letter.
            p.circle_filled(c, h / 2.0, f(rim));
            p.circle_filled(c, h / 2.0 - 1.5, f(Color32::from_rgb(78, 76, 72)));
            p.circle_stroke(c, h * 0.3, Stroke::new(1.0, f(Color32::from_rgb(120, 117, 110))));
            let (t, pressed) = match b {
                Btn::L3 => ("L", true),
                Btn::R3 => ("R", true),
                Btn::LS => ("L", false),
                _ => ("R", false),
            };
            p.text(c + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, t, FontId::new(h * 0.46, bold()), f(light));
            if pressed {
                // A little "push" mark under the letter.
                p.circle_filled(c + Vec2::new(0.0, h * 0.33), h * 0.06, f(light));
            }
        }
    }
}

/// A glyph as a widget in a layout.
pub fn show(ui: &mut Ui, g: &Glyph, h: f32) -> Response {
    let w = width(ui.painter(), g, h);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    paint(ui.painter(), Pos2::new(r.min.x, r.center().y), g, h, false);
    resp
}

/// Text split into plain runs and prompts.
enum Piece<'a> {
    Text(&'a str),
    Glyph(Glyph),
}

fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let Some(len) = rest[open..].find('}') else { break };
        if open > 0 {
            out.push(Piece::Text(&rest[..open]));
        }
        let inner = &rest[open + 1..open + len];
        let g = if let Some(k) = inner.strip_prefix("k:") {
            Some(Glyph::Key(k.to_string()))
        } else if let Some(b) = inner.strip_prefix("p:") {
            Btn::parse(b).map(Glyph::Pad)
        } else {
            let (k, pd) = inner.split_once('|').unwrap_or((inner, ""));
            pick(k, pd)
        };
        if let Some(g) = g {
            out.push(Piece::Glyph(g));
        }
        rest = &rest[open + len + 1..];
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

/// Text with prompts (`{K|A}`), wrapping like a label.
pub fn rich(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    let h = size * 1.4;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.spacing_mut().interact_size.y = h;
        for piece in pieces(text) {
            match piece {
                Piece::Text(t) => {
                    // Word by word, so long runs wrap around the glyphs.
                    for word in t.split_inclusive(' ') {
                        ui.label(RichText::new(word).size(size).color(color));
                    }
                }
                Piece::Glyph(g) => {
                    ui.add_space(2.0);
                    show(ui, &g, h);
                    ui.add_space(2.0);
                }
            }
        }
    });
}

/// One line of text with prompts painted centred at `pos` (no wrapping). Returns its size.
pub fn line(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) -> Vec2 {
    line_ex(p, pos, align, text, font, color, false)
}

fn line_ex(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, faded: bool) -> Vec2 {
    let h = font.size * 1.4;
    let parts = pieces(text);
    let gap = 3.0;
    let widths: Vec<f32> = parts
        .iter()
        .map(|pc| match pc {
            Piece::Text(t) => p.layout_no_wrap(t.to_string(), font.clone(), color).size().x,
            Piece::Glyph(g) => width(p, g, h) + gap * 2.0,
        })
        .collect();
    let total = Vec2::new(widths.iter().sum(), h);
    let r = align.anchor_size(pos, total);
    let mut x = r.min.x;
    let y = r.center().y;
    for (pc, w) in parts.iter().zip(&widths) {
        match pc {
            Piece::Text(t) => {
                p.text(Pos2::new(x, y), Align2::LEFT_CENTER, *t, font.clone(), color);
            }
            Piece::Glyph(g) => {
                paint(p, Pos2::new(x + gap, y), g, h, faded);
            }
        }
        x += w;
    }
    total
}

/// A button whose label ends in a prompt (`"Keep {K|X}"`), centred.
pub fn button(ui: &mut Ui, label: &str, size: f32, min: Vec2, enabled: bool, stroke: Option<Stroke>, fill: Option<Color32>, color: Option<Color32>) -> Response {
    let mut b = egui::Button::new("").min_size(min);
    if let Some(s) = stroke {
        b = b.stroke(s);
    }
    if let Some(f) = fill {
        b = b.fill(f);
    }
    let resp = ui.add_enabled(enabled, b);
    let col = color.unwrap_or(if enabled { TEXT } else { DIM });
    let p = ui.painter_at(resp.rect);
    // Faded prompts on disabled buttons.
    line_ex(&p, resp.rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(size), col, !enabled);
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prompts() {
        let p = pieces("Keep {K|X} now {Tab|}.");
        assert!(matches!(p[0], Piece::Text("Keep ")));
        assert!(matches!(&p[1], Piece::Glyph(Glyph::Key(k)) if k == "K"));
        assert!(matches!(p[2], Piece::Text(" now ")));
        assert!(matches!(&p[3], Piece::Glyph(Glyph::Key(k)) if k == "Tab"));
        assert!(matches!(p[4], Piece::Text(".")));
        assert_eq!(Btn::parse("LeftRight"), Some(Btn::LeftRight));
        assert!(matches!(&pieces("{p:Y}")[0], Piece::Glyph(Glyph::Pad(Btn::Y))));
    }
}
