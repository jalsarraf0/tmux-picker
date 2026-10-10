//! Neon rendering primitives: palette resolution (truecolor, or nearest
//! xterm-256 colour), gradients, a stateless hash that drives the glitch
//! effects, the banner font, and clipped writes into a ratatui `Buffer`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::config::{ColorMode, Theme, ThemeStyle};

/// An sRGB triple. All colour math happens here before resolving to a
/// terminal `Color`.
pub type Rgb = [u8; 3];

/// Background assumed for derived shades when the theme uses the
/// terminal's own background.
const FALLBACK_BG: Rgb = [8, 5, 18];

/// Linear blend from `a` (t = 0) to `b` (t = 1); `t` is clamped.
pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
}

/// Sample a multi-stop gradient at `t` in 0..=1.
pub fn gradient(stops: &[Rgb], t: f32) -> Rgb {
    match stops {
        [] => [0, 0, 0],
        [only] => *only,
        _ => {
            let scaled = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
            let i = (scaled as usize).min(stops.len() - 2);
            lerp(stops[i], stops[i + 1], scaled - i as f32)
        }
    }
}

/// splitmix64: a cheap, well-mixed hash. Glitch effects hash the time
/// slot and cell position, so a frame is a pure function of its inputs.
pub fn hash(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Hash of several values.
pub fn hash3(a: u64, b: u64, c: u64) -> u64 {
    hash(a ^ hash(b ^ hash(c)))
}

// ---------------------------------------------------------------------------
// Colour resolution
// ---------------------------------------------------------------------------

const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

const ANSI16: [Rgb; 16] = [
    [0, 0, 0],
    [205, 0, 0],
    [0, 205, 0],
    [205, 205, 0],
    [0, 0, 238],
    [205, 0, 205],
    [0, 205, 205],
    [229, 229, 229],
    [127, 127, 127],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [92, 92, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

/// RGB for an xterm-256 palette index.
pub fn indexed_rgb(n: u8) -> Rgb {
    match n {
        0..=15 => ANSI16[usize::from(n)],
        16..=231 => {
            let i = n - 16;
            [
                CUBE[usize::from(i / 36)],
                CUBE[usize::from(i / 6 % 6)],
                CUBE[usize::from(i % 6)],
            ]
        }
        _ => {
            let level = 8 + (n - 232) * 10;
            [level, level, level]
        }
    }
}

fn dist2(a: Rgb, b: Rgb) -> u32 {
    let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    // Weight green highest, blue lowest, roughly as the eye does.
    2 * d(a[0], b[0]) + 4 * d(a[1], b[1]) + 3 * d(a[2], b[2])
}

fn nearest_cube_level(c: u8) -> usize {
    CUBE.iter()
        .enumerate()
        .min_by_key(|&(_, &level)| level.abs_diff(c))
        .map_or(0, |(i, _)| i)
}

/// Nearest xterm-256 index (colour cube or grey ramp) to `rgb`.
pub fn xterm256(rgb: Rgb) -> u8 {
    let [r, g, b] = rgb.map(nearest_cube_level);
    let cube = (16 + 36 * r + 6 * g + b) as u8;
    let avg = (u32::from(rgb[0]) + u32::from(rgb[1]) + u32::from(rgb[2])) / 3;
    let step = (avg.saturating_sub(3) / 10).min(23) as u8;
    let grey = 232 + step;
    if dist2(indexed_rgb(grey), rgb) < dist2(indexed_rgb(cube), rgb) {
        grey
    } else {
        cube
    }
}

/// Approximate RGB of any ratatui colour; None for `Color::Reset`.
pub fn to_rgb(color: Color) -> Option<Rgb> {
    Some(match color {
        Color::Reset => return None,
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Indexed(n) => indexed_rgb(n),
        Color::Black => ANSI16[0],
        Color::Red => ANSI16[1],
        Color::Green => ANSI16[2],
        Color::Yellow => ANSI16[3],
        Color::Blue => ANSI16[4],
        Color::Magenta => ANSI16[5],
        Color::Cyan => ANSI16[6],
        Color::Gray => ANSI16[7],
        Color::DarkGray => ANSI16[8],
        Color::LightRed => ANSI16[9],
        Color::LightGreen => ANSI16[10],
        Color::LightYellow => ANSI16[11],
        Color::LightBlue => ANSI16[12],
        Color::LightMagenta => ANSI16[13],
        Color::LightCyan => ANSI16[14],
        Color::White => ANSI16[15],
    })
}

/// True when the environment advertises 24-bit colour. SSH does not
/// forward `$COLORTERM`, so known truecolor `$TERM`s count too; anything
/// else gets the 256-colour fallback (or set `theme.color`).
pub fn env_truecolor() -> bool {
    let var = |k: &str| std::env::var(k).unwrap_or_default().to_lowercase();
    let colorterm = var("COLORTERM");
    if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        return true;
    }
    let term = var("TERM");
    if ["direct", "truecolor", "24bit"]
        .iter()
        .any(|k| term.contains(k))
    {
        return true;
    }
    const TRUECOLOR_TERMS: &[&str] = &[
        "xterm-kitty",
        "xterm-ghostty",
        "ghostty",
        "alacritty",
        "foot",
        "wezterm",
        "contour",
        "rio",
        "konsole",
        "iterm2",
    ];
    if TRUECOLOR_TERMS.iter().any(|t| term.starts_with(t)) {
        return true;
    }
    const TRUECOLOR_PROGRAMS: &[&str] = &[
        "iterm.app",
        "wezterm",
        "vscode",
        "ghostty",
        "tabby",
        "hyper",
    ];
    let program = var("TERM_PROGRAM");
    TRUECOLOR_PROGRAMS.contains(&program.as_str())
}

/// The resolved neon palette for one theme.
#[derive(Debug, Clone)]
pub struct Palette {
    truecolor: bool,
    /// The classic look: thin borders, plain wording, no HUD effects, and
    /// ANSI colours sent as palette indexes so the terminal's theme shows.
    pub classic: bool,
    /// Glitch, pulse and shimmer effects.
    pub animate: bool,
    /// CRT shading on alternate rows.
    pub scanlines: bool,
    /// Screen background; None keeps the terminal's own.
    pub bg: Option<Rgb>,
    /// Background of shaded (odd) rows.
    pub scan: Rgb,
    /// Highlighted row background.
    pub hilite: Rgb,
    /// Selected-row beam, left to right: primary-hot, violet, fading.
    pub beam: [Rgb; 3],
    /// Modal card background.
    pub overlay: Rgb,
    pub pink: Rgb,
    pub purple: Rgb,
    pub cyan: Rgb,
    pub yellow: Rgb,
    pub green: Rgb,
    pub red: Rgb,
    pub white: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub faint: Rgb,
}

impl Palette {
    /// Resolve `theme`, detecting truecolor support from the environment
    /// when `theme.color` is "auto".
    pub fn new(theme: &Theme) -> Self {
        let truecolor = match theme.color_mode {
            ColorMode::Auto => env_truecolor(),
            ColorMode::TrueColor => true,
            ColorMode::Ansi256 => false,
        };
        Self::with_truecolor(theme, truecolor)
    }

    /// Resolve `theme` with an explicit colour depth (tests, overrides).
    pub fn with_truecolor(theme: &Theme, truecolor: bool) -> Self {
        let d = Theme::default();
        let pick = |c: Color, fallback: Color| {
            to_rgb(c)
                .or_else(|| to_rgb(fallback))
                .unwrap_or(FALLBACK_BG)
        };
        let classic = theme.style == ThemeStyle::Classic;
        let bg = to_rgb(theme.background);
        let base = bg.unwrap_or(if classic { [0, 0, 0] } else { FALLBACK_BG });
        let text = pick(theme.text, d.text);
        let purple = pick(theme.secondary, d.secondary);
        let hilite = pick(theme.selection_bg, d.selection_bg);
        let dim = lerp(lerp(base, text, 0.58), purple, 0.12);
        let pink = pick(theme.primary, d.primary);
        Palette {
            truecolor,
            classic,
            animate: theme.animations,
            scanlines: theme.scanlines,
            bg,
            scan: lerp(base, hilite, 0.22),
            hilite,
            beam: [
                lerp(hilite, pink, 0.45),
                lerp(hilite, purple, 0.42),
                lerp(hilite, purple, 0.2),
            ],
            overlay: lerp(base, hilite, 0.45),
            pink,
            purple,
            cyan: pick(theme.accent, d.accent),
            yellow: pick(theme.highlight, d.highlight),
            green: pick(theme.success, d.success),
            red: pick(theme.warning, d.warning),
            white: lerp(text, [255, 255, 255], 0.65),
            text,
            dim,
            faint: lerp(base, dim, 0.45),
        }
    }

    /// True when colours are sent as 24-bit RGB.
    pub fn truecolor(&self) -> bool {
        self.truecolor
    }

    /// Resolve an RGB value for this terminal.
    pub fn c(&self, rgb: Rgb) -> Color {
        if self.classic
            && let Some(i) = ANSI16.iter().position(|&a| a == rgb)
        {
            return Color::Indexed(i as u8);
        }
        if self.truecolor {
            Color::Rgb(rgb[0], rgb[1], rgb[2])
        } else {
            Color::Indexed(xterm256(rgb))
        }
    }

    /// The background colours blend toward: the theme's, or a near-black
    /// stand-in when the terminal's own shows through.
    pub fn base(&self) -> Rgb {
        self.bg.unwrap_or(FALLBACK_BG)
    }

    /// The screen background colour (`Color::Reset` when transparent).
    pub fn bg_color(&self) -> Color {
        self.bg.map_or(Color::Reset, |bg| self.c(bg))
    }

    /// Foreground-only style.
    pub fn fg(&self, rgb: Rgb) -> Style {
        Style::new().fg(self.c(rgb))
    }

    /// Bold foreground style.
    pub fn bold(&self, rgb: Rgb) -> Style {
        self.fg(rgb).add_modifier(Modifier::BOLD)
    }

    /// Neon sweep used by the banner: pink → violet → cyan → pink.
    pub fn neon(&self) -> [Rgb; 4] {
        [self.pink, self.purple, self.cyan, self.pink]
    }
}

// ---------------------------------------------------------------------------
// Animation clock
// ---------------------------------------------------------------------------

/// Seconds since the picker started, and whether effects run. With
/// effects off every frame is the settled, glitch-free one.
#[derive(Debug, Clone, Copy)]
pub struct Fx {
    pub t: f32,
    pub on: bool,
}

impl Fx {
    /// Effect time: frozen past the intro when effects are off.
    pub fn time(self) -> f32 {
        if self.on { self.t } else { INTRO + 10.0 }
    }

    /// True during a brief, randomly scheduled banner glitch.
    pub fn glitching(self) -> bool {
        if !self.on || self.t < INTRO {
            return false;
        }
        let slot = (self.t / GLITCH_SLOT) as u64;
        hash(slot ^ 0x6e65_6f6e) % 100 < 6 && (self.t / GLITCH_SLOT).fract() < 0.55
    }

    /// Slow blink for prompts; always lit with effects off.
    pub fn blink(self, hz: f32) -> bool {
        !self.on || ((self.t * hz * 2.0) as u64).is_multiple_of(2)
    }
}

/// Seconds of banner "decryption" at startup.
pub const INTRO: f32 = 0.7;
/// Glitch scheduling granularity in seconds.
const GLITCH_SLOT: f32 = 0.22;

// ---------------------------------------------------------------------------
// Banner font (5-pixel bitmap glyphs, drawn two pixels per cell)
// ---------------------------------------------------------------------------

/// Pixel rows per glyph. With half blocks that is two and a half cells,
/// leaving the last half row for the drop shadow.
pub const FONT_H: usize = 5;

fn glyph(c: char) -> [&'static str; FONT_H] {
    match c {
        'T' => ["###", ".#.", ".#.", ".#.", ".#."],
        'M' => ["#...#", "##.##", "#.#.#", "#...#", "#...#"],
        'U' => ["#.#", "#.#", "#.#", "#.#", "###"],
        'X' => ["#.#", "#.#", ".#.", "#.#", "#.#"],
        'P' => ["##.", "#.#", "##.", "#..", "#.."],
        'I' => ["###", ".#.", ".#.", ".#.", "###"],
        'C' => [".##", "#..", "#..", "#..", ".##"],
        'K' => ["#.#", "#.#", "##.", "#.#", "#.#"],
        'E' => ["###", "#..", "##.", "#..", "###"],
        'R' => ["##.", "#.#", "##.", "#.#", "#.#"],
        '/' => ["..#", "..#", ".#.", "#..", "#.."],
        _ => [".", ".", ".", ".", "."],
    }
}

/// The banner bitmap for `text`: `FONT_H` rows, true where a pixel is lit.
/// Glyphs are one blank column apart.
pub fn banner_bits(text: &str) -> [Vec<bool>; FONT_H] {
    let mut rows: [Vec<bool>; FONT_H] = Default::default();
    for (i, c) in text.chars().enumerate() {
        for (row, part) in rows.iter_mut().zip(glyph(c)) {
            if i > 0 {
                row.push(false);
            }
            row.extend(part.chars().map(|p| p == '#'));
        }
    }
    rows
}

/// The half-block glyph for a cell whose top and bottom pixels are lit.
pub fn half_block(top: bool, bottom: bool) -> char {
    match (top, bottom) {
        (true, true) => '█',
        (true, false) => '▀',
        (false, true) => '▄',
        (false, false) => ' ',
    }
}

/// The banner as plain half-block text rows (three cells tall).
pub fn banner_rows(text: &str) -> [Vec<char>; 3] {
    let bits = banner_bits(text);
    let px = |y: usize, x: usize| bits.get(y).is_some_and(|r| r[x]);
    std::array::from_fn(|row| {
        (0..bits[0].len())
            .map(|x| half_block(px(2 * row, x), px(2 * row + 1, x)))
            .collect()
    })
}

/// Characters used for glitch noise.
pub const NOISE: &[char] = &['▓', '▒', '░', '#', '/', '\\', '<', '>', '_', '=', '+', '¦'];
/// Braille spinner frames.
pub const SPINNER: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

// ---------------------------------------------------------------------------
// Clipped buffer writes
// ---------------------------------------------------------------------------

/// Write `text` at (x, y) with `style`, at most `max` columns wide and
/// never past the buffer edge. Wide characters are measured properly.
/// Returns the column after the last written cell.
pub fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, max: u16, style: Style) -> u16 {
    let area = buf.area;
    if y < area.top() || y >= area.bottom() || x >= area.right() || x < area.left() || max == 0 {
        return x;
    }
    buf.set_stringn(x, y, text, usize::from(max), style).0
}

/// Set one cell's symbol and style (merged onto what is there).
pub fn cell(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(c) = buf.cell_mut((x, y)) {
        let mut tmp = [0u8; 4];
        c.set_symbol(ch.encode_utf8(&mut tmp)).set_style(style);
    }
}

/// Paint the background of a rectangle (clipped).
pub fn fill_bg(buf: &mut Buffer, area: Rect, bg: Color) {
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_symbol(" ").set_bg(bg);
            }
        }
    }
}

/// Display width of `text` in terminal columns.
pub fn width(text: &str) -> u16 {
    u16::try_from(unicode_width::UnicodeWidthStr::width(text)).unwrap_or(u16::MAX)
}

/// Truncate `text` to `max` columns, ending in "…" when cut.
pub fn ellipsize(text: &str, max: u16) -> std::borrow::Cow<'_, str> {
    use unicode_width::UnicodeWidthChar;
    if width(text) <= max {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::new();
    let mut used = 0u16;
    for c in text.chars() {
        let w = u16::try_from(c.width().unwrap_or(0)).unwrap_or(0);
        if used + w + 1 > max {
            break;
        }
        used += w;
        out.push(c);
    }
    if max > 0 {
        out.push('…');
    }
    std::borrow::Cow::Owned(out)
}

/// A heavy-line box whose border runs along a gradient, with a title set
/// into the top edge (`┏━━ TITLE ━━━`) and an optional right-hand tag.
/// `hot` boxes glow violet→cyan with a pink title; cold ones fade to the
/// background.
pub struct NeonBox<'a> {
    pub title: &'a str,
    pub tag: Option<&'a str>,
    pub hot: bool,
    /// Override border stops (e.g. red for the kill prompt).
    pub stops: Option<(Rgb, Rgb)>,
}

impl NeonBox<'_> {
    /// Column of the tag's opening `┫`, when the tag fits beside the title.
    pub fn tag_x(&self, area: Rect) -> Option<u16> {
        let tag = self.tag?;
        let room = area.width.saturating_sub(6);
        if self.title.is_empty() || room <= 2 || area.height < 2 {
            return None;
        }
        let title_end = area.x + 3 + width(&ellipsize(self.title, room)) + 2;
        let tag_x = (area.right() - 1).saturating_sub(width(tag) + 4);
        (tag_x > title_end + 2).then_some(tag_x)
    }

    pub fn render(&self, buf: &mut Buffer, pal: &Palette, area: Rect) {
        if area.width < 2 || area.height < 2 {
            return;
        }
        let (a, b) = self.stops.unwrap_or(if self.hot {
            (pal.purple, pal.cyan)
        } else {
            (pal.faint, pal.purple)
        });
        let (x0, y0) = (area.x, area.y);
        let (x1, y1) = (area.right() - 1, area.bottom() - 1);
        // Classic: thin rounded lines in one colour, no gradient.
        let (a, b) = if pal.classic {
            let line = self.stops.map_or(pal.purple, |(a, _)| a);
            (line, line)
        } else {
            (a, b)
        };
        let [h, v, tl, tr, bl, br] = if pal.classic {
            ['─', '│', '╭', '╮', '╰', '╯']
        } else {
            ['━', '┃', '┏', '┓', '┗', '┛']
        };
        let span = f32::from(area.width.saturating_sub(1).max(1));
        for x in x0..=x1 {
            let col = pal.fg(gradient(&[a, b], f32::from(x - x0) / span));
            cell(buf, x, y0, h, col);
            cell(buf, x, y1, h, col);
        }
        for y in y0 + 1..y1 {
            cell(buf, x0, y, v, pal.fg(a));
            cell(buf, x1, y, v, pal.fg(b));
        }
        cell(buf, x0, y0, tl, pal.fg(a));
        cell(buf, x1, y0, tr, pal.fg(b));
        cell(buf, x0, y1, bl, pal.fg(a));
        cell(buf, x1, y1, br, pal.fg(b));

        // The title and tag cut into the top edge; the line itself frames
        // them.
        let room = area.width.saturating_sub(6);
        if !self.title.is_empty() && room > 2 {
            let title = ellipsize(self.title, room);
            let title_style = pal.bold(if self.hot { pal.pink } else { pal.dim });
            put(
                buf,
                x0 + 3,
                y0,
                &format!(" {title} "),
                room + 2,
                title_style,
            );
            if let (Some(tag), Some(tag_x)) = (self.tag, self.tag_x(area)) {
                let tag_w = width(tag) + 2;
                put(
                    buf,
                    tag_x + 1,
                    y0,
                    &format!(" {tag} "),
                    tag_w,
                    pal.fg(pal.dim),
                );
            }
        }
    }
}

/// Fade every cell in `area` toward the background by `amount` (0..=1),
/// so a modal card stands out from the frame behind it.
pub fn scrim(buf: &mut Buffer, pal: &Palette, area: Rect, amount: f32) {
    let base = pal.base();
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                let fg = to_rgb(c.fg).unwrap_or(pal.text);
                c.fg = pal.c(lerp(fg, base, amount));
                if let Some(bg) = to_rgb(c.bg) {
                    c.bg = pal.c(lerp(bg, base, amount));
                }
                c.modifier.remove(Modifier::BOLD);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------

/// A progress bar: gradient fill pink→violet→cyan with a travelling
/// shimmer and an eighth-block leading edge. `frac = None` draws an
/// indeterminate sweep.
pub fn bar(buf: &mut Buffer, pal: &Palette, x: u16, y: u16, w: u16, frac: Option<f32>, fx: Fx) {
    if w == 0 {
        return;
    }
    if pal.classic {
        let filled = frac
            .map_or(0.0, |f| f.clamp(0.0, 1.0) * f32::from(w))
            .round() as u16;
        for i in 0..w {
            if i < filled {
                cell(buf, x + i, y, '━', pal.fg(pal.cyan));
            } else {
                cell(buf, x + i, y, '─', pal.fg(pal.faint));
            }
        }
        return;
    }
    let t = fx.time();
    let Some(frac) = frac else {
        let center = ((t * 2.2).sin() + 1.0) / 2.0 * f32::from(w - 1);
        for i in 0..w {
            let d = (f32::from(i) - center).abs();
            if d < 4.0 {
                cell(
                    buf,
                    x + i,
                    y,
                    '█',
                    pal.fg(lerp(pal.pink, pal.faint, d / 4.0)),
                );
            } else {
                cell(buf, x + i, y, '─', pal.fg(pal.faint));
            }
        }
        return;
    };
    let frac = frac.clamp(0.0, 1.0);
    let filled = frac * f32::from(w);
    let whole = filled as u16;
    let shimmer = (t * 25.0) % (f32::from(w) + 12.0) - 6.0;
    let span = f32::from(w.saturating_sub(1).max(1));
    for i in 0..w {
        if i < whole {
            let mut col = gradient(&[pal.pink, pal.purple, pal.cyan], f32::from(i) / span);
            if fx.on && (f32::from(i) - shimmer).abs() < 2.0 {
                col = lerp(col, pal.white, 0.55);
            }
            cell(buf, x + i, y, '█', pal.fg(col));
        } else if i == whole && frac < 1.0 {
            const EIGHTHS: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
            let part = ((filled - f32::from(i)) * 8.0) as usize;
            cell(buf, x + i, y, EIGHTHS[part.min(7)], pal.fg(pal.cyan));
        } else {
            cell(buf, x + i, y, '░', pal.fg(pal.faint));
        }
    }
}

/// Hex "digital rain" inside a rectangle, used where there is no signal.
pub fn rain(buf: &mut Buffer, pal: &Palette, area: Rect, fx: Fx, seed: u64) {
    let t = fx.time();
    let h = i64::from(area.height);
    if h == 0 {
        return;
    }
    for col in (0..area.width).step_by(2) {
        let c = u64::from(col) + seed;
        let speed = 5.0 + (c * 7919 % 9) as f32;
        let tick = (t * speed) as i64;
        let head = (tick + (c * 104_729 % (area.height as u64 * 3)) as i64).rem_euclid(h + 8);
        for row in 0..area.height {
            let dist = head - i64::from(row);
            if (0..6).contains(&dist) {
                let glyph =
                    b"0123456789ABCDEF"[(hash3(c, u64::from(row), tick as u64) % 16) as usize];
                let base = if dist == 0 { pal.cyan } else { pal.purple };
                let fade = pal.base();
                let mut style = pal.fg(lerp(base, fade, dist as f32 / 6.0));
                if dist == 0 {
                    style = style.add_modifier(Modifier::BOLD);
                }
                cell(buf, area.x + col, area.y + row, char::from(glyph), style);
            }
        }
    }
}

/// CRT scanlines: shade odd rows wherever the plain background shows.
pub fn scanlines(buf: &mut Buffer, pal: &Palette, area: Rect) {
    let Some(bg) = pal.bg else { return };
    if !pal.scanlines {
        return;
    }
    let (plain, shaded) = (pal.c(bg), pal.c(pal.scan));
    if plain == shaded {
        return;
    }
    let area = area.intersection(buf.area);
    for y in (area.top()..area.bottom()).filter(|y| y % 2 == 1) {
        for x in area.left()..area.right() {
            if let Some(c) = buf.cell_mut((x, y))
                && c.bg == plain
            {
                c.set_bg(shaded);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_endpoints_and_clamp() {
        assert_eq!(lerp([0, 0, 0], [200, 100, 50], 0.0), [0, 0, 0]);
        assert_eq!(lerp([0, 0, 0], [200, 100, 50], 1.0), [200, 100, 50]);
        assert_eq!(lerp([0, 0, 0], [200, 100, 50], 7.0), [200, 100, 50]);
        assert_eq!(lerp([0, 0, 0], [200, 100, 50], 0.5), [100, 50, 25]);
    }

    #[test]
    fn gradient_hits_every_stop() {
        let stops = [[255, 0, 0], [0, 255, 0], [0, 0, 255]];
        assert_eq!(gradient(&stops, 0.0), [255, 0, 0]);
        assert_eq!(gradient(&stops, 0.5), [0, 255, 0]);
        assert_eq!(gradient(&stops, 1.0), [0, 0, 255]);
        assert_eq!(gradient(&[], 0.3), [0, 0, 0]);
        assert_eq!(gradient(&[[1, 2, 3]], 0.3), [1, 2, 3]);
    }

    #[test]
    fn xterm256_exact_cube_and_grey_hits() {
        assert_eq!(xterm256([255, 0, 0]), 196);
        assert_eq!(xterm256([0, 0, 0]), 16);
        assert_eq!(xterm256([255, 255, 255]), 231);
        assert_eq!(xterm256([128, 128, 128]), 244);
        // Round trip: every cube colour maps back to itself.
        for n in 16u8..=231 {
            assert_eq!(xterm256(indexed_rgb(n)), n, "index {n}");
        }
    }

    #[test]
    fn xterm256_neon_background_stays_dark() {
        let n = xterm256([8, 5, 18]);
        let [r, g, b] = indexed_rgb(n);
        assert!(r < 20 && g < 20 && b < 20, "bg mapped to {n}");
    }

    #[test]
    fn to_rgb_resets_to_none() {
        assert_eq!(to_rgb(Color::Reset), None);
        assert_eq!(to_rgb(Color::Rgb(1, 2, 3)), Some([1, 2, 3]));
        assert_eq!(to_rgb(Color::Indexed(196)), Some([255, 0, 0]));
    }

    #[test]
    fn palette_256_mode_emits_only_indexed_colours() {
        let pal = Palette::with_truecolor(&Theme::default(), false);
        assert!(matches!(pal.c(pal.pink), Color::Indexed(_)));
        let pal = Palette::with_truecolor(&Theme::default(), true);
        assert_eq!(pal.c(pal.pink), Color::Rgb(0xff, 0x2a, 0x6d));
    }

    #[test]
    fn palette_reset_background_is_transparent() {
        let theme = Theme {
            background: Color::Reset,
            ..Theme::default()
        };
        let pal = Palette::with_truecolor(&theme, true);
        assert_eq!(pal.bg, None);
        assert_eq!(pal.bg_color(), Color::Reset);
    }

    #[test]
    fn banner_rows_are_equal_width() {
        let [a, b, c] = banner_rows("TMUX // PICKER");
        assert_eq!(a.len(), b.len());
        assert_eq!(b.len(), c.len());
        assert!(a.len() > 20);
    }

    #[test]
    fn fx_off_never_glitches_and_always_blinks_on() {
        for i in 0..2000 {
            let fx = Fx {
                t: i as f32 * 0.05,
                on: false,
            };
            assert!(!fx.glitching());
            assert!(fx.blink(1.0));
        }
    }

    #[test]
    fn fx_on_glitches_rarely() {
        let frames = 20_000;
        let glitched = (0..frames)
            .filter(|i| {
                Fx {
                    t: *i as f32 * 0.05,
                    on: true,
                }
                .glitching()
            })
            .count();
        assert!(glitched > 0, "glitch never fires");
        assert!(glitched < frames / 10, "glitch fires too often: {glitched}");
    }

    #[test]
    fn put_clips_instead_of_panicking() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        put(&mut buf, 0, 5, "offscreen", 10, Style::new());
        put(&mut buf, 9, 0, "offscreen", 10, Style::new());
        let end = put(&mut buf, 2, 1, "abcdef", 10, Style::new());
        assert_eq!(end, 4);
        cell(&mut buf, 40, 40, 'x', Style::new());
    }

    #[test]
    fn ellipsize_respects_width() {
        assert_eq!(ellipsize("hello", 10), "hello");
        assert_eq!(ellipsize("hello world", 6), "hello…");
        assert_eq!(width(&ellipsize("🤖🤖🤖🤖", 5)), 5);
    }
}
