//! Word wrapping and the speech / thought bubble.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::canvas::{Canvas, Rgb, CYAN, ELECTRIC, ICE, LAVENDER, PINK, SILVER};

/// Wraps `text` to at most `width` columns. Existing newlines are kept,
/// words longer than `width` are split. A width of 0 disables wrapping.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = if width == 0 { usize::MAX / 4 } else { width };
    let text = text.replace('\t', "    ").replace("\r\n", "\n");
    let mut out = Vec::new();
    for para in text.trim_end_matches('\n').split('\n') {
        let mut line = String::new();
        let mut line_w = 0;
        for word in para.split_whitespace() {
            let mut word = word.to_string();
            loop {
                let ww = UnicodeWidthStr::width(word.as_str());
                let sep = usize::from(line_w > 0);
                if line_w + sep + ww <= width {
                    if sep == 1 {
                        line.push(' ');
                    }
                    line.push_str(&word);
                    line_w += sep + ww;
                    break;
                }
                if line_w > 0 {
                    out.push(std::mem::take(&mut line));
                    line_w = 0;
                    continue;
                }
                // a single word wider than the bubble: hard split it
                let mut head = String::new();
                let mut hw = 0;
                let mut rest = String::new();
                for ch in word.chars() {
                    let cw = ch.width().unwrap_or(0);
                    if rest.is_empty() && hw + cw <= width {
                        head.push(ch);
                        hw += cw;
                    } else {
                        rest.push(ch);
                    }
                }
                if head.is_empty() {
                    // even one char does not fit (width 1 and a wide char)
                    let mut it = rest.chars();
                    head.extend(it.next());
                    rest = it.collect();
                }
                out.push(head);
                if rest.is_empty() {
                    break;
                }
                word = rest;
            }
        }
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BubbleKind {
    Say,
    Think,
}

/// Look of the speech / thought bubble.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, clap::ValueEnum)]
pub enum Theme {
    /// Neon bubble with a `byanca` tag (default)
    #[default]
    Cyber,
    /// The original look: rounded box (cowsay borders in ASCII)
    Classic,
    /// Just a neon bar next to the text
    Minimal,
}

/// Tag shown in the top border of the cyber bubble.
const TAG_UNICODE: &str = " ✦ byanca ";
const TAG_ASCII: &str = "[ byanca ]";

fn text_width(lines: &[String]) -> usize {
    lines
        .iter()
        .map(|l| UnicodeWidthStr::width(l.as_str()))
        .max()
        .unwrap_or(0)
}

fn dash(n: usize, ch: char) -> String {
    std::iter::repeat_n(ch, n).collect()
}

/// Draws the bubble into its own canvas. `unicode` selects box drawing
/// characters (color mode) instead of plain ASCII.
pub fn bubble(lines: &[String], kind: BubbleKind, unicode: bool, color: bool, theme: Theme) -> Canvas {
    match theme {
        Theme::Classic => classic(lines, kind, unicode, color),
        Theme::Cyber => cyber(lines, kind, unicode, color),
        Theme::Minimal => minimal(lines, kind, unicode, color),
    }
}

fn classic(lines: &[String], kind: BubbleKind, unicode: bool, color: bool) -> Canvas {
    let w = text_width(lines) + 4;
    let h = lines.len() + 2;
    let mut c = Canvas::new(w, h);
    let border: Option<Rgb> = color.then_some(CYAN);
    let text: Option<Rgb> = color.then_some(LAVENDER);

    if unicode {
        let (tl, tr, bl, br, hz, vt) = match kind {
            BubbleKind::Say => ('╭', '╮', '╰', '╯', '─', '│'),
            BubbleKind::Think => ('╭', '╮', '╰', '╯', '┄', '┆'),
        };
        c.put_str(0, 0, &format!("{tl}{}{tr}", dash(w - 2, hz)), border);
        c.put_str(0, h - 1, &format!("{bl}{}{br}", dash(w - 2, hz)), border);
        for (i, l) in lines.iter().enumerate() {
            c.put_str(0, i + 1, &vt.to_string(), border);
            c.put_str(w - 1, i + 1, &vt.to_string(), border);
            c.put_str(2, i + 1, l, text);
        }
    } else {
        c.put_str(1, 0, &dash(w - 2, '_'), border);
        c.put_str(1, h - 1, &dash(w - 2, '-'), border);
        let n = lines.len();
        for (i, l) in lines.iter().enumerate() {
            let (a, b) = match kind {
                BubbleKind::Think => ('(', ')'),
                BubbleKind::Say if n == 1 => ('<', '>'),
                BubbleKind::Say if i == 0 => ('/', '\\'),
                BubbleKind::Say if i == n - 1 => ('\\', '/'),
                BubbleKind::Say => ('|', '|'),
            };
            c.put_str(0, i + 1, &a.to_string(), border);
            c.put_str(w - 1, i + 1, &b.to_string(), border);
            c.put_str(2, i + 1, l, text);
        }
    }
    c
}

/// Neon bubble: electric-blue frame, ice-blue corners and a `byanca` tag.
fn cyber(lines: &[String], kind: BubbleKind, unicode: bool, color: bool) -> Canvas {
    let tag = if unicode { TAG_UNICODE } else { TAG_ASCII };
    let tag_w = UnicodeWidthStr::width(tag);
    // wide enough for the tag: corner + 1 dash + tag + 1 dash + corner
    let w = (text_width(lines) + 4).max(tag_w + 4);
    let h = lines.len() + 2;
    let mut c = Canvas::new(w, h);
    let edge = color.then_some(ELECTRIC);
    let glow = color.then_some(ICE);
    let text = color.then_some(SILVER);
    let (tl, tr, bl, br, hz, vt_l, vt_r) = match (unicode, kind) {
        (true, BubbleKind::Say) => ('╭', '╮', '╰', '╯', '─', '│', '│'),
        (true, BubbleKind::Think) => ('╭', '╮', '╰', '╯', '┄', '┆', '┆'),
        (false, BubbleKind::Say) => ('.', '.', '\'', '\'', '-', '|', '|'),
        (false, BubbleKind::Think) => ('.', '.', '\'', '\'', '~', '(', ')'),
    };
    // top: corner, one dash, tag, dashes, corner
    c.put_str(0, 0, &tl.to_string(), glow);
    c.put_str(1, 0, &dash(w - 2, hz), edge);
    c.put_str(w - 1, 0, &tr.to_string(), glow);
    if unicode {
        let mut x = 2;
        for ch in tag.chars() {
            let fg = match ch {
                '✦' => color.then_some(PINK),
                _ => glow,
            };
            c.put_str(x, 0, &ch.to_string(), fg);
            x += ch.width().unwrap_or(0);
        }
    } else {
        c.put_str(2, 0, tag, glow);
    }
    // bottom
    c.put_str(0, h - 1, &bl.to_string(), glow);
    c.put_str(1, h - 1, &dash(w - 2, hz), edge);
    c.put_str(w - 1, h - 1, &br.to_string(), glow);
    for (i, l) in lines.iter().enumerate() {
        c.put_str(0, i + 1, &vt_l.to_string(), edge);
        c.put_str(w - 1, i + 1, &vt_r.to_string(), edge);
        c.put_str(2, i + 1, l, text);
    }
    c
}

/// No box at all: a neon bar on the left of the text.
fn minimal(lines: &[String], kind: BubbleKind, unicode: bool, color: bool) -> Canvas {
    let w = text_width(lines) + 2;
    let mut c = Canvas::new(w, lines.len());
    let bar = match (unicode, kind) {
        (true, BubbleKind::Say) => "▎",
        (true, BubbleKind::Think) => "┆",
        (false, BubbleKind::Say) => "|",
        (false, BubbleKind::Think) => ":",
    };
    for (i, l) in lines.iter().enumerate() {
        c.put_str(0, i, bar, color.then_some(ELECTRIC));
        c.put_str(2, i, l, color.then_some(SILVER));
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words() {
        assert_eq!(
            wrap("hello there general kenobi", 11),
            vec!["hello there", "general", "kenobi"]
        );
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("a\n\nb", 10), vec!["a", "", "b"]);
        assert_eq!(wrap("", 10), vec![""]);
        assert_eq!(wrap("no wrap at all here", 0), vec!["no wrap at all here"]);
    }

    fn text(c: &Canvas) -> Vec<String> {
        c.rows
            .iter()
            .map(|r| r.iter().map(|c| c.ch).filter(|&ch| ch != '\0').collect())
            .collect()
    }

    #[test]
    fn ascii_bubble_shape() {
        let c = bubble(&["hi".to_string()], BubbleKind::Say, false, false, Theme::Classic);
        assert_eq!(text(&c), vec![" ____ ", "< hi >", " ---- "]);
    }

    #[test]
    fn cyber_bubble_has_tag_and_fits_it() {
        let c = bubble(&["oi".to_string()], BubbleKind::Say, true, true, Theme::Cyber);
        let rows = text(&c);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].starts_with("╭─ ✦ byanca ─"));
        assert!(rows[0].ends_with('╮'));
        assert!(rows[1].starts_with("│ oi") && rows[1].ends_with('│'));
        assert!(rows[2].starts_with('╰') && rows[2].ends_with('╯'));
        let widths: Vec<usize> = rows.iter().map(|r| UnicodeWidthStr::width(r.as_str())).collect();
        assert!(widths.iter().all(|&w| w == widths[0]));
        // corners glow ice blue, edges are electric blue
        assert_eq!(c.rows[0][0].fg, Some(ICE));
        assert_eq!(c.rows[0][1].fg, Some(ELECTRIC));
    }

    #[test]
    fn cyber_ascii_is_plain_ascii() {
        let c = bubble(
            &["hello".to_string()],
            BubbleKind::Say,
            false,
            false,
            Theme::Cyber,
        );
        let rows = text(&c);
        assert_eq!(rows[0], ".-[ byanca ]-.");
        assert_eq!(rows[1], "| hello      |");
        assert_eq!(rows[2], "'------------'");
        assert!(rows.iter().all(|r| r.is_ascii()));
        assert!(c.rows.iter().flatten().all(|cell| cell.fg.is_none()));
    }

    #[test]
    fn minimal_has_no_box() {
        let lines = vec!["a".to_string(), "bc".to_string()];
        let c = bubble(&lines, BubbleKind::Think, false, false, Theme::Minimal);
        assert_eq!(text(&c), vec![": a ", ": bc"]);
    }
}
