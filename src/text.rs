//! Fonts and text helpers. Headings use Cinzel (an inscription-style serif), body text uses
//! Liberation Sans. Both are bundled under the SIL Open Font License (see assets/fonts).

use std::cell::RefCell;

use macroquad::prelude::*;

struct Fonts {
    heading: Font,
    body: Font,
    bold: Font,
}

thread_local! {
    static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) };
}

pub fn init() {
    let load = |bytes: &[u8]| load_ttf_font_from_bytes(bytes).expect("bundled font is valid");
    let fonts = Fonts {
        heading: load(include_bytes!("../assets/fonts/Cinzel.ttf")),
        body: load(include_bytes!("../assets/fonts/LiberationSans-Regular.ttf")),
        bold: load(include_bytes!("../assets/fonts/LiberationSans-Bold.ttf")),
    };
    FONTS.with(|f| *f.borrow_mut() = Some(fonts));
}

#[derive(Clone, Copy)]
pub enum Style {
    Body,
    Bold,
    Heading,
}

fn with_font<R>(style: Style, f: impl FnOnce(Option<&Font>) -> R) -> R {
    FONTS.with(|fonts| {
        let fonts = fonts.borrow();
        let font = fonts.as_ref().map(|fs| match style {
            Style::Body => &fs.body,
            Style::Bold => &fs.bold,
            Style::Heading => &fs.heading,
        });
        f(font)
    })
}

pub fn draw(t: &str, x: f32, y: f32, size: f32, color: Color, style: Style) {
    with_font(style, |font| {
        draw_text_ex(t, x.round(), y.round(), TextParams { font, font_size: size.round() as u16, color, ..Default::default() });
    });
}

/// Body text at baseline (x, y).
pub fn text(t: &str, x: f32, y: f32, size: f32, color: Color) {
    draw(t, x, y, size, color, Style::Body);
}

pub fn bold(t: &str, x: f32, y: f32, size: f32, color: Color) {
    draw(t, x, y, size, color, Style::Bold);
}

pub fn heading(t: &str, x: f32, y: f32, size: f32, color: Color) {
    draw(t, x, y, size, color, Style::Heading);
}

pub fn measure(t: &str, size: f32, style: Style) -> TextDimensions {
    with_font(style, |font| measure_text(t, font, size.round() as u16, 1.0))
}

pub fn width(t: &str, size: f32) -> f32 {
    measure(t, size, Style::Body).width
}

/// Text centered horizontally on x and vertically on y.
pub fn centered(t: &str, x: f32, y: f32, size: f32, color: Color, style: Style) {
    let d = measure(t, size, style);
    draw(t, x - d.width / 2.0, y + d.offset_y / 2.0 - 1.0, size, color, style);
}

/// Heading text with a soft drop shadow, centered.
pub fn title(t: &str, x: f32, y: f32, size: f32, color: Color) {
    centered(t, x + 2.0, y + 3.0, size, Color::new(0.0, 0.0, 0.0, color.a * 0.6), Style::Heading);
    centered(t, x, y, size, color, Style::Heading);
}

/// Word-wrapped body text starting at top y; returns y below the last line.
pub fn wrapped(t: &str, x: f32, mut y: f32, max_w: f32, size: f32, color: Color) -> f32 {
    let line_h = size * 1.25;
    let mut line = String::new();
    for word in t.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if width(&candidate, size) > max_w && !line.is_empty() {
            y += line_h;
            text(&line, x, y, size, color);
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        y += line_h;
        text(&line, x, y, size, color);
    }
    y + 4.0
}
