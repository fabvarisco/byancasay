//! byancasay: cowsay, but it's Byanca, a cyberpunk neko.

mod anim;
mod art;
mod bubble;
mod canvas;
mod scene;

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::time::Duration;

use clap::builder::styling::{Ansi256Color, Color, Style as AnsiStyle, Styles};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::anim::Animation;
use crate::art::{Pose, Style};
use crate::bubble::{wrap, BubbleKind, Theme};
use crate::canvas::{render_row, ColorMode};
use crate::scene::Scene;

/// 256-color palette entries close to Byanca's neon: electric blue,
/// ice blue and lavender-silver (256 colors keep `--help` readable everywhere).
const HELP_BLUE: u8 = 33;
const HELP_ICE: u8 = 117;
const HELP_SILVER: u8 = 189;

fn fg(c: u8) -> AnsiStyle {
    AnsiStyle::new().fg_color(Some(Color::Ansi256(Ansi256Color(c))))
}

fn help_styles() -> Styles {
    Styles::styled()
        .header(fg(HELP_BLUE).bold().underline())
        .usage(fg(HELP_BLUE).bold())
        .literal(fg(HELP_ICE).bold())
        .placeholder(fg(HELP_SILVER))
        .valid(fg(HELP_ICE))
        .invalid(fg(203).bold())
        .error(fg(203).bold())
}

/// Footer of `--help`: examples plus a small neon badge. clap strips the
/// escapes when color is off (NO_COLOR, pipes, `--color never` for clap).
const AFTER_HELP: &str = "\x1b[1;4;38;5;33mExamples:\x1b[0m
  \x1b[1;38;5;117mbyancasay say\x1b[0m \"Hello, nya~\"
  fortune | \x1b[1;38;5;117mbyancasay say\x1b[0m -w 30
  \x1b[1;38;5;117mbyancasay dance\x1b[0m \"♪ nya nya ♪\"
  \x1b[1;38;5;117mbyancasay animate\x1b[0m --anim wag
  \x1b[1;38;5;117mbyancasay freestyle\x1b[0m --quotes
  \x1b[1;38;5;117mbyancasay say\x1b[0m --style minimal \"less is more\"
  \x1b[1;38;5;117mbyancasay say\x1b[0m --ascii \"plain ASCII works everywhere\"

\x1b[38;5;33m╭─\x1b[38;5;211m ✦ \x1b[38;5;117mbyancasay\x1b[38;5;33m ─ \x1b[38;5;189mneon neko mode\x1b[38;5;33m ─╮\x1b[0m
\x1b[38;5;33m╰─\x1b[38;5;189m =^.^=  \x1b[38;5;117mnya~ \x1b[38;5;33m──────────────────╯\x1b[0m";

#[derive(Parser, Debug)]
#[command(
    name = "byancasay",
    version,
    about = "cowsay, but it's Byanca: a cyberpunk neko for your terminal",
    styles = help_styles(),
    after_help = AFTER_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    look: Look,
}

#[derive(Args, Debug, Clone)]
struct Look {
    /// Use the hand-drawn plain ASCII Byanca (no color at all)
    #[arg(long, global = true)]
    ascii: bool,

    /// When to use color. `auto` detects truecolor support and honours NO_COLOR
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true)]
    color: ColorChoice,

    /// Bubble style: neon `cyber` (default), the original `classic`, or `minimal`
    #[arg(long, value_enum, default_value_t = Theme::Cyber, global = true)]
    style: Theme,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ColorChoice {
    /// Color on a terminal, truecolor when supported (256 colors otherwise)
    Auto,
    /// Always color, even when piped; depth is still auto-detected
    Always,
    /// Force 24-bit color
    Truecolor,
    /// Force the 256-color palette
    #[value(name = "256")]
    Ansi256,
    /// No color: falls back to the ASCII art
    Never,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Byanca says something (text from arguments or stdin)
    Say(SayArgs),
    /// Animate Byanca in place (blinking, waving, tail wag). Quit with q or Ctrl+C
    Animate(AnimateArgs),
    /// Keep switching between random animations (great for ricing)
    Freestyle(FreestyleArgs),
    /// Byanca dances! Shortcut for `animate --anim dance`
    #[command(visible_alias = "dancinha")]
    Dance(DanceArgs),
}

#[derive(Args, Debug)]
struct SayArgs {
    /// What to say. Reads stdin when omitted and stdin is piped
    text: Vec<String>,

    /// Wrap the text at this many columns (0 = no wrapping)
    #[arg(short, long, default_value_t = 40)]
    width: usize,

    /// Use a thought bubble instead of a speech bubble
    #[arg(short, long)]
    think: bool,
}

#[derive(Args, Debug)]
struct AnimateArgs {
    /// Optional text shown in a speech bubble during the animation
    text: Vec<String>,

    /// Which animation to play
    #[arg(short, long, visible_alias = "anim", value_enum, default_value_t = Animation::Wave)]
    animation: Animation,

    /// Frames per second
    #[arg(long, default_value_t = 8)]
    fps: u64,

    /// Stop after this many seconds (default: run until q / Ctrl+C)
    #[arg(short, long)]
    duration: Option<f64>,

    /// Wrap the bubble text at this many columns (0 = no wrapping)
    #[arg(short, long, default_value_t = 40)]
    width: usize,
}

#[derive(Args, Debug)]
struct DanceArgs {
    /// Optional text shown in a speech bubble while she dances
    text: Vec<String>,

    /// Frames per second (the dance loop is 8 frames, so 8 = one loop per second)
    #[arg(long, default_value_t = 8)]
    fps: u64,

    /// Stop after this many seconds (default: run until q / Ctrl+C)
    #[arg(short, long)]
    duration: Option<f64>,

    /// Wrap the bubble text at this many columns (0 = no wrapping)
    #[arg(short, long, default_value_t = 40)]
    width: usize,
}

impl From<DanceArgs> for AnimateArgs {
    fn from(d: DanceArgs) -> Self {
        AnimateArgs {
            text: d.text,
            animation: Animation::Dance,
            fps: d.fps,
            duration: d.duration,
            width: d.width,
        }
    }
}

#[derive(Args, Debug)]
struct FreestyleArgs {
    /// Seconds between animation switches
    #[arg(short, long, default_value_t = 5.0)]
    interval: f64,

    /// Frames per second
    #[arg(long, default_value_t = 8)]
    fps: u64,

    /// Stop after this many seconds (default: run until q / Ctrl+C)
    #[arg(short, long)]
    duration: Option<f64>,

    /// Say a random quote with every switch
    #[arg(short, long)]
    quotes: bool,
}

fn env_nonempty(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|v| !v.is_empty())
}

fn detect_truecolor() -> bool {
    let var = |k: &str| std::env::var(k).unwrap_or_default().to_lowercase();
    let colorterm = var("COLORTERM");
    if colorterm == "truecolor" || colorterm == "24bit" {
        return true;
    }
    if env_nonempty("WT_SESSION") {
        return true; // Windows Terminal (WSL)
    }
    let term = var("TERM");
    let term_program = var("TERM_PROGRAM");
    [
        "direct",
        "truecolor",
        "24bit",
        "kitty",
        "alacritty",
        "wezterm",
        "ghostty",
        "foot",
        "konsole",
    ]
    .iter()
    .any(|t| term.contains(t))
        || [
            "iterm.app",
            "wezterm",
            "vscode",
            "ghostty",
            "hyper",
            "tabby",
            "rio",
            "zed",
        ]
        .iter()
        .any(|t| term_program.contains(t))
}

fn color_mode(choice: ColorChoice) -> ColorMode {
    let depth = || {
        if detect_truecolor() {
            ColorMode::TrueColor
        } else {
            ColorMode::Ansi256
        }
    };
    match choice {
        ColorChoice::Never => ColorMode::None,
        ColorChoice::Truecolor => ColorMode::TrueColor,
        ColorChoice::Ansi256 => ColorMode::Ansi256,
        ColorChoice::Always => depth(),
        ColorChoice::Auto => {
            let dumb = std::env::var("TERM").is_ok_and(|t| t == "dumb");
            if env_nonempty("NO_COLOR") || dumb || !io::stdout().is_terminal() {
                ColorMode::None
            } else {
                depth()
            }
        }
    }
}

fn style_for(look: &Look, mode: ColorMode) -> (Style, ColorMode) {
    if look.ascii || mode == ColorMode::None {
        (Style::Ascii, ColorMode::None)
    } else {
        (Style::Chibi, mode)
    }
}

fn greeting() -> String {
    let mut rng = anim::Rng::seeded();
    anim::QUOTES[rng.below(anim::QUOTES.len() as u64) as usize].to_string()
}

fn say(look: &Look, args: SayArgs) -> io::Result<()> {
    let mut text = args.text.join(" ");
    if text.is_empty() && !io::stdin().is_terminal() {
        io::stdin().read_to_string(&mut text)?;
    }
    if text.trim().is_empty() {
        text = greeting();
    }
    let (style, mode) = style_for(look, color_mode(look.color));
    let lines = wrap(&text, args.width);
    let kind = if args.think {
        BubbleKind::Think
    } else {
        BubbleKind::Say
    };
    let scene = Scene {
        style,
        color: mode != ColorMode::None,
        text: Some(&lines),
        kind,
        theme: look.style,
    };
    let canvas = scene.draw(Pose::default());
    let mut out = io::BufWriter::new(io::stdout().lock());
    for row in &canvas.rows {
        writeln!(out, "{}", render_row(row, mode, usize::MAX, false))?;
    }
    out.flush()
}

fn secs(d: Option<f64>) -> Option<Duration> {
    d.filter(|s| s.is_finite() && *s >= 0.0)
        .map(Duration::from_secs_f64)
}

fn animate(look: &Look, args: AnimateArgs) -> io::Result<()> {
    let (style, mode) = style_for(look, color_mode_for_tty(look.color));
    let text = args.text.join(" ");
    let text = (!text.trim().is_empty()).then(|| wrap(&text, args.width));
    anim::run(anim::Options {
        style,
        color: mode != ColorMode::None,
        kind: BubbleKind::Say,
        theme: look.style,
        text,
        mode,
        fps: args.fps,
        duration: secs(args.duration),
        freestyle: None,
        quotes: false,
        start: args.animation,
    })
}

fn freestyle(look: &Look, args: FreestyleArgs) -> io::Result<()> {
    let (style, mode) = style_for(look, color_mode_for_tty(look.color));
    anim::run(anim::Options {
        style,
        color: mode != ColorMode::None,
        kind: BubbleKind::Say,
        theme: look.style,
        text: None,
        mode,
        fps: args.fps,
        duration: secs(args.duration),
        freestyle: Some(Duration::from_secs_f64(args.interval.clamp(0.5, 3600.0))),
        quotes: args.quotes,
        start: Animation::Wave,
    })
}

/// Animations always draw to a terminal, so `auto` only looks at NO_COLOR.
fn color_mode_for_tty(choice: ColorChoice) -> ColorMode {
    if choice == ColorChoice::Auto {
        if env_nonempty("NO_COLOR") {
            ColorMode::None
        } else {
            color_mode(ColorChoice::Always)
        }
    } else {
        color_mode(choice)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let animating = matches!(
        cli.command,
        Some(Command::Animate(_)) | Some(Command::Freestyle(_)) | Some(Command::Dance(_))
    );
    if animating && !io::stdout().is_terminal() {
        eprintln!("byancasay: animations need a terminal (stdout is not a TTY)");
        return ExitCode::FAILURE;
    }
    let result = match cli.command {
        Some(Command::Say(a)) => say(&cli.look, a),
        Some(Command::Animate(a)) => animate(&cli.look, a),
        Some(Command::Freestyle(a)) => freestyle(&cli.look, a),
        Some(Command::Dance(a)) => animate(&cli.look, a.into()),
        None => say(
            &cli.look,
            SayArgs {
                text: vec![],
                width: 40,
                think: false,
            },
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("byancasay: {e}");
            ExitCode::FAILURE
        }
    }
}
