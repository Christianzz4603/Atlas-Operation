//! Audio scaffold (CONFIG.audio port): sound-effect and music hook points for the war sim's key
//! moments (war declared, peace made, battle won/lost, capital falls/recaptured, nation eliminated,
//! empire founded, crowns awarded, focused attack, UI clicks).
//!
//! No sound assets ship with this build yet -- every clip is loaded lazily, once, at startup, from
//! `assets/audio/<name>.ogg` relative to the executable; a missing file just means that one hook plays
//! nothing (logged once, not per-miss). Drop matching .ogg/.wav files into that folder later (see the
//! `file()` names below) and every hook in app.rs/sim.rs lights up with zero further code changes.

use macroquad::audio::{load_sound, play_sound, stop_sound, PlaySoundParams, Sound};
use std::collections::HashMap;

use crate::config::log;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sfx {
    WarDeclared,
    PeaceMade,
    BattleWon,
    BattleLost,
    CapitalFallen,
    CapitalRecaptured,
    NationEliminated,
    EmpireFounded,
    CrownAwarded,
    FocusAttack,
    UiClick,
}

impl Sfx {
    const ALL: [Sfx; 11] = [
        Sfx::WarDeclared,
        Sfx::PeaceMade,
        Sfx::BattleWon,
        Sfx::BattleLost,
        Sfx::CapitalFallen,
        Sfx::CapitalRecaptured,
        Sfx::NationEliminated,
        Sfx::EmpireFounded,
        Sfx::CrownAwarded,
        Sfx::FocusAttack,
        Sfx::UiClick,
    ];

    fn file(self) -> &'static str {
        match self {
            Sfx::WarDeclared => "war_declared.ogg",
            Sfx::PeaceMade => "peace_made.ogg",
            Sfx::BattleWon => "battle_won.ogg",
            Sfx::BattleLost => "battle_lost.ogg",
            Sfx::CapitalFallen => "capital_fallen.ogg",
            Sfx::CapitalRecaptured => "capital_recaptured.ogg",
            Sfx::NationEliminated => "nation_eliminated.ogg",
            Sfx::EmpireFounded => "empire_founded.ogg",
            Sfx::CrownAwarded => "crown_awarded.ogg",
            Sfx::FocusAttack => "focus_attack.ogg",
            Sfx::UiClick => "ui_click.ogg",
        }
    }
}

const SFX_DIR: &str = "assets/audio";
const MUSIC_FILE: &str = "assets/audio/music_loop.ogg";

pub struct Audio {
    sfx: HashMap<Sfx, Sound>,
    music: Option<Sound>,
    pub sfx_on: bool,
    pub music_on: bool,
    volume: f32,
}

impl Audio {
    pub fn new() -> Self {
        Self { sfx: HashMap::new(), music: None, sfx_on: true, music_on: true, volume: 0.8 }
    }

    /// Kick off every load at startup. Safe to call even when `assets/audio/` doesn't exist at all --
    /// a missing folder just means every load fails, is caught, and the game runs silently.
    pub async fn load_all(&mut self) {
        // Some headless/CI environments (and some stripped-down Linux installs) have no working
        // ALSA/audio device at all; quad-snd's backend init panics in that case rather than
        // returning an error, so we never touch it there. Set CW_NO_AUDIO=1 to opt out up front.
        if std::env::var("CW_NO_AUDIO").is_ok() {
            log("audio: disabled via CW_NO_AUDIO env var, skipping all loads");
            return;
        }
        let mut missing = 0u32;
        for s in Sfx::ALL {
            let path = format!("{}/{}", SFX_DIR, s.file());
            match load_sound(&path).await {
                Ok(snd) => {
                    self.sfx.insert(s, snd);
                }
                Err(_) => missing += 1,
            }
        }
        match load_sound(MUSIC_FILE).await {
            Ok(snd) => self.music = Some(snd),
            Err(_) => missing += 1,
        }
        if missing > 0 {
            log(&format!(
                "audio: {} of {} clip(s) not found under assets/audio/ -- running without them (drop matching .ogg files there to enable)",
                missing,
                Sfx::ALL.len() + 1
            ));
        }
    }

    pub fn play(&self, s: Sfx) {
        if !self.sfx_on {
            return;
        }
        if let Some(snd) = self.sfx.get(&s) {
            play_sound(snd, PlaySoundParams { looped: false, volume: self.volume });
        }
    }

    pub fn start_music(&self) {
        if !self.music_on {
            return;
        }
        if let Some(m) = &self.music {
            play_sound(m, PlaySoundParams { looped: true, volume: self.volume * 0.6 });
        }
    }

    pub fn set_music_enabled(&mut self, on: bool) {
        self.music_on = on;
        match (&self.music, on) {
            (Some(m), false) => stop_sound(m),
            (Some(_), true) => self.start_music(),
            _ => {}
        }
    }
}
