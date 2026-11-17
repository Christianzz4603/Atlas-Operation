//! Adaptive immediate-mode GUI toolkit in the style of the HTML game's shell theme
//! (dark panels, gold accent, chunky press-down buttons, Fredoka display font).
//! Everything is laid out in units `u` = min(screen w, h) / 720 * user scale, so it adapts to
//! phones (landscape or portrait), tablets and desktop windows automatically.
use crate::config::log;
use crate::input::Pointer;
use macroquad::prelude::*;
use std::collections::{HashMap, VecDeque};

// ---- theme (from #shell-theme in the HTML)
pub fn col(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}
pub fn c_bg() -> Color { col(20, 26, 33) }
pub fn c_bg2() -> Color { col(15, 21, 27) }
pub fn c_panel() -> Color { col(28, 36, 46) }
pub fn c_panel2() -> Color { col(34, 44, 56) }
pub fn c_line() -> Color { col(46, 58, 72) }
pub fn c_text() -> Color { col(238, 243, 248) }
pub fn c_dim() -> Color { col(147, 163, 180) }
pub fn c_faint() -> Color { col(95, 114, 133) }
pub fn c_accent() -> Color { col(240, 180, 41) }
pub fn c_accent_dark() -> Color { col(184, 134, 15) }
pub fn c_accent_ink() -> Color { col(32, 23, 10) }
pub fn c_blue() -> Color { col(79, 157, 224) }
pub fn c_red() -> Color { col(224, 106, 94) }
pub fn c_green() -> Color { col(99, 192, 138) }

#[derive(Clone, Copy, PartialEq)]
pub enum Style {
    Primary,
    Secondary,
    Danger,
}

#[derive(Clone, Copy)]
pub enum Glyph {
    Plus,
    Minus,
    Close,
    Pause,
    Back,
}

#[derive(Default)]
pub struct Scroll {
    pub y: f32,
    vel: f32,
    active: bool,
}

pub struct Gui {
    pub display: Option<Font>,
    pub body: Option<Font>,
    pub logo: Option<Texture2D>,
    pub sw: f32,
    pub sh: f32,
    /// layout unit in pixels
    pub u: f32,
    pub user_scale: f32,
    pub ptr: Pointer,
    /// widgets ignore input while false (a modal is open on top)
    pub enabled: bool,
    /// a widget already took this frame's tap
    pub consumed: bool,
    blocks: Vec<Rect>,
    blocks_prev: Vec<Rect>,
    flags: HashMap<String, Option<Texture2D>>,
    // scripted UI test (CI): ids are "clicked" one per second
    pub auto: VecDeque<String>,
    auto_pending: Option<String>,
    auto_timer: f32,
}

impl Gui {
    pub fn new(display: Option<Font>, body: Option<Font>, logo: Option<Texture2D>, user_scale: f32) -> Gui {
        Gui {
            display,
            body,
            logo,
            sw: 1280.0,
            sh: 720.0,
            u: 1.0,
            user_scale,
            ptr: Pointer::default(),
            enabled: true,
            consumed: false,
            blocks: Vec::new(),
            blocks_prev: Vec::new(),
            flags: HashMap::new(),
            auto: VecDeque::new(),
            auto_pending: None,
            auto_timer: 0.0,
        }
    }

    pub fn begin(&mut self, ptr: Pointer, dt: f32) {
        self.ptr = ptr;
        self.sw = screen_width();
        self.sh = screen_height();
        self.u = (self.sw.min(self.sh) / 720.0).clamp(0.45, 3.2) * self.user_scale;
        self.enabled = true;
        self.consumed = false;
        self.blocks_prev = std::mem::take(&mut self.blocks);
        if self.auto_pending.is_none() && !self.auto.is_empty() {
            self.auto_timer += dt;
            if self.auto_timer >= 1.2 {
                self.auto_pending = self.auto.pop_front();
                self.auto_timer = 0.0;
            }
        }
    }

    pub fn end(&mut self) {
        if let Some(id) = self.auto_pending.take() {
            log(&format!("uitest: MISSING {}", id));
        }
    }

    /// Scripted-test hook: true if the test script wants `id` triggered now.
    pub fn auto_take(&mut self, id: &str) -> bool {
        if self.auto_pending.as_deref() == Some(id) {
            self.auto_pending = None;
            log(&format!("uitest: clicked {}", id));
            true
        } else {
            false
        }
    }

    pub fn over_ui(&self, p: Vec2) -> bool {
        self.blocks.iter().any(|r| r.contains(p))
    }

    /// Based on last frame's widgets (the camera runs before the HUD is drawn).
    pub fn was_over_ui(&self, p: Vec2) -> bool {
        self.blocks_prev.iter().any(|r| r.contains(p)) || self.blocks.iter().any(|r| r.contains(p))
    }

    pub fn block(&mut self, r: Rect) {
        self.blocks.push(r);
    }

    // ------------------------------------------------------------------ text
    pub fn px(&self, size: f32) -> u16 {
        (size * self.u).round().clamp(8.0, 320.0) as u16
    }

    pub fn text_w(&self, s: &str, size: f32) -> f32 {
        measure_text(s, self.display.as_ref(), self.px(size), 1.0).width
    }

    fn text_px(&self, s: &str, x: f32, y: f32, px: u16, color: Color) {
        let off = (px as f32 * 0.05).max(1.0);
        let shadow = Color::new(0.0, 0.0, 0.0, 0.55 * color.a);
        for (c, o) in [(shadow, off), (color, 0.0)] {
            match &self.display {
                Some(f) => {
                    draw_text_ex(s, x + o, y + o, TextParams { font: Some(f), font_size: px, color: c, ..Default::default() });
                }
                None => {
                    draw_text(s, x + o, y + o, px as f32, c);
                }
            }
        }
    }

    /// Draw text with its baseline at `y`; `size` in layout units.
    pub fn text(&self, s: &str, x: f32, y: f32, size: f32, color: Color) {
        self.text_px(s, x, y, self.px(size), color);
    }

    pub fn text_r(&self, s: &str, right: f32, y: f32, size: f32, color: Color) {
        let w = self.text_w(s, size);
        self.text(s, right - w, y, size, color);
    }

    pub fn text_c(&self, s: &str, cx: f32, y: f32, size: f32, color: Color) {
        let w = self.text_w(s, size);
        self.text(s, cx - w / 2.0, y, size, color);
    }

    /// Centered in `r`, shrinking the font if the text doesn't fit.
    pub fn text_in(&self, s: &str, r: Rect, size: f32, color: Color) {
        let mut sz = size;
        let max_w = r.w - 12.0 * self.u;
        while sz > 7.0 && self.text_w(s, sz) > max_w {
            sz -= 0.5;
        }
        let px = self.px(sz);
        let d = measure_text(s, self.display.as_ref(), px, 1.0);
        let x = r.x + (r.w - d.width) / 2.0;
        let y = r.y + r.h / 2.0 + px as f32 * 0.32;
        self.text_px(s, x, y, px, color);
    }

    // ------------------------------------------------------------------ shapes
    pub fn rrect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        let r = r.min(w / 2.0).min(h / 2.0);
        if r < 1.0 {
            draw_rectangle(x, y, w, h, c);
            return;
        }
        draw_rectangle(x + r, y, w - 2.0 * r, h, c);
        draw_rectangle(x, y + r, r, h - 2.0 * r, c);
        draw_rectangle(x + w - r, y + r, r, h - 2.0 * r, c);
        draw_circle(x + r, y + r, r, c);
        draw_circle(x + w - r, y + r, r, c);
        draw_circle(x + r, y + h - r, r, c);
        draw_circle(x + w - r, y + h - r, r, c);
    }

    pub fn panel(&mut self, r: Rect, fill: Color) {
        self.rrect(r.x, r.y, r.w, r.h, 12.0 * self.u, fill);
        self.blocks.push(r);
    }

    /// Full-screen background of the shell screens.
    pub fn screen_bg(&self) {
        let (a, b) = (col(22, 29, 37), c_bg2());
        let bands = 24;
        for i in 0..bands {
            let t = i as f32 / (bands - 1) as f32;
            let c = Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1.0);
            let y0 = self.sh * i as f32 / bands as f32;
            draw_rectangle(0.0, y0, self.sw, self.sh / bands as f32 + 1.0, c);
        }
        draw_circle(self.sw * 0.08, -self.sh * 0.05, self.sh * 0.55, Color::new(0.94, 0.7, 0.16, 0.06));
        draw_circle(self.sw * 0.85, self.sh * 1.05, self.sh * 0.6, Color::new(0.31, 0.62, 0.88, 0.07));
    }

    pub fn draw_logo(&self, x: f32, y: f32, size: f32) {
        if let Some(t) = &self.logo {
            draw_texture_ex(t, x, y, WHITE, DrawTextureParams { dest_size: Some(vec2(size, size)), ..Default::default() });
        }
    }

    // ------------------------------------------------------------------ interaction
    /// (clicked, pressed-visual)
    fn interact(&mut self, id: &str, r: Rect, enabled: bool) -> (bool, bool) {
        let live = self.enabled && enabled;
        let inside = r.contains(self.ptr.press_pos) && r.contains(self.ptr.pos);
        let pressed = live && self.ptr.down && inside && !self.ptr.moved;
        let mut clicked = live && !self.consumed && self.ptr.released && !self.ptr.moved && inside;
        if live && !self.consumed && self.auto_take(id) {
            clicked = true;
        }
        if clicked {
            self.consumed = true;
        }
        (clicked, pressed)
    }

    /// Invisible hit area (cards inside scroll lists, etc.).
    pub fn tapped(&mut self, id: &str, r: Rect) -> bool {
        self.interact(id, r, true).0
    }

    fn button_face(&mut self, r: Rect, style: Style, enabled: bool, pressed: bool) -> Rect {
        let (face, shadow) = if !enabled {
            (col(38, 46, 56), col(28, 34, 42))
        } else {
            match style {
                Style::Primary => (c_accent(), c_accent_dark()),
                Style::Secondary => (c_panel2(), col(14, 18, 23)),
                Style::Danger => (c_red(), col(150, 62, 54)),
            }
        };
        let d = (r.h * 0.11).clamp(2.0 * self.u, 7.0 * self.u);
        let off = if pressed { d * 0.8 } else { 0.0 };
        let rad = 11.0 * self.u;
        self.rrect(r.x, r.y + d, r.w, r.h - d, rad, shadow);
        self.rrect(r.x, r.y + off, r.w, r.h - d, rad, face);
        self.blocks.push(r);
        Rect::new(r.x, r.y + off, r.w, r.h - d)
    }

    fn label_color(&self, style: Style, enabled: bool) -> Color {
        if !enabled {
            return c_faint();
        }
        match style {
            Style::Primary => c_accent_ink(),
            Style::Secondary => c_text(),
            Style::Danger => col(40, 14, 10),
        }
    }

    pub fn button(&mut self, id: &str, r: Rect, label: &str, style: Style, enabled: bool) -> bool {
        let (clicked, pressed) = self.interact(id, r, enabled);
        let face = self.button_face(r, style, enabled, pressed);
        let c = self.label_color(style, enabled);
        let c = if style == Style::Primary || style == Style::Danger { c } else { c };
        // no text shadow on light faces
        if style == Style::Secondary || !enabled {
            self.text_in(label, face, 19.0, c);
        } else {
            let mut sz = 19.0;
            let max_w = face.w - 12.0 * self.u;
            while sz > 7.0 && self.text_w(label, sz) > max_w {
                sz -= 0.5;
            }
            let px = self.px(sz);
            let d = measure_text(label, self.display.as_ref(), px, 1.0);
            let x = face.x + (face.w - d.width) / 2.0;
            let y = face.y + face.h / 2.0 + px as f32 * 0.32;
            match &self.display {
                Some(f) => {
                    draw_text_ex(label, x, y, TextParams { font: Some(f), font_size: px, color: c, ..Default::default() });
                }
                None => {
                    draw_text(label, x, y, px as f32, c);
                }
            }
        }
        clicked
    }

    pub fn glyph_button(&mut self, id: &str, r: Rect, glyph: Glyph, style: Style, enabled: bool) -> bool {
        let (clicked, pressed) = self.interact(id, r, enabled);
        let face = self.button_face(r, style, enabled, pressed);
        let c = self.label_color(style, enabled);
        let (cx, cy) = (face.x + face.w / 2.0, face.y + face.h / 2.0);
        let s = face.h.min(face.w) * 0.26;
        let t = (face.h.min(face.w) * 0.11).max(2.0);
        match glyph {
            Glyph::Plus => {
                draw_line(cx - s, cy, cx + s, cy, t, c);
                draw_line(cx, cy - s, cx, cy + s, t, c);
            }
            Glyph::Minus => draw_line(cx - s, cy, cx + s, cy, t, c),
            Glyph::Close => {
                draw_line(cx - s, cy - s, cx + s, cy + s, t, c);
                draw_line(cx - s, cy + s, cx + s, cy - s, t, c);
            }
            Glyph::Pause => {
                draw_rectangle(cx - s * 0.9, cy - s, s * 0.7, s * 2.0, c);
                draw_rectangle(cx + s * 0.2, cy - s, s * 0.7, s * 2.0, c);
            }
            Glyph::Back => {
                draw_line(cx + s * 0.5, cy - s, cx - s * 0.5, cy, t, c);
                draw_line(cx - s * 0.5, cy, cx + s * 0.5, cy + s, t, c);
            }
        }
        clicked
    }

    /// Horizontal slider with the HTML game's square "cube" handle.
    pub fn slider(&mut self, r: Rect, frac: &mut f32, drag: &mut bool, enabled: bool) {
        let cube = r.h * 1.7;
        let hit = Rect::new(r.x - cube * 0.5, r.y - cube * 0.5, r.w + cube, r.h + cube);
        if self.enabled && enabled && self.ptr.pressed && hit.contains(self.ptr.pos) {
            *drag = true;
        }
        if !self.ptr.down {
            *drag = false;
        }
        if *drag && enabled {
            *frac = ((self.ptr.pos.x - r.x - cube * 0.5) / (r.w - cube).max(1.0)).clamp(0.0, 1.0);
            self.consumed = true;
        }
        self.rrect(r.x, r.y, r.w, r.h, r.h * 0.5, col(12, 16, 21));
        let hx = r.x + (r.w - cube) * *frac;
        let fill_w = (hx + cube * 0.5 - r.x).max(0.0);
        if enabled && fill_w > 0.0 {
            self.rrect(r.x, r.y, fill_w, r.h, r.h * 0.5, c_accent_dark());
        }
        let cy = r.y + r.h / 2.0 - cube / 2.0;
        draw_rectangle(hx - 1.5, cy - 1.5, cube + 3.0, cube + 3.0, col(8, 10, 13));
        draw_rectangle(hx, cy, cube, cube, if enabled { c_accent() } else { c_faint() });
        self.blocks.push(hit);
    }

    // ------------------------------------------------------------------ scrolling
    pub fn scroll(&mut self, sc: &mut Scroll, view: Rect, content_h: f32, dt: f32) {
        let max = (content_h - view.h).max(0.0);
        if self.enabled {
            if self.ptr.pressed && view.contains(self.ptr.pos) {
                sc.active = true;
                sc.vel = 0.0;
            }
            if sc.active && self.ptr.down && self.ptr.moved {
                sc.y -= self.ptr.delta.y;
                sc.vel = sc.vel * 0.5 + (-self.ptr.delta.y / dt.max(1e-3)) * 0.5;
            }
            if view.contains(self.ptr.pos) && self.ptr.wheel != 0.0 {
                sc.y -= self.ptr.wheel.signum() * 70.0 * self.u;
            }
        }
        if !self.ptr.down {
            sc.active = false;
        }
        if !sc.active && sc.vel.abs() > 8.0 {
            sc.y += sc.vel * dt;
            sc.vel *= (-5.0 * dt).exp();
        } else if !sc.active {
            sc.vel = 0.0;
        }
        sc.y = sc.y.clamp(0.0, max);
    }

    // ------------------------------------------------------------------ flags
    pub fn flag(&mut self, iso: &str, r: Rect) -> bool {
        if iso.is_empty() {
            return false;
        }
        let tex = self.flags.entry(iso.to_string()).or_insert_with(|| {
            crate::flags::flag_bytes(iso)
                .and_then(|b| Image::from_file_with_format(b, None).ok())
                .map(|img| {
                    let t = Texture2D::from_image(&img);
                    t.set_filter(FilterMode::Nearest);
                    t
                })
        });
        if let Some(t) = tex {
            let (tw, th) = (t.width().max(1.0), t.height().max(1.0));
            let k = (r.w / tw).min(r.h / th);
            let (dw, dh) = (tw * k, th * k);
            draw_texture_ex(
                t,
                r.x + (r.w - dw) / 2.0,
                r.y + (r.h - dh) / 2.0,
                WHITE,
                DrawTextureParams { dest_size: Some(vec2(dw, dh)), ..Default::default() },
            );
            true
        } else {
            false
        }
    }

    pub fn res_icon(&mut self, name: &str, r: Rect) {
        let key = format!("res:{}", name);
        let tex = self.flags.entry(key).or_insert_with(|| {
            crate::restex::res_bytes(name)
                .and_then(|b| Image::from_file_with_format(b, None).ok())
                .map(|img| {
                    let t = Texture2D::from_image(&img);
                    t.set_filter(FilterMode::Linear);
                    t
                })
        });
        if let Some(t) = tex {
            let (tw, th) = (t.width().max(1.0), t.height().max(1.0));
            let k = (r.w / tw).min(r.h / th);
            let (dw, dh) = (tw * k, th * k);
            draw_texture_ex(t, r.x + (r.w - dw) / 2.0, r.y + (r.h - dh) / 2.0, WHITE, DrawTextureParams { dest_size: Some(vec2(dw, dh)), ..Default::default() });
        }
    }

    // ------------------------------------------------------------------ shell header (Back + title)
    pub fn header(&mut self, title: &str) -> bool {
        let u = self.u;
        let h = 66.0 * u;
        draw_rectangle(0.0, 0.0, self.sw, h, c_panel());
        draw_rectangle(0.0, h - 1.0, self.sw, 1.0, c_line());
        self.blocks.push(Rect::new(0.0, 0.0, self.sw, h));
        let m = 12.0 * u;
        let back = self.button("back", Rect::new(m, m, 112.0 * u, h - 2.0 * m + 4.0 * u), "Back", Style::Secondary, true);
        self.text_c(title, self.sw / 2.0 + 40.0 * u, h / 2.0 + 8.0 * u, 24.0, c_text());
        back
    }
}
