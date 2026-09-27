"""Cut the Results Manager application icon — Direction A, "The Seal".

A white ring enclosing a gold staircase, on an indigo squircle.

Two things this file is careful about, both learned by looking at the output:

1. The stair is drawn as a rectilinear RIBBON of filled rectangles, not as a
   stroked polyline. A round join at this stroke weight swallows the right
   angle, and the staircase turns into an S-squiggle at anything under 64px.
   Square miters are what make it read as steps.

2. Each size is cut separately, and below 32px the ring is DROPPED. A ring
   heavy enough to survive at 16px spends a quarter of the box on white and
   leaves the stair three pixels to live in; every version that kept it was a
   gold smudge. Small sizes get the stair alone, filling the tile.

Drawn at SS x supersample and reduced with LANCZOS, which is what gives clean
antialiasing without a vector rasteriser.
"""

import math
import os

import numpy as np
from PIL import Image, ImageDraw

SS = 16  # supersample factor

# Lifted from src/styles/tokens.css so the icon and the app cannot drift apart.
BRAND_500 = (0x4F, 0x63, 0xD2)
BRAND_800 = (0x2B, 0x33, 0x74)
GOLD = (0xE8, 0xB0, 0x44)
WHITE = (0xFF, 0xFF, 0xFF)

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "icons")

# Both stairs are centred on (32, 32) in the 64-unit design space, counting the
# ribbon thickness — so the glyph sits true inside the ring at every size.
STAIR_INSET = [(19, 40), (27.5, 40), (27.5, 32), (36.5, 32), (36.5, 24), (45, 24)]
STAIR_FULL = [(15, 44), (27, 44), (27, 32), (39, 32), (39, 20), (50, 20)]


def cut_for(px):
    """The weights and stair this pixel size should be cut at."""
    if px < 32:
        # No ring: it cannot survive here, and the stair alone still reads.
        return dict(ring_r=0.0, ring_w=0.0, t=9.0, stair=STAIR_FULL, radius=0.20)
    if px <= 40:
        return dict(ring_r=24.5, ring_w=6.5, t=6.5, stair=STAIR_INSET, radius=0.215)
    if px <= 72:
        return dict(ring_r=24.5, ring_w=6.0, t=6.5, stair=STAIR_INSET, radius=0.22)
    return dict(ring_r=24.5, ring_w=5.5, t=6.5, stair=STAIR_INSET, radius=0.223)


def gradient(size, top, bottom, angle_deg=145.0):
    """A CSS-equivalent linear-gradient(145deg, top, bottom) as an RGB array."""
    a = math.radians(angle_deg)
    dx, dy = math.sin(a), -math.cos(a)          # CSS 0deg points to the top
    length = abs(size * dx) + abs(size * dy)    # CSS gradient-line length
    ys, xs = np.mgrid[0:size, 0:size].astype(np.float64)
    proj = (xs - size / 2) * dx + (ys - size / 2) * dy
    t = np.clip(0.5 + proj / length, 0.0, 1.0)[..., None]
    return (np.array(top) * (1 - t) + np.array(bottom) * t).astype(np.uint8)


def ribbon(draw, points, t, fill):
    """A rectilinear polyline of thickness t, with square miters and butt ends.

    Each segment becomes a rectangle extended by t/2 past every interior
    vertex; two such rectangles meeting at a right angle fill the corner square
    exactly, which is what a miter join is. The two free ends are not extended.
    """
    half = t / 2
    last = len(points) - 2
    for i, (p, q) in enumerate(zip(points, points[1:])):
        (px, py), (qx, qy) = p, q
        start_ext = 0.0 if i == 0 else half
        end_ext = 0.0 if i == last else half
        vertical = px == qx

        if vertical:
            step = math.copysign(1.0, qy - py)
            y0, y1 = py - step * start_ext, qy + step * end_ext
            draw.rectangle((px - half, min(y0, y1), px + half, max(y0, y1)), fill=fill)
        else:
            step = math.copysign(1.0, qx - px)
            x0, x1 = px - step * start_ext, qx + step * end_ext
            draw.rectangle((min(x0, x1), py - half, max(x0, x1), py + half), fill=fill)


def render(px, flat=False):
    """One icon, cut for this pixel size."""
    cut = cut_for(px)
    n = px * SS
    scale = n / 64.0

    base = Image.fromarray(gradient(n, BRAND_500, BRAND_800), "RGB").convert("RGBA")

    # The squircle, as an alpha mask so the corners stay genuinely transparent.
    if not flat:
        mask = Image.new("L", (n, n), 0)
        ImageDraw.Draw(mask).rounded_rectangle(
            (0, 0, n - 1, n - 1), radius=cut["radius"] * n, fill=255
        )
        base.putalpha(mask)

    d = ImageDraw.Draw(base)

    # The ring of office, where there is room for one.
    if cut["ring_r"]:
        r = cut["ring_r"] * scale
        c = n / 2
        d.ellipse(
            (c - r, c - r, c + r, c + r),
            outline=WHITE + (255,),
            width=int(round(cut["ring_w"] * scale)),
        )

    # The rising stair inside it.
    pts = [(x * scale, y * scale) for x, y in cut["stair"]]
    ribbon(d, pts, cut["t"] * scale, GOLD + (255,))

    return base.resize((px, px), Image.LANCZOS)


def main():
    os.makedirs(OUT, exist_ok=True)

    for name, px in {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }.items():
        render(px).save(os.path.join(OUT, name), "PNG")
        print(f"{name}: {px}x{px}")

    # The .ico carries its own cut per size — Windows picks whichever it needs
    # for the taskbar, the title bar, Alt-Tab and Explorer, and they differ.
    ico_sizes = [256, 128, 64, 48, 32, 24, 16]
    layers = [render(s) for s in ico_sizes]
    layers[0].save(
        os.path.join(OUT, "icon.ico"),
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=layers[1:],
    )
    print("icon.ico:", ", ".join(f"{s}px" for s in ico_sizes))


if __name__ == "__main__":
    main()
