#!/usr/bin/env python3
"""Render byancasay terminal output to PNG / animated GIF (for the README).

Needs Pillow and pyte (`pip install pillow pyte`).

  # static: render any ANSI text
  byancasay say --color truecolor "hi" > say.ans
  python3 tools/render_preview.py png say.ans docs/preview-say.png

  # animation: record with `script` timing, then replay into a GIF
  script -q -T anim.tm -c "byancasay animate -d 4" anim.log   # inside a 80x50 pty
  python3 tools/render_preview.py gif anim.log anim.tm docs/preview.gif --cols 80 --rows 50
"""
import argparse
import os

import pyte
from PIL import Image, ImageDraw, ImageFont

CW, CH = 10, 20
BG = (16, 16, 26)
FG = (208, 208, 255)
FONT_PATHS = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/run/current-system/sw/share/X11/fonts/DejaVuSansMono.ttf",
]
BASIC = {
    "black": (0, 0, 0), "red": (205, 49, 49), "green": (13, 188, 121), "brown": (229, 229, 16),
    "blue": (36, 114, 200), "magenta": (188, 63, 188), "cyan": (17, 168, 205), "white": (229, 229, 229),
}


def font():
    for p in FONT_PATHS:
        if os.path.exists(p):
            return ImageFont.truetype(p, 16)
    return ImageFont.load_default()


def color(c, default):
    if c == "default":
        return default
    if c in BASIC:
        return BASIC[c]
    try:
        return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))
    except ValueError:
        return default


def crop_box(screen):
    rows = [y for y in range(screen.lines)
            if any(screen.buffer[y][x].data != " " or screen.buffer[y][x].bg != "default"
                   for x in range(screen.columns))]
    cols = [x for x in range(screen.columns)
            if any(screen.buffer[y][x].data != " " or screen.buffer[y][x].bg != "default"
                   for y in range(screen.lines))]
    if not rows:
        return 0, 0, screen.columns, screen.lines
    return min(cols), min(rows), max(cols) + 1, max(rows) + 1


def render(screen, box, pad=2):
    x0, y0, x1, y1 = box
    w, h = (x1 - x0 + 2 * pad) * CW, (y1 - y0 + 2 * pad) * CH
    img = Image.new("RGB", (w, h), BG)
    d = ImageDraw.Draw(img)
    f = font()
    for y in range(y0, y1):
        for x in range(x0, x1):
            ch = screen.buffer[y][x]
            fg, bg = color(ch.fg, FG), color(ch.bg, BG)
            if ch.reverse:
                fg, bg = bg, fg
            px, py = (x - x0 + pad) * CW, (y - y0 + pad) * CH
            if bg != BG:
                d.rectangle([px, py, px + CW - 1, py + CH - 1], fill=bg)
            c = ch.data
            if c == "▀":
                d.rectangle([px, py, px + CW - 1, py + CH // 2 - 1], fill=fg)
            elif c == "▄":
                d.rectangle([px, py + CH // 2, px + CW - 1, py + CH - 1], fill=fg)
            elif c == "█":
                d.rectangle([px, py, px + CW - 1, py + CH - 1], fill=fg)
            elif c.strip():
                d.text((px, py + 1), c, font=f, fill=fg)
    return img


def new_screen(cols, rows):
    screen = pyte.Screen(cols, rows)
    screen.set_mode(pyte.modes.LNM)  # treat \n as \r\n like a cooked tty
    return screen, pyte.ByteStream(screen)


def cmd_png(a):
    data = open(a.input, "rb").read()
    rows = data.count(b"\n") + 2
    screen, stream = new_screen(a.cols, max(rows, 2))
    stream.feed(data)
    render(screen, crop_box(screen)).save(a.output)


def cmd_gif(a):
    data = open(a.input, "rb").read()
    # `script` logs start with a header line
    if data.startswith(b"Script started"):
        data = data[data.index(b"\n") + 1:]
    timing = []
    for line in open(a.timing):
        parts = line.split()
        if len(parts) >= 2:
            try:
                timing.append((float(parts[-2]), int(parts[-1])))
            except ValueError:
                pass
    screen, stream = new_screen(a.cols, a.rows)
    screen.reset_mode(pyte.modes.LNM)
    frames, t, pos, next_shot = [], 0.0, 0, a.skip
    step = 1.0 / a.fps
    for delay, n in timing:
        t += delay
        while t >= next_shot and pos > 0:
            frames.append([[(c.data, c.fg, c.bg) for c in [screen.buffer[y][x] for x in range(a.cols)]]
                           for y in range(a.rows)])
            next_shot += step
        stream.feed(data[pos:pos + n])
        pos += n
    if not frames:
        raise SystemExit("no frames captured")
    # common crop over all frames
    class Snap:
        def __init__(self, grid):
            self.columns, self.lines = a.cols, a.rows
            self.buffer = [[type("C", (), {"data": d, "fg": f, "bg": b, "reverse": False})()
                            for d, f, b in row] for row in grid]
    snaps = [Snap(g) for g in frames]
    boxes = [crop_box(s) for s in snaps]
    box = (min(b[0] for b in boxes), min(b[1] for b in boxes), max(b[2] for b in boxes), max(b[3] for b in boxes))
    imgs = [render(s, box) for s in snaps]
    if a.max_frames:
        imgs = imgs[: a.max_frames]
    imgs[0].save(a.output, save_all=True, append_images=imgs[1:], duration=int(step * 1000), loop=0,
                 optimize=True, disposal=1)
    if a.sheet:
        n = min(len(imgs), 8)
        sheet = Image.new("RGB", (imgs[0].width * n, imgs[0].height), BG)
        for i in range(n):
            sheet.paste(imgs[i], (i * imgs[0].width, 0))
        sheet.save(a.sheet)
    print(f"{len(imgs)} frames -> {a.output}")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("png")
    p.add_argument("input")
    p.add_argument("output")
    p.add_argument("--cols", type=int, default=120)
    p.set_defaults(fn=cmd_png)
    g = sub.add_parser("gif")
    g.add_argument("input")
    g.add_argument("timing")
    g.add_argument("output")
    g.add_argument("--cols", type=int, default=80)
    g.add_argument("--rows", type=int, default=50)
    g.add_argument("--fps", type=float, default=8)
    g.add_argument("--skip", type=float, default=0.3, help="seconds to skip at the start")
    g.add_argument("--max-frames", type=int, default=0)
    g.add_argument("--sheet", default=None, help="also write a contact sheet PNG of the first frames")
    g.set_defaults(fn=cmd_gif)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
