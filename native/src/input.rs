//! One unified pointer (mouse or touch) with edge events, drag slop and pinch.
use macroquad::prelude::*;

#[derive(Clone, Copy, Default)]
pub struct Pointer {
    pub pos: Vec2,
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
    pub press_pos: Vec2,
    /// moved further than the tap slop since the press
    pub moved: bool,
    /// movement since last frame while held
    pub delta: Vec2,
    /// (centre, scale ratio since last frame) while two fingers are down
    pub pinch: Option<(Vec2, f32)>,
    pub wheel: f32,
}

impl Pointer {
    pub fn tapped(&self) -> bool {
        self.released && !self.moved
    }
}

#[derive(Default)]
pub struct Input {
    prev_down: bool,
    prev_pos: Vec2,
    press_pos: Vec2,
    moved: bool,
    pinch_dist: f32,
    suppress: bool,
    last_was_touch: bool,
}

impl Input {
    pub fn new() -> Input {
        Input::default()
    }

    pub fn poll(&mut self) -> Pointer {
        let (sw, sh) = (screen_width(), screen_height());
        // finger jitter tolerance scales with the screen, not with dpi
        let slop = (sw.min(sh) * 0.022).max(8.0);
        let ts = touches();
        let mouse = Vec2::from(mouse_position());
        let mut p = Pointer { wheel: mouse_wheel().1, ..Default::default() };

        let phys_down;
        let mut pos;
        if ts.len() >= 2 {
            let (a, b) = (ts[0].position, ts[1].position);
            let d = a.distance(b);
            let c = (a + b) * 0.5;
            if self.pinch_dist > 1.0 && d > 1.0 {
                p.pinch = Some((c, d / self.pinch_dist));
            }
            self.pinch_dist = d;
            self.suppress = true;
            self.moved = true;
            phys_down = true;
            pos = c;
            self.last_was_touch = true;
        } else {
            self.pinch_dist = 0.0;
            if let Some(t) = ts.first() {
                phys_down = true;
                pos = t.position;
                self.last_was_touch = true;
            } else if is_mouse_button_down(MouseButton::Left) {
                phys_down = true;
                pos = mouse;
                self.last_was_touch = false;
            } else {
                phys_down = false;
                pos = if self.last_was_touch && self.prev_down { self.prev_pos } else { mouse };
            }
        }
        let mut down = phys_down;
        if self.suppress {
            if phys_down {
                down = false; // after a pinch, ignore the remaining finger until everything is lifted
            } else {
                self.suppress = false;
            }
        }
        p.pressed = down && !self.prev_down;
        p.released = !down && self.prev_down;
        if p.pressed {
            self.press_pos = pos;
            self.moved = false;
        }
        if down && (pos - self.press_pos).length() > slop {
            self.moved = true;
        }
        p.delta = if down && self.prev_down { pos - self.prev_pos } else { Vec2::ZERO };
        if !down && !p.released {
            pos = mouse;
        }
        p.down = down;
        p.pos = pos;
        p.press_pos = self.press_pos;
        p.moved = self.moved;
        self.prev_down = down;
        if down {
            self.prev_pos = pos;
        }
        p
    }
}
