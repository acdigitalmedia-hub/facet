//! Sound: recorded CC0 effects (Kenney) and music (Juhani Junkala) from `assets/`, plus a few
//! synthesized effects (shots, zaps, gongs, fanfares) that are generated at startup.

use std::f32::consts::TAU;

use std::collections::HashMap;

use bevy::audio::{AudioSinkPlayback, PlaybackMode, Volume};
use bevy::prelude::*;
use facet_core::game::{Event, Phase};

const RATE: u32 = 22050;

// ---------- Synthesis helpers ----------

fn len(secs: f32) -> usize {
    (secs * RATE as f32) as usize
}

fn t_of(i: usize) -> f32 {
    i as f32 / RATE as f32
}

fn midi(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}

/// Mixes `src` into `dst` starting at `at` seconds, scaled by `gain`, wrapping around
/// if `wrap` (used for seamless music loops).
fn mix(dst: &mut [f32], src: &[f32], at: f32, gain: f32, wrap: bool) {
    let start = len(at);
    let n = dst.len();
    for (k, s) in src.iter().enumerate() {
        let i = start + k;
        if i < n {
            dst[i] += s * gain;
        } else if wrap {
            dst[i % n] += s * gain;
        }
    }
}

/// A decaying sine "bell" with a couple of inharmonic partials: the basic crystal sound.
fn bell(freq: f32, secs: f32, decay: f32) -> Vec<f32> {
    (0..len(secs))
        .map(|i| {
            let t = t_of(i);
            let env = (-t * decay).exp() * (t * 400.0).min(1.0);
            env * ((TAU * freq * t).sin() * 0.6 + (TAU * freq * 2.76 * t).sin() * 0.25 + (TAU * freq * 5.4 * t).sin() * 0.1)
        })
        .collect()
}

/// A pitch sweep with a given waveform shape.
fn sweep(f0: f32, f1: f32, secs: f32, decay: f32, shape: fn(f32) -> f32) -> Vec<f32> {
    let n = len(secs);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = t_of(i);
            let k = i as f32 / n as f32;
            let f = f0 * (f1 / f0).powf(k);
            phase += f / RATE as f32;
            let env = (-t * decay).exp() * (t * 600.0).min(1.0) * (1.0 - k).min(1.0);
            env * shape(phase)
        })
        .collect()
}

fn sine(p: f32) -> f32 {
    (TAU * p).sin()
}
fn tri(p: f32) -> f32 {
    4.0 * (p - (p + 0.5).floor()).abs() - 1.0
}
fn square(p: f32) -> f32 {
    if p.fract() < 0.5 { 0.6 } else { -0.6 }
}

/// Low-passed noise burst.
fn noise(secs: f32, decay: f32, smooth: f32) -> Vec<f32> {
    let mut y = 0.0f32;
    (0..len(secs))
        .map(|i| {
            let t = t_of(i);
            let x = facet_core::rng::gen_range(-1.0f32, 1.0);
            y += (x - y) * smooth;
            y * (-t * decay).exp() * 2.0
        })
        .collect()
}

fn add(a: Vec<f32>, b: Vec<f32>) -> Vec<f32> {
    let n = a.len().max(b.len());
    (0..n).map(|i| a.get(i).unwrap_or(&0.0) + b.get(i).unwrap_or(&0.0)).collect()
}

fn notes(seq: &[(f32, f32)], note_len: f32, decay: f32) -> Vec<f32> {
    let total = seq.iter().map(|s| s.1).fold(0.0, f32::max) + note_len;
    let mut out = vec![0.0; len(total)];
    for &(n, at) in seq {
        mix(&mut out, &bell(midi(n), note_len, decay), at, 0.5, false);
    }
    out
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    let norm = if peak > 0.95 { 0.95 / peak } else { 1.0 };
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&(((s * norm).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

// ---------- Music ----------

/// An ambient loop: soft pad chords, a plucked crystal arpeggio and a sine bass.

// ---------- Player ----------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Sfx {
    Place(usize),
    Keep,
    Combine,
    Forge,
    ForgeBell,
    Shot,
    Boom,
    Zap,
    Crit,
    Death,
    BossDeath,
    BossThud,
    Leak,
    WaveStart,
    BossWave,
    WaveClear,
    Victory,
    Defeat,
    Coins,
    Upgrade,
    Error,
    Rock,
    Click,
}

/// What music should be playing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Mood {
    Title,
    Build(String),
    Wave(u32),
    Boss,
}

impl Mood {
    fn track(&self) -> String {
        match self {
            Mood::Title => "title".into(),
            Mood::Build(theme) => format!("build_{theme}"),
            Mood::Wave(n) => if n % 2 == 0 { "wave1".into() } else { "wave2".into() },
            Mood::Boss => "boss".into(),
        }
    }
}

struct Playing {
    track: String,
    entity: Entity,
    vol: f32,
}

#[derive(Resource)]
pub struct Audio {
    sounds: HashMap<Sfx, Vec<Handle<AudioSource>>>,
    tracks: HashMap<String, Handle<AudioSource>>,
    pub music_on: bool,
    pub sfx_on: bool,
    /// Volume multipliers from the settings (0..1).
    pub music_gain: f32,
    pub sfx_gain: f32,
    current: Option<Playing>,
    fading: Vec<Playing>,
    /// Last play time per sound, to avoid machine-gun stacking.
    last: HashMap<Sfx, f32>,
    /// Sounds requested this frame (played by `flush`).
    queue: Vec<(Sfx, f32)>,
    now: f32,
    pick: u32,
}

const MUSIC_VOLUME: f32 = 0.42;
const THEMES: [&str; 5] = ["meadow", "autumn", "ash", "marsh", "snow"];

fn source(samples: &[f32]) -> AudioSource {
    AudioSource { bytes: wav(samples).into() }
}

impl Audio {
    pub fn load(assets: &mut Assets<AudioSource>, server: &AssetServer, gem_count: usize, music_on: bool, sfx_on: bool) -> Audio {
        let mut sounds: HashMap<Sfx, Vec<Handle<AudioSource>>> = HashMap::new();
        let mut synth = |id: Sfx, samples: Vec<f32>| {
            sounds.entry(id).or_default().push(assets.add(source(&samples)));
        };
        synth(
            Sfx::Forge,
            add(
                notes(&[(69.0, 0.0), (76.0, 0.08), (81.0, 0.16), (85.0, 0.24), (88.0, 0.32), (93.0, 0.40)], 1.2, 3.5),
                noise(1.0, 3.0, 0.9).into_iter().map(|s| s * 0.08).collect(),
            ),
        );
        synth(Sfx::Shot, sweep(1400.0, 700.0, 0.06, 30.0, tri));
        synth(Sfx::Zap, sweep(2200.0, 400.0, 0.12, 12.0, square));
        synth(Sfx::BossDeath, add(noise(1.2, 3.5, 0.08), sweep(120.0, 30.0, 1.2, 2.5, sine)));
        synth(Sfx::WaveStart, add(bell(midi(45.0), 1.6, 2.2), bell(midi(57.0), 1.6, 2.6)));
        {
            let mut drums = vec![0.0; len(1.6)];
            for (k, at) in [0.0, 0.32, 0.64].iter().enumerate() {
                let hit = add(sweep(90.0 - k as f32 * 10.0, 40.0, 0.5, 6.0, sine), noise(0.15, 25.0, 0.2));
                mix(&mut drums, &hit, *at, 1.0, false);
            }
            synth(Sfx::BossWave, drums);
        }
        synth(
            Sfx::Victory,
            notes(&[(69.0, 0.0), (73.0, 0.15), (76.0, 0.3), (81.0, 0.45), (85.0, 0.6), (88.0, 0.75), (93.0, 0.9)], 1.8, 2.5),
        );
        synth(Sfx::Defeat, notes(&[(69.0, 0.0), (65.0, 0.3), (62.0, 0.6), (57.0, 0.9)], 1.6, 2.5));

        let mut rec = |id: Sfx, files: &[&str]| {
            for f in files {
                sounds.entry(id).or_default().push(server.load(format!("sfx/{f}.ogg")));
            }
        };
        let glass = ["impactGlass_light_000", "impactGlass_light_001", "impactGlass_light_002", "impactGlass_light_003", "impactGlass_light_004"];
        for g in 0..gem_count {
            rec(Sfx::Place(g), &[glass[g % glass.len()]]);
        }
        rec(Sfx::Keep, &["confirmation_001", "glass_001"]);
        rec(Sfx::Combine, &["impactGlass_heavy_000", "impactGlass_heavy_001"]);
        rec(Sfx::ForgeBell, &["impactBell_heavy_000"]);
        rec(Sfx::Boom, &["impactMining_000", "impactMining_002"]);
        rec(Sfx::Crit, &["impactMetal_light_000", "impactMetal_light_001"]);
        rec(Sfx::Death, &["impactSoft_medium_000", "impactSoft_medium_001", "impactSoft_medium_002", "impactSoft_medium_003"]);
        rec(Sfx::BossThud, &["impactPlate_heavy_000"]);
        rec(Sfx::Leak, &["error_006"]);
        rec(Sfx::WaveClear, &["confirmation_002"]);
        rec(Sfx::Coins, &["handleCoins", "handleCoins2"]);
        rec(Sfx::Upgrade, &["impactBell_heavy_001", "impactBell_heavy_002"]);
        rec(Sfx::Error, &["error_001"]);
        rec(Sfx::Rock, &["impactMining_001"]);
        rec(Sfx::Click, &["click_002"]);

        let mut tracks = HashMap::new();
        let mut names: Vec<String> = vec!["title".into(), "wave1".into(), "wave2".into(), "boss".into()];
        names.extend(THEMES.iter().map(|t| format!("build_{t}")));
        for n in names {
            let h = server.load(format!("music/{n}.ogg"));
            tracks.insert(n, h);
        }
        Audio {
            sounds,
            tracks,
            music_on,
            sfx_on,
            music_gain: 0.8,
            sfx_gain: 0.8,
            current: None,
            fading: Vec::new(),
            last: HashMap::new(),
            queue: Vec::new(),
            now: 0.0,
            pick: 0,
        }
    }

    fn play(&mut self, id: Sfx, volume: f32, min_gap: f32) {
        if !self.sfx_on {
            return;
        }
        if let Some(&t) = self.last.get(&id)
            && self.now - t < min_gap
        {
            return;
        }
        self.last.insert(id, self.now);
        self.queue.push((id, volume));
    }

    pub fn click(&mut self) {
        self.play(Sfx::Click, 0.35, 0.03);
    }

    pub fn event(&mut self, e: &Event) {
        match e {
            Event::Place { gem, .. } => self.play(Sfx::Place(*gem), 0.7, 0.0),
            Event::Keep { .. } => self.play(Sfx::Keep, 0.6, 0.0),
            Event::Combine { .. } => self.play(Sfx::Combine, 0.8, 0.0),
            Event::Forge { .. } => {
                self.play(Sfx::Forge, 0.7, 0.0);
                self.play(Sfx::ForgeBell, 0.7, 0.0);
            }
            Event::Upgrade { pos: None, .. } => self.play(Sfx::Coins, 0.7, 0.0),
            Event::Upgrade { .. } => self.play(Sfx::Upgrade, 0.7, 0.0),
            Event::Shot => self.play(Sfx::Shot, 0.08, 0.07),
            Event::Splash { .. } => self.play(Sfx::Boom, 0.3, 0.12),
            Event::Chain => self.play(Sfx::Zap, 0.12, 0.1),
            Event::Crit { .. } => self.play(Sfx::Crit, 0.35, 0.1),
            Event::Death { boss: false, .. } => self.play(Sfx::Death, 0.4, 0.06),
            Event::Death { boss: true, .. } => {
                self.play(Sfx::BossDeath, 0.8, 0.0);
                self.play(Sfx::BossThud, 0.9, 0.0);
            }
            Event::Leak { .. } => self.play(Sfx::Leak, 0.6, 0.15),
            Event::WaveStart { boss: false, .. } => self.play(Sfx::WaveStart, 0.45, 0.0),
            Event::WaveStart { boss: true, .. } => self.play(Sfx::BossWave, 0.85, 0.0),
            Event::WaveClear => self.play(Sfx::WaveClear, 0.6, 0.0),
            Event::Victory => self.play(Sfx::Victory, 0.8, 0.0),
            Event::Defeat => self.play(Sfx::Defeat, 0.8, 0.0),
            Event::Error => self.play(Sfx::Error, 0.5, 0.15),
            Event::RockRemoved { .. } => self.play(Sfx::Rock, 0.7, 0.0),
            Event::Impact { .. } => {}
        }
    }
}

/// The music that fits the current screen and game state.
fn mood(sim: &crate::Sim) -> Mood {
    let g = &sim.game;
    match sim.screen {
        crate::Screen::Title | crate::Screen::Editor => Mood::Title,
        crate::Screen::Playing => match g.phase {
            Phase::Wave if g.creeps.iter().any(|c| c.boss) || g.wave_info(g.wave).boss => Mood::Boss,
            Phase::Wave => Mood::Wave(g.wave),
            _ => {
                let t = g.map.theme.as_str();
                Mood::Build(if THEMES.contains(&t) { t.to_string() } else { "meadow".into() })
            }
        },
    }
}

/// Plays queued sounds and crossfades the music to fit the game.
pub fn flush(mut commands: Commands, time: Res<Time>, sim: Res<crate::Sim>, mut audio: ResMut<Audio>, mut sinks: Query<&mut AudioSink>) {
    let audio = &mut *audio;
    audio.now = time.elapsed_secs();
    let dt = time.delta_secs().min(0.1);
    let queue = std::mem::take(&mut audio.queue);
    for (id, vol) in queue {
        if let Some(list) = audio.sounds.get(&id)
            && !list.is_empty()
        {
            audio.pick = audio.pick.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let h = list[(audio.pick >> 8) as usize % list.len()].clone();
            commands.spawn((AudioPlayer(h), PlaybackSettings { mode: PlaybackMode::Despawn, volume: Volume::Linear(vol * audio.sfx_gain * 1.25), ..default() }));
        }
    }

    // Music: fade the old track out while the new one fades in.
    let audio_music_gain = audio.music_gain * 1.25;
    let want = if audio.music_on { Some(mood(&sim).track()) } else { None };
    if audio.current.as_ref().map(|p| &p.track) != want.as_ref() {
        if let Some(old) = audio.current.take() {
            audio.fading.push(old);
        }
        if let Some(track) = want
            && let Some(h) = audio.tracks.get(&track)
        {
            let entity = commands
                .spawn((AudioPlayer(h.clone()), PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), ..default() }))
                .id();
            audio.current = Some(Playing { track, entity, vol: 0.0 });
        }
    }
    if let Some(cur) = &mut audio.current {
        cur.vol = (cur.vol + dt / 1.5).min(1.0);
        if let Ok(mut sink) = sinks.get_mut(cur.entity) {
            sink.set_volume(Volume::Linear(cur.vol * MUSIC_VOLUME * audio_music_gain));
        }
    }
    audio.fading.retain_mut(|p| {
        p.vol -= dt / 1.2;
        if p.vol <= 0.0 {
            commands.entity(p.entity).despawn();
            return false;
        }
        if let Ok(mut sink) = sinks.get_mut(p.entity) {
            sink.set_volume(Volume::Linear(p.vol * MUSIC_VOLUME * audio_music_gain));
        }
        true
    });
}
