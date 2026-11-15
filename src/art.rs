//! Embedded Byanca artwork: the truecolor half-block chibi and the
//! hand-drawn plain ASCII version.

use crate::canvas::{Canvas, Cell, Rgb, ELECTRIC, ICE, PINK};

static CHIBI_BIN: &[u8] = include_bytes!("../assets/chibi.bin");
static ASCII_BASE: &str = include_str!("../assets/ascii/byanca.txt");
static ASCII_TAIL_UP: &str = include_str!("../assets/ascii/byanca_tail_up.txt");
static ASCII_TAIL_DOWN: &str = include_str!("../assets/ascii/byanca_tail_down.txt");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eyes {
    Open,
    Half,
    Closed,
}

/// Number of frames in one dance loop (both art styles).
pub const DANCE_FRAMES: u8 = 8;

/// One animation pose. `arm` and `tail` are -1, 0 or +1.
/// In ASCII mode `arm` twitches an ear instead (the drawing has no raised paw).
///
/// `dance` switches to the dance loop: the value is a beat counter, the
/// frame is `beat % DANCE_FRAMES` and the floating notes use the full value.
/// Dance frames bake their own eyes, paws and tail, so only `mouth_closed`
/// still applies (ASCII).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pose {
    pub eyes: Eyes,
    pub arm: i8,
    pub tail: i8,
    pub mouth_closed: bool,
    pub dance: Option<u8>,
}

impl Default for Pose {
    fn default() -> Self {
        Pose {
            eyes: Eyes::Open,
            arm: 0,
            tail: 0,
            mouth_closed: false,
            dance: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Chibi,
    Ascii,
}

/// Columns of empty space on each side of the chibi dance frames, where the
/// music notes float.
const NOTE_PAD: usize = 3;
/// Left margin and fixed size of the ASCII dance frames.
const ASCII_DANCE_MARGIN: usize = 2;
const ASCII_DANCE_W: usize = 60;
const ASCII_DANCE_H: usize = 26;

impl Style {
    /// Column (inside the art) that the speech bubble pointer aims at.
    pub fn anchor(self) -> usize {
        match self {
            Style::Chibi => 18,
            Style::Ascii => 22,
        }
    }

    /// Bubble anchor for a given pose (dance frames are wider).
    pub fn anchor_for(self, pose: Pose) -> usize {
        match (self, pose.dance) {
            (_, None) => self.anchor(),
            (Style::Chibi, Some(_)) => self.anchor() + NOTE_PAD + chibi_frames().dance.map_or(0, |d| d.xoff),
            (Style::Ascii, Some(_)) => self.anchor() + ASCII_DANCE_MARGIN + 2,
        }
    }
}

struct FrameSet {
    width: usize,
    height: usize,
    /// dance block only: column of the main frames' left edge
    xoff: usize,
    frames: Vec<([u8; 3], &'static [u8])>,
}

struct ChibiFrames {
    main: FrameSet,
    dance: Option<FrameSet>,
}

fn parse_set(b: &'static [u8], at: usize, magic: &[u8; 4]) -> Option<(FrameSet, usize)> {
    if b.get(at..at + 4)? != magic {
        return None;
    }
    let rd = |i: usize| b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize);
    let (width, height, count) = (rd(at + 4)?, rd(at + 6)?, rd(at + 8)?);
    let (xoff, mut off) = if magic == b"NYAD" {
        (rd(at + 10)?, at + 12)
    } else {
        (0, at + 10)
    };
    let size = width * height * 4;
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let name = b.get(off..off + 3)?;
        let name = [name[0], name[1], name[2]];
        off += 3;
        frames.push((name, b.get(off..off + size)?));
        off += size;
    }
    Some((
        FrameSet {
            width,
            height,
            xoff,
            frames,
        },
        off,
    ))
}

fn chibi_frames() -> ChibiFrames {
    let (main, end) = parse_set(CHIBI_BIN, 0, b"NYAF").expect("corrupt chibi.bin");
    let dance = parse_set(CHIBI_BIN, end, b"NYAD").map(|(set, _)| set);
    ChibiFrames { main, dance }
}

fn sign_char(v: i8) -> u8 {
    match v.signum() {
        -1 => b'-',
        1 => b'+',
        _ => b'0',
    }
}

fn chibi(pose: Pose, color: bool) -> Canvas {
    let all = chibi_frames();
    if let (Some(beat), Some(set)) = (pose.dance, all.dance.as_ref()) {
        let idx = b'0' + beat % DANCE_FRAMES;
        let px = set
            .frames
            .iter()
            .find(|(n, _)| n[0] == b'D' && n[1] == idx)
            .or_else(|| set.frames.first())
            .map(|(_, p)| *p)
            .expect("chibi.bin has no dance frames");
        let art = halfblocks(set, px);
        let mut c = Canvas::new(art.width + 2 * NOTE_PAD, art.height());
        c.blit(NOTE_PAD, 0, &art);
        float_notes(&mut c, beat, color, true);
        return c;
    }
    let set = &all.main;
    let eye = match pose.eyes {
        Eyes::Open => b'o',
        Eyes::Half => b'h',
        Eyes::Closed => b'c',
    };
    let want = [eye, sign_char(pose.arm), sign_char(pose.tail)];
    let px = set
        .frames
        .iter()
        .find(|(n, _)| *n == want)
        .or_else(|| set.frames.first())
        .map(|(_, p)| *p)
        .expect("chibi.bin has no frames");
    halfblocks(set, px)
}

/// Packs two RGBA pixel rows into each terminal row with half blocks.
fn halfblocks(set: &FrameSet, px: &[u8]) -> Canvas {
    let pixel = |x: usize, y: usize| -> Option<Rgb> {
        let i = (y * set.width + x) * 4;
        (px[i + 3] > 0).then(|| Rgb(px[i], px[i + 1], px[i + 2]))
    };
    let rows = set.height.div_ceil(2);
    let mut c = Canvas::new(set.width, rows);
    for cy in 0..rows {
        for x in 0..set.width {
            let top = pixel(x, cy * 2);
            let bot = if cy * 2 + 1 < set.height {
                pixel(x, cy * 2 + 1)
            } else {
                None
            };
            let cell = match (top, bot) {
                (None, None) => Cell::BLANK,
                (Some(t), None) => Cell {
                    ch: '▀',
                    fg: Some(t),
                    bg: None,
                },
                (None, Some(b)) => Cell {
                    ch: '▄',
                    fg: Some(b),
                    bg: None,
                },
                (Some(t), Some(b)) if t == b => Cell {
                    ch: '█',
                    fg: Some(t),
                    bg: None,
                },
                (Some(t), Some(b)) => Cell {
                    ch: '▀',
                    fg: Some(t),
                    bg: Some(b),
                },
            };
            c.set(x, cy, cell);
        }
    }
    c
}

/// A floating decoration: column (negative = from the right edge), the row
/// it starts on, how many rows it rises before looping, its phase, glyph
/// and color.
struct Note {
    x: isize,
    y: usize,
    rise: u8,
    phase: u8,
    glyph: char,
    fg: Rgb,
}

const fn note(x: isize, y: usize, rise: u8, phase: u8, glyph: char, fg: Rgb) -> Note {
    Note {
        x,
        y,
        rise,
        phase,
        glyph,
        fg,
    }
}

const CHIBI_NOTES: [Note; 8] = [
    note(1, 12, 6, 0, '♪', ELECTRIC),
    note(4, 7, 5, 2, '✦', ICE),
    note(2, 22, 6, 4, '♫', ELECTRIC),
    note(6, 3, 4, 1, '·', ICE),
    note(-2, 13, 6, 3, '♫', ELECTRIC),
    note(-5, 6, 5, 0, '♪', ICE),
    note(-3, 24, 6, 1, '♥', PINK),
    note(-7, 2, 4, 3, '✧', ICE),
];

const ASCII_NOTES: [Note; 6] = [
    note(1, 7, 5, 0, '♪', ELECTRIC),
    note(5, 3, 4, 2, '*', ICE),
    note(2, 20, 5, 3, '♫', ELECTRIC),
    note(-3, 8, 5, 1, '♫', ELECTRIC),
    note(-7, 3, 4, 3, '♪', ICE),
    note(-2, 18, 5, 4, '*', ICE),
];

/// Draws the rising music notes / sparkles onto blank cells only, so they
/// never cover the character.
fn float_notes(c: &mut Canvas, beat: u8, color: bool, chibi: bool) {
    let notes: &[Note] = if chibi { &CHIBI_NOTES } else { &ASCII_NOTES };
    for n in notes {
        let step = (beat.wrapping_add(n.phase)) % n.rise;
        let Some(y) = n.y.checked_sub(step as usize) else {
            continue;
        };
        let x = if n.x < 0 {
            c.width.checked_sub(n.x.unsigned_abs())
        } else {
            Some(n.x as usize)
        };
        let Some(x) = x else { continue };
        if y >= c.height() || x >= c.width || !c.rows[y][x].is_blank() {
            continue;
        }
        // the last step of the rise fades into a tiny dot
        let glyph = if step + 1 == n.rise && n.glyph != '·' {
            '·'
        } else {
            n.glyph
        };
        let fg = color.then_some(n.fg);
        c.set(
            x,
            y,
            Cell {
                ch: glyph,
                fg,
                bg: None,
            },
        );
    }
}

fn replace_at(line: &mut String, from: usize, pat: &str, with: &str) {
    if let Some(i) = line[from.min(line.len())..].find(pat).map(|i| i + from) {
        line.replace_range(i..i + pat.len(), with);
    }
}

/// The plain ASCII frame as text lines (no color).
pub fn ascii_lines(pose: Pose) -> Vec<String> {
    let src = match pose.tail.signum() {
        1 => ASCII_TAIL_UP,
        -1 => ASCII_TAIL_DOWN,
        _ => ASCII_BASE,
    };
    let mut lines: Vec<String> = src.lines().map(str::to_owned).collect();
    if pose.eyes == Eyes::Closed {
        for l in lines.iter_mut() {
            if l.contains("( o )") {
                *l = l.replace("( o )", "( - )");
            }
        }
    }
    if pose.mouth_closed {
        for l in lines.iter_mut() {
            if l.contains("\\/\\/") {
                *l = l.replacen("\\/\\/", "-vv-", 1);
            }
        }
    }
    if lines.len() > 1 {
        match pose.arm.signum() {
            // left ear folds down
            -1 => {
                replace_at(&mut lines[0], 0, "/\\", "  ");
                replace_at(&mut lines[1], 0, "/  \\", ",--.");
            }
            // right ear folds down
            1 => {
                let r0 = lines[0].rfind("/\\").unwrap_or(0);
                replace_at(&mut lines[0], r0, "/\\", "  ");
                let r1 = lines[1].rfind("/  \\").unwrap_or(0);
                replace_at(&mut lines[1], r1, "/  \\", ".--,");
            }
            _ => {}
        }
    }
    for l in lines.iter_mut() {
        let t = l.trim_end().len();
        l.truncate(t);
    }
    lines
}

fn ascii(pose: Pose) -> Canvas {
    let lines = ascii_lines(pose);
    // all variants share the same bounding box so animation does not jump
    let width = [ASCII_BASE, ASCII_TAIL_UP, ASCII_TAIL_DOWN]
        .iter()
        .flat_map(|s| s.lines())
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let mut c = Canvas::new(width, lines.len());
    for (y, l) in lines.iter().enumerate() {
        c.put_str(0, y, l, None);
    }
    c
}

/// Which paws are up in an ASCII dance frame.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Paws {
    Down,
    Left,
    Right,
    Both,
}

/// One ASCII dance frame: paws, sideways sway, bounce, happy eyes, tail and
/// head tilt (the head sways one column further than the body).
struct Step {
    paws: Paws,
    dx: usize,
    dy: usize,
    happy: bool,
    tail: i8,
    tilt: i8,
}

const fn step(paws: Paws, dx: usize, dy: usize, happy: bool, tail: i8, tilt: i8) -> Step {
    Step {
        paws,
        dx,
        dy,
        happy,
        tail,
        tilt,
    }
}

const ASCII_DANCE: [Step; DANCE_FRAMES as usize] = [
    step(Paws::Left, 0, 0, false, -1, -1),
    step(Paws::Left, 1, 1, false, 0, -1),
    step(Paws::Both, 2, 0, true, 1, 0),
    step(Paws::Down, 3, 1, true, 0, 0),
    step(Paws::Right, 4, 0, false, 1, 1),
    step(Paws::Right, 3, 1, false, 0, 1),
    step(Paws::Both, 2, 0, true, -1, 0),
    step(Paws::Down, 1, 1, true, 0, 0),
];

/// Raised left paw, drawn over the base drawing (row, column, text).
/// The right paw is the same patch mirrored around the drawing's center.
const LEFT_PAW_UP: [(usize, usize, &str); 12] = [
    (9, 2, " .-. "),
    (10, 1, " (,,,) "),
    (11, 2, " \\ \\ "),
    (12, 3, " \\ \\ "),
    (13, 4, " \\ \\ "),
    (14, 5, " \\ \\ "),
    (15, 6, " \\ \\ "),
    (16, 7, " \\ '------"),
    (17, 8, " '-------' "),
    (18, 15, " "),
    (19, 15, " "),
    (20, 16, "   "),
];
/// Mirror axis of the ASCII drawing (twice the center column).
const ASCII_MIRROR: usize = 52;
/// Last row of the head (the collar line); the body starts below it.
const ASCII_HEAD_ROWS: usize = 16;

fn mirror_char(ch: char) -> char {
    match ch {
        '/' => '\\',
        '\\' => '/',
        '(' => ')',
        ')' => '(',
        c => c,
    }
}

fn patch(grid: &mut [Vec<char>], row: usize, col: usize, text: &str) {
    if let Some(line) = grid.get_mut(row) {
        for (i, ch) in text.chars().enumerate() {
            if let Some(cell) = line.get_mut(col + i) {
                *cell = ch;
            }
        }
    }
}

/// The ASCII dance frame for `beat` as text lines (no notes).
pub fn ascii_dance_lines(beat: u8, mouth_closed: bool) -> Vec<String> {
    let st = &ASCII_DANCE[(beat % DANCE_FRAMES) as usize];
    let mut lines = ascii_lines(Pose {
        tail: st.tail,
        mouth_closed,
        ..Pose::default()
    });
    if st.happy {
        for l in lines.iter_mut() {
            *l = l.replace("( o )", "( ^ )");
        }
    }
    // head tilt: shift the head one column against the body
    for l in lines.iter_mut().take(ASCII_HEAD_ROWS) {
        match st.tilt.signum() {
            -1 if l.starts_with(' ') => {
                l.remove(0);
            }
            1 => l.insert(0, ' '),
            _ => {}
        }
    }
    let width = ASCII_DANCE_W - ASCII_DANCE_MARGIN - 4;
    let mut grid: Vec<Vec<char>> = lines
        .iter()
        .map(|l| {
            let mut v: Vec<char> = l.chars().collect();
            v.resize(width.max(v.len()), ' ');
            v
        })
        .collect();
    if matches!(st.paws, Paws::Left | Paws::Both) {
        for (r, c, t) in LEFT_PAW_UP {
            patch(&mut grid, r, c, t);
        }
    }
    if matches!(st.paws, Paws::Right | Paws::Both) {
        for (r, c, t) in LEFT_PAW_UP {
            let n = t.chars().count();
            let m: String = t.chars().rev().map(mirror_char).collect();
            patch(&mut grid, r, ASCII_MIRROR - (c + n - 1), &m);
        }
    }
    let pad = " ".repeat(ASCII_DANCE_MARGIN + st.dx);
    let mut out = vec![String::new(); st.dy];
    out.extend(grid.into_iter().map(|row| {
        let s: String = row.into_iter().collect();
        format!("{pad}{}", s.trim_end()).trim_end().to_string()
    }));
    out
}

fn ascii_dance(beat: u8, mouth_closed: bool) -> Canvas {
    let lines = ascii_dance_lines(beat, mouth_closed);
    let mut c = Canvas::new(ASCII_DANCE_W, ASCII_DANCE_H.max(lines.len()));
    for (y, l) in lines.iter().enumerate() {
        c.put_str(0, y, l, None);
    }
    float_notes(&mut c, beat, false, false);
    c
}

/// Renders a pose. `color` only affects decorations (the chibi is always in
/// color, the ASCII drawing never is).
pub fn render(style: Style, pose: Pose, color: bool) -> Canvas {
    match (style, pose.dance) {
        (Style::Chibi, _) => chibi(pose, color),
        (Style::Ascii, Some(beat)) => ascii_dance(beat, pose.mouth_closed),
        (Style::Ascii, None) => ascii(pose),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_chibi_frames_present() {
        let set = chibi_frames().main;
        assert_eq!(set.frames.len(), 27);
        for e in *b"ohc" {
            for a in *b"-0+" {
                for t in *b"-0+" {
                    assert!(set.frames.iter().any(|(n, _)| *n == [e, a, t]));
                }
            }
        }
    }

    #[test]
    fn ascii_variants_keep_shape() {
        let base = ascii_lines(Pose::default());
        let blink = ascii_lines(Pose {
            eyes: Eyes::Closed,
            ..Pose::default()
        });
        assert!(blink.iter().any(|l| l.contains("( - )")));
        assert_eq!(base.len(), blink.len());
        for arm in [-1, 1] {
            let ear = ascii_lines(Pose {
                arm,
                ..Pose::default()
            });
            assert_eq!(ear.len(), base.len());
            assert_ne!(ear[1], base[1]);
        }
        for tail in [-1, 1] {
            assert_eq!(
                ascii_lines(Pose {
                    tail,
                    ..Pose::default()
                })
                .len(),
                base.len()
            );
        }
    }

    #[test]
    fn chibi_dance_frames_present() {
        let dance = chibi_frames().dance.expect("chibi.bin has a dance block");
        assert_eq!(dance.frames.len(), DANCE_FRAMES as usize);
        for i in 0..DANCE_FRAMES {
            assert!(dance.frames.iter().any(|(n, _)| n[0] == b'D' && n[1] == b'0' + i));
        }
        assert!(dance.width >= chibi_frames().main.width);
    }

    #[test]
    fn ascii_dance_moves_paws_and_body() {
        let frames: Vec<Vec<String>> = (0..DANCE_FRAMES).map(|b| ascii_dance_lines(b, false)).collect();
        // every frame differs from the next one
        for w in frames.windows(2) {
            assert_ne!(w[0], w[1]);
        }
        let joined = |b: usize| frames[b].join("\n");
        assert_eq!(joined(0).matches("(,,,)").count(), 1, "left paw up");
        assert_eq!(joined(2).matches("(,,,)").count(), 2, "both paws up");
        assert_eq!(joined(3).matches("(,,,)").count(), 0, "paws down");
        assert!(joined(2).contains("( ^ )"), "happy eyes on the beat");
        // stays inside the fixed dance box
        for f in &frames {
            assert!(f.len() <= ASCII_DANCE_H);
            assert!(f.iter().all(|l| l.chars().count() <= ASCII_DANCE_W));
            assert!(f.iter().all(|l| l.is_ascii()));
        }
    }

    #[test]
    fn dance_notes_never_cover_the_art() {
        for beat in 0..60u8 {
            let plain = ascii_dance_lines(beat, false);
            let c = render(
                Style::Ascii,
                Pose {
                    dance: Some(beat),
                    ..Pose::default()
                },
                false,
            );
            for (y, l) in plain.iter().enumerate() {
                for (x, ch) in l.chars().enumerate() {
                    if ch != ' ' {
                        assert_eq!(c.rows[y][x].ch, ch);
                    }
                }
            }
            let notes = c.rows.iter().flatten().filter(|c| "♪♫*·".contains(c.ch)).count();
            assert!(notes > 0, "beat {beat} has no notes");
        }
    }
}
