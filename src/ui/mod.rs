//! The neon picker UI: a cyberpunk HUD painted straight into the ratatui
//! buffer, styled after the `update` dashboard (gradient heavy-line boxes,
//! a glitching block-letter banner, CRT scanlines, shimmer gauges).
//! Layout, panels and mouse hit-testing live here; colour math and effects
//! live in [`neon`].

pub mod neon;

use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::app::{App, Mode, PreviewMode, SortMode};
use crate::config::Theme;
use crate::session::Session;
use neon::{
    Fx, NOISE, NeonBox, Palette, Rgb, bar, cell, ellipsize, fill_bg, gradient, hash3, lerp, put,
    rain, scanlines, scrim, width,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BANNER_TEXT: &str = "TMUX//PICKER";
/// Frame pacing while effects run.
const FRAME: Duration = Duration::from_millis(100);
/// Tick without effects: only the countdown and clock move.
const IDLE_TICK: Duration = Duration::from_millis(250);

/// Fixed wording and separators for one look.
struct Words {
    /// Between status items.
    sep: &'static str,
    /// Between a label and its value ("FEED ▸ main", "name: ").
    to: &'static str,
    /// Countdown → the session it will attach.
    arrow: &'static str,
    sessions: &'static str,
    preview: &'static str,
    windows: &'static str,
    preview_tag: &'static str,
    windows_tag: &'static str,
    blank: &'static str,
    no_signal: &'static str,
    manual: &'static str,
    auto: &'static str,
    filter: &'static str,
    new_session: &'static str,
    rename: &'static str,
    kill: &'static str,
    matches: &'static str,
    kill_hint: &'static str,
    auto_off: &'static str,
    help_title: &'static str,
    help_close: &'static str,
    kill_title: &'static str,
    kill_confirm: &'static str,
    /// Link column: header, attached glyph + text, detached glyph.
    link_head: &'static str,
    linked: (&'static str, &'static str),
    unlinked: &'static str,
    /// Activity column header.
    act_head: &'static str,
}

const CYBER: Words = Words {
    sep: " ░ ",
    to: " ▸ ",
    arrow: " ▸ ",
    sessions: "SESSIONS",
    preview: "FEED",
    windows: "WINDOWS",
    preview_tag: "● LIVE · ⇥ WINDOWS",
    windows_tag: "● LIVE · ⇥ FEED",
    blank: "░ BLANK PANE ░",
    no_signal: " ◢◤ NO SIGNAL ◢◤ ",
    manual: "MANUAL",
    auto: "AUTO-ATTACH",
    filter: "FILTER",
    new_session: "NEW SESSION",
    rename: "RENAME",
    kill: "TERMINATE",
    matches: "MATCH",
    kill_hint: " ? · y ▸ confirm · any other key ▸ abort",
    auto_off: "auto-attach off · ⏎ to jack in",
    help_title: "COMMAND REFERENCE",
    help_close: "ESC / ? / Q ▸ CLOSE",
    kill_title: "TERMINATE SESSION",
    kill_confirm: "Y ▸ CONFIRM · ANY OTHER KEY ▸ ABORT",
    link_head: "LINK",
    linked: ("◆", " LINKED"),
    unlinked: " ",
    act_head: "SIGNAL",
};

const CLASSIC: Words = Words {
    sep: " · ",
    to: ": ",
    arrow: " → ",
    sessions: "sessions",
    preview: "preview",
    windows: "windows",
    preview_tag: "tab: windows",
    windows_tag: "tab: preview",
    blank: "(blank pane)",
    no_signal: "no preview",
    manual: "manual",
    auto: "auto-attach",
    filter: "filter",
    new_session: "new session",
    rename: "rename",
    kill: "kill",
    matches: "match",
    kill_hint: "? · y: confirm · any other key: abort",
    auto_off: "auto-attach off · enter to attach",
    help_title: "keys",
    help_close: "esc / ? / q: close",
    kill_title: "kill session",
    kill_confirm: "y: confirm · any other key: abort",
    link_head: "ATTACHED",
    linked: ("●", " yes"),
    unlinked: "○",
    act_head: "ACTIVITY",
};

/// Renderer state that outlives a frame: the resolved palette, the
/// animation clock, and strings that never change while the picker runs.
pub struct Ui {
    pal: Palette,
    started: Instant,
    who: String,
    banner: [Vec<bool>; neon::FONT_H],
    /// Wall clock shown in the banner (swappable so tests are stable).
    clock: fn() -> String,
}

impl Ui {
    /// Build a renderer for `theme`, detecting colour depth from the env.
    pub fn new(theme: &Theme) -> Self {
        Self::with_palette(Palette::new(theme))
    }

    /// Build a renderer around an already-resolved palette.
    pub fn with_palette(pal: Palette) -> Self {
        Ui {
            pal,
            started: Instant::now(),
            who: whoami(),
            banner: neon::banner_bits(BANNER_TEXT),
            clock: wall_clock,
        }
    }

    /// Re-resolve the palette after a config reload.
    pub fn set_theme(&mut self, theme: &Theme) {
        self.pal = Palette::new(theme);
    }

    /// Wording for the active look.
    fn words(&self) -> &'static Words {
        if self.pal.classic { &CLASSIC } else { &CYBER }
    }

    /// How often the picker loop should redraw and tick.
    pub fn frame_interval(&self) -> Duration {
        if self.pal.animate { FRAME } else { IDLE_TICK }
    }

    /// Draw one frame.
    pub fn draw(&self, frame: &mut Frame, app: &App) {
        let area = frame.area();
        let t = self.started.elapsed().as_secs_f32();
        self.render(area, frame.buffer_mut(), app, t);
    }

    /// Render the picker into `buf` as it looks `t` seconds after start.
    /// Pure apart from `app.list_offset`, which it keeps in view.
    pub fn render(&self, area: Rect, buf: &mut Buffer, app: &App, t: f32) {
        let pal = &self.pal;
        let fx = Fx { t, on: pal.animate };
        buf.set_style(area, Style::new().bg(pal.bg_color()).fg(pal.c(pal.text)));
        let a = layout(area, app, pal.classic);
        if let Some(r) = a.banner {
            self.draw_banner(buf, r, app, fx);
        }
        if let Some(r) = a.status {
            self.draw_status(buf, r, app, fx, a.banner.is_none());
        }
        self.draw_sessions(buf, &a, app, fx);
        if let Some(r) = a.preview {
            self.draw_preview(buf, r, app, fx);
        }
        if let Some(r) = a.gauge {
            self.draw_gauge(buf, r, app, fx);
        }
        if let Some(r) = a.footer {
            self.draw_footer(buf, r, app);
        }
        match app.mode {
            Mode::Help => self.draw_help(buf, area, fx),
            Mode::ConfirmKill => self.draw_kill(buf, area, app, fx),
            _ => {}
        }
        // The logo's half-lit cells would show the shading as a lighter
        // half pixel, so the scanlines start below it.
        let shaded = a.banner.map_or(area, |b| {
            Rect::new(
                area.x,
                b.bottom(),
                area.width,
                area.bottom().saturating_sub(b.bottom()),
            )
        });
        scanlines(buf, pal, shaded);
    }

    /// The visible-list index under a mouse position, if it is on a row.
    pub fn session_at(&self, app: &App, area: Rect, column: u16, row: u16) -> Option<usize> {
        let a = layout(area, app, self.pal.classic);
        let (y0, rows) = list_rows(&a);
        let s = a.sessions;
        if row < y0 || row >= y0 + rows || column < s.x || column >= s.right() {
            return None;
        }
        let idx = app.list_offset.get() + usize::from(row - y0);
        (idx < app.filtered_indices.len()).then_some(idx)
    }

    // -----------------------------------------------------------------------
    // Banner + status
    // -----------------------------------------------------------------------

    /// The logo: pixel letters two to a cell, lit along the neon sweep
    /// with a chrome highlight on top, over a violet extruded shadow.
    fn draw_banner(&self, buf: &mut Buffer, r: Rect, app: &App, fx: Fx) {
        let pal = &self.pal;
        let t = fx.time();
        let glitching = fx.glitching();
        let decrypt = (t / neon::INTRO).min(1.0);
        // Quantised drift (2 Hz) keeps redraws cheap over SSH.
        let phase = (t * 2.0).floor() / 2.0 * 0.04;
        let slot = (t * 20.0) as u64;
        let stops = pal.neon();
        let bits = &self.banner;
        let len = bits[0].len();
        let lit = |x: usize, y: usize| y < neon::FONT_H && x < len && bits[y][x];
        let shadow = lerp(pal.base(), pal.purple, 0.24);
        // Chrome: the top pixel rows run hot toward white.
        const SHINE: [f32; neon::FONT_H] = [0.42, 0.22, 0.06, 0.0, 0.0];
        let pixel = |x: usize, y: usize| -> Option<Rgb> {
            if lit(x, y) {
                let pos = x as f32 / len.max(1) as f32;
                let col = gradient(&stops, (pos + phase).fract());
                Some(lerp(col, pal.white, SHINE[y]))
            } else if x > 0 && y > 0 && lit(x - 1, y - 1) {
                Some(shadow)
            } else {
                None
            }
        };
        for row in 0..3u16 {
            let shift: i32 = if glitching {
                (hash3(slot, u64::from(row), 7) % 3) as i32 - 1
            } else {
                0
            };
            let (y_top, y_bot) = (2 * usize::from(row), 2 * usize::from(row) + 1);
            for i in 0..=len {
                let (top, bot) = (pixel(i, y_top), pixel(i, y_bot));
                if top.is_none() && bot.is_none() {
                    continue;
                }
                let x = i32::from(r.x) + 2 + i as i32 + shift;
                let Some(c) = u16::try_from(x)
                    .ok()
                    .filter(|&x| x < r.right())
                    .and_then(|x| buf.cell_mut((x, r.y + row)))
                else {
                    continue;
                };
                let pos = i as f32 / len.max(1) as f32;
                let h = hash3(slot, u64::from(row), i as u64);
                let noise = if decrypt < 1.0 && pos >= decrypt {
                    Some([pal.purple, pal.cyan, pal.faint][(h >> 8) as usize % 3])
                } else if glitching && h % 100 < 8 {
                    Some([pal.red, pal.cyan, pal.white][(h >> 8) as usize % 3])
                } else {
                    None
                };
                let mut sym = [0u8; 4];
                if let Some(col) = noise {
                    let ch = NOISE[(h % NOISE.len() as u64) as usize];
                    c.set_symbol(ch.encode_utf8(&mut sym))
                        .set_style(pal.bold(col));
                    continue;
                }
                // Both halves lit: the top colour on the glyph, the bottom
                // one behind it, so each pixel keeps its own shade.
                let (ch, fg) = match (top, bot) {
                    (Some(t), Some(b)) => {
                        c.set_bg(pal.c(b));
                        ('▀', t)
                    }
                    (Some(t), None) => ('▀', t),
                    (None, Some(b)) => ('▄', b),
                    (None, None) => continue,
                };
                c.set_symbol(ch.encode_utf8(&mut sym)).set_fg(pal.c(fg));
            }
        }

        let linked = app.sessions.iter().filter(|s| s.attached).count();
        let info = [
            (String::from("NETRUNNER ▸ "), self.who.clone()),
            (
                String::from("GRID ▸ "),
                format!("{} sessions · {linked} linked", app.sessions.len()),
            ),
            (format!("{} · T+", (self.clock)()), mmss(fx.t)),
        ];
        let banner_end = r.x + 4 + len as u16;
        let fits = |w: u16| r.right().checked_sub(w + 2).filter(|&x| x > banner_end);
        for (row, (label, value)) in (0u16..).zip(&info) {
            // Drop the label before the value when space runs short.
            let full = width(label) + width(value);
            let (label, x) = match (fits(full), fits(width(value))) {
                (Some(x), _) => (label.as_str(), x),
                (None, Some(x)) => ("", x),
                (None, None) => continue,
            };
            let x = put(buf, x, r.y + row, label, full, pal.fg(pal.dim));
            put(buf, x, r.y + row, value, full, pal.bold(pal.cyan));
        }
    }

    fn draw_status(&self, buf: &mut Buffer, r: Rect, app: &App, fx: Fx, compact: bool) {
        let pal = &self.pal;
        let w = r.width;
        let pulse = if fx.on && !pal.classic {
            (fx.t * 45.0) % (f32::from(w) + 30.0) - 15.0
        } else {
            -100.0
        };
        for i in 0..w {
            let d = (f32::from(i) - pulse).abs() / 10.0;
            cell(buf, r.x + i, r.y, '─', pal.fg(lerp(pal.cyan, pal.faint, d)));
        }
        if pal.classic {
            let (text, color) = classic_status_text(app, pal);
            let x = put(buf, r.x + 1, r.y, " tmux-picker ·", w, pal.bold(pal.pink));
            let room = r.right().saturating_sub(x + 2);
            put(
                buf,
                x,
                r.y,
                &format!(" {} ", ellipsize(&text, room.saturating_sub(2))),
                room,
                pal.bold(color),
            );
            return;
        }
        // Segments sit on the rule with a short run of line between them.
        let mut x = put(buf, r.x + 2, r.y, "◢◤", w, pal.bold(pal.pink));
        if compact {
            x = put(buf, x, r.y, " TMUX//PICKER ", w, pal.bold(pal.pink)) + 2;
        }
        for (i, (text, color)) in status_parts(app, pal, compact).iter().enumerate() {
            let room = r.right().saturating_sub(x + 2);
            if room < 4 {
                break;
            }
            let style = if i == 0 {
                pal.bold(*color)
            } else {
                pal.fg(*color)
            };
            x = put(
                buf,
                x,
                r.y,
                &format!(" {} ", ellipsize(text, room.saturating_sub(2))),
                room,
                style,
            ) + 2;
        }
    }

    // -----------------------------------------------------------------------
    // Session list
    // -----------------------------------------------------------------------

    fn draw_sessions(&self, buf: &mut Buffer, a: &Areas, app: &App, fx: Fx) {
        let pal = &self.pal;
        let r = a.sessions;
        if r.width == 0 || r.height == 0 {
            return;
        }
        let boxed = is_boxed(r);
        let total = app.sessions.len();
        let shown = app.filtered_indices.len();
        if boxed {
            let tag = if shown == total {
                format!("{total}")
            } else {
                format!("{shown}/{total}")
            };
            NeonBox {
                title: self.words().sessions,
                tag: Some(&tag),
                hot: matches!(app.mode, Mode::Pick | Mode::Filter),
                stops: None,
            }
            .render(buf, pal, r);
        }
        let (y0, rows) = list_rows(a);
        let (x0, cw) = if boxed {
            (r.x + 2, r.width.saturating_sub(4))
        } else {
            (r.x, r.width)
        };
        let cols = Columns::new(x0, cw);
        if a.header && boxed {
            cols.header(buf, pal, r.y + 1, self.words());
        }

        if shown == 0 {
            let body = Rect::new(r.x + 1, y0, r.width.saturating_sub(2), rows);
            if pal.classic {
                let msg = if app.filter.is_empty() {
                    String::from("no sessions")
                } else {
                    format!("nothing matches /{}", app.filter)
                };
                centered(buf, body, body.y + rows / 2, &msg, pal.fg(pal.dim));
                return;
            }
            rain(buf, pal, body, fx, 3);
            let msg = if app.filter.is_empty() {
                String::from("no sessions")
            } else {
                format!("nothing matches /{}", app.filter)
            };
            if rows >= 2 {
                centered(
                    buf,
                    body,
                    body.y + rows / 2 - 1,
                    " ◢◤ NO SIGNAL ◢◤ ",
                    pal.bold(pal.pink),
                );
                centered(
                    buf,
                    body,
                    body.y + rows / 2,
                    &format!(" {msg} "),
                    pal.fg(pal.dim),
                );
            }
            return;
        }

        let offset = sync_offset(app, usize::from(rows));
        for (row, (vis, &si)) in app
            .filtered_indices
            .iter()
            .enumerate()
            .skip(offset)
            .take(usize::from(rows))
            .enumerate()
        {
            let Some(session) = app.sessions.get(si) else {
                continue;
            };
            let y = y0 + row as u16;
            let selected = vis == app.selected;
            if selected {
                let band = Rect::new(
                    r.x + u16::from(boxed),
                    y,
                    r.width.saturating_sub(2 * u16::from(boxed)),
                    1,
                );
                self.selection_band(buf, band, boxed, fx);
            }
            cols.row(buf, pal, self.words(), (y, vis + 1), session, selected);
        }

        if boxed && shown > usize::from(rows) && rows > 0 {
            let rows_f = f32::from(rows);
            let thumb = (rows_f * rows_f / shown as f32).max(1.0).round() as u16;
            let span = usize::from(rows.saturating_sub(thumb));
            let pos = (offset * span)
                .checked_div(shown - usize::from(rows))
                .unwrap_or(0) as u16;
            for i in 0..thumb.min(rows) {
                cell(buf, r.right() - 1, y0 + pos + i, '┃', pal.bold(pal.cyan));
            }
        }
    }

    /// The selected row: a neon beam that burns hot at the left edge and
    /// fades to violet, with glowing caps in the box gutters (`caps`) and a
    /// slow light sweep.
    fn selection_band(&self, buf: &mut Buffer, band: Rect, caps: bool, fx: Fx) {
        let pal = &self.pal;
        if pal.classic {
            fill_bg(buf, band, pal.c(pal.hilite));
            return;
        }
        let sweep = fx
            .on
            .then(|| (fx.t * 32.0) % (f32::from(band.width) + 80.0) - 8.0);
        let span = f32::from(band.width.saturating_sub(1).max(1));
        for i in 0..band.width {
            let mut col = gradient(&pal.beam, f32::from(i) / span);
            if let Some(d) = sweep.map(|s| (f32::from(i) - s).abs())
                && d < 5.0
            {
                col = lerp(col, pal.pink, 0.45 * (1.0 - d / 5.0));
            }
            if let Some(c) = buf.cell_mut((band.x + i, band.y)) {
                c.set_symbol(" ").set_bg(pal.c(col));
            }
        }
        if caps && band.width >= 2 {
            cell(buf, band.x, band.y, '▌', pal.bold(pal.pink));
            cell(buf, band.right() - 1, band.y, '▐', pal.bold(pal.cyan));
        }
    }

    // -----------------------------------------------------------------------
    // Preview
    // -----------------------------------------------------------------------

    fn draw_preview(&self, buf: &mut Buffer, r: Rect, app: &App, fx: Fx) {
        let pal = &self.pal;
        let name = app.selected_name().unwrap_or("");
        let words = self.words();
        let (title, tag) = match app.preview_mode {
            PreviewMode::Summary => (
                format!("{}{}{name}", words.preview, words.to),
                words.preview_tag,
            ),
            PreviewMode::WindowsList => (
                format!("{}{}{name}", words.windows, words.to),
                words.windows_tag,
            ),
        };
        // The list holds focus; the feed keeps a cold frame under a live
        // title so it reads as secondary.
        let pane_box = NeonBox {
            title: &title,
            tag: Some(tag),
            hot: matches!(app.mode, Mode::Pick | Mode::Filter),
            stops: (!pal.classic).then_some((pal.faint, pal.purple)),
        };
        pane_box.render(buf, pal, r);
        if let Some(x) = pane_box.tag_x(r).filter(|_| !pal.classic) {
            let dot = if fx.blink(0.8) { pal.red } else { pal.faint };
            cell(buf, x + 2, r.y, '●', pal.bold(dot));
        }
        let inner = Rect::new(
            r.x + 2,
            r.y + 1,
            r.width.saturating_sub(4),
            r.height.saturating_sub(2),
        );
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let mut y = inner.y;
        let mut rows = inner.height;
        // "↳ ~/git/app  ·  PR #234" for sessions with a project or purpose.
        if let Some(m) = app
            .selected_session()
            .and_then(|s| s.metadata.as_ref())
            .filter(|m| m.project.is_some() || m.purpose.is_some())
        {
            let end = inner.right();
            let mut x = put(buf, inner.x, y, "↳ ", inner.width, pal.bold(pal.pink));
            if let Some(ref p) = m.project {
                let p = collapse_home(p);
                x = put(buf, x, y, &p, end.saturating_sub(x), pal.fg(pal.cyan));
            }
            if let Some(ref pu) = m.purpose {
                if m.project.is_some() {
                    x = put(buf, x, y, "  ·  ", end.saturating_sub(x), pal.fg(pal.faint));
                }
                put(buf, x, y, pu, end.saturating_sub(x), pal.bold(pal.yellow));
            }
            y += 1;
            rows -= 1;
        }
        let body = Rect::new(inner.x, y, inner.width, rows);
        match app.preview_mode {
            PreviewMode::Summary => match app.preview.as_deref() {
                Some(text) if !text.trim().is_empty() => {
                    let lines: Vec<&str> = text.lines().collect();
                    let shown = &lines[lines.len().saturating_sub(usize::from(rows))..];
                    let span = f32::from(rows.max(1));
                    for (i, line) in (0u16..).zip(shown) {
                        let age = (shown.len() as u16 - 1 - i) as f32 / span;
                        let col = lerp(pal.text, pal.faint, age * 0.9);
                        let yy = body.y + i;
                        let prompt = (!pal.classic).then(|| split_prompt(line)).flatten();
                        let Some((head, sigil, cmd)) = prompt else {
                            put(buf, body.x, yy, line, body.width, pal.fg(col));
                            continue;
                        };
                        // Shell prompts: host in violet, the sigil lit,
                        // the command bright, all fading with age.
                        let end = body.right();
                        let fade = |c: Rgb| lerp(c, pal.faint, age * 0.7);
                        let mut x =
                            put(buf, body.x, yy, head, body.width, pal.fg(fade(pal.purple)));
                        x = put(
                            buf,
                            x,
                            yy,
                            sigil,
                            end.saturating_sub(x),
                            pal.bold(fade(pal.pink)),
                        );
                        put(
                            buf,
                            x,
                            yy,
                            cmd,
                            end.saturating_sub(x),
                            pal.fg(fade(pal.white)),
                        );
                    }
                }
                Some(_) => centered(buf, body, body.y + rows / 2, words.blank, pal.fg(pal.dim)),
                None => self.no_signal(buf, body, "capture unavailable", fx),
            },
            PreviewMode::WindowsList => match app.preview_windows.as_deref() {
                Some(snaps) if !snaps.is_empty() => {
                    for (i, snap) in (0u16..).zip(snaps.iter().take(usize::from(rows))) {
                        let yy = body.y + i;
                        let mut x = body.x;
                        let mark = if snap.active { "▸ " } else { "  " };
                        x = put(buf, x, yy, mark, body.width, pal.bold(pal.pink));
                        let idx = snap
                            .index
                            .map_or_else(|| String::from("  "), |n| format!("{n:>2}"));
                        x = put(buf, x, yy, &idx, 2, pal.bold(pal.cyan));
                        x = put(
                            buf,
                            x + 1,
                            yy,
                            &format!("{:<16}", ellipsize(&snap.name, 16)),
                            16,
                            pal.bold(pal.white),
                        );
                        x = put(buf, x, yy, " › ", 3, pal.fg(pal.faint));
                        put(
                            buf,
                            x,
                            yy,
                            &snap.last_line,
                            body.right().saturating_sub(x),
                            pal.fg(pal.dim),
                        );
                    }
                }
                Some(_) => centered(
                    buf,
                    body,
                    body.y + rows / 2,
                    "(no windows)",
                    pal.fg(pal.dim),
                ),
                None => self.no_signal(buf, body, "window list unavailable", fx),
            },
        }
    }

    fn no_signal(&self, buf: &mut Buffer, body: Rect, why: &str, fx: Fx) {
        let pal = &self.pal;
        if !pal.classic {
            rain(buf, pal, body, fx, 11);
        }
        if body.height >= 2 {
            let mid = body.y + body.height / 2;
            let headline = self.words().no_signal;
            centered(buf, body, mid - 1, headline, pal.bold(pal.pink));
            centered(buf, body, mid, &format!(" {why} "), pal.fg(pal.dim));
        }
    }

    // -----------------------------------------------------------------------
    // Gauge (countdown / prompts)
    // -----------------------------------------------------------------------

    fn draw_gauge(&self, buf: &mut Buffer, r: Rect, app: &App, fx: Fx) {
        let pal = &self.pal;
        let words = self.words();
        let rename_title;
        let (title, hot, stops) = match app.mode {
            Mode::Pick | Mode::Help if app.timeout_secs == 0 => (words.manual, false, None),
            Mode::Pick | Mode::Help => (words.auto, app.mode == Mode::Pick, None),
            Mode::Filter => (words.filter, true, None),
            Mode::NewInput => (words.new_session, true, None),
            Mode::Rename => {
                rename_title = format!(
                    "{}{}{}",
                    words.rename,
                    words.to,
                    app.rename_target.as_deref().unwrap_or("")
                );
                (rename_title.as_str(), true, None)
            }
            Mode::ConfirmKill => (words.kill, true, Some((pal.red, pal.pink))),
        };
        let (mut x, y, mut w) = if r.height >= 3 {
            NeonBox {
                title,
                tag: None,
                hot,
                stops,
            }
            .render(buf, pal, r);
            (r.x + 2, r.y + 1, r.width.saturating_sub(4))
        } else {
            (r.x + 1, r.y, r.width.saturating_sub(2))
        };
        if r.height < 3 {
            let lead = if pal.classic {
                format!("{title}: ")
            } else {
                format!("◢ {title} ▸ ")
            };
            let end = put(buf, x, y, &lead, w, pal.bold(pal.pink));
            w = w.saturating_sub(end - x);
            x = end;
        }
        match app.mode {
            Mode::Pick | Mode::Help => self.countdown(buf, x, y, w, app, fx),
            Mode::Filter => {
                let right = format!(
                    "{}/{} {}",
                    app.filtered_indices.len(),
                    app.sessions.len(),
                    words.matches
                );
                self.prompt(buf, (x, y, w), "/", &app.filter, Some(&right), None, fx);
            }
            Mode::NewInput => {
                self.prompt(
                    buf,
                    (x, y, w),
                    &format!("name{}", words.to),
                    &app.input,
                    None,
                    app.input_error.as_deref(),
                    fx,
                );
            }
            Mode::Rename => {
                self.prompt(
                    buf,
                    (x, y, w),
                    &format!("new name{}", words.to),
                    &app.input,
                    None,
                    app.input_error.as_deref(),
                    fx,
                );
            }
            Mode::ConfirmKill => {
                let target = app.kill_target.as_deref().unwrap_or("?");
                let x = put(buf, x, y, "✗ kill ", w, pal.bold(pal.red));
                let x = put(buf, x, y, target, w, pal.bold(pal.white));
                put(buf, x, y, words.kill_hint, w, pal.fg(pal.dim));
            }
        }
    }

    fn countdown(&self, buf: &mut Buffer, x: u16, y: u16, w: u16, app: &App, fx: Fx) {
        let pal = &self.pal;
        if app.timeout_secs == 0 {
            let info = self.words().auto_off;
            let bar_w = w.saturating_sub(width(info) + 2).max(4).min(w);
            bar(buf, pal, x, y, bar_w, None, fx);
            put(
                buf,
                x + bar_w + 2,
                y,
                info,
                w.saturating_sub(bar_w + 2),
                pal.fg(pal.dim),
            );
            return;
        }
        let total = app.timeout_secs as f32;
        let remaining = app.timeout_remaining.as_secs_f32();
        let target = app.auto_target().map_or("shell", |s| s.name.as_str());
        let urgent = remaining <= 3.0 && app.mode == Mode::Pick;
        let secs = format!("{remaining:04.1}s");
        let info_w = (width(&secs) + 3 + width(target)).min(w / 2);
        let bar_w = w.saturating_sub(info_w + 2).min(w);
        bar(buf, pal, x, y, bar_w, Some(remaining / total), fx);
        let mut ix = x + bar_w + 2;
        let room = (x + w).saturating_sub(ix);
        let secs_col = if urgent && !fx.blink(2.5) {
            pal.white
        } else if urgent {
            pal.yellow
        } else {
            pal.cyan
        };
        ix = put(buf, ix, y, &secs, room, pal.bold(secs_col));
        ix = put(
            buf,
            ix,
            y,
            self.words().arrow,
            (x + w).saturating_sub(ix),
            pal.fg(pal.pink),
        );
        put(
            buf,
            ix,
            y,
            &ellipsize(target, (x + w).saturating_sub(ix)),
            (x + w).saturating_sub(ix),
            pal.bold(pal.white),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn prompt(
        &self,
        buf: &mut Buffer,
        (x, y, w): (u16, u16, u16),
        label: &str,
        value: &str,
        right: Option<&str>,
        error: Option<&str>,
        fx: Fx,
    ) {
        let pal = &self.pal;
        let end = x + w;
        let right_w = right.map_or(0, |r| width(r) + 2);
        let mut cx = put(buf, x, y, label, w, pal.bold(pal.pink));
        // Show the tail of long input so the cursor stays visible.
        let room = end.saturating_sub(cx + 1 + right_w);
        let shown = tail_fit(value, room);
        cx = put(buf, cx, y, shown, room, pal.bold(pal.white));
        if fx.blink(1.4) {
            cell(buf, cx, y, '█', pal.fg(pal.cyan));
        }
        if let Some(err) = error {
            put(
                buf,
                cx + 2,
                y,
                &format!("✗ {err}"),
                end.saturating_sub(cx + 2),
                pal.bold(pal.red),
            );
        }
        if let Some(right) = right {
            let rx = end.saturating_sub(width(right));
            if rx > cx + 2 {
                put(buf, rx, y, right, width(right), pal.fg(pal.dim));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Footer
    // -----------------------------------------------------------------------

    fn draw_footer(&self, buf: &mut Buffer, r: Rect, app: &App) {
        let pal = &self.pal;
        if !pal.classic {
            self.draw_chips(buf, r, app);
            return;
        }
        let mut x = r.x + 1;
        let right = format!("tmux-picker v{VERSION} ");
        let right_w = width(&right);
        let limit = r.right().saturating_sub(right_w + 1);
        for (i, (key, label)) in hints(app, pal.classic).iter().enumerate() {
            let need = width(key) + 1 + width(label) + if i > 0 { 3 } else { 0 };
            if x + need > limit {
                break;
            }
            if i > 0 {
                x = put(buf, x, r.y, self.words().sep, 3, pal.fg(pal.dim));
            }
            x = put(buf, x, r.y, key, need, pal.bold(pal.white));
            x = put(buf, x + 1, r.y, label, need, pal.fg(pal.text));
        }
        if r.right() > x + right_w + 1 {
            put(
                buf,
                r.right() - right_w,
                r.y,
                &right,
                right_w,
                pal.fg(pal.dim),
            );
        }
    }

    /// Neon footer: each key on a violet keycap, its action in dim text.
    fn draw_chips(&self, buf: &mut Buffer, r: Rect, app: &App) {
        let pal = &self.pal;
        let cap = pal
            .bold(pal.white)
            .bg(pal.c(lerp(pal.base(), pal.purple, 0.4)));
        let right = format!("tmux-picker v{VERSION} ");
        let right_w = width(&right);
        let limit = r.right().saturating_sub(right_w + 1);
        let mut x = r.x + 1;
        for (i, (key, label)) in hints(app, false).iter().enumerate() {
            let gap = if i > 0 { 2 } else { 0 };
            let need = gap + width(key) + 3 + width(label);
            if x + need > limit {
                break;
            }
            x = put(buf, x + gap, r.y, &format!(" {key} "), need, cap);
            x = put(buf, x + 1, r.y, label, need, pal.fg(pal.dim));
        }
        if r.right() > x + right_w + 1 {
            put(
                buf,
                r.right() - right_w,
                r.y,
                &right,
                right_w,
                pal.fg(pal.faint),
            );
        }
    }

    // -----------------------------------------------------------------------
    // Overlays
    // -----------------------------------------------------------------------

    /// A modal card over a dimmed frame: a gradient frame with the title
    /// flickering in its top edge. Returns the card's rectangle.
    fn card(
        &self,
        buf: &mut Buffer,
        area: Rect,
        size: (u16, u16),
        edge: Rgb,
        title: &str,
        fx: Fx,
    ) -> Rect {
        let pal = &self.pal;
        if !pal.classic {
            scrim(buf, pal, area, 0.7);
        }
        let w = size.0.min(area.width.saturating_sub(2));
        let h = size.1.min(area.height);
        let card = Rect::new(
            area.x + (area.width - w) / 2,
            area.y + (area.height - h) / 2,
            w,
            h,
        );
        let bg = pal.c(pal.overlay);
        buf.set_style(card, Style::new().bg(bg).fg(pal.c(pal.text)));
        fill_bg(buf, card, bg);
        if w < 4 || h < 3 {
            return card;
        }
        if pal.classic {
            let stops = (edge != pal.pink).then_some((edge, edge));
            NeonBox {
                title,
                tag: None,
                hot: true,
                stops,
            }
            .render(buf, pal, card);
            return card;
        }
        NeonBox {
            title: "",
            tag: None,
            hot: true,
            stops: Some((edge, pal.cyan)),
        }
        .render(buf, pal, card);
        let flicker = if fx.on && ((fx.t * 6.0) as u64).is_multiple_of(13) {
            pal.white
        } else {
            edge
        };
        centered(
            buf,
            card,
            card.y,
            &format!(" ◢◤ {title} ◢◤ "),
            pal.bold(flicker),
        );
        card
    }

    fn draw_help(&self, buf: &mut Buffer, area: Rect, fx: Fx) {
        let pal = &self.pal;
        let sections = help_sections();
        let two_col = area.width >= 80;
        let col_h =
            |secs: &[HelpSection]| secs.iter().map(|(_, rows)| rows.len() + 2).sum::<usize>();
        let (left, right) = sections.split_at(1);
        let body_h = if two_col {
            col_h(left).max(col_h(right))
        } else {
            col_h(&sections)
        };
        let size = (if two_col { 78 } else { 40 }, body_h as u16 + 4);
        let card = self.card(buf, area, size, pal.pink, self.words().help_title, fx);
        let draw_col = |buf: &mut Buffer, secs: &[HelpSection], x: u16, w: u16| {
            let mut y = card.y + 2;
            for (title, rows) in secs {
                if y + 1 >= card.bottom() {
                    break;
                }
                put(buf, x, y, title, w, pal.bold(pal.cyan));
                y += 1;
                for (keys, desc) in rows.iter() {
                    if y + 1 >= card.bottom() {
                        break;
                    }
                    let kx = put(buf, x + 1, y, keys, 10, pal.bold(pal.pink));
                    let dx = (x + 12).max(kx + 1);
                    put(
                        buf,
                        dx,
                        y,
                        desc,
                        (x + w).saturating_sub(dx),
                        pal.fg(pal.text),
                    );
                    y += 1;
                }
                y += 1;
            }
        };
        if two_col {
            let half = card.width.saturating_sub(6) / 2;
            draw_col(buf, left, card.x + 3, half);
            draw_col(buf, right, card.x + 3 + half + 2, half);
        } else {
            draw_col(buf, &sections, card.x + 2, card.width.saturating_sub(4));
        }
        if fx.blink(1.0) && card.height >= 3 {
            centered(
                buf,
                card,
                card.bottom() - 2,
                self.words().help_close,
                pal.bold(pal.cyan),
            );
        }
    }

    fn draw_kill(&self, buf: &mut Buffer, area: Rect, app: &App, fx: Fx) {
        let pal = &self.pal;
        let card = self.card(buf, area, (58, 7), pal.red, self.words().kill_title, fx);
        let target = app.kill_target.as_deref().unwrap_or("?");
        centered(
            buf,
            card,
            card.y + 2,
            &ellipsize(target, card.width.saturating_sub(4)),
            pal.bold(pal.white),
        );
        centered(
            buf,
            card,
            card.y + 3,
            "every pane and process in it ends",
            pal.fg(pal.dim),
        );
        if card.height >= 7 && fx.blink(1.2) {
            centered(
                buf,
                card,
                card.bottom() - 2,
                self.words().kill_confirm,
                pal.bold(pal.yellow),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
struct Areas {
    banner: Option<Rect>,
    status: Option<Rect>,
    sessions: Rect,
    header: bool,
    preview: Option<Rect>,
    /// Height 3: a boxed gauge; height 1: an inline one.
    gauge: Option<Rect>,
    footer: Option<Rect>,
}

/// Responsive layout. Wide terminals put the live feed beside the list;
/// short ones drop the banner, then the boxed gauge, then the preview.
fn layout(area: Rect, app: &App, classic: bool) -> Areas {
    let (w, h) = (area.width, area.height);
    let mut a = Areas::default();
    let mut top = area.y;
    let mut bottom = area.bottom();
    if h >= 3 {
        bottom -= 1;
        a.footer = Some(Rect::new(area.x, bottom, w, 1));
    }
    if !classic && h >= 22 && w >= 64 {
        a.banner = Some(Rect::new(area.x, top, w, 3));
        top += 3;
    }
    if h >= 5 {
        a.status = Some(Rect::new(area.x, top, w, 1));
        top += 1;
    }
    let gauge_h = if h >= 14 {
        3
    } else if h >= 7 {
        1
    } else {
        0
    };
    if gauge_h > 0 {
        bottom -= gauge_h;
        a.gauge = Some(Rect::new(area.x, bottom, w, gauge_h));
    }
    let rest = bottom.saturating_sub(top);
    a.header = rest >= 12 && w >= 50;
    let has_selection = app.selected_name().is_some();
    let needed = u16::try_from(app.filtered_indices.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2 + u16::from(a.header));
    if has_selection && w >= 140 && rest >= 8 {
        let left = u16::try_from(u32::from(w) * 11 / 20).unwrap_or(w).max(72);
        a.sessions = Rect::new(area.x, top, left, rest);
        a.preview = Some(Rect::new(area.x + left, top, w - left, rest));
    } else if has_selection && rest >= 11 {
        let preview_h = rest.saturating_sub(needed).clamp(6, 16).min(rest - 5);
        a.sessions = Rect::new(area.x, top, w, rest - preview_h);
        a.preview = Some(Rect::new(area.x, top + rest - preview_h, w, preview_h));
    } else {
        a.sessions = Rect::new(area.x, top, w, rest);
    }
    a
}

fn is_boxed(r: Rect) -> bool {
    r.height >= 3 && r.width >= 10
}

/// First screen row of the session list and how many rows it has.
fn list_rows(a: &Areas) -> (u16, u16) {
    let r = a.sessions;
    if is_boxed(r) {
        let header = u16::from(a.header);
        (r.y + 1 + header, r.height.saturating_sub(2 + header))
    } else {
        (r.y, r.height)
    }
}

/// Scroll the list just enough to keep the selection visible.
fn sync_offset(app: &App, rows: usize) -> usize {
    let len = app.filtered_indices.len();
    let mut off = app.list_offset.get();
    if rows == 0 {
        return off;
    }
    if app.selected < off {
        off = app.selected;
    } else if app.selected >= off + rows {
        off = app.selected + 1 - rows;
    }
    off = off.min(len.saturating_sub(rows));
    app.list_offset.set(off);
    off
}

/// Column plan for one list width; narrower lists shed columns right to
/// left (link text, process, windows, activity).
struct Columns {
    x: u16,
    name_w: u16,
    win: bool,
    cmd: bool,
    link_text: bool,
    act: bool,
}

const COL_WIN: u16 = 6;
const COL_CMD: u16 = 11;
const COL_ACT: u16 = 12;

impl Columns {
    fn new(x: u16, cw: u16) -> Self {
        let mut c = Columns {
            x,
            name_w: 0,
            win: cw >= 46,
            cmd: cw >= 58,
            link_text: cw >= 70,
            act: cw >= 30,
        };
        let fixed = 5
            + if c.win { COL_WIN + 1 } else { 0 }
            + if c.cmd { COL_CMD + 1 } else { 0 }
            + if c.link_text { 9 } else { 2 }
            + if c.act { COL_ACT } else { 0 };
        c.name_w = cw.saturating_sub(fixed);
        c
    }

    fn win_x(&self) -> u16 {
        self.x + 5 + self.name_w + 1
    }

    fn cmd_x(&self) -> u16 {
        self.win_x() + if self.win { COL_WIN + 1 } else { 0 }
    }

    fn link_x(&self) -> u16 {
        self.cmd_x() + if self.cmd { COL_CMD + 1 } else { 0 }
    }

    fn act_x(&self) -> u16 {
        self.link_x() + if self.link_text { 9 } else { 2 }
    }

    fn header(&self, buf: &mut Buffer, pal: &Palette, y: u16, words: &Words) {
        let style = pal.bold(pal.dim);
        let hash = if pal.classic { "##" } else { " #" };
        put(buf, self.x + 2, y, hash, 2, style);
        put(
            buf,
            self.x + 8,
            y,
            "SESSION",
            self.name_w.saturating_sub(3),
            style,
        );
        if self.win {
            put(buf, self.win_x() + COL_WIN - 3, y, "WIN", 3, style);
        }
        if self.cmd {
            put(buf, self.cmd_x(), y, "PROCESS", COL_CMD, style);
        }
        if self.link_text {
            put(buf, self.link_x(), y, words.link_head, 8, style);
        }
        if self.act {
            put(buf, self.act_x(), y, words.act_head, COL_ACT, style);
        }
    }

    fn row(
        &self,
        buf: &mut Buffer,
        pal: &Palette,
        words: &Words,
        (y, number): (u16, usize),
        s: &Session,
        selected: bool,
    ) {
        let stale = s.is_stale();
        // Muted colours would sink into the selection band; lift them.
        let lifted = if pal.classic { pal.white } else { pal.text };
        let muted = |c: Rgb| if selected { lifted } else { c };
        // Selector + 1-indexed number within the visible list, so digit
        // selection lines up with what the user sees. The neon beam marks
        // the row by itself.
        if selected && pal.classic {
            put(buf, self.x, y, "▶", 1, pal.bold(pal.cyan));
        }
        let num_style = if selected {
            pal.bold(pal.white)
        } else {
            pal.fg(pal.dim)
        };
        put(buf, self.x + 2, y, &format!("{number:>2}"), 3, num_style);

        // Marker slot (3 columns) + label/name.
        let nx = self.x + 5;
        if let Some(glyph) = s.marker.as_deref() {
            put(buf, nx, y, glyph, 2, Style::new());
        }
        let name_style = if stale {
            pal.fg(muted(pal.dim))
        } else if s.is_claude() {
            pal.bold(pal.cyan)
        } else {
            pal.bold(pal.white)
        };
        let room = self.name_w.saturating_sub(3);
        let label = s.metadata.as_ref().and_then(|m| m.label.as_deref());
        match label {
            Some(label) => {
                let end = put(buf, nx + 3, y, &ellipsize(label, room), room, name_style);
                let left = (nx + 3 + room).saturating_sub(end);
                if left > 3 {
                    let tail = format!(" ({})", s.name);
                    put(
                        buf,
                        end,
                        y,
                        &ellipsize(&tail, left),
                        left,
                        pal.fg(muted(pal.dim)),
                    );
                }
            }
            None => {
                put(buf, nx + 3, y, &ellipsize(&s.name, room), room, name_style);
            }
        }

        if self.win {
            let win = if pal.classic {
                s.windows_display()
            } else {
                s.window_count.to_string()
            };
            let w = width(&win).min(COL_WIN);
            put(
                buf,
                self.win_x() + COL_WIN - w,
                y,
                &win,
                COL_WIN,
                pal.fg(muted(pal.dim)),
            );
        }
        if self.cmd {
            let col = if stale { muted(pal.dim) } else { pal.yellow };
            put(
                buf,
                self.cmd_x(),
                y,
                &ellipsize(&s.current_command, COL_CMD),
                COL_CMD,
                pal.fg(col),
            );
        }
        let (glyph, text) = if s.attached {
            words.linked
        } else {
            (words.unlinked, "")
        };
        let link_col = if s.attached {
            pal.green
        } else {
            muted(pal.faint)
        };
        let lx = put(buf, self.link_x(), y, glyph, 1, pal.bold(link_col));
        if self.link_text {
            put(buf, lx, y, text, 7, pal.bold(link_col));
        }
        if self.act && !pal.classic {
            signal(buf, pal, (self.act_x(), y), s, selected);
        } else if self.act {
            let secs = s.last_activity.as_secs();
            let dot = if stale {
                None
            } else if secs < 60 {
                Some(pal.green)
            } else if secs < 300 {
                Some(pal.yellow)
            } else {
                Some(muted(pal.dim))
            };
            let ax = self.act_x();
            if let Some(col) = dot {
                put(buf, ax, y, "●", 1, pal.fg(col));
            }
            let text_col = if selected {
                pal.white
            } else if stale {
                pal.faint
            } else {
                pal.text
            };
            put(
                buf,
                ax + 2,
                y,
                &s.activity_display(),
                COL_ACT - 2,
                pal.fg(text_col),
            );
        }
    }
}

/// Recency as a phone-style signal meter and a compact age ("now",
/// "40s", "7m", "2h", "3d"). Four braille bars, two to a cell, fill and
/// glow green while the session is busy and drain through yellow and
/// violet as it idles.
fn signal(buf: &mut Buffer, pal: &Palette, (x, y): (u16, u16), s: &Session, selected: bool) {
    // Bars of height 1-4 dots: odd bars in a cell's left dot column,
    // even ones in its right.
    const METER: [[char; 2]; 5] = [['⡀', ' '], ['⡀', ' '], ['⣠', ' '], ['⣠', '⡆'], ['⣠', '⣾']];
    let secs = s.last_activity.as_secs();
    let (lit, on) = match secs {
        0..60 => (4, pal.green),
        60..300 => (3, pal.yellow),
        300..3600 => (2, pal.purple),
        3600..86400 => (1, pal.dim),
        _ => (0, pal.faint),
    };
    let bar_col = if selected && lit < 2 { pal.text } else { on };
    for (i, ch) in (0u16..).zip(METER[lit]) {
        cell(buf, x + i, y, ch, pal.bold(bar_col));
    }
    let age = match secs {
        0..5 => String::from("now"),
        5..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
    };
    let text_col = if selected {
        pal.white
    } else if s.is_stale() {
        pal.faint
    } else if secs < 300 {
        pal.text
    } else {
        pal.dim
    };
    put(
        buf,
        x + 3,
        y,
        &format!("{age:>3}"),
        COL_ACT - 3,
        pal.fg(text_col),
    );
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

/// Status rule segments for the neon look: the mode first, then what
/// shapes the list (sort, filter). Counts only appear when the banner,
/// which already shows them, is gone.
fn status_parts(app: &App, pal: &Palette, compact: bool) -> Vec<(String, Rgb)> {
    if let Some(ref flash) = app.flash {
        return vec![(format!("◆ {flash}"), pal.cyan)];
    }
    let n = app.sessions.len();
    match app.mode {
        Mode::Pick => {
            let mut parts = vec![(String::from("SELECT TARGET"), pal.yellow)];
            if compact {
                let linked = app.sessions.iter().filter(|s| s.attached).count();
                parts.push((format!("{n} SESSIONS · {linked} LINKED"), pal.dim));
            }
            if app.sort_mode != SortMode::Default {
                parts.push((format!("SORT ▸ {}", app.sort_mode.label()), pal.cyan));
            }
            if !app.filter.is_empty() {
                parts.push((format!("FILTER ▸ /{}", app.filter), pal.cyan));
            }
            parts
        }
        Mode::Filter => vec![
            (String::from("FILTERING"), pal.yellow),
            (
                format!("{}/{n} MATCH", app.filtered_indices.len()),
                pal.cyan,
            ),
        ],
        Mode::NewInput => vec![
            (String::from("NEW SESSION"), pal.yellow),
            (String::from("NAME THE TARGET"), pal.dim),
        ],
        Mode::Rename => vec![
            (String::from("RENAME"), pal.yellow),
            (app.rename_target.clone().unwrap_or_default(), pal.cyan),
        ],
        Mode::ConfirmKill => vec![
            (String::from("TERMINATE"), pal.red),
            (
                app.kill_target.clone().unwrap_or_else(|| String::from("?")),
                pal.white,
            ),
        ],
        Mode::Help => vec![(String::from("COMMAND REFERENCE"), pal.yellow)],
    }
}

/// Status line for the classic look: plain words, `·` separators.
fn classic_status_text(app: &App, pal: &Palette) -> (String, Rgb) {
    if let Some(ref flash) = app.flash {
        return (flash.clone(), pal.cyan);
    }
    let n = app.sessions.len();
    match app.mode {
        Mode::Pick => {
            let linked = app.sessions.iter().filter(|s| s.attached).count();
            let mut text = format!("{n} sessions · {linked} attached");
            if app.sort_mode != SortMode::Default {
                text.push_str(&format!(" · sort: {}", app.sort_mode.label()));
            }
            if !app.filter.is_empty() {
                text.push_str(&format!(" · filter: /{}", app.filter));
            }
            (text, pal.text)
        }
        Mode::Filter => (
            format!("filter · {}/{n} match", app.filtered_indices.len()),
            pal.yellow,
        ),
        Mode::NewInput => (String::from("new session · enter a name"), pal.yellow),
        Mode::Rename => (
            format!("rename · {}", app.rename_target.as_deref().unwrap_or("")),
            pal.yellow,
        ),
        Mode::ConfirmKill => (
            format!("kill · {}", app.kill_target.as_deref().unwrap_or("?")),
            pal.red,
        ),
        Mode::Help => (String::from("help"), pal.yellow),
    }
}

fn hints(app: &App, classic: bool) -> &'static [(&'static str, &'static str)] {
    match app.mode {
        Mode::Pick if classic => &[
            ("⏎", "attach"),
            ("?", "help"),
            ("/", "filter"),
            ("n", "new"),
            ("K", "kill"),
            ("r", "rename"),
            ("o", "sort"),
            ("y", "yank"),
            ("tab", "preview"),
            ("q", "shell"),
        ],
        Mode::Pick => &[
            ("⏎", "attach"),
            ("?", "help"),
            ("/", "filter"),
            ("n", "new"),
            ("K", "kill"),
            ("r", "rename"),
            ("o", "sort"),
            ("y", "yank"),
            ("⇥", "feed"),
            ("q", "shell"),
        ],
        Mode::Filter => &[
            ("type", "narrow"),
            ("↑↓", "move"),
            ("⏎", "attach"),
            ("esc", "clear"),
        ],
        Mode::NewInput | Mode::Rename => &[("⏎", "confirm"), ("esc", "cancel")],
        Mode::ConfirmKill => &[("y", "kill"), ("any", "abort")],
        Mode::Help => &[("esc ? q", "close")],
    }
}

type HelpSection = (&'static str, &'static [(&'static str, &'static str)]);

fn help_sections() -> [HelpSection; 4] {
    [
        (
            "PICK MODE",
            &[
                ("↑ ↓ / j k", "move selection"),
                ("1-9", "jump to row N"),
                ("enter", "attach to session"),
                ("n", "new session"),
                ("/", "filter sessions"),
                ("K", "kill session"),
                ("r", "rename session"),
                ("o", "cycle sort modes"),
                ("y", "yank session name"),
                ("Tab", "feed / windows view"),
                ("click", "select, twice: attach"),
                ("wheel", "scroll the list"),
                ("s q esc", "drop to shell"),
                ("?", "show this help"),
            ],
        ),
        (
            "FILTER MODE",
            &[
                ("type", "narrow the list"),
                ("backspace", "widen the list"),
                ("↑ ↓", "move through matches"),
                ("enter", "attach to match"),
                ("esc", "clear and exit"),
            ],
        ),
        (
            "NEW SESSION",
            &[
                ("type", "name the session"),
                ("enter", "create or attach"),
                ("esc", "cancel"),
            ],
        ),
        (
            "KILL CONFIRM",
            &[("y / Y", "kill the session"), ("any other", "cancel")],
        ),
    ]
}

/// Every help entry as "<keys>  <description>" lines, with section
/// headers. Used to size the overlay and by tests.
pub fn help_overlay_lines() -> Vec<String> {
    let mut out = Vec::new();
    for (title, rows) in help_sections() {
        out.push(title.to_string());
        out.extend(rows.iter().map(|(k, d)| format!("  {k:<10} {d}")));
    }
    out
}

fn collapse_home(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME")
        && let Some(rest) = path.strip_prefix(&home)
    {
        return format!("~{rest}");
    }
    path.to_string()
}

/// Split a captured line at a shell prompt into (prompt, sigil, command):
/// `user@host:~$ make` gives ("user@host:~", "$", " make"). The prompt
/// before the sigil must be one short word, so `$` and `#` inside
/// ordinary output don't count; `#` needs a word before it (root
/// prompts) so comment lines don't either.
fn split_prompt(line: &str) -> Option<(&str, &str, &str)> {
    const SIGILS: [char; 6] = ['$', '#', '%', '❯', '➜', 'λ'];
    let lead = line.len() - line.trim_start().len();
    let (i, sigil) = line[lead..]
        .char_indices()
        .take(48)
        .find(|&(_, c)| SIGILS.contains(&c))?;
    let at = lead + i;
    let head = &line[lead..at];
    let numeric = !head.is_empty() && head.chars().all(|c| c.is_ascii_digit() || c == '.');
    if head.contains(char::is_whitespace)
        || (sigil == '#' && head.is_empty())
        || (sigil == '%' && numeric)
    {
        return None;
    }
    let rest = &line[at + sigil.len_utf8()..];
    if !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    Some((&line[..at], &line[at..at + sigil.len_utf8()], rest))
}

/// The longest suffix of `text` that fits in `max` columns.
fn tail_fit(text: &str, max: u16) -> &str {
    if width(text) <= max {
        return text;
    }
    let mut start = text.len();
    let mut used = 0u16;
    for (i, c) in text.char_indices().rev() {
        let w = u16::try_from(unicode_width::UnicodeWidthChar::width(c).unwrap_or(0)).unwrap_or(0);
        if used + w > max {
            break;
        }
        used += w;
        start = i;
    }
    &text[start..]
}

fn centered(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style) {
    let w = width(text).min(area.width);
    let x = area.x + (area.width - w) / 2;
    put(buf, x, y, text, w, style);
}

fn mmss(t: f32) -> String {
    let secs = t.max(0.0) as u64;
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// Local wall-clock time as HH:MM:SS.
fn wall_clock() -> String {
    // SAFETY: localtime_r writes only into the provided struct.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::from("--:--:--");
        }
        format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
    }
}

/// "user@host" for the banner.
fn whoami() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| String::from("operator"));
    let mut name = [0u8; 256];
    // SAFETY: gethostname writes at most `len` bytes into the buffer.
    let ok = unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len()) } == 0;
    let host = if ok {
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        String::from_utf8_lossy(&name[..end]).into_owned()
    } else {
        String::from("localhost")
    };
    let short = host.split('.').next().unwrap_or(&host);
    format!("{user}@{short}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::metadata::Metadata;
    use ratatui::style::Color;

    fn make_session(name: &str, label: Option<&str>) -> Session {
        Session {
            name: name.into(),
            window_count: 1,
            attached: false,
            current_command: "bash".into(),
            last_activity: Duration::from_secs(0),
            metadata: label.map(|l| Metadata {
                label: Some(l.into()),
                ..Default::default()
            }),
            marker: None,
        }
    }

    fn frozen(theme: &Theme, truecolor: bool) -> Ui {
        let mut ui = Ui::with_palette(Palette::with_truecolor(theme, truecolor));
        ui.clock = || String::from("12:00:00");
        ui.who = String::from("op@grid");
        ui
    }

    fn ui() -> Ui {
        frozen(&Theme::default(), true)
    }

    fn render(ui: &Ui, app: &App, w: u16, h: u16, t: f32) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        ui.render(area, &mut buf, app, t);
        buf
    }

    fn dump(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn sessions(n: usize) -> Vec<Session> {
        (0..n)
            .map(|i| make_session(&format!("sess-{i:02}"), None))
            .collect()
    }

    fn detail_text(metadata: Metadata) -> String {
        let s = Session {
            metadata: Some(metadata),
            ..make_session("main", None)
        };
        let app = App::new(vec![s], &Config::default());
        let text = dump(&render(&ui(), &app, 100, 30, 5.0));
        text.lines()
            .find(|l| l.contains('↳'))
            .map(|l| {
                l.split('↳')
                    .nth(1)
                    .unwrap_or("")
                    .trim_end_matches(['┃', ' '])
                    .to_string()
            })
            .unwrap_or_default()
    }

    #[test]
    fn detail_line_with_project_only() {
        // SAFETY: tests may run in parallel. Best-effort assertion that prefix
        // logic works at all.
        unsafe {
            std::env::set_var("HOME", "/home/u");
        }
        let text = detail_text(Metadata {
            project: Some("/home/u/git/app".into()),
            ..Default::default()
        });
        assert_eq!(text, " ~/git/app");
    }

    #[test]
    fn detail_line_with_purpose_only() {
        let text = detail_text(Metadata {
            purpose: Some("PR #234".into()),
            ..Default::default()
        });
        assert_eq!(text, " PR #234");
    }

    #[test]
    fn detail_line_joins_project_and_purpose() {
        let text = detail_text(Metadata {
            project: Some("/srv/app".into()),
            purpose: Some("PR #234".into()),
            ..Default::default()
        });
        assert_eq!(text, " /srv/app  ·  PR #234");
    }

    #[test]
    fn detail_line_none_without_project_or_purpose() {
        let text = detail_text(Metadata {
            label: Some("just a label".into()),
            ..Default::default()
        });
        assert_eq!(text, "");
    }

    #[test]
    fn label_renders_with_session_name() {
        let app = App::new(
            vec![make_session("claude-app", Some("Refactoring auth"))],
            &Config::default(),
        );
        let text = dump(&render(&ui(), &app, 100, 30, 5.0));
        assert!(text.contains("Refactoring auth (claude-app)"), "{text}");
    }

    #[test]
    fn marker_glyph_renders_before_name() {
        let mut s = make_session("claude-app", None);
        s.marker = Some("\u{1F916}".into());
        let app = App::new(vec![s], &Config::default());
        let text = dump(&render(&ui(), &app, 100, 30, 5.0));
        assert!(text.contains('\u{1F916}'), "{text}");
    }

    #[test]
    fn help_overlay_lists_every_keybinding_section() {
        let joined = help_overlay_lines().join("\n");
        assert!(joined.contains("PICK MODE"));
        assert!(joined.contains("FILTER MODE"));
        assert!(joined.contains("NEW SESSION"));
        assert!(joined.contains("KILL CONFIRM"));
        assert!(joined.contains('?'));
        assert!(joined.contains("show this help"));
    }

    #[test]
    fn help_overlay_renders_through_buffer() {
        let mut app = App::new(sessions(3), &Config::default());
        app.enter_help();
        let text = dump(&render(&ui(), &app, 100, 32, 5.0));
        assert!(text.contains("COMMAND REFERENCE"), "{text}");
        assert!(text.contains("PICK MODE"), "{text}");
        assert!(text.contains("FILTER MODE"), "{text}");
    }

    #[test]
    fn kill_confirm_shows_target_card() {
        let mut app = App::new(sessions(3), &Config::default());
        app.enter_kill_confirm();
        let text = dump(&render(&ui(), &app, 100, 30, 5.0));
        assert!(text.contains("TERMINATE SESSION"), "{text}");
        assert!(text.contains("sess-00"), "{text}");
    }

    #[test]
    fn filtered_table_renders_only_matching_rows() {
        let sessions = vec![
            make_session("alpha", None),
            make_session("beta", None),
            make_session("gamma", None),
        ];
        let mut app = App::new(sessions, &Config::default());
        app.enter_filter_mode();
        for c in "be".chars() {
            app.filter_char(c);
        }
        assert_eq!(app.filtered_indices.len(), 1);
        assert_eq!(app.selected_name(), Some("beta"));
        let text = dump(&render(&ui(), &app, 80, 14, 5.0));
        assert!(text.contains("beta"));
        assert!(!text.contains("alpha"));
        assert!(!text.contains("gamma"));
        assert!(text.contains("1/3 MATCH"), "{text}");
    }

    #[test]
    fn empty_filter_result_shows_no_signal() {
        let mut app = App::new(sessions(3), &Config::default());
        app.enter_filter_mode();
        for c in "zzzz".chars() {
            app.filter_char(c);
        }
        let text = dump(&render(&ui(), &app, 80, 24, 5.0));
        assert!(text.contains("NO SIGNAL"), "{text}");
        assert!(text.contains("nothing matches /zzzz"), "{text}");
    }

    #[test]
    fn banner_and_countdown_render_at_full_size() {
        let app = App::new(sessions(3), &Config::default());
        let text = dump(&render(&ui(), &app, 120, 32, 5.0));
        assert!(text.contains("AUTO-ATTACH"), "{text}");
        assert!(text.contains("SESSIONS"), "{text}");
        assert!(text.contains("NETRUNNER"), "{text}");
        let logo: String = text.lines().next().unwrap_or("").chars().take(52).collect();
        assert!(
            logo.matches('▀').count() > 10,
            "banner glyphs missing: {text}"
        );
        assert!(text.contains(&format!("v{VERSION}")), "{text}");
    }

    fn still() -> Ui {
        let theme = Theme {
            animations: false,
            ..Theme::default()
        };
        frozen(&theme, true)
    }

    /// Each logo cell carries two pixels: the top one in the glyph colour
    /// and, when lit, the bottom one as the cell background.
    #[test]
    fn logo_cells_shade_each_pixel() {
        let app = App::new(sessions(3), &Config::default());
        let buf = render(&still(), &app, 120, 32, 5.0);
        // The T's stem (column 1 of the glyph, drawn from x = 2).
        let stem = &buf[(3, 1)];
        assert_eq!(stem.symbol(), "▀");
        assert!(matches!(stem.bg, Color::Rgb(..)), "bottom pixel unlit");
        // The chrome highlight: the top pixel row is brighter than the
        // one below it.
        let lum = |c: Color| match c {
            Color::Rgb(r, g, b) => u32::from(r) + u32::from(g) + u32::from(b),
            other => panic!("expected rgb, got {other:?}"),
        };
        let top = &buf[(3, 0)];
        assert!(lum(top.fg) > lum(top.bg), "no highlight on the top row");
    }

    #[test]
    fn scanlines_skip_the_logo() {
        let app = App::new(sessions(3), &Config::default());
        let buf = render(&still(), &app, 120, 32, 5.0);
        let base = Color::Rgb(8, 5, 18);
        // Row 1 is odd: shaded everywhere except the logo band.
        assert_eq!(buf[(60, 1)].bg, base, "logo band shaded");
        let below = (4..32).find(|&y| y % 2 == 1 && buf[(60, y)].symbol() == " ");
        let y = below.expect("a plain odd row below the logo");
        assert_ne!(buf[(60, y)].bg, base, "scanlines missing below the logo");
    }

    #[test]
    fn signal_meter_drains_with_idle_time() {
        let mut list = sessions(3);
        list[1].last_activity = Duration::from_secs(420);
        list[2].last_activity = Duration::from_secs(3 * 86_400);
        let app = App::new(list, &Config::default());
        let text = dump(&render(&still(), &app, 120, 32, 5.0));
        let row = |name: &str| {
            text.lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("{name} missing: {text}"))
                .to_string()
        };
        assert!(text.contains("SIGNAL"), "{text}");
        assert!(row("sess-00").contains("⣠⣾ now"), "{}", row("sess-00"));
        assert!(row("sess-01").contains("⣠   7m"), "{}", row("sess-01"));
        assert!(row("sess-02").contains("⡀   3d"), "{}", row("sess-02"));
        // The neon rows drop the filler: no "win" suffix, no hollow link.
        assert!(!row("sess-01").contains("win"), "{}", row("sess-01"));
        assert!(!row("sess-01").contains('◇'), "{}", row("sess-01"));
    }

    #[test]
    fn split_prompt_finds_shell_prompts_only() {
        assert_eq!(split_prompt("$ make"), Some(("", "$", " make")));
        assert_eq!(
            split_prompt("op@grid:~/src$ cargo test"),
            Some(("op@grid:~/src", "$", " cargo test"))
        );
        assert_eq!(
            split_prompt("root@box:/# ls"),
            Some(("root@box:/", "#", " ls"))
        );
        assert_eq!(split_prompt("  ❯ ls"), Some(("  ", "❯", " ls")));
        assert_eq!(split_prompt("sh-5.3$"), Some(("sh-5.3", "$", "")));
        assert_eq!(split_prompt("# a comment"), None);
        assert_eq!(split_prompt("costs $ 5"), None);
        assert_eq!(split_prompt("echo $HOME"), None);
        assert_eq!(split_prompt("100% done"), None);
        assert_eq!(split_prompt("plain output"), None);
    }

    #[test]
    fn feed_lights_up_prompts() {
        let mut app = App::new(sessions(2), &Config::default());
        app.preview = Some(String::from("ok\nop@grid$ make"));
        let buf = render(&still(), &app, 120, 32, 5.0);
        let (x, y) = (0..120)
            .flat_map(|x| (0..32).map(move |y| (x, y)))
            .find(|&(x, y)| buf[(x, y)].symbol() == "$")
            .expect("prompt rendered");
        let pal = Palette::with_truecolor(&Theme::default(), true);
        assert_eq!(buf[(x, y)].fg, pal.c(pal.pink), "sigil not lit");
        assert_eq!(buf[(x - 1, y)].fg, pal.c(pal.purple), "host not violet");
    }

    /// A modal fades the frame behind it.
    #[test]
    fn modal_scrim_dims_the_frame() {
        let mut app = App::new(sessions(3), &Config::default());
        let plain = render(&still(), &app, 120, 32, 5.0);
        app.enter_kill_confirm();
        let dimmed = render(&still(), &app, 120, 32, 5.0);
        let lum = |c: Color| match c {
            Color::Rgb(r, g, b) => u32::from(r) + u32::from(g) + u32::from(b),
            other => panic!("expected rgb, got {other:?}"),
        };
        // The "AUTO-ATTACH" box title sits well outside the card.
        let y = (0..32)
            .find(|&y| {
                (0..120)
                    .map(|x| plain[(x, y)].symbol())
                    .collect::<String>()
                    .contains("AUTO-ATTACH")
            })
            .expect("gauge title");
        let x = (0..120).find(|&x| plain[(x, y)].symbol() == "A").unwrap();
        assert!(lum(dimmed[(x, y)].fg) * 2 < lum(plain[(x, y)].fg));
    }

    #[test]
    fn status_counts_only_without_the_banner() {
        let app = App::new(sessions(3), &Config::default());
        let full = dump(&render(&ui(), &app, 120, 32, 5.0));
        let status = full.lines().nth(3).unwrap_or("");
        assert!(status.contains("SELECT TARGET"), "{full}");
        assert!(!status.contains("SESSIONS"), "counts repeated: {status}");
        let short = dump(&render(&ui(), &app, 120, 16, 5.0));
        assert!(short.contains("3 SESSIONS · 0 LINKED"), "{short}");
    }

    #[test]
    fn footer_keys_sit_on_keycaps() {
        let app = App::new(sessions(3), &Config::default());
        let buf = render(&still(), &app, 120, 32, 5.0);
        let y = 31;
        let line: String = (0..120).map(|x| buf[(x, y)].symbol()).collect();
        assert!(line.contains(" ? ") && line.contains("help"), "{line}");
        let x = (0..120).find(|&x| buf[(x, y)].symbol() == "?").unwrap();
        let cap = buf[(x, y)].bg;
        assert_ne!(cap, buf[(x + 3, y)].bg, "no keycap behind '?'");
        assert_eq!(cap, buf[(x - 1, y)].bg, "keycap is padded");
    }

    #[test]
    fn renders_at_every_size_and_mode_without_panicking() {
        let mut base = sessions(25);
        base[3].attached = true;
        base[4].marker = Some("\u{270F}\u{FE0F}".into());
        base[5].metadata = Some(Metadata {
            label: Some("a very long label that will not fit anywhere at all".into()),
            project: Some("/srv/x".into()),
            purpose: Some("PR #1".into()),
            label_at: None,
        });
        let ui = ui();
        for mode in 0..7 {
            let mut app = App::new(base.clone(), &Config::default());
            app.preview = Some("line one\n\tTabbed\nlast line".into());
            app.selected = 5;
            match mode {
                1 => app.enter_filter_mode(),
                2 => app.enter_new_mode(),
                3 => app.enter_kill_confirm(),
                4 => app.enter_help(),
                5 => app.enter_rename(),
                6 => app.cycle_preview_mode(),
                _ => {}
            }
            for w in [
                0, 1, 2, 5, 9, 10, 12, 20, 30, 45, 60, 64, 80, 100, 139, 140, 200,
            ] {
                for h in [
                    0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 13, 14, 16, 21, 22, 24, 30, 50,
                ] {
                    for t in [0.0, 0.3, 5.0] {
                        render(&ui, &app, w, h, t);
                    }
                }
            }
        }
    }

    #[test]
    fn selection_scrolls_into_view() {
        let mut app = App::new(sessions(40), &Config::default());
        for _ in 0..35 {
            app.move_down();
        }
        let text = dump(&render(&ui(), &app, 100, 24, 5.0));
        assert!(text.contains("sess-35"), "selected row not visible: {text}");
        // sess-00 is still named as the auto-attach target; sess-01 is not.
        assert!(!text.contains("sess-01"), "list did not scroll: {text}");
    }

    #[test]
    fn session_at_maps_clicks_to_visible_rows() {
        let app = App::new(sessions(5), &Config::default());
        let ui = ui();
        let area = Rect::new(0, 0, 100, 30);
        let buf = render(&ui, &app, 100, 30, 5.0);
        let y = (0..30)
            .find(|&y| {
                let row: String = (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect();
                row.contains("sess-02")
            })
            .expect("row rendered");
        assert_eq!(ui.session_at(&app, area, 20, y), Some(2));
        assert_eq!(ui.session_at(&app, area, 20, 0), None);
    }

    // -----------------------------------------------------------------------
    // Classic style
    // -----------------------------------------------------------------------

    fn classic() -> Ui {
        frozen(&Theme::classic(), true)
    }

    /// Nothing cyberpunk survives in the classic frame, in any mode.
    #[test]
    fn classic_style_drops_the_neon_hud() {
        let mut app = App::new(sessions(6), &Config::default());
        app.preview = Some("$ make\nok".into());
        let neon = [
            "NETRUNNER",
            "GRID",
            "SELECT TARGET",
            "LINKED",
            "FEED",
            "NO SIGNAL",
            "jack in",
            "TERMINATE",
            "COMMAND REFERENCE",
            "T+",
            "◢",
            "◤",
            "░",
            "┃",
            "┏",
            "┫",
        ];
        for mode in [Mode::Pick, Mode::Help, Mode::ConfirmKill, Mode::Filter] {
            app.mode = mode.clone();
            app.kill_target = Some("sess-00".into());
            let text = dump(&render(&classic(), &app, 100, 30, 5.0));
            for word in neon {
                assert!(!text.contains(word), "{mode:?} frame has {word:?}:\n{text}");
            }
            assert!(text.contains('╭') && text.contains("sessions"), "{text}");
        }
    }

    #[test]
    fn classic_style_uses_terminal_colours() {
        let mut app = App::new(sessions(6), &Config::default());
        app.move_down();
        let buf = render(&classic(), &app, 100, 30, 5.0);
        let name = |vis: usize| app.sessions[app.filtered_indices[vis]].name.clone();
        let line = |y: u16| {
            (0..100)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        let row_of = |name: &str| {
            (0..30)
                .find(|&y| line(y).contains(name))
                .expect("row rendered")
        };
        let (sel, plain) = (row_of(&name(1)), row_of(&name(2)));
        // A solid band in a fixed grey that terminal themes don't remap ...
        for x in [2, 50, 97] {
            assert_eq!(buf[(x, sel)].bg, Color::Rgb(78, 78, 78), "band col {x}");
        }
        // ... and the terminal's own background everywhere else.
        assert_eq!(buf[(50, plain)].bg, Color::Reset);
        assert_eq!(buf[(1, sel)].symbol(), " ", "no neon edge caps");
        // Running commands in the terminal's yellow (ANSI 3).
        let cmd = (0..96)
            .find(|&x| {
                (0..4)
                    .map(|i| buf[(x + i, plain)].symbol())
                    .collect::<String>()
                    == "bash"
            })
            .expect("command column");
        assert_eq!(buf[(cmd, plain)].fg, Color::Indexed(3));
    }

    #[test]
    fn classic_layout_has_no_banner_and_clicks_still_map() {
        let app = App::new(sessions(5), &Config::default());
        let ui = classic();
        let area = Rect::new(0, 0, 100, 30);
        let buf = render(&ui, &app, 100, 30, 5.0);
        let top: String = (0..100).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(top.contains("tmux-picker"), "status line first: {top:?}");
        let y = (0..30)
            .find(|&y| {
                let row: String = (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect();
                row.contains("sess-02")
            })
            .expect("row rendered");
        assert_eq!(ui.session_at(&app, area, 20, y), Some(2));
    }

    #[test]
    fn classic_frames_do_not_change() {
        let app = App::new(sessions(4), &Config::default());
        let ui = classic();
        assert_eq!(
            render(&ui, &app, 100, 30, 3.0),
            render(&ui, &app, 100, 30, 9.5)
        );
    }

    /// The selected row must stand out from the CRT-shaded rows around it,
    /// and carry neon edge caps in the box gutters.
    #[test]
    fn selection_band_stands_out() {
        let theme = Theme {
            animations: false,
            ..Theme::default()
        };
        let ui = frozen(&theme, true);
        let mut app = App::new(sessions(6), &Config::default());
        app.move_down();
        let buf = render(&ui, &app, 100, 30, 5.0);
        let row_of = |name: &str| {
            (0..30)
                .find(|&y| {
                    let row: String = (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect();
                    row.contains(name)
                })
                .expect("row rendered")
        };
        let rgb = |c: Color| match c {
            Color::Rgb(r, g, b) => [i32::from(r), i32::from(g), i32::from(b)],
            other => panic!("expected rgb, got {other:?}"),
        };
        let dist = |a: [i32; 3], b: [i32; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).sum::<i32>();
        let name = |vis: usize| app.sessions[app.filtered_indices[vis]].name.clone();
        let sel = row_of(app.selected_name().expect("selection"));
        let (above, below) = (row_of(&name(0)), row_of(&name(2)));
        assert_eq!((above + 1, below - 1), (sel, sel));
        for other in [above, below] {
            for x in [10, 50, 90] {
                let d = dist(rgb(buf[(x, sel)].bg), rgb(buf[(x, other)].bg));
                assert!(
                    d >= 100,
                    "col {x}: selected bg too close to row {other} ({d})"
                );
            }
        }
        assert_eq!(buf[(1, sel)].symbol(), "▌", "left edge cap");
        assert_eq!(buf[(98, sel)].symbol(), "▐", "right edge cap");
        assert_eq!(buf[(1, above)].symbol(), " ");
    }

    #[test]
    fn session_at_accounts_for_scroll_offset() {
        let mut app = App::new(sessions(40), &Config::default());
        for _ in 0..30 {
            app.move_down();
        }
        let ui = ui();
        let area = Rect::new(0, 0, 100, 24);
        let buf = render(&ui, &app, 100, 24, 5.0);
        let y = (0..24)
            .find(|&y| {
                let row: String = (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect();
                row.contains("sess-30")
            })
            .expect("selected row rendered");
        assert_eq!(ui.session_at(&app, area, 20, y), Some(30));
    }

    #[test]
    fn ansi256_mode_emits_no_truecolor() {
        let ui = frozen(&Theme::default(), false);
        let app = App::new(sessions(4), &Config::default());
        let buf = render(&ui, &app, 100, 30, 5.0);
        for c in buf.content() {
            assert!(!matches!(c.fg, Color::Rgb(..)), "rgb fg in 256 mode");
            assert!(!matches!(c.bg, Color::Rgb(..)), "rgb bg in 256 mode");
        }
    }

    #[test]
    fn reset_background_leaves_terminal_background() {
        let theme = Theme {
            background: Color::Reset,
            ..Theme::default()
        };
        let ui = frozen(&theme, true);
        let app = App::new(sessions(2), &Config::default());
        let buf = render(&ui, &app, 80, 24, 5.0);
        // Row 5 is inside the list box, away from the selection band.
        assert_eq!(buf[(40, 12)].bg, Color::Reset);
    }

    #[test]
    fn static_mode_frames_do_not_change() {
        let theme = Theme {
            animations: false,
            ..Theme::default()
        };
        let ui = frozen(&theme, true);
        let app = App::new(sessions(4), &Config::default());
        let a = render(&ui, &app, 100, 30, 3.0);
        let b = render(&ui, &app, 100, 30, 3.5);
        // The T+ clocks both read 00:03, so nothing may differ.
        assert_eq!(a, b);
        assert_eq!(ui.frame_interval(), IDLE_TICK);
    }

    /// Over SSH every changed cell costs bytes; keep steady-state frames
    /// small so the animation stays cheap on slow links.
    #[test]
    fn animated_frames_redraw_few_cells() {
        let ui = ui();
        let mut app = App::new(sessions(6), &Config::default());
        app.preview = Some("$ cargo test\nok".into());
        let (w, h) = (100u16, 30u16);
        let mut worst = 0;
        let mut total = 0;
        let frames = 200;
        for i in 0..frames {
            let t = 2.0 + i as f32 * 0.07;
            let a = render(&ui, &app, w, h, t);
            let b = render(&ui, &app, w, h, t + 0.07);
            let changed = a
                .content()
                .iter()
                .zip(b.content())
                .filter(|(x, y)| x != y)
                .count();
            worst = worst.max(changed);
            total += changed;
        }
        let cells = usize::from(w) * usize::from(h);
        assert!(
            total / frames < cells / 20,
            "avg {} cells/frame",
            total / frames
        );
        assert!(worst < cells / 6, "worst frame redrew {worst} cells");
    }

    #[test]
    fn tail_fit_keeps_the_end() {
        assert_eq!(tail_fit("abcdef", 3), "def");
        assert_eq!(tail_fit("abc", 5), "abc");
        assert_eq!(tail_fit("", 0), "");
    }

    #[test]
    fn mmss_formats() {
        assert_eq!(mmss(0.0), "00:00");
        assert_eq!(mmss(75.9), "01:15");
        assert_eq!(mmss(-3.0), "00:00");
    }
}
