//! Pan / zoom camera (port of the JS Camera smoothing constants). Input comes from `Pointer`.
use crate::input::Pointer;
use macroquad::prelude::*;

const ZOOM_TAU: f32 = 0.075;
const PAN_TAU: f32 = 0.06;
const KEY_SPEED: f32 = 1100.0;
const FRICTION: f32 = 6.5;
const MAX_SCALE_MULT: f32 = 40.0;

pub struct MapCamera {
    map: Vec2,
    view: Vec2,
    pub scale: f32,
    target_scale: f32,
    min_scale: f32,
    max_scale: f32,
    pub offset: Vec2,
    target_offset: Vec2,
    vel: Vec2,
    anchor: Option<(Vec2, Vec2)>, // (screen point, world point) kept fixed while zooming
    dragging: bool,
    pan_active: bool,
    ready: bool,
}

fn clamp_axis(o: f32, view: f32, content: f32) -> f32 {
    if content <= view {
        (view - content) / 2.0
    } else {
        o.clamp(view - content, 0.0)
    }
}

impl MapCamera {
    pub fn new(map_w: f32, map_h: f32) -> Self {
        MapCamera {
            map: vec2(map_w, map_h),
            view: vec2(1.0, 1.0),
            scale: 1.0,
            target_scale: 1.0,
            min_scale: 1.0,
            max_scale: 40.0,
            offset: Vec2::ZERO,
            target_offset: Vec2::ZERO,
            vel: Vec2::ZERO,
            anchor: None,
            dragging: false,
            pan_active: false,
            ready: false,
        }
    }

    pub fn resize(&mut self, view: Vec2) {
        self.view = view;
        self.min_scale = (view.x / self.map.x).max(view.y / self.map.y);
        self.max_scale = self.min_scale * MAX_SCALE_MULT;
        if !self.ready {
            self.scale = self.min_scale;
            self.target_scale = self.min_scale;
            self.ready = true;
        }
        self.target_scale = self.target_scale.clamp(self.min_scale, self.max_scale);
        self.scale = self.scale.clamp(self.min_scale, self.max_scale);
        self.clamp_offsets();
    }

    fn clamp_offsets(&mut self) {
        let c = self.map * self.scale;
        self.offset.x = clamp_axis(self.offset.x, self.view.x, c.x);
        self.offset.y = clamp_axis(self.offset.y, self.view.y, c.y);
        let t = self.map * self.target_scale;
        self.target_offset.x = clamp_axis(self.target_offset.x, self.view.x, t.x);
        self.target_offset.y = clamp_axis(self.target_offset.y, self.view.y, t.y);
    }

    pub fn zoom_at(&mut self, p: Vec2, next: f32) {
        let t = next.clamp(self.min_scale, self.max_scale);
        if (t - self.target_scale).abs() < f32::EPSILON {
            return;
        }
        self.anchor = Some((p, (p - self.offset) / self.scale));
        self.target_scale = t;
        self.vel = Vec2::ZERO;
    }

    /// Zoom around the middle of the screen (the on-screen +/- buttons).
    pub fn zoom_center(&mut self, factor: f32) {
        self.zoom_at(self.view * 0.5, self.target_scale * factor);
    }

    pub fn screen_to_cell(&self, p: Vec2) -> (i32, i32) {
        let w = (p - self.offset) / self.scale;
        (w.x.floor() as i32, w.y.floor() as i32)
    }

    /// `over_ui`: the pointer is currently over a HUD element (don't pan / zoom through it).
    pub fn update(&mut self, dt: f32, ptr: &Pointer, over_ui: bool) {
        let dt = dt.max(1e-4);
        if ptr.pressed {
            self.pan_active = !over_ui;
            self.vel = Vec2::ZERO;
        }
        self.dragging = false;
        if ptr.down && self.pan_active && ptr.moved {
            self.target_offset += ptr.delta;
            self.vel = self.vel * 0.5 + (ptr.delta / dt) * 0.5;
            self.dragging = true;
        }
        if let Some((c, ratio)) = ptr.pinch {
            self.zoom_at(c, self.target_scale * ratio);
        }
        if ptr.wheel != 0.0 && !over_ui {
            let f = 1.18f32.powf(ptr.wheel.signum());
            self.zoom_at(ptr.pos, self.target_scale * f);
        }

        // keyboard pan / zoom
        let mut k = Vec2::ZERO;
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) { k.x += 1.0; }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) { k.x -= 1.0; }
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) { k.y += 1.0; }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) { k.y -= 1.0; }
        if k != Vec2::ZERO {
            self.target_offset += k * KEY_SPEED * dt;
            self.vel = Vec2::ZERO;
        }
        if is_key_down(KeyCode::Q) { self.zoom_at(self.view * 0.5, self.target_scale * (1.0 + 1.6 * dt)); }
        if is_key_down(KeyCode::E) { self.zoom_at(self.view * 0.5, self.target_scale / (1.0 + 1.6 * dt)); }

        // inertia
        if !self.dragging && !ptr.down && (self.vel.x.abs() > 1.0 || self.vel.y.abs() > 1.0) {
            self.target_offset += self.vel * dt;
            self.vel *= (-FRICTION * dt).exp();
        }

        // smoothing
        let zk = 1.0 - (-dt / ZOOM_TAU).exp();
        if (self.target_scale - self.scale).abs() > 0.0001 {
            self.scale += (self.target_scale - self.scale) * zk;
            if let Some((s, w)) = self.anchor {
                self.target_offset = s - w * self.scale;
            }
        } else {
            self.scale = self.target_scale;
            self.anchor = None;
        }
        self.clamp_offsets();
        let pk = if self.dragging { 1.0 } else { 1.0 - (-dt / PAN_TAU).exp() };
        self.offset += (self.target_offset - self.offset) * pk;
        self.clamp_offsets();
    }
}
