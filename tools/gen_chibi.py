#!/usr/bin/env python3
"""Regenerate the truecolor half-block chibi frames for byancasay.

Usage:
    python3 tools/gen_chibi.py SOURCE.png [--width 48] [--out assets/chibi.bin] [--preview DIR]

Requires Pillow and numpy (e.g. `nix shell nixpkgs#python3Packages.pillow
nixpkgs#python3Packages.numpy`).

Pipeline:
 1. Remove the white sticker border and every floating decoration (hearts)
    by flood-filling the "background or near-white" region from the image
    edges and keeping only the character's connected silhouette.
 2. Build animation variants at full resolution:
      eyes: open / half / closed (blink)
      arm:  -1 / 0 / +1 (waving paw, rotated around the shoulder)
      tail: -1 / 0 / +1 (tail wag, rotated around its base)
    plus an 8-frame dance loop (body sway around the feet, bounce with a
    little squash, paw pumping, tail swinging, happy eyes on the beat).
 3. Downscale each variant to WIDTH pixels wide (each terminal cell shows
    two vertical pixels with the upper/lower half block) and write them to
    a small binary file that the Rust binary embeds with include_bytes!.

Binary format (little endian):
    b"NYAF" u16 width u16 height u16 count
    count x (3 bytes name e.g. b"o0+" ; width*height*4 bytes RGBA)
    optional dance block:
    b"NYAD" u16 width u16 height u16 count u16 xoff
    count x (3 bytes name e.g. b"D3c" ; width*height*4 bytes RGBA)
    `xoff` is the column where the main frames' left edge sits inside a dance
    frame, so the speech bubble can point at the same spot.
"""
import argparse
import math
import os
import struct
from collections import deque

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

# Coordinates below are for the 512x512 byanca_chibi_oi.png sticker.
EYES = [  # (cx, cy, rx, ry)
    (210, 151, 17, 17),
    (268, 139, 16, 17),
]
LASH = (40, 30, 66)
SKIN = (252, 222, 220)
ARM_POLY = [(86, 98), (160, 98), (163, 140), (170, 175), (178, 212),
            (150, 220), (125, 217), (100, 186), (86, 152)]
ARM_PIVOT = (165, 214)
ARM_ANGLE = 12.0  # degrees per step
TAIL_POLY = [(305, 328), (338, 332), (362, 326), (384, 318), (388, 292),
             (425, 290), (425, 352), (395, 378), (330, 378), (300, 365)]
TAIL_PIVOT = (312, 350)
TAIL_ANGLE = 10.0
NEON = (24, 90, 200)  # rim light around dark edges

# Dance loop: 8 frames. Each frame is (tilt, arm, tail, dy, squash, eyes):
#   tilt   whole-body sway in degrees around the feet (positive = lean left)
#   arm    raised paw rotation in degrees around the shoulder
#   tail   tail rotation in degrees around its base
#   dy     vertical bounce in source pixels (positive = down)
#   squash vertical squash on the down beat (1.0 = none), widened to keep volume
#   eyes   "o" open, "h" half, "c" happy closed
FEET_PIVOT = (245, 492)
PAD = 140  # room around the 512x512 sticker so the sway never clips
DANCE = [
    (-6.0, 20.0, 14.0, 0, 1.0, "o"),
    (-4.0, 4.0, 10.0, 7, 0.955, "o"),
    (0.0, -16.0, 0.0, 0, 1.0, "c"),
    (4.0, 4.0, -10.0, 7, 0.955, "c"),
    (6.0, 20.0, -14.0, 0, 1.0, "o"),
    (4.0, 4.0, -10.0, 7, 0.955, "o"),
    (0.0, -16.0, 0.0, 0, 1.0, "c"),
    (-4.0, 4.0, 10.0, 7, 0.955, "h"),
]


def flood(mask, seeds):
    h, w = mask.shape
    seen = np.zeros_like(mask, bool)
    q = deque()
    for y, x in seeds:
        if mask[y, x] and not seen[y, x]:
            seen[y, x] = True
            q.append((y, x))
    while q:
        y, x = q.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < h and 0 <= nx < w and mask[ny, nx] and not seen[ny, nx]:
                seen[ny, nx] = True
                q.append((ny, nx))
    return seen


def clean(img):
    a = np.array(img.convert("RGBA")).astype(int)
    r, g, b, al = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
    h, w = al.shape
    white = (r > 225) & (g > 225) & (b > 225)
    bg = (al < 40) | white
    edges = ([(0, x) for x in range(w)] + [(h - 1, x) for x in range(w)]
             + [(y, 0) for y in range(h)] + [(y, w - 1) for y in range(h)])
    outside = flood(bg, edges)
    body = flood(~outside, [(h // 2, w // 2)])
    body = ~flood(~body, edges)  # fill holes
    m = Image.fromarray((body * 255).astype("uint8")).filter(ImageFilter.MinFilter(5))
    out = a.copy()
    out[..., 3] = np.array(m)
    return Image.fromarray(out.astype("uint8"), "RGBA")


def poly_mask(size, poly):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).polygon(poly, fill=255)
    return m


def rotate_part(img, poly, pivot, angle):
    if angle == 0:
        return img
    mask = poly_mask(img.size, poly)
    part = Image.new("RGBA", img.size, (0, 0, 0, 0))
    part.paste(img, (0, 0), mask)
    base = img.copy()
    hole = Image.new("RGBA", img.size, (0, 0, 0, 0))
    base.paste(hole, (0, 0), mask)
    part = part.rotate(angle, resample=Image.BICUBIC, center=pivot)
    base.alpha_composite(part)
    return base


def eyes(img, state):
    if state == "o":
        return img
    a = np.array(img).astype(int)
    h, w = a.shape[:2]
    yy, xx = np.mgrid[0:h, 0:w]
    out = img.copy()
    d = ImageDraw.Draw(out)
    for cx, cy, rx, ry in EYES:
        inside = ((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2 <= 1
        if state == "h":
            inside &= yy < cy + 2
        r, g, b = a[..., 0], a[..., 1], a[..., 2]
        hair = (r > 170) & (b > 200) & (b > r + 15) & (g > 150)
        paint = inside & ~hair
        arr = np.array(out)
        arr[paint] = SKIN + (255,)
        out = Image.fromarray(arr, "RGBA")
        d = ImageDraw.Draw(out)
        if state == "c":
            # gentle downward curve, like a relaxed closed eye
            pts = [(cx - rx + 2 + i, cy + 4 + 5 * math.sin(math.pi * i / (2 * rx - 4)))
                   for i in range(0, 2 * rx - 3)]
            d.line(pts, fill=LASH + (255,), width=8)
        else:
            # half-open: upper lid line across the middle of the iris
            pts = [(cx - rx + 1 + i, cy + 2 - 2 * math.sin(math.pi * i / (2 * rx - 2)))
                   for i in range(0, 2 * rx - 1)]
            d.line(pts, fill=LASH + (255,), width=7)
    return out


def despeckle(arr, min_size=6):
    """Drop tiny floating specks left over by the rotations."""
    op = arr[..., 3] > 0
    seen = np.zeros_like(op)
    h, w = op.shape
    for y in range(h):
        for x in range(w):
            if op[y, x] and not seen[y, x]:
                comp = flood(op & ~seen, [(y, x)])
                seen |= comp
                if comp.sum() < min_size:
                    arr[comp] = 0
    return arr


def rim(arr):
    """Give dark silhouette edges a thin neon rim so the black hoodie and
    tail stay readable on dark terminals."""
    arr = arr.astype(int)
    op = arr[..., 3] > 0
    lum = arr[..., :3].mean(axis=2)
    dark = op & (lum < 60)
    out = arr.copy()
    h, w = op.shape
    for y in range(h):
        for x in range(w):
            if not op[y, x]:
                continue
            if not dark[y, x]:
                continue
            edge = False
            for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                ny, nx = y + dy, x + dx
                if not (0 <= ny < h and 0 <= nx < w) or not op[ny, nx]:
                    edge = True
            if edge:
                out[y, x, :3] = [max(c, n) for c, n in zip(arr[y, x, :3], NEON)]
    return out


def dance_frame(base, spec):
    tilt, arm, tail, dy, squash, e = spec
    im = eyes(base, e)
    im = rotate_part(im, TAIL_POLY, TAIL_PIVOT, tail)
    im = rotate_part(im, ARM_POLY, ARM_PIVOT, arm)
    big = Image.new("RGBA", (im.width + 2 * PAD, im.height + 2 * PAD), (0, 0, 0, 0))
    big.paste(im, (PAD, PAD))
    fx, fy = FEET_PIVOT[0] + PAD, FEET_PIVOT[1] + PAD
    if squash != 1.0:
        sw, sh = round(big.width * (2.0 - squash)), round(big.height * squash)
        scaled = big.resize((sw, sh), Image.BICUBIC)
        # keep the feet planted: map the pivot back onto itself
        ox, oy = round(fx - fx * sw / big.width), round(fy - fy * sh / big.height)
        big = Image.new("RGBA", big.size, (0, 0, 0, 0))
        big.alpha_composite(scaled, (max(ox, 0), max(oy, 0)), (max(-ox, 0), max(-oy, 0)))
    big = big.rotate(tilt, resample=Image.BICUBIC, center=(fx, fy))
    if dy:
        moved = Image.new("RGBA", big.size, (0, 0, 0, 0))
        moved.paste(big, (0, dy))
        big = moved
    return big


def finish(im, box, w, h):
    small = im.crop(box).resize((w, h), Image.LANCZOS)
    arr = np.array(small)
    arr[..., 3] = np.where(arr[..., 3] >= 110, 255, 0)
    arr[arr[..., 3] == 0] = 0
    arr = despeckle(arr)
    return rim(arr)


def save_preview(arr, w, h, path):
    big = Image.fromarray(arr.astype("uint8"), "RGBA").resize((w * 8, h * 8), Image.NEAREST)
    bg = Image.new("RGBA", big.size, (24, 24, 32, 255))
    bg.alpha_composite(big)
    bg.save(path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source")
    ap.add_argument("--width", type=int, default=48)
    ap.add_argument("--out", default="assets/chibi.bin")
    ap.add_argument("--preview", default=None, help="write PNG previews here")
    args = ap.parse_args()

    base = clean(Image.open(args.source))
    variants = {}
    for e in "ohc":
        for arm in (-1, 0, 1):
            for tail in (-1, 0, 1):
                im = eyes(base, e)
                im = rotate_part(im, TAIL_POLY, TAIL_PIVOT, -TAIL_ANGLE * tail)
                im = rotate_part(im, ARM_POLY, ARM_PIVOT, -ARM_ANGLE * arm)
                name = e + "-0+"[arm + 1] + "-0+"[tail + 1]
                variants[name] = im

    # common crop box over all variants so frames line up
    boxes = [v.getbbox() for v in variants.values()]
    x0 = min(b[0] for b in boxes) - 2
    y0 = min(b[1] for b in boxes) - 2
    x1 = max(b[2] for b in boxes) + 2
    y1 = max(b[3] for b in boxes) + 2
    w = args.width
    h = round((y1 - y0) * w / (x1 - x0))
    h += h % 2
    blob = bytearray(b"NYAF" + struct.pack("<HHH", w, h, len(variants)))
    if args.preview:
        os.makedirs(args.preview, exist_ok=True)
    for name, im in variants.items():
        arr = finish(im, (x0, y0, x1, y1), w, h)
        blob += name.encode() + arr.astype("uint8").tobytes()
        if args.preview:
            save_preview(arr, w, h, os.path.join(args.preview, f"{name}.png"))
            if name == "o00":
                save_preview(arr, w, h, os.path.join(args.preview, "_base.png"))

    # dance block: same scale as the main frames, bigger box for the sway
    scale = w / (x1 - x0)
    dance = [dance_frame(base, spec) for spec in DANCE]
    dboxes = [d.getbbox() for d in dance]
    dx0 = min(b[0] for b in dboxes) - 2
    dy0 = min(b[1] for b in dboxes) - 2
    dx1 = max(b[2] for b in dboxes) + 2
    dy1 = max(b[3] for b in dboxes) + 2
    dw = round((dx1 - dx0) * scale)
    dw += dw % 2
    dh = round((dy1 - dy0) * scale)
    dh += dh % 2
    dx1, dy1 = dx0 + dw / scale, dy0 + dh / scale
    # column of the main frames' left edge inside the dance frames (bubble anchor)
    xoff = max(0, round((x0 + PAD - dx0) * scale))
    blob += b"NYAD" + struct.pack("<HHHH", dw, dh, len(dance), xoff)
    for i, (im, spec) in enumerate(zip(dance, DANCE)):
        arr = finish(im, (dx0, dy0, dx1, dy1), dw, dh)
        name = f"D{i}{spec[5]}"
        blob += name.encode() + arr.astype("uint8").tobytes()
        if args.preview:
            save_preview(arr, dw, dh, os.path.join(args.preview, f"{name}.png"))
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "wb") as f:
        f.write(blob)
    print(f"wrote {args.out}: {w}x{h} px ({w}x{h // 2} cells), {len(variants)} frames"
          f" + {len(dance)} dance frames at {dw}x{dh} px (x offset {xoff})")


if __name__ == "__main__":
    main()
