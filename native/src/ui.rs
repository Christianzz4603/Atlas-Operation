//! Immediate-mode HUD: top bar, speed controls, country panel, peace offers, leaderboard, news log.
use crate::camera::MapCamera;
use crate::sim::Sim;
use crate::world::World;
use macroquad::prelude::*;

const SPEEDS: [f32; 5] = [0.5, 1.0, 2.0, 4.0, 8.0];
const GOLD: Color = Color::new(1.0, 0.878, 0.541, 1.0);

pub fn label(font: &Option<Font>, s: &str, x: f32, y: f32, size: u16, color: Color) {
    // drop shadow keeps text readable over any terrain
    for (c, o) in [(Color::new(0.0, 0.0, 0.0, 0.75 * color.a), 1.5f32), (color, 0.0f32)] {
        match font {
            Some(f) => {
                draw_text_ex(
                    s,
                    x + o,
                    y + o,
                    TextParams { font: Some(f), font_size: size, color: c, ..Default::default() },
                );
            }
            None => {
                draw_text(s, x + o, y + o, size as f32, c);
            }
        }
    }
}

fn text_w(font: &Option<Font>, s: &str, size: u16) -> f32 {
    measure_text(s, font.as_ref(), size, 1.0).width
}

fn fmt_num(v: f32) -> String {
    if v >= 1.0e6 {
        format!("{:.1}M", v / 1.0e6)
    } else if v >= 1.0e3 {
        format!("{:.0}K", v / 1.0e3)
    } else {
        format!("{:.0}", v)
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

fn button(font: &Option<Font>, r: Rect, text: &str, enabled: bool, tap: Option<Vec2>, size: u16) -> bool {
    let bg = if enabled { Color::from_rgba(38, 72, 118, 235) } else { Color::from_rgba(52, 52, 56, 215) };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(1.0, 1.0, 1.0, if enabled { 0.8 } else { 0.3 }));
    let tw = text_w(font, text, size);
    let col = if enabled { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.45) };
    label(font, text, r.x + (r.w - tw) / 2.0, r.y + r.h / 2.0 + size as f32 * 0.32, size, col);
    enabled && tap.map_or(false, |t| r.contains(t))
}

#[derive(Default)]
pub struct Ui {
    pub selected: Option<u16>,
    pub observer: bool,
}

impl Ui {
    pub fn frame(&mut self, font: &Option<Font>, sim: &mut Sim, world: &World, cam: &MapCamera, tap: Option<Vec2>) {
        let (sw, sh) = (screen_width(), screen_height());
        let s = (sh / 720.0).clamp(0.9, 1.8);
        let fs = (20.0 * s) as u16;
        let fs_small = (16.0 * s) as u16;
        let bar_h = 46.0 * s;
        let mut tap = tap;
        let mouse = Vec2::from(mouse_position());

        // ---------- top bar
        draw_rectangle(0.0, 0.0, sw, bar_h, Color::new(0.04, 0.07, 0.12, 0.82));
        let started = sim.player.is_some() || self.observer;
        let title = match sim.player {
            Some(p) => format!(
                "{}   MANPOWER {} (+{} ON FRONT)   {}",
                sim.names[p as usize],
                fmt_num(sim.manpower[p as usize]),
                fmt_num(sim.committed_total(p)),
                date_string(sim.game_days)
            ),
            None if self.observer => format!("OBSERVER   {}", date_string(sim.game_days)),
            None => "ATLAS MINIWARS".to_string(),
        };
        label(font, &title, 12.0 * s, bar_h * 0.66, fs, WHITE);

        // speed controls (right side of the bar)
        let bw = 40.0 * s;
        let bh = 32.0 * s;
        let by = (bar_h - bh) / 2.0;
        let mut x = sw - 12.0 * s - bw;
        let plus = Rect::new(x, by, bw, bh);
        x -= bw + 6.0 * s;
        let pause = Rect::new(x, by, bw, bh);
        x -= bw + 6.0 * s;
        let minus = Rect::new(x, by, bw, bh);
        let idx = SPEEDS.iter().position(|v| (*v - sim.speed).abs() < 0.01).unwrap_or(1);
        let speed_lbl = format!("{}x", sim.speed);
        label(font, &speed_lbl, x - text_w(font, &speed_lbl, fs) - 10.0 * s, bar_h * 0.66, fs, GOLD);
        let mut consumed = false;
        if button(font, plus, "+", started, tap, fs) {
            sim.speed = SPEEDS[(idx + 1).min(SPEEDS.len() - 1)];
            consumed = true;
        }
        if button(font, pause, if sim.paused { ">" } else { "||" }, started, tap, fs) {
            sim.paused = !sim.paused;
            consumed = true;
        }
        if button(font, minus, "-", started, tap, fs) {
            sim.speed = SPEEDS[idx.saturating_sub(1)];
            consumed = true;
        }
        if is_key_pressed(KeyCode::Space) && started {
            sim.paused = !sim.paused;
        }
        if tap.map_or(false, |t| t.y < bar_h) {
            consumed = true;
        }

        // ---------- spawn picker
        if !started {
            let msg = "TAP A COUNTRY TO PLAY AS IT";
            label(font, msg, (sw - text_w(font, msg, fs * 2)) / 2.0, sh * 0.16, fs * 2, WHITE);
            let hint = "(press O to just watch the AI fight)";
            label(font, hint, (sw - text_w(font, hint, fs)) / 2.0, sh * 0.16 + 30.0 * s, fs, Color::new(1.0, 1.0, 1.0, 0.8));
            if is_key_pressed(KeyCode::O) {
                self.observer = true;
                sim.paused = false;
            }
            if !consumed {
                if let Some(t) = tap {
                    let (cx, cy) = cam.screen_to_cell(t);
                    let id = world.owner_at(cx, cy);
                    if id != 0 {
                        sim.player = Some(id);
                        sim.paused = false;
                        self.selected = None;
                    }
                }
            }
            self.hover(font, world, cam, mouse, sh, fs);
            return;
        }
        if consumed {
            tap = None;
        }

        // ---------- peace offers
        if let (Some(p), Some(&from)) = (sim.player, sim.peace_offers.first()) {
            let w = (sw - 20.0).min(420.0 * s);
            let r = Rect::new((sw - w) / 2.0, bar_h + 8.0 * s, w, 82.0 * s);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.07, 0.12, 0.92));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, GOLD);
            label(font, &format!("{} ASKS FOR PEACE", sim.names[from as usize]), r.x + 12.0 * s, r.y + 26.0 * s, fs, WHITE);
            let b1 = Rect::new(r.x + 12.0 * s, r.y + 38.0 * s, (r.w - 36.0 * s) / 2.0, 34.0 * s);
            let b2 = Rect::new(b1.x + b1.w + 12.0 * s, b1.y, b1.w, b1.h);
            if button(font, b1, "ACCEPT", true, tap, fs) {
                sim.make_peace(p, from);
                tap = None;
            } else if button(font, b2, "DECLINE", true, tap, fs) {
                sim.decline_offer(from);
                tap = None;
            }
            if tap.map_or(false, |t| r.contains(t)) {
                tap = None;
            }
        }

        // ---------- selected-country panel
        if let (Some(p), Some(c)) = (sim.player, self.selected) {
            let w = (sw - 20.0).min(470.0 * s);
            let h = 156.0 * s;
            let r = Rect::new((sw - w) / 2.0, sh - h - 10.0 * s, w, h);
            draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.07, 0.12, 0.9));
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, Color::new(1.0, 1.0, 1.0, 0.6));
            let ci = c as usize;
            let cd = &world.map.countries[ci];
            let share = 100.0 * sim.cells[ci] as f32 / sim.orig_cells[ci].max(1) as f32;
            label(font, &sim.names[ci], r.x + 14.0 * s, r.y + 28.0 * s, fs + 4, WHITE);
            label(
                font,
                &format!("POP {:.1}M   ARMY {}K   MANPOWER {}   LAND {:.0}%", cd.pop, cd.military.round(), fmt_num(sim.manpower[ci]), share),
                r.x + 14.0 * s,
                r.y + 54.0 * s,
                fs_small,
                Color::new(1.0, 1.0, 1.0, 0.85),
            );
            let by = r.y + h - 48.0 * s;
            let bh = 38.0 * s;
            if c == p {
                let st = if sim.capital_fallen[ci] { "YOUR CAPITAL HAS FALLEN!" } else { "YOUR NATION" };
                label(font, st, r.x + 14.0 * s, r.y + 82.0 * s, fs, GOLD);
            } else if sim.eliminated[ci] {
                label(font, "ELIMINATED", r.x + 14.0 * s, r.y + 82.0 * s, fs, RED);
            } else if sim.is_at_war(p, c) {
                label(font, "AT WAR", r.x + 14.0 * s, r.y + 82.0 * s, fs, Color::from_rgba(255, 110, 110, 255));
                let bw = (r.w - 14.0 * s * 4.0) / 3.0;
                let b1 = Rect::new(r.x + 14.0 * s, by, bw, bh);
                let b2 = Rect::new(b1.x + bw + 14.0 * s, by, bw, bh);
                let b3 = Rect::new(b2.x + bw + 14.0 * s, by, bw, bh);
                if button(font, b1, "SEND 25%", true, tap, fs) {
                    sim.commit_fraction(p, c, 0.25);
                    tap = None;
                } else if button(font, b2, "SEND 50%", true, tap, fs) {
                    sim.commit_fraction(p, c, 0.50);
                    tap = None;
                } else if button(font, b3, "PEACE", true, tap, fs) {
                    sim.propose_peace(p, c);
                    tap = None;
                }
            } else {
                let adjacent = sim.are_adjacent(p, c);
                let locked = sim.war_locked(p, c);
                let st = if locked {
                    "RECENT PEACE - WAIT A FEW SECONDS"
                } else if !adjacent {
                    "NOT ON YOUR BORDER (no naval invasions yet)"
                } else {
                    "AT PEACE"
                };
                label(font, st, r.x + 14.0 * s, r.y + 82.0 * s, fs, Color::new(1.0, 1.0, 1.0, 0.85));
                let b1 = Rect::new(r.x + 14.0 * s, by, r.w - 28.0 * s, bh);
                if button(font, b1, "DECLARE WAR + ATTACK (25%)", adjacent && !locked, tap, fs) {
                    if sim.declare_war(p, c) {
                        sim.commit_fraction(p, c, 0.25);
                    }
                    tap = None;
                }
            }
            if tap.map_or(false, |t| r.contains(t)) {
                tap = None;
            }
        }

        // ---------- map tap -> select
        if let Some(t) = tap {
            let (cx, cy) = cam.screen_to_cell(t);
            let id = world.owner_at(cx, cy);
            self.selected = if id == 0 { None } else { Some(id) };
        }

        // ---------- leaderboard (territory share)
        let mut rank: Vec<usize> = (1..sim.n).filter(|&i| !sim.eliminated[i] && sim.cells[i] > 0).collect();
        rank.sort_by(|&a, &b| sim.cells[b].cmp(&sim.cells[a]));
        let total: f32 = sim.cells.iter().sum::<u32>() as f32;
        let lw = 210.0 * s;
        let mut y = bar_h + 22.0 * s;
        for (k, &id) in rank.iter().take(6).enumerate() {
            let me = sim.player == Some(id as u16);
            let txt = format!("{} {} {:.1}%", k + 1, sim.names[id], 100.0 * sim.cells[id] as f32 / total.max(1.0));
            let col = if me { GOLD } else { WHITE };
            label(font, &txt, sw - lw, y, fs_small, col);
            y += 20.0 * s;
        }

        // ---------- news log
        let mut y = bar_h + 22.0 * s;
        for (msg, t) in sim.log.iter().rev().take(5) {
            let age = sim.time - *t;
            if age > 9.0 {
                continue;
            }
            let a = if age > 6.0 { (9.0 - age) / 3.0 } else { 1.0 };
            label(font, msg, 12.0 * s, y, fs_small, Color::new(1.0, 0.94, 0.7, a));
            y += 20.0 * s;
        }

        // ---------- game over
        if let Some(p) = sim.player {
            if sim.eliminated[p as usize] {
                let msg = "YOU WERE ELIMINATED - PRESS R TO RESTART";
                label(font, msg, (sw - text_w(font, msg, fs)) / 2.0, sh * 0.45, fs, RED);
            }
        }
        self.hover(font, world, cam, mouse, sh, fs_small);
    }

    fn hover(&self, font: &Option<Font>, world: &World, cam: &MapCamera, mouse: Vec2, sh: f32, size: u16) {
        let (cx, cy) = cam.screen_to_cell(mouse);
        let id = world.owner_at(cx, cy);
        if id != 0 {
            label(font, world.name(id), 12.0, sh - 8.0, size, GOLD);
        }
    }
}
