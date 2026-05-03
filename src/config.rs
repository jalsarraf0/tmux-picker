//! User config loaded from `~/.config/tmux-picker/config.toml`.
//!
//! All keys are optional. Missing file or missing keys fall back to defaults.
//! A malformed file logs one stderr line and uses defaults.

use ratatui::style::Color;

const DEFAULT_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone)]
pub struct Config {
    pub timeout_secs: u64,
    pub theme: Theme,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub warning: Color,
    pub selection_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            accent: Color::Cyan,
            warning: Color::Red,
            selection_bg: Color::DarkGray,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            theme: Theme::default(),
        }
    }
}

impl Config {
    /// Load from the default config path. Never fails — falls back to
    /// defaults on any error and logs a single line to stderr.
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Config::default();
        };
        let Ok(s) = std::fs::read_to_string(&path) else {
            return Config::default();
        };
        Config::from_str(&s)
    }

    /// Parse a config from a TOML string. On parse errors, log to stderr and
    /// return defaults. Unknown color names log a warning and that field
    /// keeps its default.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        let table = match s.parse::<toml::Table>() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("tmux-picker config: parse error ({e}); using defaults");
                return Config::default();
            }
        };

        let mut cfg = Config::default();

        if let Some(v) = table.get("timeout_secs")
            && let Some(n) = v.as_integer()
            && n >= 0
        {
            cfg.timeout_secs = n as u64;
        }

        if let Some(theme_val) = table.get("theme")
            && let Some(theme_table) = theme_val.as_table()
        {
            apply_color(theme_table, "accent", &mut cfg.theme.accent);
            apply_color(theme_table, "warning", &mut cfg.theme.warning);
            apply_color(theme_table, "selection_bg", &mut cfg.theme.selection_bg);
        }

        cfg
    }
}

fn apply_color(table: &toml::Table, key: &str, dest: &mut Color) {
    let Some(val) = table.get(key) else { return };
    let Some(name) = val.as_str() else {
        eprintln!("tmux-picker config: theme.{key} must be a string; using default");
        return;
    };
    match parse_color(name) {
        Some(c) => *dest = c,
        None => {
            eprintln!("tmux-picker config: unknown color '{name}' for theme.{key}; using default")
        }
    }
}

fn parse_color(s: &str) -> Option<Color> {
    Some(match s.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "darkgray" | "gray" | "grey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        _ => return None,
    })
}

fn config_path() -> Option<std::path::PathBuf> {
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        std::path::PathBuf::from(xdg)
    } else {
        let home = std::env::var("HOME").ok()?;
        std::path::PathBuf::from(home).join(".config")
    };
    Some(base.join("tmux-picker").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_defaults() {
        let cfg = Config::from_str("");
        assert_eq!(cfg.timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn malformed_toml_falls_back_to_defaults() {
        let cfg = Config::from_str("not[valid::toml");
        assert_eq!(cfg.timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn timeout_secs_override() {
        let cfg = Config::from_str("timeout_secs = 30");
        assert_eq!(cfg.timeout_secs, 30);
    }

    #[test]
    fn timeout_secs_zero_disables() {
        let cfg = Config::from_str("timeout_secs = 0");
        assert_eq!(cfg.timeout_secs, 0);
    }

    #[test]
    fn theme_accent_override() {
        let cfg = Config::from_str("[theme]\naccent = \"magenta\"");
        assert!(matches!(cfg.theme.accent, Color::Magenta));
    }

    #[test]
    fn theme_unknown_color_keeps_default() {
        let cfg = Config::from_str("[theme]\naccent = \"chartreuse\"");
        // Default is Cyan
        assert!(matches!(cfg.theme.accent, Color::Cyan));
    }

    #[test]
    fn parse_color_case_insensitive() {
        assert!(matches!(parse_color("MAGENTA"), Some(Color::Magenta)));
        assert!(matches!(parse_color("DarkGray"), Some(Color::DarkGray)));
    }

    #[test]
    fn parse_color_aliases() {
        assert!(matches!(parse_color("gray"), Some(Color::DarkGray)));
        assert!(matches!(parse_color("grey"), Some(Color::DarkGray)));
    }
}
