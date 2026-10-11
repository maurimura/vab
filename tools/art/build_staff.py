#!/usr/bin/env python3
"""The bar's staff (client/src/staff.rs), dressed like players (tools/art/build_avatar.py).

Run: python3 tools/art/build_staff.py (requires Pillow).
Writes assets/characters/<job>_<name>.png: five 24x32 frames side by side, standing, two
walking and two working (wiping the counter, mopping), each a 16x32 character dressed in the
avatar catalog's layers in the middle, feet on the bottom row's shadow, with room either side for
the arm, the rag and the mop. A drawn sheet laid out the same replaces any of them. Also the arrow
that shows the way when they point somewhere (client/src/talk/guide.rs), white to be tinted,
pointing right.
"""
import json
from pathlib import Path

from PIL import Image

import build_avatar as avatar

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets/characters"
WIDTH, HEIGHT = 24, 32
FRAMES = 5
# Where the 16 wide character sits in a frame.
LEFT = (WIDTH - avatar.WIDTH) // 2

OUTLINE = (20, 18, 28, 255)
SHADOW = (0, 0, 0, 90)
RAG = [(238, 238, 232, 255), (110, 160, 214, 255)]
HANDLE = (150, 100, 52, 255)
MOP = [(226, 220, 196, 255), (176, 170, 150, 255)]

# Who's on the staff, in the order client/src/staff.rs lists them: a look for each, as
# {layer: (item, [colours])} from assets/avatar/catalog.json.
PEOPLE = [
    ("bartender", "rosa", {
        "body": ("body", ["#e0ac69"]), "face": ("smile", []), "hair": ("bun", ["#34201a"]),
        "top": ("shirt", ["#f4f4f0"]), "jacket": ("vest", ["#1e1f26"]),
        "legs": ("trousers", ["#1e1f26"]), "shoes": ("sneakers", ["#1e1f26"])}),
    ("bartender", "lou", {
        "body": ("body", ["#f1c27d"]), "face": ("calm", []), "hair": ("buzz", ["#9a9aa0"]),
        "beard": ("mustache", ["#9a9aa0"]), "top": ("shirt", ["#f4f4f0"]),
        "jacket": ("vest", ["#6e2028"]), "legs": ("trousers", ["#3a2e26"]),
        "shoes": ("boots", ["#1e1f26"])}),
    ("bartender", "kenji", {
        "body": ("body", ["#e8be8c"]), "face": ("grin", []), "hair": ("spiky", ["#17130f"]),
        "top": ("shirt", ["#f4f4f0"]), "jacket": ("vest", ["#225c3e"]),
        "legs": ("trousers", ["#1e1f26"]), "shoes": ("sneakers", ["#1e1f26"])}),
    ("cleaner", "doris", {
        "body": ("body", ["#c6865a"]), "face": ("smile", []), "hair": ("curly", ["#b8b8c2"]),
        "glasses": ("round", ["#1e1f26"]), "top": ("longsleeve", ["#3e70ac"]),
        "legs": ("trousers", ["#3e70ac"]), "shoes": ("sneakers", ["#f4f4f0"])}),
    ("cleaner", "sal", {
        "body": ("body", ["#8d5524"]), "face": ("determined", []), "hair": ("short", ["#463219"]),
        "hat": ("cap", ["#ce7c34", "#96561e"]), "top": ("polo", ["#767e8c"]),
        "legs": ("cargo", ["#767e8c", "#5a606c"]), "shoes": ("boots", ["#4a3524"])}),
    ("cleaner", "marta", {
        "body": ("body", ["#f1c27d"]), "face": ("wink", []), "hair": ("long", ["#281e1a"]),
        "hat": ("headband", ["#be343e"]), "top": ("longsleeve", ["#48845c"]),
        "legs": ("trousers", ["#48845c"]), "shoes": ("sneakers", ["#1e1f26"])}),
]


def skin(look):
    return (*avatar.ramp(look["body"][1][0])[1], 255)


def lift(frame, xs):
    """One foot a row up: the lower leg in columns xs, rows 25-30 of the character."""
    lifted = frame.copy()
    for x in xs:
        for y in range(25, 31):
            lifted.putpixel((LEFT + x, y - 1), frame.getpixel((LEFT + x, y)))
        lifted.putpixel((LEFT + x, 30), (0, 0, 0, 0))
    return lifted


def wipe(frame, look, reach):
    """The right arm out over the counter, a rag under the hand; `reach` columns past the body."""
    out = frame.copy()
    sleeve = frame.getpixel((LEFT + 13, 16))
    # The arm as it hung, gone, and the body's side drawn in.
    for y in range(16, 22):
        for x in range(12, 15):
            out.putpixel((LEFT + x, y), (0, 0, 0, 0))
        out.putpixel((LEFT + 12, y), OUTLINE)
    end = 14 + reach
    for x in range(12, end + 1):
        out.putpixel((LEFT + x, 15), OUTLINE)
        out.putpixel((LEFT + x, 16), sleeve if x < end else skin(look))
        out.putpixel((LEFT + x, 17), OUTLINE)
    out.putpixel((LEFT + end + 1, 16), OUTLINE)
    for x in range(end - 2, end + 2):
        for y in (18, 19):
            out.putpixel((LEFT + x, y), RAG[(x + y) % 2])
    return out


def line(a, b):
    """The pixels from a to b (Bresenham)."""
    (x0, y0), (x1, y1) = a, b
    dx, dy = abs(x1 - x0), -abs(y1 - y0)
    sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
    err, points = dx + dy, []
    while True:
        points.append((x0, y0))
        if (x0, y0) == (x1, y1):
            return points
        e2 = 2 * err
        if e2 >= dy:
            err, x0 = err + dy, x0 + sx
        if e2 <= dx:
            err, y0 = err + dx, y0 + sy


def mop(frame, top, head):
    """A mop behind the character from `top` to its head on the floor at column `head`, both in
    frame pixels."""
    out = Image.new("RGBA", frame.size)
    for point in line(top, (head, 29)):
        if 0 <= point[0] < WIDTH:
            out.putpixel(point, HANDLE)
    for dx in range(-2, 3):
        for y, colour in ((29, MOP[0]), (30, MOP[dx % 2])):
            if 0 <= head + dx < WIDTH:
                out.putpixel((head + dx, y), colour)
        if 0 <= head + dx < WIDTH:
            out.putpixel((head + dx, 31), SHADOW)
    for x in (head - 3, head + 3):
        for y in (29, 30):
            if 0 <= x < WIDTH:
                out.putpixel((x, y), OUTLINE)
    out.alpha_composite(frame)
    return out


def sheet(job, look, catalog):
    body = Image.new("RGBA", (WIDTH, HEIGHT))
    body.alpha_composite(avatar.compose(catalog, look), (LEFT, 0))
    walking = [lift(body, range(8, 14)), lift(body, range(2, 8))]
    if job == "bartender":
        frames = [body, *walking, wipe(body, look, 3), wipe(body, look, 4)]
    else:
        hand = skin(look)
        # Upright in the right hand while standing and walking; both hands on it, swept out to
        # one side and back, working.
        upright = [mop(frame, (LEFT + 16, 6), LEFT + 16) for frame in (body, *walking)]
        for frame in upright:
            frame.putpixel((LEFT + 15, 19), hand)
            frame.putpixel((LEFT + 16, 19), hand)
        working = []
        for head in (WIDTH - 2, 1):
            frame = mop(body, (LEFT + 8, 12), head)
            for point in line((LEFT + 8, 12), (head, 29))[5:9:3]:
                frame.putpixel(point, hand)
            working.append(frame)
        frames = [*upright, *working]
    image = Image.new("RGBA", (WIDTH * FRAMES, HEIGHT))
    for index, frame in enumerate(frames):
        image.alpha_composite(frame, (index * WIDTH, 0))
    return image


ARROW = [
    "......o.....",
    "......oo....",
    "......owo...",
    "oooooooowo..",
    "owwwwwwwwwo.",
    "owwwwwwwwwwo",
    "owwwwwwwwwo.",
    "oooooooowo..",
    "......owo...",
    "......oo....",
    "......o.....",
]


def arrow():
    colors = {"o": OUTLINE, "w": (255, 255, 255, 255)}
    image = Image.new("RGBA", (len(ARROW[0]), len(ARROW)))
    for y, row in enumerate(ARROW):
        for x, ch in enumerate(row):
            if ch != ".":
                image.putpixel((x, y), colors[ch])
    return image


def main():
    catalog = json.loads((avatar.OUT / "catalog.json").read_text())
    for job, name, look in PEOPLE:
        sheet(job, look, catalog).save(OUT / f"{job}_{name}.png")
        print(f"assets/characters/{job}_{name}.png")
    (ROOT / "assets/ui").mkdir(exist_ok=True)
    arrow().save(ROOT / "assets/ui/guide_arrow.png")
    print("assets/ui/guide_arrow.png")


if __name__ == "__main__":
    main()
