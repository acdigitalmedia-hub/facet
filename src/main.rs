mod art;
mod audio;
mod data;
mod game;
mod icon;
mod path;
mod save;
mod text;
mod ui;

use macroquad::prelude::*;

use audio::Audio;
use data::{Balance, MapDef};
use game::{Game, Phase, STEP};
use save::Save;
use ui::{record_key, Action, Title, Ui};

pub const TITLE: &str = "Facet";

fn window_conf() -> Conf {
    Conf {
        window_title: TITLE.to_string(),
        window_width: 1280,
        window_height: 880,
        high_dpi: true,
        window_resizable: true,
        sample_count: 4,
        icon: Some(icon::window_icon()),
        ..Default::default()
    }
}

/// Balance and maps, reloaded each time the title screen is shown so data edits apply.
struct Data {
    bal: Balance,
    maps: Vec<MapDef>,
    err: Option<String>,
}

fn load_data() -> Data {
    let (bal, e1) = Balance::load();
    let (maps, e2) = MapDef::load_all();
    let err = e1.or(e2);
    if let Some(e) = &err {
        eprintln!("{e}");
    }
    Data { bal, maps, err }
}

/// Tracks which end-of-run records have been saved for the current game.
#[derive(Default)]
struct RunRecord {
    won: bool,
    ended: bool,
}

#[macroquad::main(window_conf)]
async fn main() {
    rand::srand((miniquad::date::now() * 1000.0) as u64);

    text::init();
    clear_background(Color::new(0.05, 0.05, 0.06, 1.0));
    text::title("Tuning the crystals...", screen_width() / 2.0, screen_height() / 2.0, 32.0, Color::new(0.93, 0.77, 0.42, 1.0));
    next_frame().await;

    let mut save = Save::load();
    let mut data: Option<Data> = Some(load_data());
    let gem_count = data.as_ref().map_or(6, |d| d.bal.gems.len());
    let mut audio = Audio::load(gem_count, save.music, save.sfx).await;

    let mut game: Option<Game> = None;
    let mut title = Title::new();
    let mut ui: Option<Ui> = None;
    let mut run = RunRecord::default();
    let mut acc = 0.0f32;

    loop {
        if is_key_pressed(KeyCode::M) {
            save.music = !save.music;
            audio.music_on = save.music;
            save.store();
        }
        if is_key_pressed(KeyCode::N) {
            save.sfx = !save.sfx;
            audio.sfx_on = save.sfx;
            save.store();
        }
        audio.update_music();

        match game.as_mut() {
            None => {
                let d = data.get_or_insert_with(load_data);
                if let Some((m, diff)) = title.draw(&d.bal, &d.maps, &save, d.err.as_deref()) {
                    let Data { bal, maps, .. } = data.take().unwrap();
                    let g = Game::new(bal, maps[m].clone(), diff);
                    ui = Some(Ui::new(&g));
                    game = Some(g);
                    run = RunRecord::default();
                    acc = 0.0;
                    audio.click();
                }
                if title.button_clicked.replace(false) {
                    audio.click();
                }
            }
            Some(g) => {
                let ui = ui.as_mut().expect("ui exists while playing");
                if ui.input(g) == Action::ToTitle {
                    game = None;
                    next_frame().await;
                    continue;
                }
                if !ui.paused && !ui.book {
                    acc += get_frame_time().min(0.1) * ui.speed as f32;
                    let mut steps = 0;
                    while acc >= STEP && steps < 32 {
                        g.step();
                        acc -= STEP;
                        steps += 1;
                    }
                }
                ui.draw(g);

                // Input and the panel can also create events (placing, keeping, forging...).
                for e in std::mem::take(&mut g.events) {
                    audio.event(&e);
                    ui.effect(&e);
                }

                // Save records once per outcome.
                let diff = record_key(&g.map.name, &g.bal.difficulties[g.diff].name);
                if g.phase == Phase::Victory && !run.won {
                    run.won = true;
                    ui.new_best = save.record_win(&diff, g.lives, g.wave);
                }
                if g.phase == Phase::GameOver && !run.ended {
                    run.ended = true;
                    ui.new_best = save.record_end(&diff, g.wave, run.won);
                }
                if ui.button_clicked.replace(false) {
                    audio.click();
                }
            }
        }
        next_frame().await;
    }
}
