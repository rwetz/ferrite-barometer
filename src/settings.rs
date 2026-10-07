//! The station's settings, saved as plain `key = value` lines in
//! `<config>/ferrite/barometer.conf`.

use std::path::PathBuf;

use crate::forecast::Units;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub scheme: String,
    /// `dark`, `light` or `system`.
    pub appearance: String,
    pub fps: u32,
    /// A place name or `lat,lon`; overrides `BAROMETER_LOCATION` when set.
    pub location: String,
    pub units: Units,
}

impl Default for Settings {
    fn default() -> Self {
        Self { scheme: "phosphor".into(), appearance: "dark".into(), fps: 240, location: String::new(), units: Units::Metric }
    }
}

impl Settings {
    pub fn parse(src: &str) -> Self {
        let mut s = Self::default();
        for line in src.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "scheme" if !v.is_empty() => s.scheme = v.into(),
                "appearance" if matches!(v, "dark" | "light" | "system") => s.appearance = v.into(),
                "fps" => s.fps = v.parse().map(|f: u32| f.clamp(12, 240)).unwrap_or(s.fps),
                "location" => s.location = v.into(),
                "units" => s.units = if v == "imperial" { Units::Imperial } else { Units::Metric },
                _ => {}
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        format!(
            "# Barometer settings.\nscheme = {}\nappearance = {}\nfps = {}\nlocation = {}\nunits = {}\n",
            self.scheme,
            self.appearance,
            self.fps,
            self.location,
            self.units.key(),
        )
    }

    pub fn path() -> Option<PathBuf> {
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        };
        base.map(|b| b.join("ferrite").join("barometer.conf"))
    }

    /// Whether settings have ever been saved: false on the very first launch.
    pub fn exists() -> bool {
        Self::path().is_some_and(|p| p.exists())
    }

    pub fn load() -> Self {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| Self::parse(&s)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("no config directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, self.serialize()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Settings { scheme: "harbor".into(), appearance: "light".into(), fps: 25, location: "Fargo, US".into(), units: Units::Imperial };
        assert_eq!(Settings::parse(&s.serialize()), s);
    }

    #[test]
    fn defaults_to_phosphor() {
        assert_eq!(Settings::parse("").scheme, "phosphor");
    }

    #[test]
    fn junk_falls_back_to_defaults() {
        assert_eq!(Settings::parse("appearance = purple\nfps = 9000\n# x = y\n"), Settings { fps: 240, ..Settings::default() });
    }
}
