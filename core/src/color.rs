//! A renderer-independent RGBA color (components 0..1).

#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }
    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0)
    }
}

pub const GOLD: Color = Color::new(1.0, 0.8, 0.0, 1.0);
pub const GRAY: Color = Color::new(0.51, 0.51, 0.51, 1.0);
