//! A tiny cell grid that both art styles and the speech bubble draw into,
//! plus the code that turns it into ANSI escape sequences.

use std::fmt::Write as _;

use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Bubble border / pointer color (neon cyan).
pub const CYAN: Rgb = Rgb(0x00, 0xe5, 0xff);
/// Bubble text color (lavender).
pub const LAVENDER: Rgb = Rgb(0xd0, 0xd0, 0xff);
/// Electric-blue neon (cyber bubble border, dance notes).
pub const ELECTRIC: Rgb = Rgb(0x2f, 0x7d, 0xff);
/// Soft ice blue highlight (bubble corners, label, sparkles).
pub const ICE: Rgb = Rgb(0x9c, 0xd4, 0xff);
/// Lavender-silver, like Byanca's hair (cyber bubble text).
pub const SILVER: Rgb = Rgb(0xe2, 0xdc, 0xf5);
/// Soft pink accent (tiny hearts).
pub const PINK: Rgb = Rgb(0xff, 0x8f, 0xc8);

/// Marker stored in the cell to the right of a double-width character.
pub const WIDE_TAIL: char = '\0';

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
}

impl Cell {
    pub const BLANK: Cell = Cell {
        ch: ' ',
        fg: None,
        bg: None,
    };

    pub fn is_blank(&self) -> bool {
        self.ch == ' ' && self.bg.is_none()
    }
}

#[derive(Clone, Debug)]
pub struct Canvas {
    pub width: usize,
    pub rows: Vec<Vec<Cell>>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Canvas {
            width,
            rows: vec![vec![Cell::BLANK; width]; height],
        }
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    pub fn set(&mut self, x: usize, y: usize, cell: Cell) {
        if y < self.rows.len() && x < self.width {
            self.rows[y][x] = cell;
        }
    }

    /// Writes `s` starting at (x, y). Double-width characters take two cells,
    /// zero-width and control characters are dropped.
    pub fn put_str(&mut self, x: usize, y: usize, s: &str, fg: Option<Rgb>) {
        let mut cx = x;
        for ch in s.chars() {
            let w = ch.width().unwrap_or(0);
            if w == 0 {
                continue;
            }
            if cx + w > self.width {
                break;
            }
            self.set(cx, y, Cell { ch, fg, bg: None });
            if w == 2 {
                self.set(
                    cx + 1,
                    y,
                    Cell {
                        ch: WIDE_TAIL,
                        fg,
                        bg: None,
                    },
                );
            }
            cx += w;
        }
    }

    /// Copies every non-blank cell of `other` onto this canvas at (x, y).
    pub fn blit(&mut self, x: usize, y: usize, other: &Canvas) {
        for (dy, row) in other.rows.iter().enumerate() {
            for (dx, cell) in row.iter().enumerate() {
                if !cell.is_blank() {
                    self.set(x + dx, y + dy, *cell);
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorMode {
    None,
    Ansi256,
    TrueColor,
}

fn ansi256(c: Rgb) -> u8 {
    fn cube(v: u8) -> u8 {
        match v {
            0..=47 => 0,
            48..=114 => 1,
            _ => ((v as u16 - 35) / 40) as u8,
        }
    }
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let (r, g, b) = (cube(c.0), cube(c.1), cube(c.2));
    let cube_rgb = (LEVELS[r as usize], LEVELS[g as usize], LEVELS[b as usize]);
    let avg = (c.0 as u16 + c.1 as u16 + c.2 as u16) / 3;
    let gray_idx = if avg > 238 { 23 } else { avg.saturating_sub(3) / 10 } as u8;
    let gray = 8 + 10 * gray_idx as u16;
    let dist = |a: (u16, u16, u16)| {
        let d = |x: u16, y: u8| (x as i32 - y as i32).pow(2);
        d(a.0, c.0) + d(a.1, c.1) + d(a.2, c.2)
    };
    let dc = dist((cube_rgb.0 as u16, cube_rgb.1 as u16, cube_rgb.2 as u16));
    let dg = dist((gray, gray, gray));
    if dg < dc {
        232 + gray_idx
    } else {
        16 + 36 * r + 6 * g + b
    }
}

fn push_color(out: &mut String, mode: ColorMode, c: Rgb, bg: bool) {
    let base = if bg { 48 } else { 38 };
    match mode {
        ColorMode::TrueColor => {
            let _ = write!(out, "\x1b[{};2;{};{};{}m", base, c.0, c.1, c.2);
        }
        ColorMode::Ansi256 => {
            let _ = write!(out, "\x1b[{};5;{}m", base, ansi256(c));
        }
        ColorMode::None => {}
    }
}

/// Renders one canvas row. With `pad` the row is emitted at full width
/// (needed when redrawing in place); otherwise trailing blanks are trimmed.
/// At most `max_cols` columns are written.
pub fn render_row(row: &[Cell], mode: ColorMode, max_cols: usize, pad: bool) -> String {
    let mut end = row.len().min(max_cols);
    if !pad {
        while end > 0 && row[end - 1].is_blank() {
            end -= 1;
        }
    }
    // never cut a wide character in half
    if end > 0 && end < row.len() && row[end].ch == WIDE_TAIL {
        end -= 1;
    }
    let mut out = String::new();
    let (mut cur_fg, mut cur_bg): (Option<Rgb>, Option<Rgb>) = (None, None);
    for cell in &row[..end] {
        if cell.ch == WIDE_TAIL {
            continue;
        }
        if mode != ColorMode::None && (cell.fg != cur_fg || cell.bg != cur_bg) {
            if (cur_fg.is_some() && cell.fg.is_none()) || (cur_bg.is_some() && cell.bg.is_none()) {
                out.push_str("\x1b[0m");
                cur_fg = None;
                cur_bg = None;
            }
            if cell.fg != cur_fg {
                if let Some(c) = cell.fg {
                    push_color(&mut out, mode, c, false);
                }
            }
            if cell.bg != cur_bg {
                if let Some(c) = cell.bg {
                    push_color(&mut out, mode, c, true);
                }
            }
            cur_fg = cell.fg;
            cur_bg = cell.bg;
        }
        out.push(cell.ch);
    }
    if mode != ColorMode::None && (cur_fg.is_some() || cur_bg.is_some()) {
        out.push_str("\x1b[0m");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_resets() {
        let mut c = Canvas::new(6, 1);
        c.put_str(0, 0, "ab", Some(CYAN));
        assert_eq!(render_row(&c.rows[0], ColorMode::None, 99, false), "ab");
        assert_eq!(render_row(&c.rows[0], ColorMode::None, 99, true), "ab    ");
        let s = render_row(&c.rows[0], ColorMode::TrueColor, 99, false);
        assert!(s.starts_with("\x1b[38;2;0;229;255m") && s.ends_with("\x1b[0m"));
    }

    #[test]
    fn wide_chars() {
        let mut c = Canvas::new(4, 1);
        c.put_str(0, 0, "猫x", None);
        assert_eq!(render_row(&c.rows[0], ColorMode::None, 99, false), "猫x");
        assert_eq!(render_row(&c.rows[0], ColorMode::None, 1, false), "");
    }

    #[test]
    fn palette_256() {
        assert_eq!(ansi256(Rgb(0, 0, 0)), 16);
        assert_eq!(ansi256(Rgb(255, 255, 255)), 231);
        assert_eq!(ansi256(Rgb(0, 0, 255)), 21);
    }
}
