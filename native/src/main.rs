mod camera;
mod mapdata;
mod sim;
mod ui;
mod world;

use camera::MapCamera;
use macroquad::prelude::*;
use sim::Sim;
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

fn window_conf() -> Conf {
    Conf {
        window_title: "Atlas MiniWars".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
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
    let font = load_ttf_font_from_bytes(include_bytes!("../assets/extracted/fonts/Fredoka-600.ttf")).ok();
    let ocean_bg = Color::from_rgba(58, 111, 143, 255); // CONFIG.colors.OCEAN

    let mut cur = 0usize;
    let (mut world, mut sim, mut cam) = new_game(cur);
    let mut ui = Ui::default();
    let mut last_view = Vec2::ZERO;

    loop {
        let mut want = cur;
        let mut restart = false;
        if is_key_pressed(KeyCode::Key1) { want = 0; }
        if is_key_pressed(KeyCode::Key2) { want = 1; }
        if is_key_pressed(KeyCode::R) { restart = true; }
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
        ui.frame(&font, &mut sim, &world, &cam, tap);
        draw_text(&format!("{} FPS", get_fps()), screen_width() - 70.0, screen_height() - 8.0, 16.0, Color::new(1.0, 1.0, 1.0, 0.5));

        next_frame().await
    }
}
