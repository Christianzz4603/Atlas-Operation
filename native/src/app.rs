//! Screens: Menu -> Map select -> Nation select -> Match (HUD), plus Settings and Pause menus.
//! Mirrors the HTML game's shell flow (menu-screen, browser-screen, nation-screen, in-match HUD).
use crate::camera::MapCamera;
use crate::config::{log, Config};
use crate::input::Input;
use crate::econ::{Special, CATALOG, IDX_AIRDEF, RARITY, Stock};
use crate::ideology;
use crate::sim::{air_defense_range, flight_pos, Sim};
use crate::audio::{Audio, Sfx};
use crate::ui::*;
use crate::world::World;
use macroquad::prelude::*;

pub struct MapAsset {
    pub name: &'static str,
    pub sub: &'static str,
    pub data: &'static [u8],
    pub ocean: &'static [u8],
}

pub static MAPS: [MapAsset; 2] = [
    MapAsset {
        name: "2025 Map (Classic)",
        sub: "Europe and neighbours",
        data: include_bytes!("../assets/maps/europe-2025.atlasmap.z"),
        ocean: include_bytes!("../assets/maps/europe-2025_ocean.png"),
    },
    MapAsset {
        name: "Afro-Eurasia",
        sub: "Africa, Europe and Asia",
        data: include_bytes!("../assets/maps/afro-eurasia.atlasmap.z"),
        ocean: include_bytes!("../assets/maps/afro-eurasia_ocean.png"),
    },
];

const SPEEDS: [f32; 5] = [0.5, 1.0, 2.0, 4.0, 8.0];

pub fn fmt_num(v: f32) -> String {
    if v >= 1.0e6 {
        format!("{:.1}M", v / 1.0e6)
    } else if v >= 1.0e3 {
        format!("{:.0}K", v / 1.0e3)
    } else {
        format!("{:.0}", v)
    }
}

/// GDP is stored in $B.
fn fmt_money(b: f32) -> String {
    if b >= 1000.0 {
        format!("${:.2}T", b / 1000.0)
    } else {
        format!("${:.0}B", b)
    }
}

/// In-game date: calendar starts 2025-01-01, 1 real second = 1 game day (at 1x).
fn date_string(days: f32) -> String {
    let z = 20089 + days.floor() as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m, d)
}

const RES_UNIT: [&str; 6] = ["B$", "kt bricks", "kt food", "kt steel", "kt oil", "kt uranium"];

fn is_coastal(w: &World, i: usize) -> bool {
    let m = &w.map;
    let (mw, mh) = (m.w, m.h);
    let (x, y) = (i % mw, i / mw);
    (x > 0 && m.land[i - 1] == 0) || (x + 1 < mw && m.land[i + 1] == 0) || (y > 0 && m.land[i - mw] == 0) || (y + 1 < mh && m.land[i + mw] == 0)
}

/// _navalLandingCell: the tapped cell if it is the defender's land, else the nearest coastal cell.
fn landing_cell(w: &World, def: u16, cx: i32, cy: i32, scale: f32) -> Option<usize> {
    let m = &w.map;
    let (mw, mh) = (m.w as i32, m.h as i32);
    if cx >= 0 && cy >= 0 && cx < mw && cy < mh {
        let i = (cy * mw + cx) as usize;
        if m.land[i] == 1 && m.owner[i] == def {
            return Some(i);
        }
    }
    let radius = ((30.0 / scale.max(0.5)).ceil() as i32).clamp(3, 40);
    let mut best: Option<usize> = None;
    let mut bd = i64::MAX;
    for y in (cy - radius).max(0)..=(cy + radius).min(mh - 1) {
        for x in (cx - radius).max(0)..=(cx + radius).min(mw - 1) {
            let i = (y * mw + x) as usize;
            if m.owner[i] != def || m.land[i] != 1 || !is_coastal(w, i) {
                continue;
            }
            let d = ((x - cx) as i64).pow(2) + ((y - cy) as i64).pow(2);
            if d < bd {
                bd = d;
                best = Some(i);
            }
        }
    }
    best
}

fn any_cell_of(w: &World, def: u16) -> Option<usize> {
    let m = &w.map;
    (0..m.owner.len()).find(|&i| m.owner[i] == def && m.land[i] == 1)
}

fn any_coastal_cell(w: &World, def: u16) -> Option<usize> {
    let m = &w.map;
    (0..m.owner.len()).find(|&i| m.owner[i] == def && m.land[i] == 1 && is_coastal(w, i))
}

fn fmt_cost(c: &Stock) -> String {
    let mut parts: Vec<String> = Vec::new();
    if c[0] > 0.0 {
        parts.push(format!("${:.0}B", c[0]));
    }
    for r in 1..6 {
        if c[r] > 0.0 {
            parts.push(format!("{:.0} {}", c[r], &RES_UNIT[r][3..]));
        }
    }
    parts.join("  ")
}

/// "Money +10%   Production -5%" for multiplier keys from..to (rounded, no "-0").
fn fmt_mods(m: &ideology::Mods, from: usize, to: usize) -> String {
    (from..to)
        .map(|k| {
            let v = ((m[k] - 1.0) * 100.0).round();
            let v = if v == 0.0 { 0.0 } else { v };
            format!("{} {:+.0}%", ideology::KEY_LABELS[k], v)
        })
        .collect::<Vec<_>>()
        .join("   ")
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Menu,
    Maps,
    Nations,
    Playing,
}

enum PlayAction {
    None,
    Restart,
    Leave,
}

struct Play {
    world: World,
    sim: Sim,
    cam: MapCamera,
    map_idx: usize,
    nation: u16,
    target: Option<u16>,
    send_frac: f32,
    slider_drag: bool,
    menu_open: bool,
    last_view: Vec2,
    naval_mode: bool,
    naval_cell: Option<usize>,
    naval_boats: u32,
    missile_mode: bool,
    focus_mode: bool,
    missile_target: Option<(i32, i32)>,
    build_open: bool,
    regime_open: bool,
    empire_open: bool,
    regime_sel: Option<usize>,
    placing: Option<usize>,
    sel_building: Option<u32>,
    overlay: Texture2D,
}

fn make_play(map_idx: usize, nation: u16) -> Play {
    let world = World::load(MAPS[map_idx].data, MAPS[map_idx].ocean).expect("failed to load map");
    let seed = (macroquad::miniquad::date::now() * 1000.0) as u32;
    let mut sim = Sim::new(&world, seed);
    if std::env::var("CW_UITEST").is_ok() {
        // the scripted UI test needs spare materials for ports etc.
        sim.econ.stock[nation as usize] = [2000.0, 700.0, 100.0, 700.0, 50.0, 5.0];
        sim.force_regime(nation as usize, 0); // liberal democracy, so the scripted switch to technocracy is legal
    }
    sim.player = Some(nation);
    sim.paused = false;
    let cam = MapCamera::new(world.w(), world.h());
    let mut img = Image::gen_image_color(world.map.w as u16, world.map.h as u16, Color::new(0.0, 0.0, 0.0, 0.0));
    sim.econ.fill_overlay(&mut img.bytes);
    let overlay = Texture2D::from_image(&img);
    overlay.set_filter(FilterMode::Nearest);
    Play { world, sim, cam, map_idx, nation, target: None, send_frac: 0.25, slider_drag: false, menu_open: false, last_view: Vec2::ZERO, naval_mode: false, naval_cell: None, naval_boats: 3, missile_mode: false, missile_target: None, focus_mode: false, build_open: false, regime_open: false, regime_sel: None, empire_open: false, placing: None, sel_building: None, overlay }
}

pub struct App {
    pub gui: Gui,
    input: Input,
    pub cfg: Config,
    worlds: Vec<World>,
    cur_map: usize,
    screen: Screen,
    selected: Option<u16>,
    nation_order: Vec<u16>,
    nation_scroll: Scroll,
    maps_scroll: Scroll,
    settings_open: bool,
    play: Option<Play>,
    pub audio: Audio,
    // CI self tests
    autotest: Option<f64>,
    uitest: Option<f64>,
    t0: f64,
    last_report: f64,
}

impl App {
    pub fn new(worlds: Vec<World>, display: Option<Font>, body: Option<Font>, logo: Option<Texture2D>, cfg: Config) -> App {
        let gui = Gui::new(display, body, logo, cfg.ui_scale);
        let autotest = std::env::var("CW_AUTOTEST").ok().and_then(|v| v.parse::<f64>().ok());
        let uitest = std::env::var("CW_UITEST").ok().and_then(|v| v.parse::<f64>().ok());
        let mut app = App {
            gui,
            input: Input::new(),
            cfg,
            worlds,
            cur_map: 0,
            screen: Screen::Menu,
            selected: None,
            nation_order: Vec::new(),
            nation_scroll: Scroll::default(),
            maps_scroll: Scroll::default(),
            settings_open: false,
            play: None,
            audio: Audio::new(),
            autotest,
            uitest,
            t0: get_time(),
            last_report: 0.0,
        };
        if app.autotest.is_some() {
            app.start_autotest();
        }
        if app.uitest.is_some() {
            // walk through every screen and press every button
            for id in [
                "New Game", "map0", "nation:Germany", "Deploy", "BUILD", "build:farm", "place:auto", "BUILD", "build:port", "place:coast",
                "select:neighbor", "WAR", "NAVAL", "naval:tap", "SEND", "BUILD", "build:missile-silo", "place:auto", "BUILD", "build:air-defense", "place:auto", "MISSILE", "missile:tap", "LAUNCH", "SUPPORT", "pause", "Settings", "UI+", "UI-", "Close", "Resume",
                "speed+", "speed+", "speed-", "zoom+", "zoom-", "STOP", "PEACE", "card-close", "select:other", "ALLY", "card-close",
                "select:building", "UPGRADE", "DISMANTLE", "bcard-close", "BUILD", "build-close", "REGIME", "regime:technocracy", "ADOPT", "pol+:1", "pol-:1", "regime-close", "EMPIRE_BTN", "empire-close", "pause", "Restart", "pause",
                "Leave match", "Settings", "Close", "New Game", "back",
            ] {
                app.gui.auto.push_back(id.to_string());
            }
        }
        app
    }

    fn start_autotest(&mut self) {
        let probe = World::load(MAPS[0].data, MAPS[0].ocean).expect("probe map");
        let s = Sim::new(&probe, 1);
        for a in 1..s.n {
            if s.cells[a] < 400 {
                continue;
            }
            for b in 1..s.n {
                if a != b && s.are_adjacent(a as u16, b as u16) {
                    let mut pl = make_play(0, a as u16);
                    pl.sim.speed = 8.0;
                    pl.sim.econ.stock[a][2] += 50.0; // food so the scripted war is allowed
                    let ok = pl.sim.declare_war(a as u16, b as u16);
                    let sent = pl.sim.commit_fraction(a as u16, b as u16, 0.5);
                    log(&format!("autotest: player={} vs {} declared={} sent={}", probe.name(a as u16), probe.name(b as u16), ok, sent));
                    self.play = Some(pl);
                    self.screen = Screen::Playing;
                    return;
                }
            }
        }
    }

    pub fn frame(&mut self, dt: f32) {
        let ptr = self.input.poll();
        self.gui.begin(ptr, dt);
        match self.screen {
            Screen::Menu => self.menu_screen(),
            Screen::Maps => self.maps_screen(dt),
            Screen::Nations => self.nations_screen(dt),
            Screen::Playing => self.playing_screen(dt),
        }
        if self.settings_open {
            self.settings_modal();
        }
        self.gui.end();
        if self.gui.click_sound_pending {
            self.gui.click_sound_pending = false;
            self.audio.play(Sfx::UiClick);
        }
        self.selftest_tick();
    }

    fn selftest_tick(&mut self) {
        let el = get_time() - self.t0;
        if let Some(secs) = self.autotest {
            if el - self.last_report >= 5.0 {
                self.last_report = el;
                if let Some(pl) = &self.play {
                    let sim = &pl.sim;
                    let wars = (1..sim.n).flat_map(|a| (a + 1..sim.n).map(move |b| (a, b))).filter(|&(a, b)| sim.is_at_war(a as u16, b as u16)).count();
                    log(&format!("autotest t={:.0}s fps={} days={:.0} fronts={} wars={} crowns={} annexed={} empires={}", el, get_fps(), sim.game_days, sim.fronts.len(), wars, sim.crowns.iter().skip(1).sum::<u32>(), sim.stat_annexed, sim.empire.iter().filter(|&&e| e).count()));
                }
            }
            if el >= secs {
                if let Some(pl) = &self.play {
                    let sim = &pl.sim;
                    let mut rank: Vec<usize> = (1..sim.n).collect();
                    rank.sort_by(|&a, &b| sim.cells[b].cmp(&sim.cells[a]));
                    let top: Vec<String> = rank.iter().take(5).map(|&i| format!("{}={}", sim.names[i], sim.cells[i])).collect();
                    log(&format!("autotest done: days={:.0} top5=[{}]", sim.game_days, top.join(", ")));
                }
                std::process::exit(0);
            }
        }
        if let Some(secs) = self.uitest {
            if el >= secs {
                log(&format!("uitest done: screen_ok remaining_steps={}", self.gui.auto.len()));
                std::process::exit(0);
            }
        }
    }

    // ------------------------------------------------------------------ settings modal (shared)
    fn settings_modal(&mut self) {
        let g = &mut self.gui;
        g.enabled = true;
        let (sw, sh, u) = (g.sw, g.sh, g.u);
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.62));
        g.block(Rect::new(0.0, 0.0, sw, sh));
        let w = (sw * 0.92).min(480.0 * u);
        let h = 250.0 * u;
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        g.panel(r, c_panel());
        g.text("Settings", r.x + 22.0 * u, r.y + 46.0 * u, 28.0, c_text());
        g.text("Interface size", r.x + 22.0 * u, r.y + 108.0 * u, 20.0, c_dim());
        let bs = 46.0 * u;
        let by = r.y + 78.0 * u + 4.0 * u;
        let plus = Rect::new(r.x + r.w - 22.0 * u - bs, by + 36.0 * u, bs, bs);
        let lbl_w = 84.0 * u;
        let minus = Rect::new(plus.x - lbl_w - bs - 8.0 * u, plus.y, bs, bs);
        let mut scale = g.user_scale;
        if g.glyph_button("UI-", minus, Glyph::Minus, Style::Secondary, true) {
            scale = (scale / 1.15).max(0.6);
        }
        g.text_c(&format!("{:.0}%", g.user_scale * 100.0), minus.x + bs + (plus.x - minus.x - bs) / 2.0, plus.y + bs / 2.0 + 7.0 * u, 22.0, c_text());
        if g.glyph_button("UI+", plus, Glyph::Plus, Style::Secondary, true) {
            scale = (scale * 1.15).min(2.5);
        }
        let close = Rect::new(r.x + 22.0 * u, r.y + h - 22.0 * u - 54.0 * u, w - 44.0 * u, 54.0 * u);
        let closed = g.button("Close", close, "Close", Style::Primary, true);
        if (scale - g.user_scale).abs() > 1e-4 {
            g.user_scale = scale;
            self.cfg.ui_scale = scale;
            self.cfg.save();
        }
        if closed {
            self.settings_open = false;
        }
    }

    // ------------------------------------------------------------------ menu
    fn menu_screen(&mut self) {
        let modal = self.settings_open;
        let g = &mut self.gui;
        g.enabled = !modal;
        let (sw, sh, u) = (g.sw, g.sh, g.u);
        // blurred-looking world as backdrop + shade (menu-map-bg / menu-map-shade)
        let w = &self.worlds[0];
        let sc = (sw / w.w()).max(sh / w.h());
        let (dw, dh) = (w.w() * sc, w.h() * sc);
        w.draw_rect((sw - dw) / 2.0, (sh - dh) / 2.0, dw, dh, Color::new(0.8, 0.84, 0.92, 1.0));
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.04, 0.06, 0.09, 0.62));

        let ls = (sh * 0.34).min(sw * 0.6);
        let ly = sh * 0.05;
        g.draw_logo((sw - ls) / 2.0, ly, ls);
        let bw = (sw * 0.92).min(420.0 * u);
        let bh = 56.0 * u;
        let gap = 14.0 * u;
        let x = (sw - bw) / 2.0;
        let mut y = ly + ls + 20.0 * u;
        let half = (bw - gap) / 2.0;
        let mut go = false;
        if g.button("New Game", Rect::new(x, y, half, bh), "New Game", Style::Primary, true) {
            go = true;
        }
        g.button("Online", Rect::new(x + half + gap, y, half, bh), "Online", Style::Secondary, false);
        g.text_r("SOON", x + bw - 10.0 * u, y + 17.0 * u, 11.0, c_accent());
        y += bh + gap;
        g.button("Load Map", Rect::new(x, y, bw, bh), "Load Map", Style::Secondary, false);
        g.text_r("SOON", x + bw - 10.0 * u, y + 17.0 * u, 11.0, c_accent());
        y += bh + gap;
        let mut settings = false;
        if g.button("Settings", Rect::new(x, y, bw, bh), "Settings", Style::Secondary, true) {
            settings = true;
        }
        g.text_c("Country Warzone - native build", sw / 2.0, sh - 14.0 * u, 13.0, c_faint());
        if go {
            self.maps_scroll = Scroll::default();
            self.screen = Screen::Maps;
        }
        if settings {
            self.settings_open = true;
        }
    }

    // ------------------------------------------------------------------ map select
    fn maps_screen(&mut self, dt: f32) {
        let modal = self.settings_open;
        let g = &mut self.gui;
        g.enabled = !modal;
        g.screen_bg();
        let (sw, sh, u) = (g.sw, g.sh, g.u);
        let head = 66.0 * u;
        let m = 16.0 * u;
        let cols = (((sw - m) / (300.0 * u)).floor() as usize).clamp(1, 4);
        let cw = (sw - m * (cols as f32 + 1.0)) / cols as f32;
        let ch = cw * 0.82;
        let rows = (MAPS.len() + cols - 1) / cols;
        let content_h = m + rows as f32 * (ch + m);
        let view = Rect::new(0.0, head, sw, sh - head);
        g.scroll(&mut self.maps_scroll, view, content_h, dt);
        let mut pick: Option<usize> = None;
        for (i, ma) in MAPS.iter().enumerate() {
            let (r, c) = (i / cols, i % cols);
            let x = m + c as f32 * (cw + m);
            let y = head + m + r as f32 * (ch + m) - self.maps_scroll.y;
            if y + ch < head || y > sh {
                continue;
            }
            g.rrect(x, y, cw, ch, 14.0 * u, c_panel());
            let tw = cw - 20.0 * u;
            let th = ch * 0.66;
            let wd = &self.worlds[i];
            let k = (tw / wd.w()).min(th / wd.h());
            let (dw, dh) = (wd.w() * k, wd.h() * k);
            wd.draw_rect(x + 10.0 * u + (tw - dw) / 2.0, y + 10.0 * u + (th - dh) / 2.0, dw, dh, WHITE);
            g.text(ma.name, x + 14.0 * u, y + ch - 44.0 * u, 20.0, c_text());
            g.text(ma.sub, x + 14.0 * u, y + ch - 18.0 * u, 14.0, c_dim());
            if let Some(vis) = Rect::new(x, y, cw, ch).intersect(view) {
                if g.tapped(&format!("map{}", i), vis) {
                    pick = Some(i);
                }
            }
        }
        let back = g.header("Select map");
        if back {
            self.screen = Screen::Menu;
        }
        if let Some(i) = pick {
            self.cur_map = i;
            self.enter_nations();
        }
    }

    fn enter_nations(&mut self) {
        let w = &self.worlds[self.cur_map];
        let mut ids: Vec<u16> = (1..w.map.countries.len() as u16).collect();
        ids.sort_by(|&a, &b| w.map.countries[a as usize].name.cmp(&w.map.countries[b as usize].name));
        self.nation_order = ids;
        self.selected = None;
        self.nation_scroll = Scroll::default();
        self.screen = Screen::Nations;
    }

    // ------------------------------------------------------------------ nation select
    fn nations_screen(&mut self, dt: f32) {
        let modal = self.settings_open;
        let g = &mut self.gui;
        g.enabled = !modal;
        g.screen_bg();
        let (sw, sh, u) = (g.sw, g.sh, g.u);
        let head = 66.0 * u;
        let foot = 92.0 * u;
        let m = 12.0 * u;
        let cols = (((sw - m) / (150.0 * u)).floor() as usize).clamp(2, 9);
        let cw = (sw - m * (cols as f32 + 1.0)) / cols as f32;
        let ch = cw * 0.86;
        let n = self.nation_order.len();
        let rows = (n + cols - 1) / cols;
        let content_h = m + rows as f32 * (ch + m);
        let view = Rect::new(0.0, head, sw, sh - head - foot);
        g.scroll(&mut self.nation_scroll, view, content_h, dt);
        let world = &self.worlds[self.cur_map];
        let mut picked: Option<u16> = None;
        for (k, &id) in self.nation_order.iter().enumerate() {
            let (r, c) = (k / cols, k % cols);
            let x = m + c as f32 * (cw + m);
            let y = head + m + r as f32 * (ch + m) - self.nation_scroll.y;
            if y + ch < head || y > sh - foot {
                continue;
            }
            let country = &world.map.countries[id as usize];
            let sel = self.selected == Some(id);
            if sel {
                g.rrect(x - 3.0 * u, y - 3.0 * u, cw + 6.0 * u, ch + 6.0 * u, 14.0 * u, c_accent());
            }
            g.rrect(x, y, cw, ch, 12.0 * u, if sel { c_panel2() } else { c_panel() });
            let fr = Rect::new(x + 10.0 * u, y + 10.0 * u, cw - 20.0 * u, ch * 0.56);
            if !g.flag(&country.iso, fr) {
                let sw_c = country.rgb;
                draw_rectangle(fr.x + fr.w * 0.2, fr.y + fr.h * 0.15, fr.w * 0.6, fr.h * 0.7, col(sw_c[0], sw_c[1], sw_c[2]));
            }
            g.text_in(&country.name, Rect::new(x, y + ch * 0.64, cw, ch * 0.32), 15.0, c_text());
            if let Some(vis) = Rect::new(x, y, cw, ch).intersect(view) {
                if g.tapped(&format!("nation:{}", country.name), vis) {
                    picked = Some(id);
                }
            }
        }
        if picked.is_some() {
            self.selected = picked;
        }
        // footer: selected nation + Deploy
        let g = &mut self.gui;
        let fr = Rect::new(0.0, sh - foot, sw, foot);
        draw_rectangle(fr.x, fr.y, fr.w, fr.h, c_panel());
        draw_rectangle(0.0, fr.y, sw, 1.0, c_line());
        g.block(fr);
        let mut deploy = false;
        let bw = (sw * 0.42).clamp(150.0 * u, 280.0 * u);
        let b = Rect::new(sw - m - bw, fr.y + 14.0 * u, bw, foot - 28.0 * u);
        if g.button("Deploy", b, "Deploy", Style::Primary, self.selected.is_some()) {
            deploy = true;
        }
        match self.selected {
            Some(id) => {
                let c = &self.worlds[self.cur_map].map.countries[id as usize];
                let fr2 = Rect::new(m, fr.y + 18.0 * u, 56.0 * u, foot - 36.0 * u);
                g.flag(&c.iso, fr2);
                g.text(&c.name, m + 66.0 * u, fr.y + foot / 2.0 - 2.0 * u, 20.0, c_text());
                g.text(&format!("Pop {:.1}M  -  Army {}K", c.pop, c.military.round()), m + 66.0 * u, fr.y + foot / 2.0 + 20.0 * u, 13.0, c_dim());
            }
            None => g.text("Choose your nation", m, fr.y + foot / 2.0 + 6.0 * u, 18.0, c_dim()),
        }
        let back = g.header("Select nation");
        if back {
            self.screen = Screen::Maps;
        }
        if deploy {
            if let Some(id) = self.selected {
                log(&format!("deploy: map={} nation={}", self.cur_map, id));
                self.play = Some(make_play(self.cur_map, id));
                self.screen = Screen::Playing;
            }
        }
    }

    // ------------------------------------------------------------------ match
    fn playing_screen(&mut self, dt: f32) {
        let Some(mut pl) = self.play.take() else {
            self.screen = Screen::Menu;
            return;
        };
        match self.playing_inner(&mut pl, dt) {
            PlayAction::None => self.play = Some(pl),
            PlayAction::Restart => {
                let np = make_play(pl.map_idx, pl.nation);
                self.play = Some(np);
            }
            PlayAction::Leave => {
                self.screen = Screen::Menu;
            }
        }
    }

    fn playing_inner(&mut self, pl: &mut Play, dt: f32) -> PlayAction {
        let mut action = PlayAction::None;
        let settings_open = self.settings_open;
        let g = &mut self.gui;
        let ptr = g.ptr;
        let (sw, sh, u) = (g.sw, g.sh, g.u);
        let p = pl.nation;
        let pi = p as usize;

        let view = vec2(sw, sh);
        if view != pl.last_view {
            pl.cam.resize(view);
            pl.last_view = view;
        }
        let modal = pl.menu_open || settings_open;
        if !modal {
            let over = g.was_over_ui(ptr.pos);
            pl.cam.update(dt, &ptr, over);
        }
        pl.sim.paused = pl.menu_open;
        pl.sim.step(dt, &mut pl.world);
        for e in pl.sim.sfx_queue.drain(..) {
            self.audio.play(sfx_for_event(e));
        }
        for b in pl.sim.dirty.drain(..) {
            pl.world.refresh_box(b);
        }
        pl.world.flush();

        if g.auto_take("place:auto") {
            if let Some(k) = pl.placing {
                if let Some((gx, gy)) = pl.sim.econ_any_spot(&pl.world.map, p) {
                    if pl.sim.econ.place(&pl.world.map, gx, gy, p, k).is_ok() {
                        pl.placing = None;
                    }
                }
            }
        }
        if g.auto_take("select:building") {
            pl.sel_building = pl.sim.econ.buildings.iter().find(|b| b.country == p).map(|b| b.id);
            pl.target = None;
        }
        if g.auto_take("select:other") {
            if let Some(n) = (1..pl.sim.n).find(|&b| b != pi && pl.sim.are_adjacent(p, b as u16) && !pl.sim.is_at_war(p, b as u16) && !pl.sim.are_allies(p, b as u16)) {
                pl.target = Some(n as u16);
            }
        }
        if g.auto_take("place:coast") {
            if let Some(k) = pl.placing {
                if let Some((gx, gy)) = pl.sim.econ_any_coastal_spot(&pl.world.map, p) {
                    if pl.sim.econ.place(&pl.world.map, gx, gy, p, k).is_ok() {
                        pl.placing = None;
                    }
                }
            }
        }
        if g.auto_take("missile:tap") {
            if let Some(t) = pl.target {
                if let Some(c) = any_cell_of(&pl.world, t) {
                    let mw = pl.world.map.w;
                    pl.missile_target = Some(((c % mw) as i32, (c / mw) as i32));
                }
            }
        }
        if g.auto_take("naval:tap") {
            if let Some(t) = pl.target {
                if let Some(c) = any_coastal_cell(&pl.world, t) {
                    pl.naval_cell = Some(c);
                    pl.naval_mode = false;
                }
            }
        }
        if g.auto_take("select:neighbor") {
            let pick = (1..pl.sim.n)
                .find(|&b| b != pi && pl.sim.are_adjacent(p, b as u16) && any_coastal_cell(&pl.world, b as u16).is_some())
                .or_else(|| (1..pl.sim.n).find(|&b| b != pi && pl.sim.are_adjacent(p, b as u16)));
            if let Some(n) = pick {
                pl.target = Some(n as u16);
            }
        }

        clear_background(Color::from_rgba(58, 111, 143, 255));
        pl.world.draw(&pl.cam);
        if pl.placing.is_some() {
            let (mw, mh) = (pl.world.w() * pl.cam.scale, pl.world.h() * pl.cam.scale);
            draw_texture_ex(&pl.overlay, pl.cam.offset.x.round(), pl.cam.offset.y.round(), WHITE, DrawTextureParams { dest_size: Some(vec2(mw, mh)), ..Default::default() });
        }
        for b in &pl.sim.econ.buildings {
            let cat = &CATALOG[b.kind];
            let s = (4.0 * pl.cam.scale).max(4.0);
            let x = pl.cam.offset.x + b.gx as f32 * pl.cam.scale;
            let y = pl.cam.offset.y + b.gy as f32 * pl.cam.scale;
            if x > sw || y > sh || x + s < 0.0 || y + s < 0.0 {
                continue;
            }
            let mine = b.country == p;
            draw_rectangle(x - 1.0, y - 1.0, s + 2.0, s + 2.0, if mine { Color::new(1.0, 1.0, 1.0, 0.95) } else { Color::new(0.05, 0.07, 0.09, 0.9) });
            draw_rectangle(x, y, s, s, col(cat.color[0], cat.color[1], cat.color[2]));
            if s >= 12.0 {
                for l in 0..b.level {
                    draw_rectangle(x + 2.0 + l as f32 * s * 0.22, y + s - s * 0.22, s * 0.16, s * 0.16, Color::new(0.05, 0.07, 0.09, 0.95));
                }
            }
        }
        if let Some(k) = pl.placing {
            let (gcx, gcy) = pl.cam.screen_to_cell(ptr.pos);
            let (gx, gy) = (gcx - 2, gcy - 2);
            let ok = pl.sim.econ.can_place(&pl.world.map, gx, gy, p, Some(k));
            let s = 4.0 * pl.cam.scale;
            let x = pl.cam.offset.x + gx as f32 * pl.cam.scale;
            let y = pl.cam.offset.y + gy as f32 * pl.cam.scale;
            draw_rectangle(x, y, s, s, if ok { Color::new(0.3, 0.9, 0.4, 0.6) } else { Color::new(0.9, 0.3, 0.3, 0.6) });
            let cc = CATALOG[k].color;
            draw_rectangle_lines(x, y, s, s, 2.0, col(cc[0], cc[1], cc[2]));
        }

        for f in &pl.sim.fronts {
            let Some(nv) = &f.naval else { continue };
            let ac = pl.world.map.countries[f.attacker as usize].rgb;
            let mw = pl.world.map.w;
            let cell_px = |i: usize| -> (f32, f32) {
                (pl.cam.offset.x + ((i % mw) as f32 + 0.5) * pl.cam.scale, pl.cam.offset.y + ((i / mw) as f32 + 0.5) * pl.cam.scale)
            };
            let bs = (7.5 * pl.cam.scale).max(8.0);
            if nv.boats.is_empty() && nv.pending > 0 {
                let (ex, ey) = cell_px(nv.embark);
                draw_rectangle(ex - bs * 0.6, ey - bs * 1.1, bs * 1.2, 4.0, Color::new(0.0, 0.0, 0.0, 0.6));
                draw_rectangle(ex - bs * 0.6, ey - bs * 1.1, bs * 1.2 * nv.load, 4.0, WHITE);
            }
            if let Some(&t) = nv.boats.first() {
                if nv.path.len() > 1 {
                    let fi = t * (nv.path.len() - 1) as f32;
                    let i0 = (fi.floor() as usize).min(nv.path.len() - 1);
                    let i1 = (i0 + 1).min(nv.path.len() - 1);
                    let (x0, y0) = cell_px(nv.path[i0]);
                    let (x1, y1) = cell_px(nv.path[i1]);
                    let k = fi - fi.floor();
                    let (bx, by) = (x0 + (x1 - x0) * k, y0 + (y1 - y0) * k);
                    draw_rectangle(bx - bs * 0.6 - 1.5, by - bs * 0.35 - 1.5, bs * 1.2 + 3.0, bs * 0.7 + 3.0, Color::new(0.04, 0.06, 0.09, 0.95));
                    draw_rectangle(bx - bs * 0.6, by - bs * 0.35, bs * 1.2, bs * 0.7, col(ac[0], ac[1], ac[2]));
                    draw_triangle(vec2(bx + bs * 0.6, by - bs * 0.35), vec2(bx + bs * 0.6, by + bs * 0.35), vec2(bx + bs * 1.0, by), col(ac[0], ac[1], ac[2]));
                }
            }
        }

        {
            let now = pl.sim.time;
            let cam = &pl.cam;
            let to_px = |x: f32, y: f32| (cam.offset.x + x * cam.scale, cam.offset.y + y * cam.scale);
            let sc = cam.scale.max(0.6);
            for s in &pl.sim.sites {
                let age = now - s.impact_at;
                if (0.0..3.0).contains(&age) {
                    let (px, py) = to_px(s.x, s.y);
                    let t = (age / 3.0).clamp(0.0, 1.0);
                    let rr = s.radius as f32 * cam.scale;
                    draw_circle(px, py, rr * (0.3 + 0.7 * (t * 2.0).min(1.0)), Color::new(1.0, 0.65, 0.2, 0.55 * (1.0 - t)));
                    draw_circle(px, py, rr * 0.45 * (1.0 - t), Color::new(1.0, 1.0, 0.9, 0.8 * (1.0 - t)));
                    draw_circle_lines(px, py, rr * (0.4 + 0.8 * t), 2.0, Color::new(1.0, 0.9, 0.6, 0.8 * (1.0 - t)));
                }
            }
            for f in &pl.sim.flights {
                if let Some(t) = f.intercepted_at {
                    let k = ((now - t) / 2.0).clamp(0.0, 1.0);
                    let (px, py) = to_px(f.ix, f.iy);
                    draw_circle(px, py, (6.0 + 22.0 * k) * sc, Color::new(1.0, 0.85, 0.4, 0.7 * (1.0 - k)));
                } else if now < f.start_at {
                    let (px, py) = to_px(f.sx, f.sy);
                    let pulse = 0.5 + 0.5 * (now * 8.0).sin();
                    draw_circle(px, py, (5.0 + 4.0 * pulse) * sc, Color::new(1.0, 0.3, 0.2, 0.55));
                } else if !f.impact_done {
                    for k in 0..6 {
                        let (x0, y0) = flight_pos(f, now - k as f32 * 0.3);
                        let (x1, y1) = flight_pos(f, now - (k as f32 + 1.0) * 0.3);
                        let (ax, ay) = to_px(x0, y0);
                        let (bx, by) = to_px(x1, y1);
                        draw_line(ax, ay, bx, by, (3.0 - k as f32 * 0.4).max(1.0), Color::new(1.0, 0.9, 0.8, 0.8 - k as f32 * 0.13));
                    }
                    let (x, y) = flight_pos(f, now);
                    let (hx, hy) = to_px(x, y);
                    draw_circle(hx, hy, 5.0, Color::new(1.0, 0.5, 0.15, 0.6));
                    draw_circle(hx, hy, 2.8, WHITE);
                }
            }
            for it in &pl.sim.interceptors {
                match it.resolved {
                    None => {
                        let t = ((now - it.start_at) / (it.arrive_at - it.start_at).max(0.01)).clamp(0.0, 1.0);
                        let (x0, y0) = to_px(it.sx, it.sy);
                        let (x1, y1) = to_px(it.sx + (it.ax - it.sx) * t, it.sy + (it.ay - it.sy) * t);
                        draw_line(x0, y0, x1, y1, 1.5, Color::new(0.5, 0.95, 1.0, 0.7));
                        draw_circle(x1, y1, 3.0, Color::new(0.7, 1.0, 1.0, 1.0));
                    }
                    Some(true) => {
                        let k = (now - it.resolved_at).clamp(0.0, 1.0);
                        let (px, py) = to_px(it.ax, it.ay);
                        draw_circle(px, py, (5.0 + 16.0 * k) * sc, Color::new(0.6, 1.0, 1.0, 0.7 * (1.0 - k)));
                    }
                    Some(false) => {
                        let k = (now - it.resolved_at).clamp(0.0, 1.0);
                        let (px, py) = to_px(it.ax, it.ay);
                        draw_circle(px, py, (3.0 + 6.0 * k) * sc, Color::new(0.7, 0.7, 0.7, 0.4 * (1.0 - k)));
                    }
                }
            }
            if pl.missile_mode {
                if let Some((tx, ty)) = pl.missile_target {
                    let (px, py) = to_px(tx as f32 + 0.5, ty as f32 + 0.5);
                    draw_circle_lines(px, py, 17.0 * cam.scale, 2.5, Color::new(1.0, 0.25, 0.2, 0.9));
                    draw_circle(px, py, 3.0, Color::new(1.0, 0.25, 0.2, 1.0));
                }
            }
            if let Some(bid) = pl.sel_building {
                if let Some(b) = pl.sim.econ.buildings.iter().find(|b| b.id == bid && b.kind == IDX_AIRDEF) {
                    let (px, py) = to_px(b.gx as f32 + 2.0, b.gy as f32 + 2.0);
                    draw_circle_lines(px, py, air_defense_range(b.level) * cam.scale, 2.0, Color::new(0.4, 0.9, 1.0, 0.8));
                }
            }
        }

        g.enabled = !modal;
        let m = 10.0 * u;
        let btn = 46.0 * u;
        let portrait = sh > sw;

        // ---- top-left: pause + zoom
        if g.glyph_button("pause", Rect::new(m, m, btn, btn), Glyph::Pause, Style::Secondary, true) {
            pl.menu_open = true;
        }
        if g.glyph_button("zoom+", Rect::new(m, m + btn + 10.0 * u, btn, btn), Glyph::Plus, Style::Secondary, true) {
            pl.cam.zoom_center(1.6);
        }
        if g.glyph_button("zoom-", Rect::new(m, m + 2.0 * btn + 16.0 * u, btn, btn), Glyph::Minus, Style::Secondary, true) {
            pl.cam.zoom_center(1.0 / 1.6);
        }
        if g.button("REGIME", Rect::new(m, m + 3.0 * btn + 22.0 * u, btn, btn), "GOV", Style::Secondary, true) {
            pl.regime_open = !pl.regime_open;
            pl.build_open = false;
            pl.target = None;
            pl.sel_building = None;
            pl.naval_mode = false;
            pl.missile_mode = false;
        }
        if g.button("EMPIRE_BTN", Rect::new(m, m + 4.0 * btn + 28.0 * u, btn, btn), "EMP", Style::Secondary, true) {
            pl.empire_open = !pl.empire_open;
            pl.build_open = false;
            pl.regime_open = false;
            pl.target = None;
            pl.sel_building = None;
            pl.naval_mode = false;
            pl.missile_mode = false;
        }

        // ---- top-right: speed
        let idx = SPEEDS.iter().position(|v| (*v - pl.sim.speed).abs() < 0.01).unwrap_or(1);
        let plus = Rect::new(sw - m - btn, m, btn, btn);
        let lbl_w = 56.0 * u;
        let minus = Rect::new(plus.x - lbl_w - btn, m, btn, btn);
        if g.glyph_button("speed+", plus, Glyph::Plus, Style::Secondary, true) {
            pl.sim.speed = SPEEDS[(idx + 1).min(SPEEDS.len() - 1)];
        }
        if g.glyph_button("speed-", minus, Glyph::Minus, Style::Secondary, true) {
            pl.sim.speed = SPEEDS[idx.saturating_sub(1)];
        }
        g.text_c(&format!("{}x", pl.sim.speed), minus.x + btn + lbl_w / 2.0, m + btn / 2.0 + 7.0 * u, 20.0, c_accent());

        // ---- top-centre: nation pill (flag, name, date, population, manpower)
        let pill_x0 = m + btn + 12.0 * u;
        let pill_x1 = minus.x - 12.0 * u;
        let pill_w = (pill_x1 - pill_x0).max(150.0 * u);
        let pill = Rect::new(pill_x0, m, pill_w.min(520.0 * u), btn + 8.0 * u);
        g.panel(pill, c_panel());
        let pc = &pl.world.map.countries[pi];
        g.flag(&pc.iso, Rect::new(pill.x + 10.0 * u, pill.y + 8.0 * u, 40.0 * u, 30.0 * u));
        g.text(&pc.name, pill.x + 60.0 * u, pill.y + 24.0 * u, 18.0, c_text());
        g.text_r(&date_string(pl.sim.game_days), pill.x + pill.w - 12.0 * u, pill.y + 24.0 * u, 14.0, c_dim());
        let stats = format!(
            "POP {:.1}M   TROOPS {} (+{} on front)",
            pl.sim.pop[pi],
            fmt_num(pl.sim.manpower[pi]),
            fmt_num(pl.sim.committed_total(p))
        );
        g.text(&stats, pill.x + 60.0 * u, pill.y + 44.0 * u, 13.0, c_accent());

        // ---- peace offers
        // ---- resource bar (resource-bar in the HTML)
        let st = pl.sim.econ.stock[pi];
        let items: [(&str, String); 8] = [
            ("population", format!("{:.1}M", pl.sim.pop[pi])),
            ("vp", format!("{}", pl.sim.crowns_of(pi))),
            ("bricks", format!("{:.0}kt", st[1])),
            ("food", format!("{:.1}kt", st[2])),
            ("money", format!("${:.1}B", st[0])),
            ("steel", format!("{:.0}kt", st[3])),
            ("oil", format!("{:.1}kt", st[4])),
            ("uranium", format!("{:.2}kt", st[5])),
        ];
        let rb_x0 = pill.x;
        let rb_x1 = pill.x + pill.w;
        let chip_h = 30.0 * u;
        let mut chip_x = rb_x0;
        let mut chip_y = pill.y + pill.h + 6.0 * u;
        for (name, val) in items.iter() {
            let cw_chip = 34.0 * u + g.text_w(val, 14.0) + 8.0 * u;
            if chip_x + cw_chip > rb_x1 && chip_x > rb_x0 {
                chip_x = rb_x0;
                chip_y += chip_h + 4.0 * u;
            }
            g.rrect(chip_x, chip_y, cw_chip, chip_h, 9.0 * u, c_panel());
            g.block(Rect::new(chip_x, chip_y, cw_chip, chip_h));
            g.res_icon(name, Rect::new(chip_x + 5.0 * u, chip_y + 4.0 * u, 22.0 * u, 22.0 * u));
            g.text(val, chip_x + 32.0 * u, chip_y + chip_h / 2.0 + 5.0 * u, 14.0, c_text());
            chip_x += cw_chip + 5.0 * u;
        }
        let mut below = chip_y + chip_h + 10.0 * u;

        // ---- placing banner
        if let Some(k) = pl.placing {
            let w = (sw - 2.0 * m).min(520.0 * u);
            let r = Rect::new((sw - w) / 2.0, below, w, 66.0 * u);
            g.panel(r, c_panel());
            g.text(&format!("Place {} - tap the map", CATALOG[k].name), r.x + 16.0 * u, r.y + 28.0 * u, 17.0, c_text());
            g.text(&fmt_cost(&pl.sim.econ.build_cost(k)), r.x + 16.0 * u, r.y + 50.0 * u, 13.0, c_accent());
            if g.button("place-cancel", Rect::new(r.x + r.w - 16.0 * u - 110.0 * u, r.y + 10.0 * u, 110.0 * u, 46.0 * u), "Cancel", Style::Secondary, true) {
                pl.placing = None;
            }
            below += 76.0 * u;
        }
        if let Some(&from) = pl.sim.peace_offers.first() {
            let w = (sw - 20.0 * u).min(440.0 * u);
            let r = Rect::new((sw - w) / 2.0, below, w, 96.0 * u);
            g.panel(r, c_panel());
            g.text(&format!("{} asks for peace", pl.sim.names[from as usize]), r.x + 16.0 * u, r.y + 32.0 * u, 18.0, c_text());
            let bw = (r.w - 44.0 * u) / 2.0;
            if g.button("Accept peace", Rect::new(r.x + 16.0 * u, r.y + 44.0 * u, bw, 40.0 * u), "Accept", Style::Primary, true) {
                pl.sim.make_peace(p, from);
            } else if g.button("Decline peace", Rect::new(r.x + 28.0 * u + bw, r.y + 44.0 * u, bw, 40.0 * u), "Decline", Style::Secondary, true) {
                pl.sim.decline_offer(from);
            }
            below += 106.0 * u;
        }
        if let Some(rq) = pl.sim.assists.iter().find(|a| a.helper == p).cloned() {
            let w = (sw - 20.0 * u).min(440.0 * u);
            let r = Rect::new((sw - w) / 2.0, below, w, 96.0 * u);
            g.panel(r, c_panel());
            g.text(&format!("{} asks you to join the war on {}", pl.sim.names[rq.caller as usize], pl.sim.names[rq.target as usize]), r.x + 16.0 * u, r.y + 30.0 * u, 15.0, c_text());
            let bw = (r.w - 44.0 * u) / 2.0;
            if g.button("Join war", Rect::new(r.x + 16.0 * u, r.y + 44.0 * u, bw, 40.0 * u), "Join", Style::Danger, true) {
                pl.sim.join_war(p, rq.target);
            } else if g.button("Decline war", Rect::new(r.x + 28.0 * u + bw, r.y + 44.0 * u, bw, 40.0 * u), "Stay out", Style::Secondary, true) {
                pl.sim.decline_assist(p, rq.target);
            }
            below += 106.0 * u;
        }

        // ---- news log (left) and leaderboard (right)
        let mut ny = below.max(m + 4.0 * btn + 48.0 * u);
        for (msg, t) in pl.sim.log.iter().rev().take(5) {
            let age = pl.sim.time - *t;
            if age > 9.0 {
                continue;
            }
            let a = if age > 6.0 { (9.0 - age) / 3.0 } else { 1.0 };
            g.text(msg, m + 4.0 * u, ny, 14.0, Color::new(1.0, 0.94, 0.7, a));
            ny += 20.0 * u;
        }
        if !portrait {
            let mut rank: Vec<usize> = (1..pl.sim.n).filter(|&i| !pl.sim.eliminated[i] && pl.sim.cells[i] > 0).collect();
            rank.sort_by(|&a, &b| pl.sim.cells[b].cmp(&pl.sim.cells[a]));
            let total: f32 = pl.sim.cells.iter().sum::<u32>() as f32;
            let mut ly = pill.y + pill.h + 30.0 * u;
            for (k, &id) in rank.iter().take(5).enumerate() {
                let me = id == pi;
                let txt = format!("{} {}  {:.1}%", k + 1, pl.sim.names[id], 100.0 * pl.sim.cells[id] as f32 / total.max(1.0));
                g.text_r(&txt, sw - m - 4.0 * u, ly, 13.0, if me { c_accent() } else { c_text() });
                ly += 18.0 * u;
            }
        }

        // ---- bottom-right: troops slider (hud-bar-right)
        let sl_w = if portrait { sw - 2.0 * m } else { (sw * 0.40).clamp(210.0 * u, 360.0 * u) };
        let sl_h = 74.0 * u;
        let sl = Rect::new(sw - m - sl_w, sh - m - sl_h, sl_w, sl_h);
        g.panel(sl, c_panel());
        let avail = pl.sim.manpower[pi];
        let sel = (pl.send_frac * avail).floor();
        g.text("TROOPS TO SEND", sl.x + 14.0 * u, sl.y + 24.0 * u, 12.0, c_dim());
        g.text_r(&format!("{} / {}", fmt_num(sel), fmt_num(avail)), sl.x + sl.w - 14.0 * u, sl.y + 26.0 * u, 18.0, c_text());
        let track = Rect::new(sl.x + 22.0 * u, sl.y + 46.0 * u, sl.w - 44.0 * u, 11.0 * u);
        g.slider(track, &mut pl.send_frac, &mut pl.slider_drag, avail > 0.0);

        // ---- BUILD / MISSILE buttons
        let has_silo = !pl.sim.econ.silos_of(p).is_empty();
        let has_silo_f = if has_silo { 1.0f32 } else { 0.0 };
        if !has_silo {
            pl.missile_mode = false;
        }
        let build_btn = if portrait {
            Rect::new(m, sl.y - m - 54.0 * u, sw - 2.0 * m, 54.0 * u)
        } else {
            Rect::new(sl.x - m - 120.0 * u, sl.y, 120.0 * u, sl_h)
        };
        if pl.placing.is_none() && g.button("BUILD", build_btn, "BUILD", Style::Primary, true) {
            pl.build_open = !pl.build_open;
            pl.regime_open = false;
            pl.target = None;
            pl.sel_building = None;
        }
        if has_silo && pl.placing.is_none() {
            let mrect = if portrait {
                Rect::new(m, build_btn.y - m - 54.0 * u, sw - 2.0 * m, 54.0 * u)
            } else {
                Rect::new(build_btn.x - m - 120.0 * u, sl.y, 120.0 * u, sl_h)
            };
            if g.button("MISSILE", mrect, "MISSILE", Style::Danger, true) {
                pl.missile_mode = true;
                pl.missile_target = None;
                pl.build_open = false;
                pl.naval_mode = false;
            }
        }

        // ---- country card (bottom-left)
        let mut close_card = false;
        if let (Some(t), false) = (pl.target, pl.build_open || pl.regime_open) {
            let ti = t as usize;
            let cw = if portrait { sw - 2.0 * m } else { (sw - sl_w - 258.0 * u - 5.0 * m).clamp(240.0 * u, 380.0 * u) };
            let chh = 296.0 * u;
            let cy = if portrait { sl.y - (54.0 * u + m) * (1.0 + has_silo_f) - m - chh } else { sh - m - chh };
            let card = Rect::new(m, cy, cw, chh);
            g.panel(card, c_panel());
            let tc = &pl.world.map.countries[ti];
            let x0 = card.x + 16.0 * u;
            draw_rectangle(x0, card.y + 18.0 * u, 20.0 * u, 20.0 * u, col(tc.rgb[0], tc.rgb[1], tc.rgb[2]));
            g.flag(&tc.iso, Rect::new(x0 + 28.0 * u, card.y + 12.0 * u, 40.0 * u, 32.0 * u));
            g.text_in(&tc.name, Rect::new(x0 + 76.0 * u, card.y + 8.0 * u, card.w - 76.0 * u - 70.0 * u, 38.0 * u), 22.0, c_text());
            if g.glyph_button("card-close", Rect::new(card.x + card.w - 16.0 * u - 38.0 * u, card.y + 10.0 * u, 38.0 * u, 38.0 * u), Glyph::Close, Style::Secondary, true) {
                close_card = true;
            }
            let adjacent = pl.sim.are_adjacent(p, t);
            let at_war = pl.sim.is_at_war(p, t);
            let locked = pl.sim.war_locked(p, t);
            let (status, scol) = if t == p {
                ("YOUR NATION", c_accent())
            } else if pl.sim.eliminated[ti] {
                ("ELIMINATED", c_red())
            } else if pl.sim.are_allies(p, t) {
                ("ALLY", c_green())
            } else if at_war {
                ("AT WAR", c_red())
            } else if locked {
                ("RECENT PEACE", c_dim())
            } else if !adjacent {
                ("NOT ON YOUR BORDER", c_dim())
            } else if !pl.sim.has_war_food(p) {
                ("BUILD A FARM: TROOPS NEED FOOD", c_accent())
            } else {
                ("AT PEACE", c_green())
            };
            g.text(status, x0, card.y + 70.0 * u, 14.0, scol);
            if t != p {
                let held = pl.world.held(p, t);
                if held > 0 {
                    g.text(&format!("HOLDING {} CELLS - ANNEX {} CROWNS", held, pl.sim.annex_cost_cells(ti, held)), x0, card.y + 86.0 * u, 12.0, c_dim());
                }
            }
            let mut ry = card.y + 100.0 * u;
            let rr = card.x + card.w - 16.0 * u;
            g.text("POPULATION", x0, ry, 13.0, c_dim());
            g.text_r(&format!("{:.1}M", pl.sim.pop[ti]), rr, ry, 16.0, c_text());
            ry += 28.0 * u;
            g.text("GDP", x0, ry, 13.0, c_dim());
            g.text_r(&fmt_money(tc.gdp), rr, ry, 16.0, c_text());
            ry += 28.0 * u;
            g.text("MANPOWER", x0, ry, 13.0, c_dim());
            g.text_r(&fmt_num(pl.sim.manpower[ti]), rr, ry, 16.0, c_text());
            let cap = pl.sim.effective_cap(ti).max(1.0);
            let bar = Rect::new(x0, ry + 8.0 * u, card.w - 32.0 * u, 8.0 * u);
            g.rrect(bar.x, bar.y, bar.w, bar.h, 4.0 * u, col(12, 16, 21));
            let fillw = bar.w * (pl.sim.manpower[ti] / cap).clamp(0.0, 1.0);
            if fillw > 1.0 {
                g.rrect(bar.x, bar.y, fillw, bar.h, 4.0 * u, c_accent());
            }
            ry += 36.0 * u;
            g.text("CAP", x0, ry, 13.0, c_dim());
            g.text_r(&fmt_num(cap), rr, ry, 16.0, c_text());

            let by = card.y + card.h - 14.0 * u - 48.0 * u;
            let bw_all = card.w - 32.0 * u;
            let amount = (pl.send_frac * pl.sim.manpower[pi]).floor();
            if t != p && !pl.sim.eliminated[ti] {
                if at_war {
                    let bw = (bw_all - 32.0 * u) / 5.0;
                    if g.button("SUPPORT", Rect::new(x0, by, bw, 48.0 * u), "SUPPORT", Style::Primary, avail > 0.0) {
                        pl.sim.commit(p, t, amount);
                    }
                    if g.button("NAVAL", Rect::new(x0 + (bw + 8.0 * u), by, bw, 48.0 * u), "NAVAL", Style::Secondary, pl.sim.has_port(&pl.world.map, p) && pl.sim.naval_max_boats(p) > 0) {
                        pl.naval_mode = true;
                        pl.naval_cell = None;
                        pl.naval_boats = pl.naval_boats.clamp(1, pl.sim.naval_max_boats(p).max(1));
                    }
                    if g.button("FOCUS", Rect::new(x0 + 2.0 * (bw + 8.0 * u), by, bw, 48.0 * u), "FOCUS", Style::Secondary, true) {
                        pl.focus_mode = true;
                    }
                    if g.button("STOP", Rect::new(x0 + 3.0 * (bw + 8.0 * u), by, bw, 48.0 * u), "STOP", Style::Secondary, true) {
                        pl.sim.stop_attack(p, t);
                    }
                    if g.button("PEACE", Rect::new(x0 + 4.0 * (bw + 8.0 * u), by, bw, 48.0 * u), "PEACE", Style::Secondary, true) {
                        pl.sim.propose_peace(p, t);
                    }
                } else if pl.sim.are_allies(p, t) {
                    g.text_c("YOU ARE ALLIED", card.x + card.w / 2.0, by + 30.0 * u, 18.0, c_green());
                } else {
                    let half = (bw_all - 8.0 * u) / 2.0;
                    let food_ok = pl.sim.has_war_food(p);
                    if g.button("WAR", Rect::new(x0, by, half, 48.0 * u), "WAR", Style::Danger, adjacent && !locked && avail > 0.0 && food_ok) {
                        if pl.sim.declare_war(p, t) {
                            pl.sim.commit(p, t, amount);
                        }
                    }
                    let pending = pl.sim.ally_offer_pending(p, t);
                    let ally_ok = !pending && !pl.sim.ally_on_cooldown(p, t);
                    if g.button("ALLY", Rect::new(x0 + half + 8.0 * u, by, half, 48.0 * u), if pending { "PENDING" } else { "ALLY" }, Style::Secondary, ally_ok) {
                        pl.sim.request_ally(p, t);
                    }
                }
            }
        }
        if close_card {
            pl.target = None;
        }

        // ---- missile targeting banner
        if pl.missile_mode {
            let ready = pl.sim.econ.silos_of(p).iter().map(|&i| pl.sim.econ.buildings[i].id).find(|&id| pl.sim.silo_cooldown_left(id) <= 0.0);
            let w = (sw - 2.0 * m).min(560.0 * u);
            let r = Rect::new((sw - w) / 2.0, below, w, 100.0 * u);
            g.panel(r, c_panel());
            let status = match (ready, pl.missile_target) {
                (None, _) => "silo reloading...".to_string(),
                (Some(_), None) => "tap a target on the map".to_string(),
                (Some(_), Some(_)) => "target locked".to_string(),
            };
            g.text(&format!("Missile strike: {}", status), r.x + 16.0 * u, r.y + 30.0 * u, 16.0, c_text());
            g.text(
                &format!("Cost ${:.0}B, {:.0}kt oil, {:.1}kt uranium", crate::sim::MISSILE_COST_MONEY, crate::sim::MISSILE_COST_OIL, crate::sim::MISSILE_COST_URANIUM),
                r.x + 16.0 * u,
                r.y + 54.0 * u,
                13.0,
                c_accent(),
            );
            g.text("5s warm-up, then the missile flies. Air defense can shoot it down.", r.x + 16.0 * u, r.y + 80.0 * u, 12.0, c_dim());
            let can = ready.is_some() && pl.missile_target.is_some() && pl.sim.can_afford_missile(p);
            let bw = 118.0 * u;
            if g.button("LAUNCH", Rect::new(r.x + r.w - 14.0 * u - bw, r.y + 8.0 * u, bw, 40.0 * u), "LAUNCH", Style::Danger, can) {
                if let (Some(sid), Some((tx, ty))) = (ready, pl.missile_target) {
                    match pl.sim.launch_missile(p, sid, tx, ty, &pl.world) {
                        Ok(()) => {
                            pl.missile_mode = false;
                            pl.missile_target = None;
                        }
                        Err(e) => pl.sim.say(e.to_string()),
                    }
                }
            }
            if g.button("missile-cancel", Rect::new(r.x + r.w - 14.0 * u - bw, r.y + 52.0 * u, bw, 40.0 * u), "Cancel", Style::Secondary, true) {
                pl.missile_mode = false;
                pl.missile_target = None;
            }
            below += 110.0 * u;
        }

        // ---- focused-attack targeting banner
        if pl.focus_mode {
            if let Some(t) = pl.target {
                let w = (sw - 2.0 * m).min(520.0 * u);
                let r = Rect::new((sw - w) / 2.0, below, w, 66.0 * u);
                g.panel(r, c_panel());
                g.text(&format!("Tap a frontier cell inside {} to focus the push", pl.sim.names[t as usize]), r.x + 16.0 * u, r.y + 28.0 * u, 16.0, c_text());
                g.text("Short, concentrated push; cooldown between orders", r.x + 16.0 * u, r.y + 50.0 * u, 13.0, c_accent());
                if g.button("focus-cancel-mode", Rect::new(r.x + r.w - 16.0 * u - 110.0 * u, r.y + 10.0 * u, 110.0 * u, 46.0 * u), "Cancel", Style::Secondary, true) {
                    pl.focus_mode = false;
                }
            } else {
                pl.focus_mode = false;
            }
            below += 76.0 * u;
        }

        // ---- naval targeting banner + invasion panel
        if pl.naval_mode {
            if let Some(t) = pl.target {
                let w = (sw - 2.0 * m).min(520.0 * u);
                let r = Rect::new((sw - w) / 2.0, below, w, 66.0 * u);
                g.panel(r, c_panel());
                g.text(&format!("Tap the coast of {} to land", pl.sim.names[t as usize]), r.x + 16.0 * u, r.y + 28.0 * u, 17.0, c_text());
                g.text("Boats leave from your port", r.x + 16.0 * u, r.y + 50.0 * u, 13.0, c_accent());
                if g.button("naval-cancel-mode", Rect::new(r.x + r.w - 16.0 * u - 110.0 * u, r.y + 10.0 * u, 110.0 * u, 46.0 * u), "Cancel", Style::Secondary, true) {
                    pl.naval_mode = false;
                }
            } else {
                pl.naval_mode = false;
            }
        }
        if let (Some(cell), Some(t)) = (pl.naval_cell, pl.target) {
            let max_b = pl.sim.naval_max_boats(p).max(1);
            pl.naval_boats = pl.naval_boats.clamp(1, max_b);
            let w = (sw - 2.0 * m).min(440.0 * u);
            let r = Rect::new((sw - w) / 2.0, (sh - 220.0 * u) / 2.0, w, 220.0 * u);
            g.panel(r, c_panel());
            g.text(&format!("Naval invasion of {}", pl.sim.names[t as usize]), r.x + 18.0 * u, r.y + 38.0 * u, 20.0, c_text());
            g.text(&format!("{} boats - {} troops", pl.naval_boats, fmt_num(pl.naval_boats as f32 * 4000.0)), r.x + 18.0 * u, r.y + 72.0 * u, 15.0, c_accent());
            let bs = 46.0 * u;
            if g.glyph_button("naval-minus", Rect::new(r.x + r.w - 18.0 * u - 2.0 * bs - 8.0 * u, r.y + 50.0 * u, bs, bs), Glyph::Minus, Style::Secondary, pl.naval_boats > 1) {
                pl.naval_boats -= 1;
            }
            if g.glyph_button("naval-plus", Rect::new(r.x + r.w - 18.0 * u - bs, r.y + 50.0 * u, bs, bs), Glyph::Plus, Style::Secondary, pl.naval_boats < max_b) {
                pl.naval_boats += 1;
            }
            let bw = (r.w - 44.0 * u) / 2.0;
            let by = r.y + r.h - 16.0 * u - 52.0 * u;
            if g.button("SEND", Rect::new(r.x + 16.0 * u, by, bw, 52.0 * u), "SEND", Style::Primary, true) {
                match pl.sim.launch_naval(p, t, cell, pl.naval_boats, &pl.world) {
                    Ok(()) => {}
                    Err(e) => pl.sim.say(e.to_string()),
                }
                pl.naval_cell = None;
            } else if g.button("naval-cancel", Rect::new(r.x + 28.0 * u + bw, by, bw, 52.0 * u), "Cancel", Style::Secondary, true) {
                pl.naval_cell = None;
            }
        }

        // ---- build panel
        if pl.build_open && pl.placing.is_none() {
            let w = (sw - 2.0 * m).min(560.0 * u);
            let row_h = 50.0 * u;
            let h = 64.0 * u + CATALOG.len() as f32 * row_h + 8.0 * u;
            let r = Rect::new((sw - w) / 2.0, (sh - h - m).max(m), w, h);
            g.panel(r, c_panel());
            g.text("Build", r.x + 18.0 * u, r.y + 40.0 * u, 26.0, c_text());
            if g.glyph_button("build-close", Rect::new(r.x + r.w - 14.0 * u - 40.0 * u, r.y + 12.0 * u, 40.0 * u, 40.0 * u), Glyph::Close, Style::Secondary, true) {
                pl.build_open = false;
            }
            let mut ry = r.y + 64.0 * u;
            for (i, cat) in CATALOG.iter().enumerate() {
                let cost = pl.sim.econ.build_cost(i);
                let ok = pl.sim.econ.can_afford(pi, &cost);
                let row = Rect::new(r.x + 10.0 * u, ry, r.w - 20.0 * u, row_h - 6.0 * u);
                g.rrect(row.x, row.y, row.w, row.h, 10.0 * u, if ok { c_panel2() } else { col(24, 30, 38) });
                draw_rectangle(row.x + 10.0 * u, row.y + 8.0 * u, 28.0 * u, 28.0 * u, col(cat.color[0], cat.color[1], cat.color[2]));
                g.text(cat.name, row.x + 48.0 * u, row.y + 19.0 * u, 16.0, if ok { c_text() } else { c_faint() });
                g.text(&fmt_cost(&cost), row.x + 48.0 * u, row.y + 37.0 * u, 12.0, if ok { c_accent() } else { c_red() });
                let prod = match (cat.special, cat.produces) {
                    (Special::City, _) => "raises pop cap".to_string(),
                    (Special::Village, _) => format!("+{:.3}M pop/yr", crate::econ::VILLAGE_POP_PER_YEAR),
                    (_, Some((r, per))) => format!("+{:.1} {}/yr", per, RES_UNIT[r as usize]),
                    _ => match cat.kind {
                        "missile-silo" => "launches missiles".to_string(),
                        "air-defense" => "intercepts missiles".to_string(),
                        "port" => "sea invasions".to_string(),
                        _ => String::new(),
                    },
                };
                g.text_r(&prod, row.x + row.w - 12.0 * u, row.y + 19.0 * u, 12.0, c_dim());
                if g.tapped(&format!("build:{}", cat.kind), row) && ok {
                    pl.placing = Some(i);
                    pl.build_open = false;
                }
                ry += row_h;
            }
        }

        // ---- government panel (GOV): effects of the current regime, policy sliders, regime picker
        if pl.regime_open && pl.placing.is_none() {
            let w = (sw - 2.0 * m).min(620.0 * u);
            let h = (sh - 2.0 * m).min(610.0 * u);
            let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
            g.panel(r, c_panel());
            let cur = pl.sim.regime_def(pi);
            g.text("Government", r.x + 18.0 * u, r.y + 38.0 * u, 26.0, c_text());
            g.text_r(cur.name, r.x + r.w - 70.0 * u, r.y + 36.0 * u, 16.0, c_accent());
            if g.glyph_button("regime-close", Rect::new(r.x + r.w - 14.0 * u - 40.0 * u, r.y + 12.0 * u, 40.0 * u, 40.0 * u), Glyph::Close, Style::Secondary, true) {
                pl.regime_open = false;
            }
            let mut ry = r.y + 62.0 * u;
            // policy sliders: -/+ step buttons keep it touch friendly
            let pol = pl.sim.policy[pi];
            for (k, sl) in ideology::SLIDERS.iter().enumerate() {
                g.text_in(sl.name, Rect::new(r.x + 18.0 * u, ry, 168.0 * u, 34.0 * u), 15.0, c_text());
                let mx = r.x + 192.0 * u;
                let px = r.x + r.w - 18.0 * u - 34.0 * u;
                if g.glyph_button(&format!("pol-:{}", k), Rect::new(mx, ry, 34.0 * u, 34.0 * u), Glyph::Minus, Style::Secondary, true) {
                    pl.sim.set_policy(pi, k, pol[k] - sl.step);
                }
                if g.glyph_button(&format!("pol+:{}", k), Rect::new(px, ry, 34.0 * u, 34.0 * u), Glyph::Plus, Style::Secondary, true) {
                    pl.sim.set_policy(pi, k, pol[k] + sl.step);
                }
                let bx = mx + 34.0 * u + 8.0 * u;
                let bar = Rect::new(bx, ry + 9.0 * u, (px - 8.0 * u - bx).max(10.0 * u), 16.0 * u);
                g.rrect(bar.x, bar.y, bar.w, bar.h, 8.0 * u, col(12, 16, 21));
                let frac = ((pol[k] - sl.min) / (sl.max - sl.min).max(1.0)).clamp(0.0, 1.0);
                if frac > 0.0 {
                    g.rrect(bar.x, bar.y, (bar.w * frac).max(bar.h), bar.h, 8.0 * u, c_accent_dark());
                }
                g.text_c(&format!("{:.0}", pol[k]), bar.x + bar.w / 2.0, bar.y + bar.h - 3.0 * u, 12.0, c_text());
                ry += 36.0 * u;
            }
            ry += 6.0 * u;
            // what the current regime + sliders add up to
            let md = pl.sim.mods_of(pi);
            g.text(&fmt_mods(&md, 0, 4), r.x + 18.0 * u, ry + 12.0 * u, 12.0, c_dim());
            g.text(&fmt_mods(&md, 4, ideology::NKEYS), r.x + 18.0 * u, ry + 30.0 * u, 12.0, c_dim());
            ry += 42.0 * u;
            // regime picker (tap to inspect, ADOPT to switch)
            let gap = 8.0 * u;
            let cw2 = (r.w - 36.0 * u - gap) / 2.0;
            let rh = 34.0 * u;
            let cur_i = pl.sim.ideology[pi];
            let chosen = pl.regime_sel;
            for (i, rg) in ideology::REGIMES.iter().enumerate() {
                let rect = Rect::new(r.x + 18.0 * u + (i % 2) as f32 * (cw2 + gap), ry + (i / 2) as f32 * (rh + 4.0 * u), cw2, rh);
                let fill = if chosen == Some(i) { c_accent_dark() } else if i == cur_i { col(40, 70, 52) } else { c_panel2() };
                g.rrect(rect.x, rect.y, rect.w, rect.h, 9.0 * u, fill);
                g.text_in(rg.name, rect, 14.0, if chosen == Some(i) { c_accent_ink() } else { c_text() });
                if g.tapped(&format!("regime:{}", rg.id), rect) {
                    pl.regime_sel = Some(i);
                }
            }
            ry += 6.0 * (rh + 4.0 * u) + 6.0 * u;
            if let Some(si) = pl.regime_sel {
                let rg = &ideology::REGIMES[si];
                let chk = pl.sim.can_switch_ideology(pi, si);
                let cost = pl.sim.ideology_switch_cost(pi, si);
                g.text(rg.name, r.x + 18.0 * u, ry + 16.0 * u, 17.0, c_text());
                g.text(rg.blurb, r.x + 18.0 * u, ry + 34.0 * u, 12.0, c_dim());
                g.text(&fmt_mods(&rg.mods, 0, 4), r.x + 18.0 * u, ry + 52.0 * u, 12.0, c_accent());
                g.text(&fmt_mods(&rg.mods, 4, ideology::NKEYS), r.x + 18.0 * u, ry + 68.0 * u, 12.0, c_accent());
                let note = match &chk {
                    Ok(()) => format!("Cost ${:.0}B", cost),
                    Err(e) => format!("{}  (cost ${:.0}B)", e, cost),
                };
                g.text(&note, r.x + 18.0 * u, ry + 86.0 * u, 12.0, if chk.is_ok() { c_green() } else { c_red() });
                let br = Rect::new(r.x + r.w - 18.0 * u - 130.0 * u, ry + 10.0 * u, 130.0 * u, 50.0 * u);
                if g.button("ADOPT", br, "ADOPT", Style::Primary, chk.is_ok()) {
                    pl.sim.set_ideology(pi, si);
                    pl.regime_sel = None;
                }
            } else {
                g.text("Tap a regime to see its effects.", r.x + 18.0 * u, ry + 24.0 * u, 13.0, c_dim());
            }
        }

        // ---- empire panel: eligibility / founding / perk shop
        if pl.empire_open && pl.placing.is_none() {
            let w = (sw - 2.0 * m).min(560.0 * u);
            let h = (sh - 2.0 * m).min(610.0 * u);
            let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
            g.panel(r, c_panel());
            g.text("Empire", r.x + 18.0 * u, r.y + 38.0 * u, 26.0, c_text());
            g.text_r(&format!("{} CROWNS", pl.sim.crowns_of(pi)), r.x + r.w - 20.0 * u, r.y + 36.0 * u, 15.0, c_accent());
            if g.glyph_button("empire-close", Rect::new(r.x + r.w - 14.0 * u - 40.0 * u, r.y + 12.0 * u, 40.0 * u, 40.0 * u), Glyph::Close, Style::Secondary, true) {
                pl.empire_open = false;
            }
            let mut ry = r.y + 64.0 * u;
            if pl.sim.is_empire(pi) {
                g.text(&pl.sim.empire_name[pi], r.x + 18.0 * u, ry, 16.0, c_accent());
                ry += 24.0 * u;
                let active = pl.sim.empire_perks[pi].len();
                g.text(&format!("{}/{} perks active", active, ideology::EMPIRE_MAX_ACTIVE_PERKS), r.x + 18.0 * u, ry, 12.0, c_dim());
                let dr = Rect::new(r.x + r.w - 18.0 * u - 150.0 * u, ry - 18.0 * u, 150.0 * u, 34.0 * u);
                if g.button("empire-dissolve", dr, "DISSOLVE", Style::Secondary, true) {
                    pl.sim.revert_empire(pi);
                }
                ry += 16.0 * u;
                let rh = 40.0 * u;
                for (i, perk) in ideology::PERKS.iter().enumerate() {
                    let rect = Rect::new(r.x + 18.0 * u, ry, r.w - 36.0 * u, rh);
                    let owned = pl.sim.empire_perks[pi].contains(&(i as u8));
                    g.rrect(rect.x, rect.y, rect.w, rect.h, 8.0 * u, if owned { col(40, 70, 52) } else { c_panel2() });
                    g.text_in(&format!("{} ({}: x{:.2})", perk.name, ideology::KEY_LABELS[perk.stat_key], perk.mult), Rect::new(rect.x + 8.0 * u, rect.y, rect.w - 100.0 * u, rect.h), 12.0, c_text());
                    if !owned {
                        let cost = (perk.cost as f32 * ideology::EMPIRE_SHOP_PRICE_MULT).round() as u32;
                        let affordable = pl.sim.crowns_of(pi) >= cost && active < ideology::EMPIRE_MAX_ACTIVE_PERKS;
                        let br = Rect::new(rect.x + rect.w - 90.0 * u, rect.y + 4.0 * u, 82.0 * u, rect.h - 8.0 * u);
                        if g.button(&format!("perk:{}", i), br, &format!("{}cr", cost), Style::Primary, affordable) {
                            pl.sim.buy_empire_perk(pi, i);
                        }
                    } else {
                        g.text_r("OWNED", rect.x + rect.w - 10.0 * u, rect.y + rect.h / 2.0 + 4.0 * u, 11.0, c_green());
                    }
                    ry += rh + 6.0 * u;
                }
            } else {
                let chk = pl.sim.can_form_empire(pi, &pl.world);
                g.text(&format!("Foreign land held: {}/{}", pl.world.foreign_held(pi as u16), ideology::EMPIRE_REQUIRE_FOREIGN_CELLS), r.x + 18.0 * u, ry, 13.0, c_dim());
                ry += 22.0 * u;
                g.text(&format!("Crowns: {}/{}", pl.sim.crowns_of(pi), ideology::EMPIRE_REQUIRE_VP + ideology::EMPIRE_FORM_VP_COST), r.x + 18.0 * u, ry, 13.0, c_dim());
                ry += 22.0 * u;
                g.text(&format!("Manpower: {:.0}/{:.0}", pl.sim.manpower[pi], ideology::EMPIRE_REQUIRE_MANPOWER), r.x + 18.0 * u, ry, 13.0, c_dim());
                ry += 30.0 * u;
                let note = match &chk {
                    Ok(()) => format!("Ready to declare (-{} crowns)", ideology::EMPIRE_FORM_VP_COST),
                    Err(e) => e.clone(),
                };
                g.text(&note, r.x + 18.0 * u, ry, 13.0, if chk.is_ok() { c_green() } else { c_red() });
                ry += 30.0 * u;
                let br = Rect::new(r.x + 18.0 * u, ry, 180.0 * u, 50.0 * u);
                if g.button("empire-found", br, "FOUND EMPIRE", Style::Primary, chk.is_ok()) {
                    pl.sim.form_empire(pi, &pl.world);
                }
            }
        }

        // ---- building card
        let mut close_bcard = false;
        if let Some(bid) = pl.sel_building {
            if let Some(bi) = pl.sim.econ.buildings.iter().position(|b| b.id == bid) {
                let b = pl.sim.econ.buildings[bi].clone();
                let cat = &CATALOG[b.kind];
                let cw = if portrait { sw - 2.0 * m } else { (sw - sl_w - 258.0 * u - 5.0 * m).clamp(240.0 * u, 380.0 * u) };
                let chh = 250.0 * u;
                let cy = if portrait { sl.y - (54.0 * u + m) * (1.0 + has_silo_f) - m - chh } else { sh - m - chh };
                let card = Rect::new(m, cy, cw, chh);
                g.panel(card, c_panel());
                let x0 = card.x + 16.0 * u;
                draw_rectangle(x0, card.y + 16.0 * u, 30.0 * u, 30.0 * u, col(cat.color[0], cat.color[1], cat.color[2]));
                g.text(&format!("{}  Lv {}", cat.name, b.level), x0 + 40.0 * u, card.y + 38.0 * u, 20.0, c_text());
                if g.glyph_button("bcard-close", Rect::new(card.x + card.w - 14.0 * u - 38.0 * u, card.y + 10.0 * u, 38.0 * u, 38.0 * u), Glyph::Close, Style::Secondary, true) {
                    close_bcard = true;
                }
                g.text(&format!("Owner: {}", pl.sim.names[b.country as usize]), x0, card.y + 70.0 * u, 13.0, c_dim());
                let out_txt = match (cat.special, cat.produces) {
                    (Special::City, _) => format!("+{:.0}% pop cap", (pl.sim.econ.city_cap_bonus(b.country as usize) - 1.0) * 100.0),
                    (Special::Village, _) => format!("+{:.3}M pop/yr", pl.sim.econ.village_output(&b)),
                    (_, Some((r, _))) => format!("+{:.1} {}/yr", pl.sim.econ.output_per_year(&b), RES_UNIT[r as usize]),
                    _ => match cat.kind {
                        "missile-silo" => "launches missiles".to_string(),
                        "air-defense" => "intercepts missiles".to_string(),
                        "port" => "sea invasions".to_string(),
                        _ => String::new(),
                    },
                };
                g.text("OUTPUT", x0, card.y + 100.0 * u, 13.0, c_dim());
                g.text_r(&out_txt, card.x + card.w - 16.0 * u, card.y + 100.0 * u, 15.0, c_text());
                let spot = match (cat.spot, pl.sim.econ.resource_at(b.gx, b.gy)) {
                    (Some(k), Some((rk, tier))) if rk == k => format!("On a {} deposit (x{:.2})", RARITY[tier.min(2)].0, RARITY[tier.min(2)].2),
                    (Some(_), _) => "No matching deposit here".to_string(),
                    _ => String::new(),
                };
                g.text(&spot, x0, card.y + 128.0 * u, 13.0, c_accent());
                if b.country == p {
                    let by = card.y + card.h - 14.0 * u - 48.0 * u;
                    if let Some(t) = b.dismantle_at {
                        g.text(&format!("DISMANTLING... {:.0}s", (t - pl.sim.time).max(0.0)), x0, by + 30.0 * u, 18.0, c_red());
                    } else {
                        let half = (card.w - 32.0 * u - 8.0 * u) / 2.0;
                        match pl.sim.econ.upgrade_cost(&b) {
                            Some(uc) => {
                                g.text(&format!("Upgrade: {}", fmt_cost(&uc)), x0, by - 10.0 * u, 13.0, c_dim());
                                let can = pl.sim.econ.can_afford(pi, &uc);
                                if g.button("UPGRADE", Rect::new(x0, by, half, 48.0 * u), "UPGRADE", Style::Primary, can) {
                                    let msg = match pl.sim.econ.upgrade(b.id, p) {
                                        Ok(l) => format!("{} UPGRADED {} TO LVL {}", pl.sim.names[pi], cat.kind.to_uppercase(), l),
                                        Err(e) => e.to_string(),
                                    };
                                    pl.sim.say(msg);
                                }
                            }
                            None => g.text("MAX LEVEL", x0, by + 30.0 * u, 18.0, c_accent()),
                        }
                        if g.button("DISMANTLE", Rect::new(x0 + half + 8.0 * u, by, half, 48.0 * u), "DISMANTLE", Style::Danger, true) {
                            let now = pl.sim.time;
                            let msg = match pl.sim.econ.dismantle(b.id, p, now) {
                                Ok(()) => format!("{} DISMANTLES {}", pl.sim.names[pi], cat.kind.to_uppercase()),
                                Err(e) => e.to_string(),
                            };
                            pl.sim.say(msg);
                        }
                    }
                }
            } else {
                pl.sel_building = None;
            }
        }
        if close_bcard {
            pl.sel_building = None;
        }

        // ---- tap on the map selects a country
        if !modal && g.ptr.tapped() && !g.consumed && !g.over_ui(g.ptr.press_pos) {
            let (cx, cy) = pl.cam.screen_to_cell(g.ptr.pos);
            if pl.missile_mode {
                let (mw, mh) = (pl.world.map.w as i32, pl.world.map.h as i32);
                if cx >= 0 && cy >= 0 && cx < mw && cy < mh && pl.world.map.land[(cy * mw + cx) as usize] == 1 {
                    pl.missile_target = Some((cx, cy));
                }
            } else if pl.focus_mode {
                if let Some(t) = pl.target {
                    let (mw, mh) = (pl.world.map.w as i32, pl.world.map.h as i32);
                    if cx >= 0 && cy >= 0 && cx < mw && cy < mh {
                        let cell = (cy * mw + cx) as usize;
                        if pl.sim.focus_front(p, t, cell, &pl.world) {
                            pl.focus_mode = false;
                        } else {
                            pl.sim.say("CAN'T FOCUS THERE".to_string());
                        }
                    }
                }
            } else if pl.naval_mode {
                if let Some(t) = pl.target {
                    match landing_cell(&pl.world, t, cx, cy, pl.cam.scale) {
                        Some(c) => {
                            pl.naval_cell = Some(c);
                            pl.naval_mode = false;
                        }
                        None => {
                            let msg = format!("PICK A LANDING SPOT INSIDE {}", pl.sim.names[t as usize]);
                            pl.sim.say(msg);
                        }
                    }
                }
            } else if let Some(k) = pl.placing {
                match pl.sim.econ.place(&pl.world.map, cx - 2, cy - 2, p, k) {
                    Ok(()) => {
                        let msg = format!("{} CONSTRUCTED {}", pl.sim.names[pi], CATALOG[k].kind.to_uppercase());
                        pl.sim.say(msg);
                        pl.placing = None;
                    }
                    Err(e) => pl.sim.say(e.to_string()),
                }
            } else if !pl.build_open && !pl.regime_open {
                if let Some(bi) = pl.sim.econ.building_at(cx, cy) {
                    pl.sel_building = Some(pl.sim.econ.buildings[bi].id);
                    pl.target = None;
                } else {
                    pl.sel_building = None;
                    let id = pl.world.owner_at(cx, cy);
                    pl.target = if id == 0 { None } else { Some(id) };
                    if id != 0 && id != p && pl.send_frac < 0.05 {
                        pl.send_frac = 0.25;
                    }
                }
            }
        }

        // ---- game over
        if pl.sim.eliminated[pi] {
            let w = (sw * 0.9).min(420.0 * u);
            let r = Rect::new((sw - w) / 2.0, sh * 0.34, w, 150.0 * u);
            g.panel(r, c_panel());
            g.text_c("YOU WERE ELIMINATED", r.x + r.w / 2.0, r.y + 52.0 * u, 26.0, c_red());
            if g.button("Leave match", Rect::new(r.x + 20.0 * u, r.y + 78.0 * u, r.w - 40.0 * u, 52.0 * u), "Back to menu", Style::Primary, true) {
                action = PlayAction::Leave;
            }
        }

        // ---- pause menu
        if pl.menu_open {
            g.enabled = !settings_open;
            draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.6));
            g.block(Rect::new(0.0, 0.0, sw, sh));
            let w = (sw * 0.92).min(420.0 * u);
            let h = 350.0 * u;
            let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
            g.panel(r, c_panel());
            g.text_c("Paused", r.x + r.w / 2.0, r.y + 50.0 * u, 30.0, c_text());
            let bx = r.x + 22.0 * u;
            let bw = r.w - 44.0 * u;
            let bh = 52.0 * u;
            let mut y = r.y + 76.0 * u;
            if g.button("Resume", Rect::new(bx, y, bw, bh), "Resume", Style::Primary, true) {
                pl.menu_open = false;
            }
            y += bh + 12.0 * u;
            if g.button("Restart", Rect::new(bx, y, bw, bh), "Restart match", Style::Secondary, true) {
                action = PlayAction::Restart;
            }
            y += bh + 12.0 * u;
            let mut open_settings = false;
            if g.button("Settings", Rect::new(bx, y, bw, bh), "Settings", Style::Secondary, true) {
                open_settings = true;
            }
            y += bh + 12.0 * u;
            if g.button("Leave match", Rect::new(bx, y, bw, bh), "Leave match", Style::Danger, true) {
                action = PlayAction::Leave;
            }
            if open_settings {
                self.settings_open = true;
            }
        }
        action
    }
}

fn sfx_for_event(e: crate::sim::SfxEvent) -> Sfx {
    use crate::sim::SfxEvent as E;
    match e {
        E::WarDeclared => Sfx::WarDeclared,
        E::PeaceMade => Sfx::PeaceMade,
        E::BattleWon => Sfx::BattleWon,
        E::BattleLost => Sfx::BattleLost,
        E::CapitalFallen => Sfx::CapitalFallen,
        E::CapitalRecaptured => Sfx::CapitalRecaptured,
        E::NationEliminated => Sfx::NationEliminated,
        E::EmpireFounded => Sfx::EmpireFounded,
        E::CrownAwarded => Sfx::CrownAwarded,
        E::FocusAttack => Sfx::FocusAttack,
    }
}
