//! Ambient weather for each setting, and the light shifting from midday towards a golden
//! evening as a run goes on.

use std::f32::consts::TAU;

use bevy::light::NotShadowCaster;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use facet_core::color::Color as CColor;
use facet_core::game::Phase;
use facet_core::save::Quality;

use crate::mesh::rnd;
use crate::theme::Theme;
use crate::view::{Board, Gfx};
use crate::{Screen, Sim};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Snow,
    Leaf,
    Ember,
    Fog,
    Firefly,
    Mote,
}

/// One drifting weather particle.
#[derive(Component)]
pub struct Weather {
    kind: Kind,
    phase: f32,
    speed: f32,
    size: f32,
}

/// The board area weather wraps around in (x0, z0, x1, z1).
#[derive(Resource, Default)]
pub struct WeatherBounds(pub Vec4);

/// The theme's base lighting, before the evening shift.
#[derive(Resource, Clone)]
pub struct SunBase {
    pub color: Vec3,
    pub lux: f32,
    pub elev: f32,
    pub azim: f32,
    pub fog: Vec3,
    pub ambient: f32,
    pub center: Vec3,
    /// Current blend towards evening (0..1), eased.
    pub t: f32,
}

impl SunBase {
    pub fn new(th: &Theme, center: Vec3) -> SunBase {
        SunBase {
            color: Vec3::from(th.sun),
            lux: th.sun_lux,
            elev: th.sun_angle.0.to_radians(),
            azim: th.sun_angle.1.to_radians(),
            fog: Vec3::from(th.fog),
            ambient: th.ambient_brightness,
            center,
            t: 0.0,
        }
    }
}

/// Spawns weather for a theme. Counts scale with graphics quality.
pub fn spawn(commands: &mut Commands, theme: &str, w: f32, h: f32, quality: Quality, gfx: &mut Gfx, mats: &mut Assets<StandardMaterial>) {
    let scale = match quality {
        Quality::Low => 0.0,
        Quality::Medium => 0.5,
        Quality::High => 1.0,
    };
    let area = (w + 8.0) * (h + 8.0) / (39.0 * 39.0);
    let n = |base: f32| (base * scale * area) as usize;
    let mut add = |commands: &mut Commands, kind: Kind, count: usize, col: CColor, style: u8, glow: f32, size: f32, y: (f32, f32)| {
        let m = gfx.mat(mats, col, style, glow);
        let mesh = if kind == Kind::Leaf { gfx.cube.clone() } else { gfx.sphere.clone() };
        for i in 0..count {
            let s = i as i32;
            let x = -4.0 + rnd(s, 1, kind as u32) * (w + 8.0);
            let z = -4.0 + rnd(s, 2, kind as u32) * (h + 8.0);
            let yy = y.0 + rnd(s, 3, kind as u32) * (y.1 - y.0);
            let sz = size * (0.7 + 0.6 * rnd(s, 4, kind as u32));
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(m.clone()),
                Transform::from_xyz(x, yy, z).with_scale(Vec3::splat(sz)),
                Weather { kind, phase: rnd(s, 5, kind as u32) * TAU, speed: 0.7 + 0.6 * rnd(s, 6, kind as u32), size: sz },
                NotShadowCaster,
                Board,
            ));
        }
    };
    match theme {
        "snow" => add(commands, Kind::Snow, n(650.0), CColor::new(1.0, 1.0, 1.0, 1.0), 2, 1.2, 0.04, (0.0, 12.0)),
        "autumn" => {
            for (k, col) in [CColor::new(0.9, 0.45, 0.12, 1.0), CColor::new(0.8, 0.22, 0.1, 1.0), CColor::new(0.95, 0.7, 0.2, 1.0)].into_iter().enumerate() {
                let _ = k;
                add(commands, Kind::Leaf, n(45.0), col, 1, 0.0, 1.0, (0.0, 9.0));
            }
        }
        "ash" => add(commands, Kind::Ember, n(220.0), CColor::new(1.0, 0.5, 0.15, 1.0), 2, 9.0, 0.035, (0.0, 9.0)),
        "marsh" => {
            add(commands, Kind::Fog, n(40.0).max(if scale > 0.0 { 8 } else { 0 }), CColor::new(0.85, 0.95, 0.9, 1.0), 5, 0.0, 1.0, (0.15, 0.5));
            add(commands, Kind::Firefly, n(70.0), CColor::new(0.75, 1.0, 0.35, 1.0), 2, 9.0, 0.035, (0.4, 1.8));
        }
        _ => add(commands, Kind::Mote, n(120.0), CColor::new(1.0, 0.95, 0.7, 1.0), 2, 2.5, 0.025, (0.4, 4.0)),
    }
}

/// Moves weather particles and wraps them around the board.
pub fn update(time: Res<Time>, bounds: Res<WeatherBounds>, mut q: Query<(&Weather, &mut Transform)>) {
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();
    let b = bounds.0;
    let wrap = |v: f32, lo: f32, hi: f32| if v < lo { v + (hi - lo) } else if v > hi { v - (hi - lo) } else { v };
    for (w, mut tf) in &mut q {
        let ph = w.phase;
        let sp = w.speed;
        let mut p = tf.translation;
        match w.kind {
            Kind::Snow => {
                p.y -= 0.9 * sp * dt;
                p.x += (0.35 + 0.25 * (t * 0.7 + ph).sin()) * dt;
                p.z += 0.15 * (t * 0.9 + ph).cos() * dt;
                if p.y < -0.2 {
                    p.y += 12.0;
                }
            }
            Kind::Leaf => {
                p.y -= 0.55 * sp * dt;
                p.x += (0.6 + 0.6 * (t * 1.1 + ph).sin()) * dt;
                p.z += 0.4 * (t * 0.8 + ph).cos() * dt;
                if p.y < -0.1 {
                    p.y += 9.0;
                }
                let s = w.size;
                tf.rotation = Quat::from_euler(EulerRot::XYZ, t * 2.0 * sp + ph, t * 1.3 + ph, (t * 1.7 + ph).sin());
                tf.scale = Vec3::new(0.14 * s, 0.012, 0.09 * s);
            }
            Kind::Ember => {
                p.y += 0.7 * sp * dt;
                p.x += 0.3 * (t * 1.3 + ph).sin() * dt;
                p.z += 0.3 * (t * 1.1 + ph).cos() * dt;
                if p.y > 9.0 {
                    p.y -= 9.0;
                }
                let flicker = 0.6 + 0.4 * (t * 9.0 * sp + ph).sin().abs();
                tf.scale = Vec3::splat(w.size * flicker);
            }
            Kind::Fog => {
                p.x += 0.25 * sp * dt;
                p.z += 0.08 * (t * 0.2 + ph).sin() * dt;
                let s = w.size * (1.0 + 0.1 * (t * 0.3 + ph).sin());
                tf.scale = Vec3::new(3.2 * s, 0.9 * s, 2.6 * s);
            }
            Kind::Firefly => {
                p.x += 0.4 * (t * 0.7 * sp + ph).sin() * dt;
                p.z += 0.4 * (t * 0.6 * sp + ph * 1.3).cos() * dt;
                p.y += 0.2 * (t * 1.1 + ph).sin() * dt;
                let blink = ((t * 1.5 * sp + ph).sin() * 0.5 + 0.5).powf(3.0);
                tf.scale = Vec3::splat(w.size * (0.2 + blink));
            }
            Kind::Mote => {
                p.x += 0.15 * (t * 0.3 * sp + ph).sin() * dt;
                p.z += 0.15 * (t * 0.25 * sp + ph).cos() * dt;
                p.y += 0.1 * (t * 0.5 + ph).sin() * dt;
            }
        }
        p.x = wrap(p.x, b.x, b.z);
        p.z = wrap(p.z, b.y, b.w);
        tf.translation = p;
    }
}

/// Eases the sun lower and warmer as the run progresses: midday at wave 1, golden evening by
/// the last wave.
#[allow(clippy::too_many_arguments)]
pub fn light_shift(
    time: Res<Time>,
    sim: Res<Sim>,
    base: Option<ResMut<SunBase>>,
    mut suns: Query<(&mut DirectionalLight, &mut Transform)>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut fog: Query<&mut DistanceFog>,
) {
    let Some(mut base) = base else { return };
    let g = &sim.game;
    let target = match sim.screen {
        Screen::Playing | Screen::Title => {
            let total = g.bal.wave_count().max(1) as f32;
            let done = g.wave as f32 + if g.phase == Phase::Wave { 0.5 } else { 0.0 };
            (done / total).clamp(0.0, 1.0)
        }
        Screen::Editor => 0.0,
    };
    let dt = time.delta_secs().min(0.1);
    base.t += (target - base.t) * (1.0 - (-dt * 0.6).exp());
    let t = base.t;
    let elev = (base.elev * (1.0 - 0.5 * t)).max(10f32.to_radians());
    let azim = base.azim + 0.35 * t;
    let warm = Vec3::new(1.0, 0.62, 0.38);
    let color = base.color.lerp(warm, 0.75 * t);
    let to_sun = Vec3::new(elev.cos() * azim.sin(), elev.sin(), elev.cos() * azim.cos());
    for (mut light, mut tf) in &mut suns {
        light.color = Color::srgb(color.x, color.y, color.z);
        light.illuminance = base.lux * (1.0 - 0.3 * t);
        *tf = Transform::from_translation(base.center + to_sun * 60.0).looking_at(base.center, Vec3::Y);
    }
    ambient.brightness = base.ambient * (1.0 - 0.3 * t);
    let dusk = base.fog.lerp(Vec3::new(0.78, 0.52, 0.42) * base.fog.length().max(0.6) / 1.2, 0.55 * t);
    let fc = Color::srgb(dusk.x, dusk.y, dusk.z);
    clear.0 = fc;
    for mut f in &mut fog {
        f.color = fc;
    }
}
