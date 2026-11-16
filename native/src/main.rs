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

static LOGO_PNG: &[u8] = include_bytes!("../assets/ui/logo_small.png");
static RING_PATH: &str = include_str!("../assets/ui/ring_path.txt");

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Where logs / settings live: next to the exe on desktop, the app's private dir on Android.
fn data_dir() -> PathBuf {
    if cfg!(target_os = "android") {
        let p = PathBuf::from("/data/data/com.atlas.miniwars/files");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
        exe_dir()
    }
}

/// Append a line to `atlas-log.txt` (and to logcat on Android). Never panics.
fn log(msg: &str) {
    macroquad::miniquad::info!("atlas: {}", msg);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join("atlas-log.txt")) {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Optional `atlas.cfg` (key=value, '#' comments) in the data dir, next to the exe, or in the cwd.
#[derive(Clone)]
struct Config {
    width: i32,
    height: i32,
    fullscreen: bool,
    fps_cap: u32,
    high_dpi: bool,
    sample_count: i32,
    ui_scale: f32,
}

impl Config {
    fn load() -> Config {
        let mut c = Config { width: 1280, height: 720, fullscreen: false, fps_cap: 60, high_dpi: false, sample_count: 1, ui_scale: 1.0 };
        let text = std::fs::read_to_string(data_dir().join("atlas.cfg"))
            .or_else(|_| std::fs::read_to_string(exe_dir().join("atlas.cfg")))
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
                "ui_scale" => c.ui_scale = v.parse().unwrap_or(1.0f32).clamp(0.6, 2.5),
                _ => {}
            }
        }
        c
    }

    fn save(&self) {
        let text = format!(
            "# Atlas MiniWars settings (ui_scale is changed in-game with the UI+ / UI- buttons)\nwidth={}\nheight={}\nfullscreen={}\nfps_cap={}\nhigh_dpi={}\nsample_count={}\nui_scale={:.2}\n",
            self.width, self.height, self.fullscreen, self.fps_cap, self.high_dpi, self.sample_count, self.ui_scale
        );
        let _ = std::fs::write(data_dir().join("atlas.cfg"), text);
    }
}

fn window_conf() -> Conf {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log(&format!("PANIC: {}", info));
        prev(info);
    }));
    let c = Config::load();
    log(&format!(
        "--- start: {}x{} fullscreen={} fps_cap={} high_dpi={} msaa={} ui_scale={}",
        c.width, c.height, c.fullscreen, c.fps_cap, c.high_dpi, c.sample_count, c.ui_scale
    ));
    Conf {
        window_title: "Atlas MiniWars".to_owned(),
        window_width: c.width,
        window_height: c.height,
        fullscreen: c.fullscreen,
        high_dpi: c.high_dpi || cfg!(target_os = "android"),
        sample_count: c.sample_count,
        ..Default::default()
    }
}

// ---------------------------------------------------------------- loading screen (port of #loading)
fn ring_points() -> Vec<(f32, f32)> {
    // the original is an SVG path made only of M/H/V/Z commands in a 3810x3810 box
    let mut pts: Vec<(f32, f32)> = Vec::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut cmd = ' ';
    let mut num = String::new();
    let mut flush = |cmd: char, num: &mut String, x: &mut f32, y: &mut f32, pts: &mut Vec<(f32, f32)>, pending: &mut Option<f32>| {
        if num.is_empty() {
            return;
        }
        let v: f32 = num.parse().unwrap_or(0.0);
        num.clear();
        match cmd {
            'M' => match pending.take() {
                None => *pending = Some(v),
                Some(px) => {
                    *x = px;
                    *y = v;
                    pts.push((*x, *y));
                }
            },
            'H' => {
                *x = v;
                pts.push((*x, *y));
            }
            'V' => {
                *y = v;
                pts.push((*x, *y));
            }
            _ => {}
        }
    };
    let mut pending: Option<f32> = None;
    for ch in RING_PATH.chars() {
        if ch.is_ascii_alphabetic() {
            flush(cmd, &mut num, &mut x, &mut y, &mut pts, &mut pending);
            if ch == 'Z' || ch == 'z' {
                if let Some(first) = pts.first().copied() {
                    pts.push(first);
                }
            }
            cmd = ch.to_ascii_uppercase();
        } else if ch == ' ' || ch == ',' {
            flush(cmd, &mut num, &mut x, &mut y, &mut pts, &mut pending);
        } else {
            num.push(ch);
        }
    }
    flush(cmd, &mut num, &mut x, &mut y, &mut pts, &mut pending);
    pts
}

fn draw_splash(logo: &Option<Texture2D>, ring: &[(f32, f32)], pct: f32, font: &Option<Font>) {
    clear_background(BLACK);
    let (sw, sh) = (screen_width(), screen_height());
    let mark = (sw.min(sh) * 0.58).min(700.0);
    let (ox, oy) = ((sw - mark) / 2.0, (sh - mark) / 2.0);
    if let Some(t) = logo {
        draw_texture_ex(t, ox, oy, WHITE, DrawTextureParams { dest_size: Some(vec2(mark, mark)), ..Default::default() });
    }
    // progress ring: stroke-dashoffset animation == drawing the first `pct` of the path length
    let k = mark / 3810.0;
    let thick = (20.0 * k).max(1.5);
    let total: f32 = ring.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum();
    let mut left = total * pct.clamp(0.0, 1.0);
    for w in ring.windows(2) {
        if left <= 0.0 {
            break;
        }
        let len = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        let f = if len > 0.0 { (left / len).min(1.0) } else { 1.0 };
        let (x1, y1) = (ox + w[0].0 * k, oy + w[0].1 * k);
        let (x2, y2) = (ox + (w[0].0 + (w[1].0 - w[0].0) * f) * k, oy + (w[0].1 + (w[1].1 - w[0].1) * f) * k);
        draw_line(x1, y1, x2, y2, thick, WHITE);
        left -= len;
    }
    let txt = format!("{}%", (pct * 100.0).round() as i32);
    let size = (sh / 720.0 * 20.0).clamp(16.0, 40.0) as u16;
    let tw = measure_text(&txt, font.as_ref(), size, 1.0).width;
    ui::label(font, &txt, (sw - tw) / 2.0, oy + mark + size as f32 * 1.6, size, Color::from_rgba(216, 216, 216, 255));
}

async fn splash(logo: &Option<Texture2D>, ring: &[(f32, f32)], pct: f32, font: &Option<Font>) {
    draw_splash(logo, ring, pct, font);
    next_frame().await;
}

fn new_game(cur: usize) -> (World, Sim, MapCamera) {
    let world = World::load(MAPS[cur].data, MAPS[cur].ocean).expect("failed to load map");
    let seed = (macroquad::miniquad::date::now() * 1000.0) as u32;
    let sim = Sim::new(&world, seed);
    let cam = MapCamera::new(world.w(), world.h());
    (world, sim, cam)
}

/// Headless self-test used by CI: ATLAS_AUTOTEST=<seconds> plays a scripted game and exits.
fn autotest_seconds() -> Option<f64> {
    std::env::var("ATLAS_AUTOTEST").ok().and_then(|v| v.parse().ok())
}

fn autotest_setup(sim: &mut Sim, world: &World) {
    // pick a player with a neighbour, declare war, commit troops; AI wars run on their own
    let n = sim.n;
    for a in 1..n {
        for b in 1..n {
            if a != b && sim.are_adjacent(a as u16, b as u16) && sim.cells[a] > 400 {
                sim.player = Some(a as u16);
                sim.paused = false;
                sim.speed = 8.0;
                let ok = sim.declare_war(a as u16, b as u16);
                let sent = sim.commit_fraction(a as u16, b as u16, 0.5);
                log(&format!("autotest: player={} vs {} declared={} sent={}", world.name(a as u16), world.name(b as u16), ok, sent));
                return;
            }
        }
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut cfg = Config::load();
    let auto = autotest_seconds();
    log("window created");
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/extracted/fonts/Fredoka-600.ttf")).ok();
    if font.is_none() {
        log("warning: embedded font failed to load, using default font");
    }
    let logo = Image::from_file_with_format(LOGO_PNG, None).ok().map(|img| {
        let t = Texture2D::from_image(&img);
        t.set_filter(FilterMode::Nearest);
        t
    });
    let ring = ring_points();
    log(&format!("splash assets ready (logo={}, ring points={})", logo.is_some(), ring.len()));
    splash(&logo, &ring, 0.10, &font).await;

    let ocean_bg = Color::from_rgba(58, 111, 143, 255); // CONFIG.colors.OCEAN
    let mut cur = 0usize;
    let world0 = World::load(MAPS[cur].data, MAPS[cur].ocean).expect("failed to load map");
    log("map loaded");
    splash(&logo, &ring, 0.55, &font).await;
    let seed = (macroquad::miniquad::date::now() * 1000.0) as u32;
    let sim0 = Sim::new(&world0, seed);
    let cam0 = MapCamera::new(world0.w(), world0.h());
    log("sim ready");
    splash(&logo, &ring, 0.90, &font).await;
    splash(&logo, &ring, 1.00, &font).await;
    let (mut world, mut sim, mut cam) = (world0, sim0, cam0);

    let mut ui = Ui::default();
    ui.scale_mult = cfg.ui_scale;
    let mut last_view = Vec2::ZERO;
    let mut first_frame = true;
    let mut auto_started = false;
    let mut last_report = 0.0f64;
    let t_start = get_time();

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
            let scale = ui.scale_mult;
            ui = Ui::default();
            ui.scale_mult = scale;
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

        if auto.is_some() && !auto_started {
            auto_started = true;
            ui.observer = true;
            autotest_setup(&mut sim, &world);
        }
        sim.step(dt, &mut world);
        for b in sim.dirty.drain(..) {
            world.refresh_box(b);
        }
        world.flush();

        clear_background(ocean_bg);
        world.draw(&cam);
        ui.frame(&font, &mut sim, &world, &mut cam, tap);
        draw_text(&format!("{} FPS", get_fps()), 10.0, 70.0, 16.0, Color::new(1.0, 1.0, 1.0, 0.5));

        if ui.scale_changed {
            ui.scale_changed = false;
            cfg.ui_scale = ui.scale_mult;
            cfg.save();
        }

        if let Some(secs) = auto {
            let el = get_time() - t_start;
            if el - last_report >= 5.0 {
                last_report = el;
                let wars = (1..sim.n).flat_map(|a| (a + 1..sim.n).map(move |b| (a, b))).filter(|&(a, b)| sim.is_at_war(a as u16, b as u16)).count();
                log(&format!("autotest t={:.0}s fps={} days={:.0} fronts={} wars={}", el, get_fps(), sim.game_days, sim.fronts.len(), wars));
            }
            if el >= secs {
                let mut rank: Vec<usize> = (1..sim.n).collect();
                rank.sort_by(|&a, &b| sim.cells[b].cmp(&sim.cells[a]));
                let top: Vec<String> = rank.iter().take(5).map(|&i| format!("{}={}", sim.names[i], sim.cells[i])).collect();
                log(&format!("autotest done: days={:.0} top5=[{}]", sim.game_days, top.join(", ")));
                std::process::exit(0);
            }
        }

        next_frame().await;

        if first_frame {
            first_frame = false;
            log("first frame rendered OK");
        }
        if cfg.fps_cap > 0 {
            let target = 1.0 / cfg.fps_cap as f64;
            let spent = get_time() - frame_start;
            if spent < target {
                std::thread::sleep(std::time::Duration::from_secs_f64(target - spent));
            }
        }
    }
}
