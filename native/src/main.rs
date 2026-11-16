#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod camera;
mod mapdata;
mod sim;
mod ui;
mod world;

use camera::MapCamera;
use macroquad::prelude::*;
use sim::Sim;
use std::io::Write;
use std::path::PathBuf;
use ui::Ui;
use world::World;

struct MapAsset {
    data: &'static [u8],
    ocean: &'static [u8],
}

static MAPS: [MapAsset; 2] = [
    MapAsset {
        data: include_bytes!("../assets/maps/europe-2025.atlasmap.z"),
        ocean: include_bytes!("../assets/maps/europe-2025_ocean.png"),
    },
    MapAsset {
        data: include_bytes!("../assets/maps/afro-eurasia.atlasmap.z"),
        ocean: include_bytes!("../assets/maps/afro-eurasia_ocean.png"),
    },
];

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Append a line to `atlas-log.txt` next to the executable (handy when there is no console,
/// e.g. under Winlator). Never panics.
fn log(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(exe_dir().join("atlas-log.txt")) {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Optional `atlas.cfg` (key=value, '#' comments) next to the exe or in the working directory.
struct Config {
    width: i32,
    height: i32,
    fullscreen: bool,
    fps_cap: u32,
    high_dpi: bool,
    sample_count: i32,
}

impl Config {
    fn load() -> Config {
        let mut c = Config { width: 1280, height: 720, fullscreen: false, fps_cap: 60, high_dpi: false, sample_count: 1 };
        let text = std::fs::read_to_string(exe_dir().join("atlas.cfg"))
            .or_else(|_| std::fs::read_to_string("atlas.cfg"))
            .unwrap_or_default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim().to_lowercase(), v.trim().to_lowercase());
            let flag = v == "1" || v == "true" || v == "yes" || v == "on";
            match k.as_str() {
                "width" => c.width = v.parse().unwrap_or(c.width).clamp(320, 7680),
                "height" => c.height = v.parse().unwrap_or(c.height).clamp(240, 4320),
                "fullscreen" => c.fullscreen = flag,
                "fps_cap" => c.fps_cap = v.parse().unwrap_or(c.fps_cap).min(240),
                "high_dpi" => c.high_dpi = flag,
                "sample_count" => c.sample_count = v.parse().unwrap_or(1).clamp(1, 8),
                _ => {}
            }
        }
        c
    }
}

fn window_conf() -> Conf {
    std::panic::set_hook(Box::new(|info| {
        log(&format!("PANIC: {}", info));
    }));
    let c = Config::load();
    log(&format!(
        "--- start: {}x{} fullscreen={} fps_cap={} high_dpi={} msaa={}",
        c.width, c.height, c.fullscreen, c.fps_cap, c.high_dpi, c.sample_count
    ));
    Conf {
        window_title: "Atlas MiniWars".to_owned(),
        window_width: c.width,
        window_height: c.height,
        fullscreen: c.fullscreen,
        high_dpi: c.high_dpi,
        sample_count: c.sample_count,
        ..Default::default()
    }
}

fn new_game(cur: usize) -> (World, Sim, MapCamera) {
    let world = World::load(MAPS[cur].data, MAPS[cur].ocean).expect("failed to load map");
    let seed = (macroquad::miniquad::date::now() * 1000.0) as u32;
    let sim = Sim::new(&world, seed);
    let cam = MapCamera::new(world.w(), world.h());
    (world, sim, cam)
}

#[macroquad::main(window_conf)]
async fn main() {
    let fps_cap = Config::load().fps_cap;
    log("window created, loading font + map");
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/extracted/fonts/Fredoka-600.ttf")).ok();
    if font.is_none() {
        log("warning: embedded font failed to load, using default font");
    }
    let ocean_bg = Color::from_rgba(58, 111, 143, 255); // CONFIG.colors.OCEAN

    let mut cur = 0usize;
    let (mut world, mut sim, mut cam) = new_game(cur);
    log("map loaded, entering main loop");
    let mut ui = Ui::default();
    let mut last_view = Vec2::ZERO;
    let mut first_frame = true;

    loop {
        let frame_start = get_time();
        let mut want = cur;
        let mut restart = ui.want_restart;
        if ui.want_next_map { want = (cur + 1) % MAPS.len(); }
        if is_key_pressed(KeyCode::Key1) { want = 0; }
        if is_key_pressed(KeyCode::Key2) { want = 1; }
        if is_key_pressed(KeyCode::R) { restart = true; }
        if is_key_pressed(KeyCode::F11) {
            set_fullscreen(true);
        }
        if want != cur || restart {
            cur = want;
            let g = new_game(cur);
            world = g.0;
            sim = g.1;
            cam = g.2;
            ui = Ui::default();
            last_view = Vec2::ZERO;
        }

        let view = vec2(screen_width(), screen_height());
        if view != last_view {
            cam.resize(view);
            last_view = view;
        }
        let dt = get_frame_time().min(0.1);
        cam.update(dt);
        let tap = cam.take_tap();

        sim.step(dt, &mut world);
        for b in sim.dirty.drain(..) {
            world.refresh_box(b);
        }
        world.flush();

        clear_background(ocean_bg);
        world.draw(&cam);
        ui.frame(&font, &mut sim, &world, &mut cam, tap);
        draw_text(&format!("{} FPS", get_fps()), 10.0, 70.0, 16.0, Color::new(1.0, 1.0, 1.0, 0.5));

        next_frame().await;

        if first_frame {
            first_frame = false;
            log("first frame rendered OK");
        }
        if fps_cap > 0 {
            let target = 1.0 / fps_cap as f64;
            let spent = get_time() - frame_start;
            if spent < target {
                std::thread::sleep(std::time::Duration::from_secs_f64(target - spent));
            }
        }
    }
}
