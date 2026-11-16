//! Screens: Menu -> Map select -> Nation select -> Match (HUD), plus Settings and Pause menus.
//! Mirrors the HTML game's shell flow (menu-screen, browser-screen, nation-screen, in-match HUD).
use crate::camera::MapCamera;
use crate::config::{log, Config};
use crate::input::Input;
use crate::sim::Sim;
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
}

fn make_play(map_idx: usize, nation: u16) -> Play {
    let world = World::load(MAPS[map_idx].data, MAPS[map_idx].ocean).expect("failed to load map");
    let seed = (macroquad::miniquad::date::now() * 1000.0) as u32;
    let mut sim = Sim::new(&world, seed);
    sim.player = Some(nation);
    sim.paused = false;
    let cam = MapCamera::new(world.w(), world.h());
    Play { world, sim, cam, map_idx, nation, target: None, send_frac: 0.25, slider_drag: false, menu_open: false, last_view: Vec2::ZERO }
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
    // CI self tests
    autotest: Option<f64>,
    uitest: Option<f64>,
    t0: f64,
    last_report: f64,
}

impl App {
    pub fn new(worlds: Vec<World>, display: Option<Font>, body: Option<Font>, logo: Option<Texture2D>, cfg: Config) -> App {
        let gui = Gui::new(display, body, logo, cfg.ui_scale);
        let autotest = std::env::var("ATLAS_AUTOTEST").ok().and_then(|v| v.parse::<f64>().ok());
        let uitest = std::env::var("ATLAS_UITEST").ok().and_then(|v| v.parse::<f64>().ok());
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
                "New Game", "map0", "nation:Germany", "Deploy", "select:neighbor", "WAR", "SUPPORT", "pause", "Settings",
                "UI+", "UI-", "Close", "Resume", "speed+", "speed+", "speed-", "zoom+", "zoom-", "STOP", "PEACE", "card-close",
                "pause", "Restart", "pause", "Leave match", "Settings", "Close", "New Game", "back", "back",
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
                    log(&format!("autotest t={:.0}s fps={} days={:.0} fronts={} wars={}", el, get_fps(), sim.game_days, sim.fronts.len(), wars));
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
        g.text_c("Atlas MiniWars - native build", sw / 2.0, sh - 14.0 * u, 13.0, c_faint());
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
        for b in pl.sim.dirty.drain(..) {
            pl.world.refresh_box(b);
        }
        pl.world.flush();

        if g.auto_take("select:neighbor") {
            if let Some(n) = (1..pl.sim.n).find(|&b| b != pi && pl.sim.are_adjacent(p, b as u16)) {
                pl.target = Some(n as u16);
            }
        }

        clear_background(Color::from_rgba(58, 111, 143, 255));
        pl.world.draw(&pl.cam);

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
            pc.pop,
            fmt_num(pl.sim.manpower[pi]),
            fmt_num(pl.sim.committed_total(p))
        );
        g.text(&stats, pill.x + 60.0 * u, pill.y + 44.0 * u, 13.0, c_accent());

        // ---- peace offers
        let mut below = pill.y + pill.h + 10.0 * u;
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

        // ---- news log (left) and leaderboard (right)
        let mut ny = below.max(m + 3.0 * btn + 36.0 * u);
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

        // ---- country card (bottom-left)
        let mut close_card = false;
        if let Some(t) = pl.target {
            let ti = t as usize;
            let cw = if portrait { sw - 2.0 * m } else { (sw - sl_w - 3.0 * m).clamp(240.0 * u, 380.0 * u) };
            let chh = 296.0 * u;
            let cy = if portrait { sl.y - m - chh } else { sh - m - chh };
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
            } else if at_war {
                ("AT WAR", c_red())
            } else if locked {
                ("RECENT PEACE", c_dim())
            } else if !adjacent {
                ("NOT ON YOUR BORDER", c_dim())
            } else {
                ("AT PEACE", c_green())
            };
            g.text(status, x0, card.y + 70.0 * u, 14.0, scol);
            let mut ry = card.y + 100.0 * u;
            let rr = card.x + card.w - 16.0 * u;
            g.text("POPULATION", x0, ry, 13.0, c_dim());
            g.text_r(&format!("{:.1}M", tc.pop), rr, ry, 16.0, c_text());
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
                    let bw = (bw_all - 16.0 * u) / 3.0;
                    if g.button("SUPPORT", Rect::new(x0, by, bw, 48.0 * u), "SUPPORT", Style::Primary, avail > 0.0) {
                        pl.sim.commit(p, t, amount);
                    }
                    if g.button("STOP", Rect::new(x0 + bw + 8.0 * u, by, bw, 48.0 * u), "STOP", Style::Secondary, true) {
                        pl.sim.stop_attack(p, t);
                    }
                    if g.button("PEACE", Rect::new(x0 + 2.0 * (bw + 8.0 * u), by, bw, 48.0 * u), "PEACE", Style::Secondary, true) {
                        pl.sim.propose_peace(p, t);
                    }
                } else if g.button("WAR", Rect::new(x0, by, bw_all, 48.0 * u), "WAR", Style::Danger, adjacent && !locked && avail > 0.0) {
                    if pl.sim.declare_war(p, t) {
                        pl.sim.commit(p, t, amount);
                    }
                }
            }
        }
        if close_card {
            pl.target = None;
        }

        // ---- tap on the map selects a country
        if !modal && g.ptr.tapped() && !g.consumed && !g.over_ui(g.ptr.press_pos) {
            let (cx, cy) = pl.cam.screen_to_cell(g.ptr.pos);
            let id = pl.world.owner_at(cx, cy);
            pl.target = if id == 0 { None } else { Some(id) };
            if id != 0 && id != p && pl.send_frac < 0.05 {
                pl.send_frac = 0.25;
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
