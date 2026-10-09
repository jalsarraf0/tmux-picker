//! User config loaded from `~/.config/tmux-picker/config.toml`.
//!
//! All keys are optional. Missing file or missing keys fall back to defaults.
//! A malformed file logs one stderr line and uses defaults.

use std::borrow::Cow;

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

# Look: "cyberpunk" is the neon HUD (gradient panels, a glitching banner,
# CRT scanlines); "classic" is a plain terminal look in your terminal's own
# colours, with thin borders and no effects.
[theme]
style = "cyberpunk"

# Overrides on top of the style's palette; the values shown are the
# cyberpunk ones. Colour values: black|red|green|yellow|blue|magenta|cyan|
# white, darkgray (alias gray/grey), light{gray,red,green,yellow,blue,
# magenta,cyan}, 256-colour indexes ("196" or 196), hex like "#ff8800" /
# "#abc", or "reset" for the terminal's own colour.
# primary = "#ff2a6d"        # titles, selector, banner (neon pink)
# secondary = "#7a04eb"      # borders and gradients (violet)
# accent = "#05d9e8"         # values, numbers, cursor (cyan)
# highlight = "#f9f002"      # status line, running commands (yellow)
# success = "#39ff14"        # attached sessions, live activity (green)
# warning = "#ff003c"        # kill confirm, errors (red)
# text = "#c4bee4"           # body text
# background = "#080512"     # "reset" keeps your terminal's background
# selection_bg = "#1e0a36"   # selected-row background
# animations = true          # glitch, pulse and shimmer; classic: false
# scanlines = true           # CRT-style row shading; classic: false
color = "auto"               # "auto" | "truecolor" | "256"

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
/// Effective picker configuration.
pub struct Config {
    /// Seconds before automatic attachment; zero disables it.
    pub timeout_secs: u64,
    /// Whether the shell hook triggers only for SSH sessions.
    pub trigger_mode: TriggerMode,
    /// UI colors.
    pub theme: Theme,
    /// Process-name marker rules.
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
    /// Walk the merged map in match order. Returns the glyph for the first
    /// entry whose pattern is a case-insensitive substring of any element
    /// in `commands`.
    pub fn lookup(&self, commands: &[String]) -> Option<String> {
        let lc: Vec<Cow<'_, str>> = commands.iter().map(|c| lowercase(c)).collect();
        let defaults: &[(&str, &str)] = if self.disable_defaults {
            &[]
        } else {
            DEFAULT_MARKERS
        };
        // User patterns first so they take precedence on identical keys.
        for (pat, glyph) in &self.patterns {
            if pattern_matches(&lc, &lowercase(pat)) {
                return Some(glyph.clone());
            }
        }
        for (pat, glyph) in defaults {
            // A default whose key a user pattern overrides was already
            // decided above; overlapping-but-different keys still apply.
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

/// Lowercase without allocating when `s` already is (the usual case for
/// process names and marker patterns).
fn lowercase(s: &str) -> Cow<'_, str> {
    if s.chars().any(char::is_uppercase) {
        Cow::Owned(s.to_lowercase())
    } else {
        Cow::Borrowed(s)
    }
}

fn pattern_matches(commands_lc: &[Cow<'_, str>], pattern_lc: &str) -> bool {
    commands_lc.iter().any(|c| c.contains(pattern_lc))
}

/// How colours are sent to the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// Truecolor when `$COLORTERM` / `$TERM` advertise it, else 256 colours.
    #[default]
    Auto,
    /// Always 24-bit colour.
    TrueColor,
    /// Always the xterm 256-colour palette (nearest match).
    Ansi256,
}

impl ColorMode {
    /// Lowercase string used in TOML.
    pub fn as_str(self) -> &'static str {
        match self {
            ColorMode::Auto => "auto",
            ColorMode::TrueColor => "truecolor",
            ColorMode::Ansi256 => "256",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "auto" => Some(ColorMode::Auto),
            "truecolor" | "24bit" | "24-bit" | "rgb" => Some(ColorMode::TrueColor),
            "256" | "ansi256" | "256color" => Some(ColorMode::Ansi256),
            _ => None,
        }
    }
}

/// The picker's overall look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeStyle {
    /// Neon HUD: gradient panels, banner, scanlines and effects.
    #[default]
    Cyberpunk,
    /// Plain terminal look: ANSI colours, thin borders, no effects.
    Classic,
}

impl ThemeStyle {
    /// Lowercase string used in TOML and by `install.sh --theme`.
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeStyle::Cyberpunk => "cyberpunk",
            ThemeStyle::Classic => "classic",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "cyberpunk" | "neon" => Some(ThemeStyle::Cyberpunk),
            "classic" | "plain" => Some(ThemeStyle::Classic),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
/// Colors and effects used by the picker UI. Defaults are the neon palette.
pub struct Theme {
    /// Overall look; picks the palette the colour keys override.
    pub style: ThemeStyle,
    /// Values, numbers and the input cursor (cyan).
    pub accent: Color,
    /// Kill confirm and errors (red).
    pub warning: Color,
    /// Base colour of the selected-row beam.
    pub selection_bg: Color,
    /// Titles, selector and banner (neon pink).
    pub primary: Color,
    /// Borders and gradient midpoints (violet).
    pub secondary: Color,
    /// Status line and running commands (yellow).
    pub highlight: Color,
    /// Attached sessions and live activity (green).
    pub success: Color,
    /// Body text.
    pub text: Color,
    /// Screen background; `Color::Reset` keeps the terminal's own.
    pub background: Color,
    /// Glitch, pulse and shimmer effects. False renders a static frame.
    pub animations: bool,
    /// CRT-style shading on alternate rows.
    pub scanlines: bool,
    /// Truecolor vs 256-colour output.
    pub color_mode: ColorMode,
}

impl Theme {
    /// The classic look: the terminal's own ANSI colours and background,
    /// thin borders, no effects.
    pub fn classic() -> Self {
        Theme {
            style: ThemeStyle::Classic,
            accent: Color::Cyan,
            warning: Color::Red,
            // Grey-ramp 239, not ANSI 8: palettes like Solarized make
            // "bright black" the background colour, hiding the bar.
            selection_bg: Color::Indexed(239),
            primary: Color::White,
            secondary: Color::DarkGray,
            highlight: Color::Yellow,
            success: Color::Green,
            text: Color::Gray,
            background: Color::Reset,
            animations: false,
            scanlines: false,
            color_mode: ColorMode::Auto,
        }
    }

    /// Every colour key in `[theme]`, in the order `to_toml` prints them.
    pub const COLOR_KEYS: [&'static str; 9] = [
        "primary",
        "secondary",
        "accent",
        "highlight",
        "success",
        "warning",
        "text",
        "background",
        "selection_bg",
    ];

    /// The colour stored under a `[theme]` key.
    pub fn color(&self, key: &str) -> Option<Color> {
        Some(match key {
            "primary" => self.primary,
            "secondary" => self.secondary,
            "accent" => self.accent,
            "highlight" => self.highlight,
            "success" => self.success,
            "warning" => self.warning,
            "text" => self.text,
            "background" => self.background,
            "selection_bg" => self.selection_bg,
            _ => return None,
        })
    }

    fn color_mut(&mut self, key: &str) -> Option<&mut Color> {
        Some(match key {
            "primary" => &mut self.primary,
            "secondary" => &mut self.secondary,
            "accent" => &mut self.accent,
            "highlight" => &mut self.highlight,
            "success" => &mut self.success,
            "warning" => &mut self.warning,
            "text" => &mut self.text,
            "background" => &mut self.background,
            "selection_bg" => &mut self.selection_bg,
            _ => return None,
        })
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            style: ThemeStyle::Cyberpunk,
            accent: Color::Rgb(0x05, 0xd9, 0xe8),
            warning: Color::Rgb(0xff, 0x00, 0x3c),
            selection_bg: Color::Rgb(0x1e, 0x0a, 0x36),
            primary: Color::Rgb(0xff, 0x2a, 0x6d),
            secondary: Color::Rgb(0x7a, 0x04, 0xeb),
            highlight: Color::Rgb(0xf9, 0xf0, 0x02),
            success: Color::Rgb(0x39, 0xff, 0x14),
            text: Color::Rgb(0xc4, 0xbe, 0xe4),
            background: Color::Rgb(0x08, 0x05, 0x12),
            animations: true,
            scanlines: true,
            color_mode: ColorMode::Auto,
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
                Some(theme_table) => apply_theme(theme_table, &mut cfg.theme, &mut warnings),
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
        out.push_str(&format!("style = \"{}\"\n", self.theme.style.as_str()));
        for key in Theme::COLOR_KEYS {
            if let Some(color) = self.theme.color(key) {
                out.push_str(&format!("{key} = {}\n", color_to_toml(color)));
            }
        }
        out.push_str(&format!("animations = {}\n", self.theme.animations));
        out.push_str(&format!("scanlines = {}\n", self.theme.scanlines));
        out.push_str(&format!("color = \"{}\"\n", self.theme.color_mode.as_str()));
        out
    }
}

/// Values the pre-neon starter config (`--init`, `install.sh`) wrote out.
/// They meant "the default", so they keep meaning that instead of pinning
/// old ANSI colours over the neon palette.
const LEGACY_DEFAULTS: [(&str, &str); 3] = [
    ("accent", "cyan"),
    ("warning", "red"),
    ("selection_bg", "darkgray"),
];

fn is_legacy_default(key: &str, value: &toml::Value) -> bool {
    value.as_str().is_some_and(|v| {
        LEGACY_DEFAULTS
            .iter()
            .any(|(k, d)| *k == key && v.trim().eq_ignore_ascii_case(d))
    })
}

fn apply_theme(table: &toml::Table, theme: &mut Theme, warnings: &mut Vec<String>) {
    if let Some(v) = table.get("style") {
        match v.as_str().and_then(ThemeStyle::from_str) {
            Some(ThemeStyle::Classic) => *theme = Theme::classic(),
            Some(ThemeStyle::Cyberpunk) => {}
            None => warnings.push(format!(
                "theme.style must be \"cyberpunk\" or \"classic\", got {v:?}; using default"
            )),
        }
    }
    let classic = theme.style == ThemeStyle::Classic;
    let neon = Theme::default();
    for key in Theme::COLOR_KEYS {
        if table.get(key).is_some_and(|v| is_legacy_default(key, v)) {
            continue;
        }
        if let Some(dest) = theme.color_mut(key) {
            let before = *dest;
            apply_color(table, key, dest, warnings);
            // The neon starter wrote every colour out; under classic those
            // values meant "the default" too, so keep the classic one.
            if classic && neon.color(key) == Some(*dest) {
                *dest = before;
            }
        }
    }
    for (key, dest) in [
        ("animations", &mut theme.animations),
        ("scanlines", &mut theme.scanlines),
    ] {
        if let Some(v) = table.get(key) {
            match v.as_bool() {
                Some(b) => *dest = b,
                None => warnings.push(format!("theme.{key} must be a boolean; using default")),
            }
        }
    }
    if let Some(v) = table.get("color") {
        match v.as_str().and_then(ColorMode::from_str) {
            Some(mode) => theme.color_mode = mode,
            None => warnings.push(format!(
                "theme.color must be \"auto\", \"truecolor\" or \"256\", got {v:?}; using default"
            )),
        }
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
        "lightgray" | "lightgrey" | "silver" => Color::Gray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "reset" | "default" | "none" => Color::Reset,
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
        Color::Gray => "\"lightgray\"".into(),
        Color::LightRed => "\"lightred\"".into(),
        Color::LightGreen => "\"lightgreen\"".into(),
        Color::LightYellow => "\"lightyellow\"".into(),
        Color::LightBlue => "\"lightblue\"".into(),
        Color::LightMagenta => "\"lightmagenta\"".into(),
        Color::LightCyan => "\"lightcyan\"".into(),
        Color::Reset => "\"reset\"".into(),
        Color::Rgb(r, g, b) => format!("\"#{r:02x}{g:02x}{b:02x}\""),
        Color::Indexed(n) => n.to_string(),
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
        let cfg = Config {
            trigger_mode: TriggerMode::SshOnly,
            ..Config::default()
        };
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
        assert_eq!(cfg.theme.accent, Theme::default().accent);
        assert!(warnings.iter().any(|w| w.contains("chartreuse")));
    }

    #[test]
    fn theme_not_a_table_warns() {
        let (cfg, warnings) = Config::from_str_with_warnings("theme = 7");
        assert_eq!(cfg.theme.accent, Theme::default().accent);
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
        assert_eq!(cfg.theme.accent, Theme::default().accent);
        assert!(warnings.iter().any(|w| w.contains("non-hex digit")));
    }

    #[test]
    fn theme_rejects_hex_wrong_length() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = \"#ff88\"");
        assert_eq!(cfg.theme.accent, Theme::default().accent);
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
        assert_eq!(cfg.theme.accent, Theme::default().accent);
        assert!(warnings.iter().any(|w| w.contains("0..=255")));
    }

    #[test]
    fn theme_rejects_indexed_negative() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = -1");
        assert_eq!(cfg.theme.accent, Theme::default().accent);
        assert!(warnings.iter().any(|w| w.contains("0..=255")));
    }

    #[test]
    fn theme_rejects_wrong_type() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\naccent = true");
        assert_eq!(cfg.theme.accent, Theme::default().accent);
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
        assert_eq!(cfg.theme, default.theme);
        assert!(!cfg.markers.disable_defaults);
    }

    // -----------------------------------------------------------------------
    // Neon theme keys
    // -----------------------------------------------------------------------

    #[test]
    fn legacy_starter_values_keep_neon_defaults() {
        let cfg = Config::from_str(
            "[theme]\naccent = \"cyan\"\nwarning = \"red\"\nselection_bg = \"darkgray\"",
        );
        assert_eq!(cfg.theme, Theme::default());
    }

    #[test]
    fn non_legacy_named_colour_still_applies() {
        let cfg = Config::from_str("[theme]\nselection_bg = \"blue\"\naccent = \"red\"");
        assert_eq!(cfg.theme.selection_bg, Color::Blue);
        assert_eq!(cfg.theme.accent, Color::Red);
    }

    #[test]
    fn new_colour_keys_parse() {
        let cfg = Config::from_str(
            "[theme]\nprimary = \"#010203\"\nsecondary = 5\nhighlight = \"yellow\"\n\
             success = \"lightgreen\"\ntext = \"white\"\nbackground = \"reset\"",
        );
        assert_eq!(cfg.theme.primary, Color::Rgb(1, 2, 3));
        assert_eq!(cfg.theme.secondary, Color::Indexed(5));
        assert_eq!(cfg.theme.highlight, Color::Yellow);
        assert_eq!(cfg.theme.success, Color::LightGreen);
        assert_eq!(cfg.theme.text, Color::White);
        assert_eq!(cfg.theme.background, Color::Reset);
    }

    #[test]
    fn effect_flags_parse_and_reject_non_bools() {
        let cfg = Config::from_str("[theme]\nanimations = false\nscanlines = false");
        assert!(!cfg.theme.animations);
        assert!(!cfg.theme.scanlines);
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\nanimations = \"no\"");
        assert!(cfg.theme.animations);
        assert!(warnings.iter().any(|w| w.contains("theme.animations")));
    }

    #[test]
    fn color_mode_parses_and_warns() {
        assert_eq!(
            Config::from_str("[theme]\ncolor = \"truecolor\"")
                .theme
                .color_mode,
            ColorMode::TrueColor
        );
        assert_eq!(
            Config::from_str("[theme]\ncolor = \"256\"")
                .theme
                .color_mode,
            ColorMode::Ansi256
        );
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\ncolor = \"cmyk\"");
        assert_eq!(cfg.theme.color_mode, ColorMode::Auto);
        assert!(warnings.iter().any(|w| w.contains("theme.color")));
    }

    #[test]
    fn to_toml_round_trips_whole_theme() {
        let mut cfg = Config::default();
        cfg.theme.background = Color::Reset;
        cfg.theme.animations = false;
        cfg.theme.color_mode = ColorMode::Ansi256;
        let (back, warnings) = Config::from_str_with_warnings(&cfg.to_toml());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back.theme, cfg.theme);
    }

    // -----------------------------------------------------------------------
    // Theme style
    // -----------------------------------------------------------------------

    #[test]
    fn style_defaults_to_cyberpunk() {
        assert_eq!(Config::default().theme.style, ThemeStyle::Cyberpunk);
        let cfg = Config::from_str("[theme]\nstyle = \"cyberpunk\"");
        assert_eq!(cfg.theme, Theme::default());
    }

    #[test]
    fn classic_style_loads_the_classic_preset() {
        let cfg = Config::from_str("[theme]\nstyle = \"classic\"");
        assert_eq!(cfg.theme, Theme::classic());
        assert_eq!(cfg.theme.style, ThemeStyle::Classic);
        assert_eq!(cfg.theme.background, Color::Reset);
        assert!(!cfg.theme.animations && !cfg.theme.scanlines);
    }

    #[test]
    fn classic_overrides_apply_on_top_of_the_preset() {
        let cfg = Config::from_str("[theme]\nselection_bg = \"blue\"\nstyle = \"Classic\"");
        assert_eq!(cfg.theme.selection_bg, Color::Blue);
        assert_eq!(cfg.theme.accent, Theme::classic().accent);
    }

    #[test]
    fn neon_starter_values_mean_default_under_classic() {
        // A config written by the neon starter, then switched to classic.
        let neon_starter = STARTER_TOML.replace("style = \"cyberpunk\"", "style = \"classic\"");
        let uncommented: String = neon_starter
            .lines()
            .map(|l| {
                l.strip_prefix("# ")
                    .filter(|r| r.contains(" = \"#"))
                    .unwrap_or(l)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let (cfg, warnings) = Config::from_str_with_warnings(&uncommented);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(cfg.theme.primary, Theme::classic().primary);
        assert_eq!(cfg.theme.selection_bg, Theme::classic().selection_bg);
        assert_eq!(cfg.theme.background, Theme::classic().background);
    }

    #[test]
    fn unknown_style_warns_and_keeps_cyberpunk() {
        let (cfg, warnings) = Config::from_str_with_warnings("[theme]\nstyle = \"vaporwave\"");
        assert_eq!(cfg.theme.style, ThemeStyle::Cyberpunk);
        assert!(warnings.iter().any(|w| w.contains("theme.style")));
    }

    #[test]
    fn to_toml_round_trips_classic() {
        let mut cfg = Config::from_str("[theme]\nstyle = \"classic\"\naccent = \"magenta\"");
        cfg.theme.color_mode = ColorMode::TrueColor;
        let out = cfg.to_toml();
        assert!(out.contains("style = \"classic\""), "{out}");
        let (back, warnings) = Config::from_str_with_warnings(&out);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back.theme, cfg.theme);
    }

    #[test]
    fn marker_lookup_lowercases_commands() {
        let m = Markers::default();
        assert!(m.lookup(&["Claude".to_string()]).is_some());
    }
}
