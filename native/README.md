# Country Warzone - native Rust port

Pure native Rust (macroquad / OpenGL). No HTML, no webview, no JS.
Ported from the original HTML game (`miniwars.html`); rebranded as Country Warzone.

## Quick start
1. Install Rust: https://rustup.rs
2. In this folder run:  `cargo run --release`
3. Tap/click a country to play as it, then tap a neighbour -> DECLARE WAR + ATTACK.

Controls: drag / WASD pan, wheel / pinch / Q,E zoom, Space pause, +/- speed buttons,
1 / 2 switch map, R restart, O observe (watch the AI), tap a country for its panel.

## Status
| Phase | State |
|---|---|
| 1. Extract assets + bake maps (`tools/bake.py`) | done (250 assets, 2 maps, country stats) |
| 2. Renderer, camera, hover | written, not compiled yet |
| 3a. Core war sim (manpower, fronts, capture, capital fall, AI wars + peace) | written, not compiled yet |
| 3b. Provinces, buildings/economy, supply, focused attacks, naval, alliances, nukes | todo |
| 3c. Supply lines (flood-fill readiness, exhaustion/recovery, per-front attrition) | written, awaiting CI |
| 4. Full UI (menus, regime/policy, leaderboard details) | todo |
| 5. Audio, multiplayer (replace PeerJS) | todo |

Sim formulas/constants are the originals from `WarSim` (power ratio, speed, cost per cell, attrition,
defense allocation, manpower regen, AI declare/peace). See the header of `src/sim.rs` for what is
simplified. Maps baked: `europe-2025`, `afro-eurasia`. Not baked yet: `ww1-1914`, `earth-2025`.

## Re-bake data (only if the HTML changes)
    pip install numpy pillow scipy scikit-image
    python3 tools/bake.py path/to/miniwars.html .

## Android
macroquad builds APKs with `cargo-quad-apk`:
    docker run --rm -v "$(pwd)":/root/src -w /root/src notfl3/cargo-apk cargo quad-apk build --release
Maps and the HUD font are embedded with `include_bytes!`, so no asset folder is needed at runtime.
Check the macroquad docs for the current `[package.metadata.android]` keys (app name, package id, icon).

## Layout
- `src/mapdata.rs`  loads `.atlasmap.z` (grids, country names/colors/stats, capitals)
- `src/world.rs`    CPU shading (noise, outlines, bump, occupied stripes) -> one RGBA texture; local repaint
- `src/camera.rs`   port of the JS Camera, mouse + touch + tap detection
- `src/sim.rs`      war simulation + AI
- `src/ui.rs`       HUD (top bar, country panel, peace offers, leaderboard, news)
- `assets/extracted/` flags, icons, fonts, 4 mp3s, misc PNGs (+ `manifest.json`)
