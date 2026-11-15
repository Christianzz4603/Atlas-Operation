//! The territory grid + its CPU-shaded texture.
//! Shading is a port of World._cellRGB / _bumpShade / occupiedShade / Color.* from the JS game
//! (province patches/lines are not ported yet).
use crate::camera::MapCamera;
use crate::mapdata::MapData;
use macroquad::prelude::*;

const LAND_NOISE_SEED: i32 = 4261;
const LAND_NOISE_AMOUNT: f32 = 0.035;
const INNER_LINE_DARKEN: f32 = 0.28;
const BUMP_LIGHT: f32 = 0.24;
const BUMP_SHADOW: f32 = 0.30;
const OCC_LIGHTEN: f32 = 0.42;
const OCC_STRIPE_W: i32 = 4;
const OCC_STRIPE_DARKEN: f32 = 0.12;
const DIRTY_PAD: i32 = 2;

fn darken(c: [u8; 3], a: f32) -> [u8; 3] {
    let f = |v: u8| ((v as f32) * (1.0 - a)).round().max(0.0) as u8;
    [f(c[0]), f(c[1]), f(c[2])]
}

fn lighten(c: [u8; 3], a: f32) -> [u8; 3] {
    let f = |v: u8| ((v as f32) + (255.0 - v as f32) * a).round().min(255.0) as u8;
    [f(c[0]), f(c[1]), f(c[2])]
}

/// Bit-exact port of Utils.hash01 (including the JS double-precision multiply).
fn hash01(x: i32, y: i32, seed: i32) -> f32 {
    let mut h = ((x as i64) * 374761393 + (y as i64) * 668265263 + seed as i64) as i32;
    let a = h ^ (((h as u32) >> 13) as i32);
    let p = (a as f64) * 1274126177.0;
    let n = p.rem_euclid(4294967296.0);
    h = if n >= 2147483648.0 { (n - 4294967296.0) as i32 } else { n as i32 };
    h ^= ((h as u32) >> 16) as i32;
    ((h as u32) % 10000) as f32 / 10000.0
}

fn land_noise(x: i32, y: i32, base: [u8; 3]) -> [u8; 3] {
    let r = hash01(x, y, LAND_NOISE_SEED);
    if r < 0.5 {
        darken(base, r * LAND_NOISE_AMOUNT * 2.0)
    } else {
        lighten(base, (r - 0.5) * LAND_NOISE_AMOUNT * 2.0)
    }
}

/// Color.occupiedShade: lightened, diagonally striped colour for conquered land.
fn occupied_shade(x: i32, y: i32, base: [u8; 3]) -> [u8; 3] {
    let w = lighten(base, OCC_LIGHTEN);
    let b = ((x - y) % OCC_STRIPE_W + OCC_STRIPE_W) % OCC_STRIPE_W;
    if b < OCC_STRIPE_W / 2 { w } else { darken(w, OCC_STRIPE_DARKEN) }
}

pub struct World {
    pub map: MapData,
    pub orig_owner: Vec<u16>,
    image: Image,
    land_tex: Texture2D,
    ocean_tex: Texture2D,
    tex_dirty: bool,
}

impl World {
    pub fn load(map_z: &[u8], ocean_png: &[u8]) -> Result<World, String> {
        let map = MapData::load(map_z)?;
        let image = Image::gen_image_color(map.w as u16, map.h as u16, Color::new(0.0, 0.0, 0.0, 0.0));
        let land_tex = Texture2D::from_image(&image);
        land_tex.set_filter(FilterMode::Nearest);

        let ocean_img = Image::from_file_with_format(ocean_png, None)
            .map_err(|e| format!("ocean png: {:?}", e))?;
        let ocean_tex = Texture2D::from_image(&ocean_img);
        ocean_tex.set_filter(FilterMode::Nearest);

        let orig_owner = map.owner.clone();
        let mut world = World { map, orig_owner, image, land_tex, ocean_tex, tex_dirty: false };
        world.repaint_all();
        Ok(world)
    }

    pub fn w(&self) -> f32 {
        self.map.w as f32
    }
    pub fn h(&self) -> f32 {
        self.map.h as f32
    }

    fn bump(&self, x: usize, y: usize, base: [u8; 3], edge: bool) -> [u8; 3] {
        if !edge {
            return base;
        }
        let m = &self.map;
        let (w, h) = (m.w as i32, m.h as i32);
        let (xi, yi) = (x as i32, y as i32);
        let (lx, ly) = (xi - 1, yi - 1); // light comes from the top-left
        let (sx, sy) = (xi + 1, yi + 1);
        let l_out = lx < 0 || ly < 0 || m.land[(ly * w + lx) as usize] != 1;
        let s_out = sx >= w || sy >= h || m.land[(sy * w + sx) as usize] != 1;
        if l_out && !s_out {
            lighten(base, BUMP_LIGHT)
        } else if s_out && !l_out {
            darken(base, BUMP_SHADOW)
        } else {
            base
        }
    }

    fn shade(&self, x: usize, y: usize) -> [u8; 3] {
        let m = &self.map;
        let i = y * m.w + x;
        let owner = m.owner[i];
        let base = m.countries[owner as usize].rgb;
        let edge = m.outline[i] == 1 || m.inner[i] == 1;
        let orig = self.orig_owner[i];
        let occupied = orig != owner && owner != 0 && orig != 0;
        let rgb = if occupied {
            let h = occupied_shade(x as i32, y as i32, base);
            if edge { darken(h, INNER_LINE_DARKEN) } else { h }
        } else if edge {
            darken(base, INNER_LINE_DARKEN)
        } else {
            land_noise(x as i32, y as i32, base)
        };
        self.bump(x, y, rgb, edge)
    }

    fn paint_cell(&mut self, x: usize, y: usize) {
        let w = self.map.w;
        let i = y * w + x;
        let p = i * 4;
        if self.map.land[i] == 1 {
            let c = self.shade(x, y);
            self.image.bytes[p] = c[0];
            self.image.bytes[p + 1] = c[1];
            self.image.bytes[p + 2] = c[2];
            self.image.bytes[p + 3] = 255;
        } else {
            self.image.bytes[p..p + 4].copy_from_slice(&[0, 0, 0, 0]);
        }
    }

    fn stamp_capitals(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let (w, h) = (self.map.w as i32, self.map.h as i32);
        for k in 0..self.map.capitals.len() {
            let (cx, cy) = (self.map.capitals[k].x as i32, self.map.capitals[k].y as i32);
            for dy in 0..2 {
                for dx in 0..2 {
                    let (x, y) = (cx + dx, cy + dy);
                    if x < x0 || x > x1 || y < y0 || y > y1 || x >= w || y >= h {
                        continue;
                    }
                    let p = ((y * w + x) * 4) as usize;
                    self.image.bytes[p..p + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
    }

    pub fn repaint_all(&mut self) {
        let (w, h) = (self.map.w, self.map.h);
        for y in 0..h {
            for x in 0..w {
                self.paint_cell(x, y);
            }
        }
        self.stamp_capitals(0, 0, w as i32 - 1, h as i32 - 1);
        self.land_tex.update(&self.image);
        self.tex_dirty = false;
    }

    pub fn set_owner(&mut self, i: usize, id: u16) {
        self.map.owner[i] = id;
    }

    /// Port of recomputeOutlineLocal / recomputeInnerLineLocal / bakeBitmapLocal for a dirty box.
    pub fn refresh_box(&mut self, b: [i32; 4]) {
        let (w, h) = (self.map.w as i32, self.map.h as i32);
        let x0 = (b[0] - DIRTY_PAD).max(0);
        let y0 = (b[1] - DIRTY_PAD).max(0);
        let x1 = (b[2] + DIRTY_PAD).min(w - 1);
        let y1 = (b[3] + DIRTY_PAD).min(h - 1);
        if x1 < x0 || y1 < y0 {
            return;
        }
        // outline
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = (y * w + x) as usize;
                if self.map.land[i] == 0 {
                    self.map.outline[i] = 0;
                    continue;
                }
                let mut edge = false;
                'n: for dy in -1..=1i32 {
                    for dx in -1..=1i32 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            edge = true;
                            break 'n;
                        }
                        let n = (ny * w + nx) as usize;
                        if self.map.land[n] == 0 || (self.map.owner[n] != self.map.owner[i] && i < n) {
                            edge = true;
                            break 'n;
                        }
                    }
                }
                self.map.outline[i] = edge as u8;
            }
        }
        // inner line
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = (y * w + x) as usize;
                if self.map.land[i] == 0 || self.map.outline[i] == 1 {
                    self.map.inner[i] = 0;
                    continue;
                }
                let mut near = false;
                'm: for dy in -1..=1i32 {
                    for dx in -1..=1i32 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            continue;
                        }
                        if self.map.outline[(ny * w + nx) as usize] == 1 {
                            near = true;
                            break 'm;
                        }
                    }
                }
                self.map.inner[i] = near as u8;
            }
        }
        // repaint (one extra ring: bump shading looks at neighbours)
        let (px0, py0) = ((x0 - 1).max(0), (y0 - 1).max(0));
        let (px1, py1) = ((x1 + 1).min(w - 1), (y1 + 1).min(h - 1));
        for y in py0..=py1 {
            for x in px0..=px1 {
                self.paint_cell(x as usize, y as usize);
            }
        }
        self.stamp_capitals(px0, py0, px1, py1);
        self.tex_dirty = true;
    }

    /// Upload the CPU image to the GPU if anything changed this frame.
    pub fn flush(&mut self) {
        if self.tex_dirty {
            self.land_tex.update(&self.image);
            self.tex_dirty = false;
        }
    }

    pub fn owner_at(&self, cx: i32, cy: i32) -> u16 {
        if cx < 0 || cy < 0 || cx >= self.map.w as i32 || cy >= self.map.h as i32 {
            return 0;
        }
        let i = cy as usize * self.map.w + cx as usize;
        if self.map.land[i] != 1 { 0 } else { self.map.owner[i] }
    }

    pub fn name(&self, id: u16) -> &str {
        &self.map.countries[id as usize].name
    }

    pub fn draw(&self, cam: &MapCamera) {
        let size = vec2(self.w() * cam.scale, self.h() * cam.scale);
        let pos = cam.offset.round();
        let params = DrawTextureParams { dest_size: Some(size), ..Default::default() };
        draw_texture_ex(&self.ocean_tex, pos.x, pos.y, WHITE, params.clone());
        draw_texture_ex(&self.land_tex, pos.x, pos.y, WHITE, params);
    }
}
