//! User config loaded from `~/.config/tmux-picker/config.toml`.
//!
//! All keys are optional. Missing file or missing keys fall back to defaults.
//! A malformed file logs one stderr line and uses defaults.

use ratatui::style::Color;

const DEFAULT_TIMEOUT_SECS: u64 = 10;

/// Starter `~/.config/tmux-picker/config.toml` content used by
/// `tmux-picker --init`. Round-trips through `from_str` to the same
/// effective config as `Config::default()`.
pub const STARTER_TOML: &str = r##"# tmux-picker — starter config
# Every key is optional. Delete or comment any line to fall back to the
# default. Run `tmux-picker --check-config` to confirm what's effective.

# Auto-attach countdown for the most-recent detached session, in seconds.
# 0 disables auto-attach (the picker waits for a manual choice).
timeout_secs = 10

# When the auto-attach shell hook fires. "always" (default) runs the picker
# on every new interactive shell — SSH login or a local terminal window
# alike. "ssh_only" restores the original behaviour and skips local
# terminals, only firing over SSH.
trigger_mode = "always"

# Theme overrides. Values: black|red|green|yellow|blue|magenta|cyan|white,
# darkgray (alias gray/grey), light{red,green,yellow,blue,magenta,cyan},
# 256-colour indexes ("196" or 196), or hex like "#ff8800" / "#abc".
[theme]
accent = "cyan"            # numbers, attached marker, prompt accents
warning = "red"            # kill-confirm prompt, error highlights
selection_bg = "darkgray"  # highlighted row background

# Process markers. The first matching pattern wins; user patterns are
# checked before the built-in defaults. Set disable_defaults to drop
# the built-ins entirely.
[markers]
disable_defaults = false

# [markers.patterns]
# foo = "★"          # any pane running `foo` gets a ★
# "my-tool" = "🚀"
"##;

/// Path tmux-picker reads / writes for its config. Public so `--init` can
/// re-use the same resolver as the loader. Honours `$XDG_CONFIG_HOME`,
/// otherwise falls back to `$HOME/.config/tmux-picker/config.toml`.
pub fn config_file_path() -> Option<std::path::PathBuf> {
    config_path()
}

#[derive(Debug, Clone)]
pub struct Config {
    pub timeout_secs: u64,
    pub trigger_mode: TriggerMode,
    pub theme: Theme,
    pub markers: Markers,
}

/// When the shell hook should run the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerMode {
    /// Every new interactive shell — SSH login or a local terminal alike.
    #[default]
    Always,
    /// Only shells started over SSH (the original tmux-picker behaviour).
    SshOnly,
}

impl TriggerMode {
    /// Lowercase string used both in TOML and on the `--print-trigger-mode`
    /// stdout line the shell hook reads.
    pub fn as_str(self) -> &'static str {
        match self {
            TriggerMode::Always => "always",
            TriggerMode::SshOnly => "ssh_only",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "always" => Some(TriggerMode::Always),
            "ssh_only" | "ssh-only" | "ssh" => Some(TriggerMode::SshOnly),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Markers {
    /// When true, the built-in default marker map (claude → 🤖, vim → ✏️ …)
    /// is dropped — only user-supplied patterns apply.
    pub disable_defaults: bool,
    /// Ordered (pattern, glyph) pairs. Pattern is matched
    /// case-insensitively as a substring against `pane_current_command`.
    /// Insertion order = match order; user overrides are pushed after
    /// defaults so they win when a key collides.
    pub patterns: Vec<(String, String)>,
}

/// Built-in markers shipped with the picker. Order matters: the first
/// matching glyph wins, so put more-specific patterns first.
pub const DEFAULT_MARKERS: &[(&str, &str)] = &[
    ("claude", "\u{1F916}"),      // 🤖
    ("nvim", "\u{270F}\u{FE0F}"), // ✏️
    ("vim", "\u{270F}\u{FE0F}"),  // ✏️
    ("htop", "\u{1F4CA}"),        // 📊
    ("btop", "\u{1F4CA}"),        // 📊
    ("top", "\u{1F4CA}"),         // 📊
    ("cargo", "\u{1F980}"),       // 🦀
    ("rustc", "\u{1F980}"),       // 🦀
    ("npm", "\u{1F4E6}"),         // 📦
    ("pnpm", "\u{1F4E6}"),        // 📦
    ("node", "\u{1F4E6}"),        // 📦
    ("python", "\u{1F40D}"),      // 🐍
    ("git", "\u{1F33F}"),         // 🌿
];

impl Markers {
    /// Walk the merged map in match order. Returns `(pattern, glyph)` for
    /// the first entry whose pattern is a case-insensitive substring of
    /// any element in `commands`.
    pub fn lookup(&self, commands: &[String]) -> Option<String> {
        let lc: Vec<String> = commands.iter().map(|c| c.to_lowercase()).collect();
        let defaults: &[(&str, &str)] = if self.disable_defaults {
            &[]
        } else {
            DEFAULT_MARKERS
        };
        // User patterns first so they take precedence on identical keys.
        for (pat, glyph) in self.patterns.iter().map(|(p, g)| (p.as_str(), g.as_str())) {
            if pattern_matches(&lc, pat) {
                return Some(glyph.to_string());
            }
        }
        for (pat, glyph) in defaults {
            // Skip a default whose key is overridden by a user pattern with
            // the same key — we already checked user patterns above, but
            // user may have set a *different* key whose pattern overlaps;
            // that's fine, defaults still apply for non-collisions.
            if self
                .patterns
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case(pat))
            {
                continue;
            }
            if pattern_matches(&lc, pat) {
                return Some((*glyph).to_string());
            }
        }
        None
    }
}

fn pattern_matches(commands_lc: &[String], pattern: &str) -> bool {
    let pat_lc = pattern.to_lowercase();
    commands_lc.iter().any(|c| c.contains(&pat_lc))
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
            trigger_mode: TriggerMode::default(),
            theme: Theme::default(),
            markers: Markers::default(),
        }
    }
}

impl Config {
    /// Load from the default config path. Never fails — falls back to
    /// defaults on any error and logs a single line per warning to stderr.
    pub fn load() -> Self {
        let (cfg, warnings) = Self::load_with_warnings();
        for w in &warnings {
            eprintln!("tmux-picker config: {w}");
        }
        cfg
    }

    /// Same as `load`, but returns warnings instead of printing them. Used by
    /// `tmux-picker --check-config`.
    pub fn load_with_warnings() -> (Self, Vec<String>) {
        let Some(path) = config_path() else {
            return (Config::default(), Vec::new());
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => Config::from_str_with_warnings(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), Vec::new()),
            Err(e) => (
                Config::default(),
                vec![format!(
                    "read error for {}: {e}; using defaults",
                    path.display()
                )],
            ),
        }
    }

    /// Parse a config from a TOML string. On parse errors, log to stderr and
    /// return defaults. Unknown color names log a warning and that field
    /// keeps its default.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        let (cfg, warnings) = Self::from_str_with_warnings(s);
        for w in &warnings {
            eprintln!("tmux-picker config: {w}");
        }
        cfg
    }

    /// Variant of `from_str` that returns the parse warnings instead of
    /// printing them, so `--check-config` can render them in a single
    /// stdout block.
    pub fn from_str_with_warnings(s: &str) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let table = match s.parse::<toml::Table>() {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("parse error ({e}); using defaults"));
                return (Config::default(), warnings);
            }
        };

        let mut cfg = Config::default();

        if let Some(v) = table.get("timeout_secs") {
            match v.as_integer() {
                Some(n) if n >= 0 => cfg.timeout_secs = n as u64,
                Some(n) => {
                    warnings.push(format!("timeout_secs must be >= 0, got {n}; using default"))
                }
                None => warnings.push("timeout_secs must be an integer; using default".into()),
            }
        }

        if let Some(v) = table.get("trigger_mode") {
            match v.as_str().and_then(TriggerMode::from_str) {
                Some(mode) => cfg.trigger_mode = mode,
                None => warnings.push(format!(
                    "trigger_mode must be \"always\" or \"ssh_only\", got {v:?}; using default"
                )),
            }
        }

        if let Some(theme_val) = table.get("theme") {
            match theme_val.as_table() {
                Some(theme_table) => {
                    apply_color(theme_table, "accent", &mut cfg.theme.accent, &mut warnings);
                    apply_color(
                        theme_table,
                        "warning",
                        &mut cfg.theme.warning,
                        &mut warnings,
                    );
                    apply_color(
                        theme_table,
                        "selection_bg",
                        &mut cfg.theme.selection_bg,
                        &mut warnings,
                    );
                }
                None => warnings.push("theme must be a table; using defaults".into()),
            }
        }

        if let Some(markers_val) = table.get("markers") {
            match markers_val.as_table() {
                Some(markers_table) => {
                    if let Some(v) = markers_table.get("disable_defaults") {
                        match v.as_bool() {
                            Some(b) => cfg.markers.disable_defaults = b,
                            None => warnings.push(
                                "markers.disable_defaults must be a boolean; using default".into(),
                            ),
                        }
                    }
                    if let Some(v) = markers_table.get("patterns") {
                        match v.as_table() {
                            Some(patterns_table) => {
                                for (k, v) in patterns_table {
                                    match v.as_str() {
                                        Some(glyph) => {
                                            cfg.markers
                                                .patterns
                                                .push((k.to_string(), glyph.to_string()));
                                        }
                                        None => warnings.push(format!(
                                            "markers.patterns.{k} must be a string; ignored"
                                        )),
                                    }
                                }
                            }
                            None => {
                                warnings.push("markers.patterns must be a table; ignored".into())
                            }
                        }
                    }
                }
                None => warnings.push("markers must be a table; using defaults".into()),
            }
        }

        (cfg, warnings)
    }

    /// Render the effective config back as a TOML document — used by
    /// `--check-config` so the user sees what the picker actually loaded.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("timeout_secs = {}\n", self.timeout_secs));
        out.push_str(&format!(
            "trigger_mode = \"{}\"\n",
            self.trigger_mode.as_str()
        ));
        out.push_str("\n[theme]\n");
        out.push_str(&format!("accent = {}\n", color_to_toml(self.theme.accent)));
        out.push_str(&format!(
            "warning = {}\n",
            color_to_toml(self.theme.warning)
        ));
        out.push_str(&format!(
            "selection_bg = {}\n",
            color_to_toml(self.theme.selection_bg)
        ));
        out
    }
}

fn apply_color(table: &toml::Table, key: &str, dest: &mut Color, warnings: &mut Vec<String>) {
    let Some(val) = table.get(key) else { return };
    match parse_color_value(val) {
        Ok(c) => *dest = c,
        Err(e) => warnings.push(format!("theme.{key}: {e}; using default")),
    }
}

fn parse_color_value(val: &toml::Value) -> Result<Color, String> {
    match val {
        toml::Value::Integer(n) => match u8::try_from(*n) {
            Ok(b) => Ok(Color::Indexed(b)),
            Err(_) => Err(format!("indexed color must be 0..=255, got {n}")),
        },
        toml::Value::String(s) => parse_color_string(s),
        other => Err(format!(
            "expected string or integer, got {}",
            other.type_str()
        )),
    }
}

fn parse_color_string(s: &str) -> Result<Color, String> {
    let trimmed = s.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Ok(n) = trimmed.parse::<u16>() {
        return match u8::try_from(n) {
            Ok(b) => Ok(Color::Indexed(b)),
            Err(_) => Err(format!("indexed color must be 0..=255, got {n}")),
        };
    }
    parse_named(trimmed).ok_or_else(|| format!("unknown color '{s}'"))
}

fn parse_hex(hex: &str) -> Result<Color, String> {
    let expanded = match hex.len() {
        3 => hex
            .chars()
            .flat_map(|c| std::iter::repeat_n(c, 2))
            .collect::<String>(),
        6 => hex.to_string(),
        n => return Err(format!("hex color must be #rgb or #rrggbb, got {n} digits")),
    };
    if !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("hex color contains non-hex digit: '#{hex}'"));
    }
    let r = u8::from_str_radix(&expanded[0..2], 16).map_err(|e| e.to_string())?;
    let g = u8::from_str_radix(&expanded[2..4], 16).map_err(|e| e.to_string())?;
    let b = u8::from_str_radix(&expanded[4..6], 16).map_err(|e| e.to_string())?;
    Ok(Color::Rgb(r, g, b))
}

fn parse_named(s: &str) -> Option<Color> {
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

fn color_to_toml(c: Color) -> String {
    match c {
        Color::Black => "\"black\"".into(),
        Color::Red => "\"red\"".into(),
        Color::Green => "\"green\"".into(),
        Color::Yellow => "\"yellow\"".into(),
        Color::Blue => "\"blue\"".into(),
        Color::Magenta => "\"magenta\"".into(),
        Color::Cyan => "\"cyan\"".into(),
        Color::White => "\"white\"".into(),
        Color::DarkGray => "\"darkgray\"".into(),
        Color::LightRed => "\"lightred\"".into(),
        Color::LightGreen => "\"lightgreen\"".into(),
        Color::LightYellow => "\"lightyellow\"".into(),
        Color::LightBlue => "\"lightblue\"".into(),
        Color::LightMagenta => "\"lightmagenta\"".into(),
        Color::LightCyan => "\"lightcyan\"".into(),
        Color::Rgb(r, g, b) => format!("\"#{r:02x}{g:02x}{b:02x}\""),
        Color::Indexed(n) => n.to_string(),
        // Reset and the bright "Light*" already covered above. Anything else
        // we have not produced ourselves; fall back to debug so the user can
        // still see what the picker has.
        other => format!("\"{other:?}\""),
    }
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
    fn malformed_toml_emits_warning() {
        let (_, warnings) = Config::from_str_with_warnings("not[valid::toml");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("parse error"));
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
    fn timeout_secs_negative_warns() {
        let (cfg, warnings) = Config::from_str_with_warnings("timeout_secs = -5");
        assert_eq!(cfg.timeout_secs, DEFAULT_TIMEOUT_SECS);
        assert!(warnings.iter().any(|w| w.contains("timeout_secs")));
    }

    #[test]
    fn timeout_secs_string_warns() {
        let (cfg, warnings) = Config::from_str_with_warnings("timeout_secs = \"oops\"");
        assert_eq!(cfg.timeout_secs, DEFAULT_TIMEOUT_SECS);
        assert!(warnings.iter().any(|w| w.contains("timeout_secs")));
    }

    #[test]
    fn trigger_mode_defaults_to_always() {
        let cfg = Config::from_str("");
        assert_eq!(cfg.trigger_mode, TriggerMode::Always);
    }

    #[test]
    fn trigger_mode_ssh_only_override() {
        let cfg = Config::from_str(r#"trigger_mode = "ssh_only""#);
        assert_eq!(cfg.trigger_mode, TriggerMode::SshOnly);
    }

    #[test]
    fn trigger_mode_case_insensitive_and_aliases() {
        assert_eq!(
            Config::from_str(r#"trigger_mode = "SSH_ONLY""#).trigger_mode,
            TriggerMode::SshOnly
        );
        assert_eq!(
            Config::from_str(r#"trigger_mode = "ssh-only""#).trigger_mode,
            TriggerMode::SshOnly
        );
        assert_eq!(
            Config::from_str(r#"trigger_mode = "ssh""#).trigger_mode,
            TriggerMode::SshOnly
        );
        assert_eq!(
            Config::from_str(r#"trigger_mode = "ALWAYS""#).trigger_mode,
            TriggerMode::Always
        );
    }

    #[test]
    fn trigger_mode_invalid_value_warns_and_keeps_default() {
        let (cfg, warnings) = Config::from_str_with_warnings(r#"trigger_mode = "sometimes""#);
        assert_eq!(cfg.trigger_mode, TriggerMode::Always);
        assert!(warnings.iter().any(|w| w.contains("trigger_mode")));
    }

    #[test]
    fn trigger_mode_wrong_type_warns() {
        let (cfg, warnings) = Config::from_str_with_warnings("trigger_mode = 7");
        assert_eq!(cfg.trigger_mode, TriggerMode::Always);
        assert!(warnings.iter().any(|w| w.contains("trigger_mode")));
    }

    #[test]
    fn to_toml_includes_trigger_mode() {
        let mut cfg = Config::default();
        cfg.trigger_mode = TriggerMode::SshOnly;
        let s = cfg.to_toml();
        assert!(s.contains("trigger_mode = \"ssh_only\""));
    }

    #[test]
    fn theme_accent_override() {
        let cfg = Config::from_str("[theme]\naccent = \"magenta\"");
        assert!(matches!(cfg.theme.accent, Color::Magenta));
    }

    #[test]
    fn theme_unknown_color_keeps_default() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = \"chartreuse\"");
        assert!(matches!(cfg.theme.accent, Color::Cyan));
        assert!(warnings.iter().any(|w| w.contains("chartreuse")));
    }

    #[test]
    fn theme_not_a_table_warns() {
        let (cfg, warnings) = Config::from_str_with_warnings("theme = 7");
        assert!(matches!(cfg.theme.accent, Color::Cyan));
        assert!(warnings.iter().any(|w| w.contains("theme")));
    }

    #[test]
    fn parse_color_case_insensitive() {
        assert!(matches!(parse_named("MAGENTA"), Some(Color::Magenta)));
        assert!(matches!(parse_named("DarkGray"), Some(Color::DarkGray)));
    }

    #[test]
    fn parse_color_aliases() {
        assert!(matches!(parse_named("gray"), Some(Color::DarkGray)));
        assert!(matches!(parse_named("grey"), Some(Color::DarkGray)));
    }

    #[test]
    fn theme_accepts_hex_six_digit() {
        let cfg = Config::from_str("[theme]\naccent = \"#ff8800\"");
        assert!(matches!(cfg.theme.accent, Color::Rgb(0xff, 0x88, 0x00)));
    }

    #[test]
    fn theme_accepts_hex_three_digit_shorthand() {
        let cfg = Config::from_str("[theme]\naccent = \"#abc\"");
        assert!(matches!(cfg.theme.accent, Color::Rgb(0xaa, 0xbb, 0xcc)));
    }

    #[test]
    fn theme_accepts_hex_uppercase() {
        let cfg = Config::from_str("[theme]\naccent = \"#FF8800\"");
        assert!(matches!(cfg.theme.accent, Color::Rgb(0xff, 0x88, 0x00)));
    }

    #[test]
    fn theme_rejects_hex_bad_chars() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = \"#gg00ff\"");
        assert!(matches!(cfg.theme.accent, Color::Cyan));
        assert!(warnings.iter().any(|w| w.contains("non-hex digit")));
    }

    #[test]
    fn theme_rejects_hex_wrong_length() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = \"#ff88\"");
        assert!(matches!(cfg.theme.accent, Color::Cyan));
        assert!(warnings.iter().any(|w| w.contains("hex color")));
    }

    #[test]
    fn theme_accepts_indexed_integer() {
        let cfg = Config::from_str("[theme]\naccent = 196");
        assert!(matches!(cfg.theme.accent, Color::Indexed(196)));
    }

    #[test]
    fn theme_accepts_indexed_string() {
        let cfg = Config::from_str("[theme]\naccent = \"196\"");
        assert!(matches!(cfg.theme.accent, Color::Indexed(196)));
    }

    #[test]
    fn theme_rejects_indexed_too_large() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = 256");
        assert!(matches!(cfg.theme.accent, Color::Cyan));
        assert!(warnings.iter().any(|w| w.contains("0..=255")));
    }

    #[test]
    fn theme_rejects_indexed_negative() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = -1");
        assert!(matches!(cfg.theme.accent, Color::Cyan));
        assert!(warnings.iter().any(|w| w.contains("0..=255")));
    }

    #[test]
    fn theme_rejects_wrong_type() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = true");
        assert!(matches!(cfg.theme.accent, Color::Cyan));
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("expected string or integer"))
        );
    }

    #[test]
    fn to_toml_round_trips_named_color() {
        let mut cfg = Config::default();
        cfg.theme.accent = Color::Magenta;
        let s = cfg.to_toml();
        assert!(s.contains("accent = \"magenta\""));
    }

    #[test]
    fn to_toml_renders_rgb() {
        let mut cfg = Config::default();
        cfg.theme.accent = Color::Rgb(0xab, 0xcd, 0xef);
        let s = cfg.to_toml();
        assert!(s.contains("accent = \"#abcdef\""));
    }

    #[test]
    fn to_toml_renders_indexed_as_integer() {
        let mut cfg = Config::default();
        cfg.theme.accent = Color::Indexed(42);
        let s = cfg.to_toml();
        assert!(s.contains("accent = 42"));
    }

    // -----------------------------------------------------------------------
    // Markers
    // -----------------------------------------------------------------------

    #[test]
    fn markers_default_matches_claude() {
        let m = Markers::default();
        let cmds = vec!["bash".to_string(), "claude".to_string()];
        assert!(m.lookup(&cmds).is_some());
    }

    #[test]
    fn markers_default_does_not_match_bash() {
        let m = Markers::default();
        let cmds = vec!["bash".to_string()];
        assert!(m.lookup(&cmds).is_none());
    }

    #[test]
    fn markers_first_match_wins_user_overrides_default() {
        let mut m = Markers::default();
        m.patterns.push(("claude".into(), "C".into()));
        let cmds = vec!["claude".to_string()];
        assert_eq!(m.lookup(&cmds).as_deref(), Some("C"));
    }

    #[test]
    fn markers_disable_defaults_drops_builtins() {
        let m = Markers {
            disable_defaults: true,
            patterns: Vec::new(),
        };
        let cmds = vec!["claude".to_string()];
        assert!(m.lookup(&cmds).is_none());
    }

    #[test]
    fn markers_pattern_is_case_insensitive() {
        let mut m = Markers {
            disable_defaults: true,
            patterns: Vec::new(),
        };
        m.patterns.push(("FOO".into(), "★".into()));
        let cmds = vec!["foobar".to_string()];
        assert_eq!(m.lookup(&cmds).as_deref(), Some("★"));
    }

    #[test]
    fn config_parses_markers_disable_defaults() {
        let cfg = Config::from_str("[markers]\ndisable_defaults = true");
        assert!(cfg.markers.disable_defaults);
    }

    #[test]
    fn config_parses_markers_patterns() {
        let cfg = Config::from_str(
            r#"
            [markers.patterns]
            foo = "★"
            bar = "✦"
            "#,
        );
        let pats = &cfg.markers.patterns;
        assert!(pats.iter().any(|(k, v)| k == "foo" && v == "★"));
        assert!(pats.iter().any(|(k, v)| k == "bar" && v == "✦"));
    }

    #[test]
    fn config_warns_on_non_string_pattern_value() {
        let (_, warnings) = Config::from_str_with_warnings(
            r#"
            [markers.patterns]
            bad = 42
            "#,
        );
        assert!(warnings.iter().any(|w| w.contains("markers.patterns.bad")));
    }

    #[test]
    fn starter_toml_parses_clean() {
        let (_, warnings) = Config::from_str_with_warnings(STARTER_TOML);
        assert!(
            warnings.is_empty(),
            "starter config should parse with no warnings, got: {warnings:?}"
        );
    }

    #[test]
    fn starter_toml_round_trips_to_default() {
        let cfg = Config::from_str(STARTER_TOML);
        let default = Config::default();
        assert_eq!(cfg.timeout_secs, default.timeout_secs);
        assert_eq!(cfg.trigger_mode, default.trigger_mode);
        assert!(matches!(cfg.theme.accent, Color::Cyan));
        assert!(matches!(cfg.theme.warning, Color::Red));
        assert!(matches!(cfg.theme.selection_bg, Color::DarkGray));
        assert!(!cfg.markers.disable_defaults);
    }
}
