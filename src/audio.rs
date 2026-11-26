//! Procedural audio: every sound effect and the music loop are synthesized at startup,
//! encoded as in-memory WAV files, and handed to macroquad.

use std::f32::consts::TAU;

use macroquad::audio::{load_sound_from_bytes, play_sound, stop_sound, PlaySoundParams, Sound};
use macroquad::rand;

use crate::game::Event;

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
            let x = rand::gen_range(-1.0f32, 1.0);
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
fn music() -> Vec<f32> {
    let bpm = 84.0;
    let beat = 60.0 / bpm;
    let bar = beat * 4.0;
    // Am, F, C, G, Am, F, Dm, E (as MIDI chord tones)
    let chords: [[f32; 4]; 8] = [
        [57.0, 60.0, 64.0, 69.0],
        [53.0, 57.0, 60.0, 65.0],
        [55.0, 60.0, 64.0, 67.0],
        [55.0, 59.0, 62.0, 67.0],
        [57.0, 60.0, 64.0, 69.0],
        [53.0, 57.0, 60.0, 65.0],
        [50.0, 53.0, 57.0, 62.0],
        [52.0, 56.0, 59.0, 64.0],
    ];
    let total = bar * chords.len() as f32;
    let mut out = vec![0.0f32; len(total)];

    for (c, chord) in chords.iter().enumerate() {
        let start = c as f32 * bar;
        // Pad: detuned sines with slow swell, overlapping into the next bar.
        let pad_len = bar * 1.5;
        for &n in chord {
            let f = midi(n);
            let pad: Vec<f32> = (0..len(pad_len))
                .map(|i| {
                    let t = t_of(i);
                    let env = (t / 0.8).min(1.0) * ((pad_len - t) / 1.2).clamp(0.0, 1.0);
                    env * ((TAU * f * t).sin() + (TAU * f * 1.003 * t).sin() * 0.7 + (TAU * f * 2.0 * t).sin() * 0.15)
                })
                .collect();
            mix(&mut out, &pad, start, 0.05, true);
        }
        // Bass
        let bass = bell(midi(chord[0] - 12.0), bar, 1.2);
        mix(&mut out, &bass, start, 0.35, true);
        // Arpeggio: eighth notes up and down the chord, an octave up.
        let pattern = [0, 1, 2, 3, 2, 1, 2, 3];
        for (k, &p) in pattern.iter().enumerate() {
            let n = chord[p] + 12.0;
            let at = start + k as f32 * beat / 2.0;
            let pluck = bell(midi(n), 1.2, 4.5);
            mix(&mut out, &pluck, at, 0.12, true);
            // Echo
            mix(&mut out, &pluck, at + beat * 0.75, 0.05, true);
        }
    }
    out
}

// ---------- Player ----------

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Sfx {
    Place(usize),
    Keep,
    Combine,
    Forge,
    Shot,
    Boom,
    Zap,
    Crit,
    Death,
    BossDeath,
    Leak,
    WaveStart,
    BossWave,
    WaveClear,
    Victory,
    Defeat,
    Upgrade,
    Error,
    Rock,
    Click,
}

pub struct Audio {
    sounds: Vec<(Sfx, Sound)>,
    music: Option<Sound>,
    pub music_on: bool,
    pub sfx_on: bool,
    music_playing: bool,
    /// Last play time per sound, to avoid machine-gun stacking.
    last: std::collections::HashMap<Sfx, f64>,
}

impl Audio {
    pub async fn load(gem_count: usize, music_on: bool, sfx_on: bool) -> Audio {
        // Pentatonic pings, one per gem type
        let penta = [76.0, 79.0, 81.0, 84.0, 86.0, 88.0, 91.0];
        let mut defs: Vec<(Sfx, Vec<f32>)> = Vec::new();
        for g in 0..gem_count {
            defs.push((Sfx::Place(g), bell(midi(penta[g % penta.len()]), 0.5, 9.0)));
        }
        defs.push((Sfx::Keep, notes(&[(81.0, 0.0), (88.0, 0.09)], 0.6, 7.0)));
        defs.push((Sfx::Combine, notes(&[(76.0, 0.0), (81.0, 0.07), (85.0, 0.14), (88.0, 0.21)], 0.7, 6.0)));
        defs.push((
            Sfx::Forge,
            add(
                notes(&[(69.0, 0.0), (76.0, 0.08), (81.0, 0.16), (85.0, 0.24), (88.0, 0.32), (93.0, 0.40)], 1.2, 3.5),
                noise(1.0, 3.0, 0.9).into_iter().map(|s| s * 0.08).collect(),
            ),
        ));
        defs.push((Sfx::Shot, sweep(1400.0, 700.0, 0.06, 30.0, tri)));
        defs.push((Sfx::Boom, add(noise(0.3, 14.0, 0.15), sweep(160.0, 50.0, 0.3, 10.0, sine))));
        defs.push((Sfx::Zap, sweep(2200.0, 400.0, 0.12, 12.0, square)));
        defs.push((Sfx::Crit, add(bell(midi(96.0), 0.25, 14.0), sweep(3000.0, 1500.0, 0.05, 40.0, tri))));
        defs.push((Sfx::Death, add(sweep(700.0, 180.0, 0.1, 20.0, sine), noise(0.05, 60.0, 0.6))));
        defs.push((Sfx::BossDeath, add(noise(1.2, 3.5, 0.08), sweep(120.0, 30.0, 1.2, 2.5, sine))));
        defs.push((Sfx::Leak, add(sweep(220.0, 160.0, 0.35, 4.0, square), sweep(110.0, 80.0, 0.35, 4.0, sine))));
        defs.push((Sfx::WaveStart, add(bell(midi(45.0), 1.6, 2.2), bell(midi(57.0), 1.6, 2.6))));
        {
            let mut drums = vec![0.0; len(1.6)];
            for (k, at) in [0.0, 0.32, 0.64].iter().enumerate() {
                let hit = add(sweep(90.0 - k as f32 * 10.0, 40.0, 0.5, 6.0, sine), noise(0.15, 25.0, 0.2));
                mix(&mut drums, &hit, *at, 1.0, false);
            }
            defs.push((Sfx::BossWave, drums));
        }
        defs.push((Sfx::WaveClear, notes(&[(81.0, 0.0), (85.0, 0.1), (88.0, 0.2)], 0.8, 5.0)));
        defs.push((
            Sfx::Victory,
            notes(&[(69.0, 0.0), (73.0, 0.15), (76.0, 0.3), (81.0, 0.45), (85.0, 0.6), (88.0, 0.75), (93.0, 0.9)], 1.8, 2.5),
        ));
        defs.push((Sfx::Defeat, notes(&[(69.0, 0.0), (65.0, 0.3), (62.0, 0.6), (57.0, 0.9)], 1.6, 2.5)));
        defs.push((Sfx::Upgrade, notes(&[(84.0, 0.0), (91.0, 0.08), (96.0, 0.16)], 0.6, 6.0)));
        defs.push((Sfx::Error, sweep(180.0, 150.0, 0.15, 8.0, square)));
        defs.push((Sfx::Rock, add(noise(0.2, 20.0, 0.3), sweep(200.0, 80.0, 0.15, 15.0, sine))));
        defs.push((Sfx::Click, sweep(2000.0, 1800.0, 0.02, 80.0, tri)));

        let mut sounds = Vec::new();
        for (id, samples) in defs {
            if let Ok(s) = load_sound_from_bytes(&wav(&samples)).await {
                sounds.push((id, s));
            }
        }
        let music = load_sound_from_bytes(&wav(&music())).await.ok();
        Audio { sounds, music, music_on, sfx_on, music_playing: false, last: Default::default() }
    }

    fn get(&self, id: Sfx) -> Option<&Sound> {
        self.sounds.iter().find(|(k, _)| *k == id).map(|(_, s)| s)
    }

    fn play(&mut self, id: Sfx, volume: f32, min_gap: f64) {
        if !self.sfx_on {
            return;
        }
        let now = macroquad::time::get_time();
        if let Some(&t) = self.last.get(&id) {
            if now - t < min_gap {
                return;
            }
        }
        self.last.insert(id, now);
        if let Some(s) = self.get(id) {
            play_sound(s, PlaySoundParams { looped: false, volume });
        }
    }

    pub fn click(&mut self) {
        self.play(Sfx::Click, 0.3, 0.03);
    }

    pub fn update_music(&mut self) {
        let Some(m) = &self.music else { return };
        if self.music_on && !self.music_playing {
            play_sound(m, PlaySoundParams { looped: true, volume: 0.45 });
            self.music_playing = true;
        } else if !self.music_on && self.music_playing {
            stop_sound(m);
            self.music_playing = false;
        }
    }

    pub fn event(&mut self, e: &Event) {
        match e {
            Event::Place { gem, .. } => self.play(Sfx::Place(*gem), 0.6, 0.0),
            Event::Keep { .. } => self.play(Sfx::Keep, 0.6, 0.0),
            Event::Combine { .. } => self.play(Sfx::Combine, 0.7, 0.0),
            Event::Forge { .. } => self.play(Sfx::Forge, 0.8, 0.0),
            Event::Upgrade { .. } => self.play(Sfx::Upgrade, 0.7, 0.0),
            Event::Shot => self.play(Sfx::Shot, 0.12, 0.07),
            Event::Splash { .. } => self.play(Sfx::Boom, 0.25, 0.1),
            Event::Chain => self.play(Sfx::Zap, 0.18, 0.1),
            Event::Crit { .. } => self.play(Sfx::Crit, 0.3, 0.08),
            Event::Death { boss: false, .. } => self.play(Sfx::Death, 0.3, 0.05),
            Event::Death { boss: true, .. } => self.play(Sfx::BossDeath, 0.9, 0.0),
            Event::Leak { .. } => self.play(Sfx::Leak, 0.6, 0.15),
            Event::WaveStart { boss: false, .. } => self.play(Sfx::WaveStart, 0.5, 0.0),
            Event::WaveStart { boss: true, .. } => self.play(Sfx::BossWave, 0.9, 0.0),
            Event::WaveClear => self.play(Sfx::WaveClear, 0.5, 0.0),
            Event::Victory => self.play(Sfx::Victory, 0.8, 0.0),
            Event::Defeat => self.play(Sfx::Defeat, 0.8, 0.0),
            Event::Error => self.play(Sfx::Error, 0.4, 0.1),
            Event::RockRemoved { .. } => self.play(Sfx::Rock, 0.5, 0.0),
        }
    }
}
