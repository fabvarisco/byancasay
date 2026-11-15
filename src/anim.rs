//! In-place terminal animation (`animate` and `freestyle`).

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Print, ResetColor},
    terminal::{self, ClearType},
};

use crate::art::{Eyes, Pose, Style};
use crate::bubble::{BubbleKind, Theme};
use crate::canvas::{render_row, ColorMode};
use crate::scene::Scene;

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum Animation {
    /// Waving paw, swaying tail, blinking
    Wave,
    /// Happy tail wag, blinking
    Wag,
    /// Calm: slow tail, blinking
    Idle,
    /// Everything at once, as fast as possible
    Party,
    /// Dancing: swaying, bouncing, paws up on the beat, floating music notes
    Dance,
}

impl Animation {
    pub const ALL: [Animation; 5] = [
        Animation::Wave,
        Animation::Wag,
        Animation::Idle,
        Animation::Party,
        Animation::Dance,
    ];
}

/// Small xorshift PRNG; good enough for blinks and freestyle.
pub struct Rng(u64);

impl Rng {
    pub fn seeded() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        Rng(t ^ (std::process::id() as u64).rotate_left(32) | 1)
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// Turns a tick counter into poses.
pub struct Animator {
    pub anim: Animation,
    ascii: bool,
    talking: bool,
    fps: u64,
    tick: u64,
    blink_at: u64,
    rng: Rng,
}

const BLINK: [Eyes; 4] = [Eyes::Half, Eyes::Closed, Eyes::Closed, Eyes::Half];
const SWING: [i8; 4] = [0, 1, 0, -1];

impl Animator {
    pub fn new(anim: Animation, ascii: bool, talking: bool, fps: u64) -> Self {
        let mut rng = Rng::seeded();
        let fps = fps.max(1);
        let blink_at = fps + rng.below(fps * 2);
        Animator {
            anim,
            ascii,
            talking,
            fps,
            tick: 0,
            blink_at,
            rng,
        }
    }

    fn eyes(&mut self) -> Eyes {
        let t = self.tick;
        if t >= self.blink_at {
            let i = (t - self.blink_at) as usize;
            if i < BLINK.len() {
                let e = BLINK[i];
                // ASCII has no half-closed eyes
                return if self.ascii && e == Eyes::Half {
                    Eyes::Open
                } else {
                    e
                };
            }
            // next blink in 2..5 seconds, sometimes a quick double blink
            let gap = if self.rng.below(6) == 0 {
                2
            } else {
                self.fps * 2 + self.rng.below(self.fps * 3)
            };
            self.blink_at = t + gap;
        }
        if self.anim == Animation::Party && (t / 4) % 4 == 3 {
            return Eyes::Closed; // happy ^^ eyes on the beat
        }
        Eyes::Open
    }

    pub fn next_pose(&mut self) -> Pose {
        let t = self.tick;
        if self.anim == Animation::Dance {
            self.tick += 1;
            // the mouth sings along every other beat while there is a bubble
            let mouth_closed = self.talking && (t / 2) % 2 == 1;
            return Pose {
                mouth_closed,
                // 240 is a multiple of every note loop length, so it wraps cleanly
                dance: Some((t % 240) as u8),
                ..Pose::default()
            };
        }
        let eyes = self.eyes();
        let slow = |div: u64| SWING[((t / div) % 4) as usize];
        let fast = SWING[(t % 4) as usize];
        let (mut arm, tail) = match self.anim {
            Animation::Wave => (fast, slow(3)),
            Animation::Wag => (0, fast),
            Animation::Idle => (0, slow(5)),
            Animation::Party => (fast, -fast),
            Animation::Dance => (0, 0),
        };
        if self.ascii {
            // the ASCII drawing twitches its ears instead of waving
            let period = match self.anim {
                Animation::Wave | Animation::Party => 8,
                Animation::Wag => 16,
                Animation::Idle => 24,
                Animation::Dance => 8, // not reached: dance returns early
            };
            arm = match t % period {
                0 => -1,
                p if p == period / 2 => 1,
                _ => 0,
            };
        }
        let mouth_closed = if self.talking { (t / 2) % 2 == 1 } else { false };
        self.tick += 1;
        Pose {
            eyes,
            arm,
            tail,
            mouth_closed,
            dance: None,
        }
    }
}

struct TermGuard;

impl TermGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let g = TermGuard;
        execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::Clear(ClearType::All)
        )?;
        Ok(g)
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            ResetColor,
            cursor::Show,
            terminal::LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

/// What is currently on screen, so unchanged rows are not redrawn.
#[derive(Default)]
struct Layout {
    key: (u16, u16, usize, usize),
    prev: Vec<String>,
}

impl Layout {
    fn reset(&mut self) {
        self.key = (0, 0, 0, 0);
        self.prev.clear();
    }
}

pub struct Options {
    pub style: Style,
    pub color: bool,
    pub kind: BubbleKind,
    pub theme: Theme,
    pub text: Option<Vec<String>>,
    pub mode: ColorMode,
    pub fps: u64,
    pub duration: Option<Duration>,
    /// freestyle: switch to a random animation every `interval`
    pub freestyle: Option<Duration>,
    /// freestyle: show a random quote with each switch
    pub quotes: bool,
    pub start: Animation,
}

pub const QUOTES: &[&str] = &[
    "Nya~",
    "Hehe~",
    "HMPH!",
    "E-Eh?!",
    "Compiling... nya~",
    "Did you commit that?",
    "It works on my machine, nya",
    "Feed me cookies",
    "rm -rf boredom",
    "Neon mode: ON",
    "Have you tried turning it off and on again?",
    "git push --force? Bold, nya~",
];

pub fn run(mut opts: Options) -> io::Result<()> {
    let fps = opts.fps.clamp(1, 60);
    let frame = Duration::from_secs_f64(1.0 / fps as f64);
    let ascii = opts.style == Style::Ascii;
    let mut rng = Rng::seeded();
    if opts.freestyle.is_some() {
        opts.start = Animation::ALL[rng.below(Animation::ALL.len() as u64) as usize];
        if opts.quotes {
            opts.text = Some(vec![QUOTES[rng.below(QUOTES.len() as u64) as usize].to_string()]);
        }
    }
    let mut animator = Animator::new(opts.start, ascii, opts.text.is_some(), fps);
    let started = Instant::now();
    let mut switched = Instant::now();

    let _guard = TermGuard::enter()?;
    let mut out = io::BufWriter::new(io::stdout());
    let mut layout = Layout::default();

    loop {
        let tick_start = Instant::now();
        if opts.duration.is_some_and(|limit| started.elapsed() >= limit) {
            break;
        }
        if let Some(every) = opts.freestyle {
            if switched.elapsed() >= every {
                switched = Instant::now();
                let choices: Vec<Animation> = Animation::ALL
                    .iter()
                    .copied()
                    .filter(|a| *a != animator.anim)
                    .collect();
                let next = choices[rng.below(choices.len() as u64) as usize];
                if opts.quotes {
                    let q = QUOTES[rng.below(QUOTES.len() as u64) as usize];
                    opts.text = Some(vec![q.to_string()]);
                }
                animator = Animator::new(next, ascii, opts.text.is_some(), fps);
            }
        }
        let scene = Scene {
            style: opts.style,
            color: opts.color,
            text: opts.text.as_deref(),
            kind: opts.kind,
            theme: opts.theme,
        };
        draw(&mut out, &scene, animator.next_pose(), opts.mode, &mut layout)?;
        if wait_for_quit(frame.saturating_sub(tick_start.elapsed()), &mut layout)? {
            break;
        }
    }
    Ok(())
}

/// Waits up to `timeout` for input. Returns true when the user asked to quit.
fn wait_for_quit(timeout: Duration, layout: &mut Layout) -> io::Result<bool> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if !event::poll(left)? {
            return Ok(false);
        }
        match event::read()? {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                match k.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(true),
                    KeyCode::Char('c') if ctrl => return Ok(true),
                    _ => {}
                }
            }
            Event::Resize(..) => layout.reset(),
            _ => {}
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
    }
}

fn draw(
    out: &mut impl Write,
    scene: &Scene,
    pose: Pose,
    mode: ColorMode,
    layout: &mut Layout,
) -> io::Result<()> {
    let canvas = scene.draw(pose);
    let (cols, rows) = terminal::size().unwrap_or((80, 24));
    let now = (cols, rows, canvas.width, canvas.height());
    if layout.key != now {
        queue!(out, ResetColor, terminal::Clear(ClearType::All))?;
        layout.key = now;
        layout.prev.clear();
    }
    let x = (cols as usize).saturating_sub(canvas.width) / 2;
    let y = (rows as usize).saturating_sub(canvas.height()) / 2;
    let max_cols = (cols as usize).saturating_sub(x);
    let visible = (rows as usize).saturating_sub(y);
    layout.prev.resize(canvas.height(), String::new());
    for (i, row) in canvas.rows.iter().enumerate().take(visible) {
        let line = render_row(row, mode, max_cols, true);
        // only redraw rows that changed since the previous frame
        if layout.prev[i] != line {
            queue!(out, cursor::MoveTo(x as u16, (y + i) as u16), Print(&line))?;
            layout.prev[i] = line;
        }
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dance_cycles_through_all_frames() {
        let mut a = Animator::new(Animation::Dance, false, false, 8);
        let beats: Vec<u8> = (0..16).map(|_| a.next_pose().dance.unwrap()).collect();
        let frames: std::collections::BTreeSet<u8> =
            beats.iter().map(|b| b % crate::art::DANCE_FRAMES).collect();
        assert_eq!(frames.len(), crate::art::DANCE_FRAMES as usize);
    }

    #[test]
    fn dance_mouth_moves_only_when_talking() {
        let mut quiet = Animator::new(Animation::Dance, true, false, 8);
        assert!((0..8).all(|_| !quiet.next_pose().mouth_closed));
        let mut talking = Animator::new(Animation::Dance, true, true, 8);
        assert!((0..8).any(|_| talking.next_pose().mouth_closed));
    }

    #[test]
    fn other_animations_do_not_dance() {
        for anim in Animation::ALL.into_iter().filter(|a| *a != Animation::Dance) {
            let mut a = Animator::new(anim, false, false, 8);
            assert!((0..32).all(|_| a.next_pose().dance.is_none()));
        }
    }
}
