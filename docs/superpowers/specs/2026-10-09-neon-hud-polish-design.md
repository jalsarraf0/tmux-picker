# tmux-picker: Sharper neon HUD

**Date:** 2026-10-09
**Status:** Implemented
**Author:** jalsarraf + Claude

## Problem

The cyberpunk look had the right palette but read as noisy:

1. **The banner fell apart.** The outline box-drawing font (`╔╦╗`) left
   gaps between rows in most fonts, so the logo looked broken.
2. **Facts repeated.** Session and attach counts appeared in the banner,
   the status line and the list's corner; the `T+` clock in the banner
   and the footer.
3. **No focus.** Both panels glowed the same, and their `┫ TITLE ┣`
   brackets read as breaks in the line.
4. **Filler in the list.** `1 win` on every row, a hollow `◇` on every
   detached session, `active 0s` / `idle 7m` spelled out.
5. **Flat footer.** Key hints separated by `░` blocks.
6. **Modals floated on a busy frame.** The help and kill cards sat over
   undimmed panels (and the feed's digital rain), edged with heavy
   `▀`/`▄` bars.

## Solution

Same palette, effects and layout; less noise, more hierarchy. Cyberpunk
only: classic keeps its look apart from the shared feed detail line and
help card spacing.

- **Pixel logo.** `TMUX//PICKER` in a 3×5 bitmap font (M is 5 wide),
  two pixels per cell with half blocks. A cell whose top and bottom
  pixels are both lit draws `▀` with the bottom pixel as its background,
  so every pixel keeps its own colour: the neon sweep across, a chrome
  highlight on the top rows, and a one-pixel violet drop shadow. Decrypt
  intro, glitch and drift are unchanged. Scanlines skip the logo rows,
  where they showed as lighter half pixels. At 80 columns the info block
  drops its labels before its values.
- **Status rule** carries the mode and what shapes the list (sort,
  filter, flash) as segments on the line; counts only when the banner,
  which shows them, is hidden.
- **Panels.** Titles and tags cut straight into the top edge. The list
  keeps the hot violet→cyan frame; the feed gets a cold frame under a
  live pink title, so focus reads at a glance.
- **List.** `#` header, bare window counts, `◆ LINKED` only on attached
  sessions, and a `SIGNAL` column: a braille signal meter (`⣠⣾`, four
  bars, two to a cell) that drains green → yellow → violet → dim with
  idle time, next to a compact age (`now`, `40s`, `7m`, `2h`, `3d`).
  The neon beam alone marks the selected row (no `▶`).
- **Feed.** Project in cyan and purpose in bold yellow on the detail
  line. Shell prompts (`user@host:~$ cmd`, `❯ cmd`, `root@box:/# cmd`)
  get a violet host, a pink sigil and a bright command, fading with age
  like the rest. The text before the sigil must be one short word, `#`
  needs one, and `100%` doesn't count, so ordinary output stays plain.
- **Footer.** Each key on a violet keycap, its action dim, the version
  faint at the right.
- **Modals.** A scrim fades the whole frame 70% toward the background,
  then the card gets a gradient heavy frame (edge colour → cyan) with
  the flickering title in its top edge.

## Budget

Over SSH every changed cell costs bytes. The logo still drifts at 2 Hz,
and the meters and keycaps are static, so `animated_frames_redraw_few_cells`
holds without changes. 256-colour mode routes every new colour through
`Palette::c`, so `ansi256_mode_emits_no_truecolor` still holds.

## Tests

Logo pixels (bottom pixel as background, brighter top row), scanlines
skipping the logo, meter and age per idle band and no `win`/`◇` filler,
prompt splitting (positive and negative cases), prompt colours in the
feed, the modal scrim, status counts with and without the banner, and
padded keycaps. The detail-line tests now read the rendered feed.
