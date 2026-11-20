//! Settings file, data dir and log file shared by everything.
use std::io::Write;
use std::path::PathBuf;

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Where logs / settings live: next to the exe on desktop, the app's private dir on Android.
pub fn data_dir() -> PathBuf {
    if cfg!(target_os = "android") {
        let p = PathBuf::from("/data/data/com.countrywarzone.game/files");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
        exe_dir()
    }
}

/// Append a line to `warzone-log.txt` (and to logcat on Android). Never panics.
pub fn log(msg: &str) {
    macroquad::miniquad::info!("warzone: {}", msg);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join("warzone-log.txt")) {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Optional `warzone.cfg` (key=value, '#' comments) in the data dir, next to the exe, or in the cwd.
/// Falls back to the old `atlas.cfg` name so settings saved by pre-rebrand builds still load.
#[derive(Clone)]
pub struct Config {
    pub width: i32,
    pub height: i32,
    pub fullscreen: bool,
    pub fps_cap: u32,
    pub high_dpi: bool,
    pub sample_count: i32,
    pub ui_scale: f32,
}

impl Config {
    pub fn load() -> Config {
        let mut c = Config { width: 1280, height: 720, fullscreen: false, fps_cap: 60, high_dpi: false, sample_count: 1, ui_scale: 1.0 };
        let text = std::fs::read_to_string(data_dir().join("warzone.cfg"))
            .or_else(|_| std::fs::read_to_string(exe_dir().join("warzone.cfg")))
            .or_else(|_| std::fs::read_to_string("warzone.cfg"))
            .or_else(|_| std::fs::read_to_string(data_dir().join("atlas.cfg")))
            .or_else(|_| std::fs::read_to_string(exe_dir().join("atlas.cfg")))
            .or_else(|_| std::fs::read_to_string("atlas.cfg"))
            .unwrap_or_default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim().to_lowercase(), v.trim().to_lowercase());
            let flag = v == "1" || v == "true" || v == "yes" || v == "on";
            match k.as_str() {
                "width" => c.width = v.parse().unwrap_or(c.width).clamp(320, 7680),
                "height" => c.height = v.parse().unwrap_or(c.height).clamp(240, 4320),
                "fullscreen" => c.fullscreen = flag,
                "fps_cap" => c.fps_cap = v.parse().unwrap_or(c.fps_cap).min(240),
                "high_dpi" => c.high_dpi = flag,
                "sample_count" => c.sample_count = v.parse().unwrap_or(1).clamp(1, 8),
                "ui_scale" => c.ui_scale = v.parse().unwrap_or(1.0f32).clamp(0.6, 2.5),
                _ => {}
            }
        }
        c
    }

    pub fn save(&self) {
        let text = format!(
            "# Country Warzone settings (ui_scale is changed in-game under Settings)\nwidth={}\nheight={}\nfullscreen={}\nfps_cap={}\nhigh_dpi={}\nsample_count={}\nui_scale={:.2}\n",
            self.width, self.height, self.fullscreen, self.fps_cap, self.high_dpi, self.sample_count, self.ui_scale
        );
        let _ = std::fs::write(data_dir().join("warzone.cfg"), text);
    }
}
