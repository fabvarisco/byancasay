//! Puts the bubble, its pointer and the artwork together.

use crate::art::{self, Pose, Style};
use crate::bubble::{bubble, BubbleKind, Theme};
use crate::canvas::{Canvas, CYAN, ELECTRIC, ICE};

pub struct Scene<'a> {
    pub style: Style,
    pub color: bool,
    pub text: Option<&'a [String]>,
    pub kind: BubbleKind,
    pub theme: Theme,
}

impl Scene<'_> {
    pub fn draw(&self, pose: Pose) -> Canvas {
        let art = art::render(self.style, pose, self.color);
        let Some(lines) = self.text else {
            return art;
        };
        let unicode = self.style == Style::Chibi;
        let b = bubble(lines, self.kind, unicode, self.color, self.theme);
        let p = self.style.anchor_for(pose).saturating_sub(2);
        let bx = p.saturating_sub(4.min(b.width.saturating_sub(2)));
        let width = (bx + b.width).max(art.width).max(p + 2);
        let mut c = Canvas::new(width, b.height() + 2 + art.height());
        c.blit(bx, 0, &b);
        let (c0, c1) = match (self.kind, unicode) {
            (BubbleKind::Say, true) => ("╲", "╲"),
            (BubbleKind::Say, false) => ("\\", "\\"),
            (BubbleKind::Think, true) => ("○", "∘"),
            (BubbleKind::Think, false) => ("o", "o"),
        };
        let (f0, f1) = match self.theme {
            Theme::Classic => (CYAN, CYAN),
            Theme::Cyber | Theme::Minimal => (ELECTRIC, ICE),
        };
        c.put_str(p, b.height(), c0, self.color.then_some(f0));
        c.put_str(p + 1, b.height() + 1, c1, self.color.then_some(f1));
        c.blit(0, b.height() + 2, &art);
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(c: &Canvas) -> Vec<String> {
        c.rows
            .iter()
            .map(|r| {
                let s: String = r.iter().map(|c| c.ch).filter(|&ch| ch != '\0').collect();
                s.trim_end().to_string()
            })
            .collect()
    }

    #[test]
    fn ascii_dance_scene_keeps_size_and_stays_ascii_art() {
        let lines = vec!["dance!".to_string()];
        let scene = Scene {
            style: Style::Ascii,
            color: false,
            text: Some(&lines),
            kind: BubbleKind::Say,
            theme: Theme::Cyber,
        };
        let sizes: Vec<(usize, usize)> = (0..art::DANCE_FRAMES)
            .map(|b| {
                let c = scene.draw(Pose {
                    dance: Some(b),
                    ..Pose::default()
                });
                (c.width, c.height())
            })
            .collect();
        assert!(sizes.iter().all(|s| *s == sizes[0]), "{sizes:?}");
        let c = scene.draw(Pose {
            dance: Some(0),
            ..Pose::default()
        });
        let rows = plain(&c);
        assert!(rows[1].contains("dance!"));
        assert!(c.rows.iter().flatten().all(|cell| cell.fg.is_none()));
    }

    #[test]
    fn chibi_dance_scene_is_stable() {
        let scene = Scene {
            style: Style::Chibi,
            color: true,
            text: None,
            kind: BubbleKind::Say,
            theme: Theme::Cyber,
        };
        let a = scene.draw(Pose {
            dance: Some(0),
            ..Pose::default()
        });
        for b in 1..art::DANCE_FRAMES * 3 {
            let c = scene.draw(Pose {
                dance: Some(b),
                ..Pose::default()
            });
            assert_eq!((c.width, c.height()), (a.width, a.height()));
        }
    }
}
