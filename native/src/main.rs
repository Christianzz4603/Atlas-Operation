#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod camera;
mod config;
mod econ;
mod flags;
mod input;
mod mapdata;
mod restex;
mod sim;
mod ui;
mod world;

use app::{App, MAPS};
use config::{log, Config};
use macroquad::prelude::*;
use world::World;

static LOGO_PNG: &[u8] = include_bytes!("../assets/ui/logo_small.png");
static RING_PATH: &str = include_str!("../assets/ui/ring_path.txt");

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
    let mut pending: Option<f32> = None;
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
    let mark = (sw.min(sh) * 0.58).min(900.0);
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
    let size = (sw.min(sh) / 720.0 * 22.0).clamp(16.0, 44.0) as u16;
    let tw = measure_text(&txt, font.as_ref(), size, 1.0).width;
    let (tx, ty) = ((sw - tw) / 2.0, oy + mark + size as f32 * 1.6);
    let c = Color::from_rgba(216, 216, 216, 255);
    match font {
        Some(f) => {
            draw_text_ex(&txt, tx, ty, TextParams { font: Some(f), font_size: size, color: c, ..Default::default() });
        }
        None => {
            draw_text(&txt, tx, ty, size as f32, c);
        }
    }
}

async fn splash(logo: &Option<Texture2D>, ring: &[(f32, f32)], pct: f32, font: &Option<Font>) {
    draw_splash(logo, ring, pct, font);
    next_frame().await;
}

#[macroquad::main(window_conf)]
async fn main() {
    let cfg = Config::load();
    log("window created");
    let display = load_ttf_font_from_bytes(include_bytes!("../assets/extracted/fonts/Fredoka-600.ttf")).ok();
    let body = load_ttf_font_from_bytes(include_bytes!("../assets/extracted/fonts/Rubik-500.ttf")).ok();
    if display.is_none() || body.is_none() {
        log("warning: an embedded font failed to load, falling back to the default font");
    }
    let logo = Image::from_file_with_format(LOGO_PNG, None).ok().map(|img| {
        let t = Texture2D::from_image(&img);
        t.set_filter(FilterMode::Nearest);
        t
    });
    let ring = ring_points();
    log(&format!("splash assets ready (logo={}, ring points={})", logo.is_some(), ring.len()));
    splash(&logo, &ring, 0.08, &display).await;

    let w0 = World::load(MAPS[0].data, MAPS[0].ocean).expect("failed to load map 0");
    log("map 0 loaded");
    splash(&logo, &ring, 0.45, &display).await;
    let w1 = World::load(MAPS[1].data, MAPS[1].ocean).expect("failed to load map 1");
    log("map 1 loaded");
    splash(&logo, &ring, 0.85, &display).await;
    splash(&logo, &ring, 1.0, &display).await;

    let fps_cap = cfg.fps_cap;
    let mut app = App::new(vec![w0, w1], display, body, logo.clone(), cfg);
    log("app ready");
    let mut first_frame = true;

    loop {
        let frame_start = get_time();
        let dt = get_frame_time().min(0.1);
        clear_background(Color::from_rgba(20, 26, 33, 255));
        app.frame(dt);
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
