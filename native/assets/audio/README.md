# Audio assets (not included yet)

This build ships with no sound files. `Audio::load_all()` (see `native/src/audio.rs`) tries to load
each clip below from this folder at startup; any clip that's missing is simply skipped (logged once),
so the game runs fine with this folder empty.

Drop in Ogg Vorbis (`.ogg`) files with these exact names to enable each hook (macroquad's loader also
accepts `.wav`, but `.ogg` is recommended for size):

| File | Plays when |
|---|---|
| `war_declared.ogg` | you or an enemy you're watching declares war |
| `peace_made.ogg` | a war you're part of ends in peace |
| `battle_won.ogg` | a front you're attacking/defending resolves in your favor |
| `battle_lost.ogg` | a front you're attacking/defending resolves against you |
| `capital_fallen.ogg` | your capital falls to an enemy |
| `capital_recaptured.ogg` | you retake your capital |
| `nation_eliminated.ogg` | your nation is eliminated |
| `empire_founded.ogg` | you found an empire |
| `crown_awarded.ogg` | you earn a victory-point crown |
| `focus_attack.ogg` | you order a focused attack on a frontier cell |
| `ui_click.ogg` | any UI button press |
| `music_loop.ogg` | background music, looped from app startup |
