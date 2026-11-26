//! Facet in 3D. The game rules live in `facet-core`; this crate draws them with Bevy and
//! puts an egui interface on top.

mod atmosphere;
mod audio;
mod creatures;
mod editor;
mod env;
mod menus;
mod mesh;
mod terrain;
mod theme;
mod ui;
mod view;

use bevy::camera::Viewport;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::CameraOutputMode;
use bevy::render::render_resource::BlendState;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
use facet_core::data::{Balance, MapDef};
use facet_core::game::{Game, Mutator, Occ, Phase, STEP};
use facet_core::save::Save;
use facet_core::{bot, rng};

use audio::Audio;
use ui::{UiState, PANEL_W};
use view::{Board, Gfx, SceneState};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// Map and difficulty selection, with the bot playing the selected map behind it.
    Title,
    Playing,
    /// The map editor (drawn entirely by the interface).
    Editor,
}

/// The running game plus its scene bookkeeping.
#[derive(Resource)]
pub struct Sim {
    pub game: Game,
    pub scene: SceneState,
    pub screen: Screen,
    pub bal: Balance,
    pub maps: Vec<MapDef>,
    pub save: Save,
    pub data_err: Option<String>,
    /// Selected map (title) or the map being played.
    pub map: usize,
    /// Set to rebuild the scene for the current game.
    pending: bool,
    acc: f32,
    think: f32,
    ended: f32,
    pub time: f32,
    /// Mutators chosen on the title screen.
    pub mutators: Vec<Mutator>,
    /// Set while playing the daily challenge.
    pub daily: Option<Daily>,
    /// Map editor state (kept while playtesting).
    pub editor: Option<editor::Editor>,
    /// Set while playtesting an editor map: quitting returns to the editor and no records are kept.
    pub test_map: Option<MapDef>,
    /// Started with debug environment variables: never write records.
    pub debug: bool,
}

impl Sim {
    fn seed() {
        rng::srand(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64));
    }

    /// A bot game on the selected map, shown behind the title screen.
    fn demo(&mut self) {
        Self::seed();
        self.game = Game::new(self.bal.clone(), self.maps[self.map].clone(), 0);
        self.acc = 0.0;
        self.think = 0.6;
        self.ended = 0.0;
        self.pending = true;
    }

    pub fn select_map(&mut self, m: usize) {
        if m != self.map {
            self.map = m;
            self.demo();
        }
    }

    /// Opens the map editor on the selected map.
    pub fn open_editor(&mut self) {
        if self.editor.is_none() {
            self.editor = Some(editor::Editor::from_map(&self.maps[self.map], true));
        }
        self.screen = Screen::Editor;
    }

    /// Plays an editor map without saving it.
    pub fn start_test(&mut self, m: MapDef) {
        Self::seed();
        self.game = Game::new(self.bal.clone(), m.clone(), 0);
        self.test_map = Some(m);
        self.daily = None;
        self.screen = Screen::Playing;
        self.acc = 0.0;
        self.pending = true;
    }

    pub fn to_title(&mut self) {
        if self.test_map.take().is_some() {
            self.screen = Screen::Editor;
            return;
        }
        // Reload data so edits to the JSON files apply to the next game.
        let (bal, e1) = Balance::load();
        let (maps, e2) = MapDef::load_all();
        self.data_err = e1.or(e2);
        self.bal = bal;
        self.maps = maps;
        self.map = self.map.min(self.maps.len() - 1);
        self.screen = Screen::Title;
        self.demo();
    }

    pub fn start(&mut self, map: usize, diff: usize) {
        Self::seed();
        self.test_map = None;
        self.map = map;
        self.game = Game::with_options(self.bal.clone(), self.maps[map].clone(), diff, self.mutators.clone(), None);
        self.daily = None;
        self.screen = Screen::Playing;
        self.acc = 0.0;
        self.pending = true;
    }

    /// Today's daily challenge.
    pub fn start_daily(&mut self) {
        Self::seed();
        self.test_map = None;
        let d = Daily::today(&self.maps, self.bal.difficulties.len());
        self.map = d.map;
        self.game = Game::with_options(self.bal.clone(), self.maps[d.map].clone(), d.diff, d.mutators.clone(), Some(d.seed));
        self.daily = Some(d);
        self.screen = Screen::Playing;
        self.acc = 0.0;
        self.pending = true;
    }

    /// Restart whatever kind of run is in progress.
    pub fn restart(&mut self) {
        if let Some(m) = self.test_map.clone() {
            self.start_test(m);
        } else if self.daily.is_some() {
            self.start_daily();
        } else {
            let saved = std::mem::replace(&mut self.mutators, self.game.mutators.clone());
            self.start(self.map, self.game.diff);
            self.mutators = saved;
        }
    }

    /// The records key for the run in progress.
    pub fn run_key(&self) -> String {
        match &self.daily {
            Some(d) => d.key(),
            None => ui::record_key_with(&self.game.map.name, &self.game.bal.difficulties[self.game.diff].name, &self.game.mutators),
        }
    }
}

/// The daily challenge: the same map, difficulty, mutators and gem rolls for everyone on a date.
#[derive(Clone)]
pub struct Daily {
    pub date: String,
    pub seed: u64,
    pub map: usize,
    pub diff: usize,
    pub mutators: Vec<Mutator>,
}

impl Daily {
    pub fn today(all_maps: &[MapDef], diffs: usize) -> Daily {
        // Only the built-in maps, so the daily is the same for everyone.
        let pool: Vec<usize> = (0..all_maps.len()).filter(|&i| !all_maps[i].custom).collect();
        let pool = if pool.is_empty() { (0..all_maps.len()).collect() } else { pool };
        let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400);
        let mix = |k: u64| {
            let mut x = days.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ k.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            x ^= x >> 31;
            x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x ^ (x >> 29)
        };
        let map = pool[(mix(1) % pool.len() as u64) as usize];
        let diff = 1.min(diffs - 1);
        let all = Mutator::ALL;
        let a = (mix(2) % all.len() as u64) as usize;
        let mut b = (mix(3) % (all.len() as u64 - 1)) as usize;
        if b >= a {
            b += 1;
        }
        Daily { date: civil_date(days as i64), seed: days, map, diff, mutators: vec![all[a.min(b)], all[a.max(b)]] }
    }

    pub fn key(&self) -> String {
        format!("Daily {}", self.date)
    }
}

/// Days since 1970-01-01 to "YYYY-MM-DD" (UTC).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Finds the `assets` folder: next to the crate when run from the build directory, or
/// beside the executable once installed.
fn asset_dir() -> String {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("assets"));
        candidates.push(dir.join("../../assets"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("facet3d/assets"));
        candidates.push(cwd.join("assets"));
    }
    candidates
        .into_iter()
        .find(|p| p.join("music").is_dir())
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "assets".into())
}

/// Orbit camera state.
#[derive(Resource, Default)]
pub struct CamRig {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    /// How far the right mouse button has dragged since it was pressed.
    right_drag: f32,
    /// A preset view being eased towards: (focus, yaw, pitch, dist).
    pub target: Option<(Vec3, f32, f32, f32)>,
    /// Which preset was chosen last (false: angled, true: top-down).
    pub top: bool,
}

impl CamRig {
    pub fn set(&mut self, v: (Vec3, f32, f32, f32)) {
        (self.focus, self.yaw, self.pitch, self.dist) = v;
    }
}

/// The preset camera views: an angled overview, or straight down. Both frame the board.
pub fn view_preset(w: f32, h: f32, top: bool) -> (Vec3, f32, f32, f32) {
    let center = Vec3::new(w / 2.0, 0.0, h / 2.0);
    let size = w.max(h * 1.25);
    if top {
        (center, 0.0, 1.53, size * 1.12)
    } else {
        (center + Vec3::new(0.0, 0.0, h * 0.04), 0.0, 58f32.to_radians(), size * 1.12)
    }
}

fn main() {
    let (bal, e1) = Balance::load();
    let (maps, e2) = MapDef::load_all();
    let data_err = e1.or(e2);
    if let Some(e) = &data_err {
        eprintln!("data: {e}");
    }
    let save = Save::load();
    let map = std::env::var("FACET3D_MAP").ok().and_then(|v| v.parse().ok()).unwrap_or(0usize).min(maps.len() - 1);
    let game = Game::new(bal.clone(), maps[map].clone(), 0);
    let mut sim = Sim {
        game,
        scene: SceneState::default(),
        screen: Screen::Title,
        bal,
        maps,
        save,
        data_err,
        map,
        pending: true,
        acc: 0.0,
        think: 0.6,
        ended: 0.0,
        time: 0.0,
        debug: std::env::vars().any(|(k, _)| k.starts_with("FACET3D_")),
        mutators: Vec::new(),
        daily: None,
        editor: None,
        test_map: None,
    };
    sim.demo();
    if std::env::var("FACET3D_EDITOR").is_ok() {
        sim.open_editor();
    }
    // Debug: FACET3D_PLAY=difficulty skips the title; FACET3D_WAVE=n fast-forwards there with the bot.
    if let Some(d) = std::env::var("FACET3D_PLAY").ok().and_then(|v| v.parse().ok()) {
        sim.start(map, d);
        if let Some(n) = std::env::var("FACET3D_WAVE").ok().and_then(|v| v.parse::<u32>().ok()) {
            let g = &mut sim.game;
            let mut into = 0;
            while !matches!(g.phase, Phase::GameOver | Phase::Victory) && !(g.wave + 1 >= n && g.phase == Phase::Wave && into > 300) {
                if g.wave + 1 >= n && g.phase == Phase::Wave {
                    into += 1;
                }
                if g.wave + 1 >= n && g.phase == Phase::Build && std::env::var("FACET3D_INTO").is_err() {
                    break;
                }
                if !bot::act(g) {
                    g.step();
                }
                g.events.clear();
            }
        }
    }

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window { title: "Facet".into(), resolution: (1600, 920).into(), ..default() }),
                    ..default()
                })
                .set(AssetPlugin { file_path: asset_dir(), ..default() }),
        )
        .add_plugins(EguiPlugin::default())
        .insert_resource(sim)
        // Animated creature bones must follow their moving roots every frame.
        .insert_resource(bevy::transform::StaticTransformOptimizations::Disabled)
        .insert_resource(ClearColor(Color::BLACK))
        .init_resource::<CamRig>()
        .init_resource::<atmosphere::WeatherBounds>()
        .insert_resource({
            let mut st = UiState::default();
            st.book = std::env::var("FACET3D_BOOK").is_ok();
            st.page = match std::env::var("FACET3D_PAGE").as_deref() {
                Ok("pause") => Some(menus::Page::Pause),
                Ok("settings") => Some(menus::Page::Settings),
                Ok("help") => Some(menus::Page::Help),
                _ => None,
            };
            if std::env::var("FACET3D_TOP").is_ok() {
                st.cam_request = Some(true);
            }
            st
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (keys, apply_settings, rebuild, drive, events, sync_scene, view::animate, creatures::corpses, atmosphere::update, atmosphere::light_shift, camera, viewport, hover, view::path_dots, draw_overlays, audio::flush).chain(),
        )
        .add_systems(EguiPrimaryContextPass, ui::interface)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut sounds: ResMut<Assets<AudioSource>>,
    server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut gizmo_store: ResMut<GizmoConfigStore>,
    mut egui_settings: ResMut<EguiGlobalSettings>,
    sim: Res<Sim>,
) {
    // The interface gets its own full-window camera; the 3D camera's viewport shrinks to
    // leave room for the side panel.
    egui_settings.auto_create_primary_context = false;
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write { blend_state: Some(BlendState::ALPHA_BLENDING), clear_color: ClearColorConfig::None },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
    commands.insert_resource(Gfx::new(&mut meshes, &mut mats, &mut images));
    commands.insert_resource(creatures::Models::load(&asset_dir(), &server, &mut graphs));
    commands.insert_resource(Audio::load(&mut sounds, &server, sim.bal.gems.len(), sim.save.music, sim.save.sfx));
    let (def, _) = gizmo_store.config_mut::<DefaultGizmoConfigGroup>();
    def.line.width = 2.5;

    commands.spawn((
        Camera3d::default(),
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        DistanceFog { color: Color::BLACK, falloff: FogFalloff::Linear { start: 40.0, end: 110.0 }, ..default() },
        Transform::from_xyz(15.0, 30.0, 40.0).looking_at(Vec3::new(15.0, 0.0, 15.0), Vec3::Y),
    ));
}

/// Keyboard shortcuts (the egui panel handles its own buttons).
fn keys(input: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>, mut st: ResMut<UiState>) {
    if sim.screen == Screen::Editor || st.typing {
        return;
    }
    if input.just_pressed(KeyCode::KeyM) && st.page != Some(menus::Page::Settings) {
        sim.save.music = !sim.save.music;
        sim.save.store();
    }
    if input.just_pressed(KeyCode::KeyN) && st.page != Some(menus::Page::Settings) {
        sim.save.sfx = !sim.save.sfx;
        sim.save.store();
    }
    if input.just_pressed(KeyCode::F11) {
        sim.save.settings.fullscreen = !sim.save.settings.fullscreen;
        sim.save.store();
    }
    let esc = input.just_pressed(KeyCode::Escape);
    if sim.screen != Screen::Playing {
        if esc {
            st.page = None;
        }
        return;
    }
    // Escape backs out of whatever is open, innermost first; otherwise opens the menu.
    if esc {
        if st.confirm_rock.is_some() {
            st.confirm_rock = None;
        } else if st.book {
            st.book = false;
        } else if let Some(p) = st.page {
            st.page = if p == menus::Page::Pause { None } else { Some(menus::Page::Pause) };
        } else if sim.game.selected.is_some() {
            sim.game.selected = None;
        } else {
            st.page = Some(menus::Page::Pause);
        }
        return;
    }
    if let Some(rock) = st.confirm_rock {
        if input.just_pressed(KeyCode::Enter) || input.just_pressed(KeyCode::NumpadEnter) || input.just_pressed(KeyCode::KeyX) {
            sim.game.remove_rock(rock);
            st.confirm_rock = None;
        }
        return;
    }
    if st.page.is_some() {
        return;
    }
    for (key, sp) in [(KeyCode::Digit1, 1), (KeyCode::Digit2, 2), (KeyCode::Digit3, 4)] {
        if input.just_pressed(key) {
            st.speed = sp;
        }
    }
    if input.just_pressed(KeyCode::Space) || input.just_pressed(KeyCode::KeyP) {
        st.paused = !st.paused;
    }
    if input.just_pressed(KeyCode::KeyB) {
        st.book = !st.book;
    }
    let g = &mut sim.game;
    if input.just_pressed(KeyCode::KeyU) {
        g.buy_luck();
    }
    if let Some(sel) = g.selected {
        if input.just_pressed(KeyCode::KeyX) && g.is_rock(sel) {
            st.confirm_rock = Some(sel);
        }
        if input.just_pressed(KeyCode::KeyL) {
            g.upgrade_special(sel);
        }
    }
    if g.phase == Phase::Choose {
        if input.just_pressed(KeyCode::KeyK) {
            g.keep(false);
        }
        if input.just_pressed(KeyCode::KeyC) {
            g.keep(true);
        }
    }
    if g.phase == Phase::Victory && input.just_pressed(KeyCode::KeyE) {
        ui::handle_request(&mut sim, &mut st, menus::Request::Endless);
    } else if matches!(g.phase, Phase::GameOver | Phase::Victory) && input.just_pressed(KeyCode::KeyR) {
        ui::handle_request(&mut sim, &mut st, menus::Request::Quit);
    }
}

/// Applies saved settings: volumes, fullscreen and graphics quality.
fn apply_settings(
    mut commands: Commands,
    sim: Res<Sim>,
    mut audio: ResMut<Audio>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    cams: Query<Entity, With<Camera3d>>,
    mut lights: Query<&mut DirectionalLight>,
    mut applied: Local<Option<facet_core::save::Quality>>,
) {
    use bevy::window::{MonitorSelection, WindowMode};
    use facet_core::save::Quality;
    let s = &sim.save.settings;
    audio.music_on = sim.save.music;
    audio.sfx_on = sim.save.sfx;
    audio.music_gain = s.music_volume;
    audio.sfx_gain = s.sfx_volume;
    if let Ok(mut w) = windows.single_mut() {
        let want = if s.fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed };
        if w.mode != want {
            w.mode = want;
        }
    }
    let shadows = s.quality != Quality::Low;
    for mut l in &mut lights {
        if l.shadow_maps_enabled != shadows {
            l.shadow_maps_enabled = shadows;
        }
    }
    if *applied != Some(s.quality) {
        *applied = Some(s.quality);
        for cam in &cams {
            let mut e = commands.entity(cam);
            match s.quality {
                Quality::Low => {
                    e.remove::<Bloom>().insert(Msaa::Off);
                }
                Quality::Medium => {
                    e.insert((Bloom::NATURAL, Msaa::Off));
                }
                Quality::High => {
                    e.insert((Bloom::NATURAL, Msaa::Sample4));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    mut sim: ResMut<Sim>,
    mut gfx: ResMut<Gfx>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut rig: ResMut<CamRig>,
    mut images: ResMut<Assets<Image>>,
    board: Query<Entity, (With<Board>, Without<ChildOf>)>,
    mut fog: Query<(Entity, &mut DistanceFog)>,
) {
    if !sim.pending {
        return;
    }
    sim.pending = false;
    for e in &board {
        commands.entity(e).despawn();
    }
    view::build_map(&mut commands, &mut sim, &mut gfx, &mut meshes, &mut mats, &mut ambient, &mut clear, &mut rig);
    if sim.screen == Screen::Title {
        rig.dist *= 1.15;
        rig.pitch = 40f32.to_radians();
    }
    let th = theme::theme(&sim.game.map.theme);
    let (mw, mh) = (sim.game.map.w as f32, sim.game.map.h as f32);
    commands.insert_resource(atmosphere::SunBase::new(&th, Vec3::new(mw / 2.0, 0.0, mh / 2.0)));
    commands.insert_resource(atmosphere::WeatherBounds(Vec4::new(-4.0, -4.0, mw + 4.0, mh + 4.0)));
    atmosphere::spawn(&mut commands, &sim.game.map.theme, mw, mh, sim.save.settings.quality, &mut gfx, &mut mats);
    let sky = images.add(env::sky_cubemap(&th));
    let size = sim.game.map.w.max(sim.game.map.h) as f32;
    for (cam, mut f) in &mut fog {
        f.color = clear.0;
        f.falloff = FogFalloff::Linear { start: size * 1.4, end: size * 3.4 };
        commands.entity(cam).insert(GeneratedEnvironmentMapLight { environment_map: sky.clone(), intensity: 900.0, ..default() });
    }
}

fn drive(time: Res<Time>, mut sim: ResMut<Sim>, mut st: ResMut<UiState>) {
    let real = time.delta_secs().min(0.1);
    sim.time = time.elapsed_secs();
    st.tick(real);
    match sim.screen {
        Screen::Playing => {
            if st.blocking() {
                return;
            }
            sim.acc += real * st.speed as f32;
            let mut steps = 0;
            while sim.acc >= STEP && steps < 32 {
                sim.game.step();
                sim.acc -= STEP;
                steps += 1;
            }
        }
        Screen::Editor => {}
        Screen::Title => {
            // The bot plays behind the menu.
            sim.acc += real;
            let mut thought = false;
            while sim.acc >= STEP {
                sim.acc -= STEP;
                let g = &mut sim.game;
                match g.phase {
                    Phase::Build | Phase::Choose => {
                        sim.think -= STEP;
                        if sim.think <= 0.0 {
                            if thought {
                                sim.acc = 0.0;
                                break;
                            }
                            thought = true;
                            let g = &mut sim.game;
                            bot::act(g);
                            sim.think = if g.phase == Phase::Choose { 0.8 } else { 0.35 };
                        }
                    }
                    Phase::Wave => g.step(),
                    Phase::Victory | Phase::GameOver => {
                        sim.ended += STEP;
                        if sim.ended > 6.0 {
                            sim.demo();
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Routes simulation events to sound and the interface, and saves records.
fn events(mut sim: ResMut<Sim>, mut st: ResMut<UiState>, mut audio: ResMut<Audio>) {
    if sim.screen != Screen::Playing {
        return;
    }
    let debug = sim.debug || sim.test_map.is_some();
    for e in &sim.game.events {
        audio.event(e);
        st.effect(e);
    }
    let key = sim.run_key();
    let Sim { game: g, save, .. } = &mut *sim;
    if debug {
        return;
    }
    if g.phase == Phase::Victory && !st.run_won {
        st.run_won = true;
        st.new_best = save.record_win(&key, g.lives, g.wave);
    }
    if g.phase == Phase::GameOver && !st.run_ended {
        st.run_ended = true;
        st.new_best = save.record_end(&key, g.wave, st.run_won);
    }
}

fn sync_scene(
    mut commands: Commands,
    mut sim: ResMut<Sim>,
    mut gfx: ResMut<Gfx>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut transforms: Query<&mut Transform, Without<view::CreatureModel>>,
    mut models: ResMut<creatures::Models>,
    time: Res<Time>,
) {
    if sim.pending {
        return;
    }
    view::sync(&mut commands, &mut sim, &mut gfx, &mut mats, &mut transforms, &mut models, time.elapsed_secs());
}

#[allow(clippy::too_many_arguments)]
fn camera(
    input: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    sim: Res<Sim>,
    mut st: ResMut<UiState>,
    mut rig: ResMut<CamRig>,
    mut cam: Query<&mut Transform, With<Camera3d>>,
) {
    let dt = time.delta_secs();
    if sim.screen != Screen::Playing {
        rig.yaw += dt * 0.04;
    } else {
        let (w, h) = (sim.game.map.w as f32, sim.game.map.h as f32);
        let request = st.cam_request.take();
        if input.just_pressed(KeyCode::Home) || request == Some(false) {
            rig.top = false;
            rig.target = Some(view_preset(w, h, false));
        }
        if input.just_pressed(KeyCode::KeyV) || request == Some(true) {
            let top = if request == Some(true) { true } else { !rig.top };
            rig.top = top;
            rig.target = Some(view_preset(w, h, top));
        }
        let before = (rig.focus, rig.yaw, rig.pitch, rig.dist);
        let fwd = Vec3::new(-rig.yaw.sin(), 0.0, -rig.yaw.cos());
        let right = Vec3::new(rig.yaw.cos(), 0.0, -rig.yaw.sin());
        let pan_speed = rig.dist * 0.9 * dt;
        let mut pan = Vec3::ZERO;
        if input.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
            pan += fwd;
        }
        if input.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
            pan -= fwd;
        }
        if input.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
            pan += right;
        }
        if input.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
            pan -= right;
        }
        rig.focus += pan * pan_speed;
        // Keep the focus on (or near) the board.
        rig.focus.x = rig.focus.x.clamp(-4.0, w + 4.0);
        rig.focus.z = rig.focus.z.clamp(-4.0, h + 4.0);
        if input.pressed(KeyCode::KeyQ) {
            rig.yaw -= 1.5 * dt;
        }
        if input.pressed(KeyCode::KeyE) && sim.game.phase != Phase::Victory {
            rig.yaw += 1.5 * dt;
        }
        let d = motion.delta;
        if mouse.just_pressed(MouseButton::Right) {
            rig.right_drag = 0.0;
        }
        if mouse.pressed(MouseButton::Right) {
            rig.right_drag += d.length();
            rig.yaw -= d.x * 0.005;
            rig.pitch = (rig.pitch + d.y * 0.004).clamp(0.35, 1.53);
        }
        if mouse.pressed(MouseButton::Middle) {
            let k = rig.dist * 0.0022;
            rig.focus += (-right * d.x + fwd * d.y) * k;
        }
        if scroll.delta.y != 0.0 && !st.pointer_over_ui {
            rig.dist = (rig.dist * (1.0 - scroll.delta.y * 0.1)).clamp(6.0, 90.0);
        }
        // Any manual movement cancels an easing preset; otherwise ease towards it.
        if (rig.focus, rig.yaw, rig.pitch, rig.dist) != before {
            rig.target = None;
        } else if let Some((f, yaw, pitch, dist)) = rig.target {
            let k = 1.0 - (-dt * 7.0).exp();
            let mut dy = (yaw - rig.yaw) % std::f32::consts::TAU;
            if dy > std::f32::consts::PI {
                dy -= std::f32::consts::TAU;
            } else if dy < -std::f32::consts::PI {
                dy += std::f32::consts::TAU;
            }
            rig.focus = rig.focus.lerp(f, k);
            rig.yaw += dy * k;
            rig.pitch += (pitch - rig.pitch) * k;
            rig.dist += (dist - rig.dist) * k;
            if rig.focus.distance(f) < 0.01 && dy.abs() < 0.001 && (pitch - rig.pitch).abs() < 0.001 && (dist - rig.dist).abs() < 0.01 {
                rig.target = None;
            }
        }
    }
    let off = Vec3::new(rig.yaw.sin() * rig.pitch.cos(), rig.pitch.sin(), rig.yaw.cos() * rig.pitch.cos()) * rig.dist;
    // Screen shake: a small, quickly decaying jitter.
    let jitter = if st.shake > 0.0 {
        let t = time.elapsed_secs() * 40.0;
        Vec3::new(t.sin() * 0.7, (t * 1.3).cos() * 0.5, (t * 0.9).sin() * 0.7) * st.shake
    } else {
        Vec3::ZERO
    };
    for mut t in &mut cam {
        *t = Transform::from_translation(rig.focus + off + jitter).looking_at(rig.focus + jitter, Vec3::Y);
    }
}

/// While playing, the 3D view fills the space left of the side panel.
fn viewport(sim: Res<Sim>, window: Query<&Window, With<PrimaryWindow>>, mut cam: Query<&mut Camera, With<Camera3d>>) {
    let (Ok(win), Ok(mut cam)) = (window.single(), cam.single_mut()) else { return };
    let (w, h) = (win.physical_width(), win.physical_height());
    let panel = if sim.screen == Screen::Playing { (PANEL_W * win.scale_factor()) as u32 } else { 0 };
    let vw = w.saturating_sub(panel).max(1);
    let size = UVec2::new(vw, h.max(1));
    if cam.viewport.as_ref().is_none_or(|v| v.physical_size != size) {
        cam.viewport = Some(Viewport { physical_position: UVec2::ZERO, physical_size: size, ..default() });
    }
}

/// Finds the hovered cell and handles clicks on the board.
#[allow(clippy::too_many_arguments)]
fn hover(
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut sim: ResMut<Sim>,
    mut st: ResMut<UiState>,
    rig: Res<CamRig>,
    mut audio: ResMut<Audio>,
) {
    st.hover = None;
    if sim.screen != Screen::Playing {
        return;
    }
    let (Ok(window), Ok((camera, ct))) = (window.single(), cam.single()) else { return };
    if st.book {
        if mouse.just_pressed(MouseButton::Left) && !st.pointer_over_ui {
            st.book = false;
        }
        return;
    }
    let over_ui = st.pointer_over_ui;
    if let Some(cursor) = window.cursor_position().filter(|_| !over_ui)
        && let Ok(ray) = camera.viewport_to_world(ct, cursor)
        && let Some(p) = ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
    {
        let c = (p.x.floor() as i32, p.z.floor() as i32);
        if sim.game.in_bounds(c) {
            st.hover = Some(c);
        }
    }

    let g = &mut sim.game;
    if mouse.just_pressed(MouseButton::Left) && !over_ui {
        if let Some(c) = st.hover {
            let i = g.idx(c);
            match (&g.grid[i], g.phase) {
                (Occ::Empty, Phase::Build) => g.place(c),
                (Occ::Tower(_) | Occ::Rock, _) => {
                    g.selected = Some(i);
                    audio.click();
                }
                _ => g.selected = None,
            }
        } else {
            g.selected = None;
        }
    }
    // A right click (not a drag) clears the selection.
    if mouse.just_released(MouseButton::Right) && rig.right_drag < 6.0 {
        g.selected = None;
    }

    // Path preview for the hovered cell, recomputed only when it or the board changes.
    st.preview = match (g.phase, st.hover) {
        (Phase::Build, Some(c)) => match st.preview.take() {
            Some(p) if p.0 == c && p.1 == g.rev => Some(p),
            _ => {
                let r = if g.can_place(c).is_ok() { g.route_with(Some(g.idx(c))) } else { None };
                Some((c, g.rev, r))
            }
        },
        _ => None,
    };
}

fn draw_overlays(sim: Res<Sim>, st: Res<UiState>, mut gizmos: Gizmos) {
    view::overlays(&sim, &st, &mut gizmos);
}
