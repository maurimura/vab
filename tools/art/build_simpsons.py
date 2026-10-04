#!/usr/bin/env python3
"""Photo-referenced four-player Simpsons cabinet, 48x56 in four facings.

Run: python3 tools/art/build_simpsons.py (requires Pillow).
The clean decal sheet supplies both cyan family-stack sides, with yellow
T-molding from the side reference. The front photo supplies the marquee, character
deck, colored controls, and actual monitor image.
"""
from PIL import Image, ImageDraw, ImageEnhance
import build_mk2 as base
import build_snowbros as shell
import build_sunsetriders as wide

ROOT = base.ROOT
REF = ROOT / "art/references/simpsons"
WIDTH, HEIGHT = 48, 56
TRIM = (247, 212, 43)
BLACK = (18, 21, 25)
PLAYERS = ((15.5, (229, 51, 42)), (10.5, (34, 107, 222)),
           (5.5, (47, 178, 77)), (.5, (246, 205, 37)))


def rectify(image, corners, size):
    a, b, c, d = corners
    return image.transform(size, Image.Transform.QUAD, (*a, *d, *c, *b),
                           Image.Resampling.BICUBIC)


def side(sheet, box, polygon):
    """Keep the actual print, extending its cyan ramp outside the cut-out."""
    image = sheet.crop(box)
    mask = Image.new("L", image.size)
    points = [(x - box[0], y - box[1]) for x, y in polygon]
    ImageDraw.Draw(mask).polygon(points, fill=255)
    background = Image.new("RGB", image.size)
    draw = ImageDraw.Draw(background)
    for row in range(image.height):
        # The left decal's rear edge is clear cyan below its sloping top.
        color = sheet.getpixel((110, max(200, min(875, box[1] + row))))
        draw.line((0, row, image.width - 1, row), fill=color)
    background.paste(image, mask=mask)
    return background


def textures():
    front = Image.open(REF / "front.png").convert("RGB")
    sheet = Image.open(REF / "decals.png").convert("RGB")
    left = side(sheet, (99, 114, 498, 882),
                ((101, 188), (322, 115), (346, 120), (356, 137), (357, 236),
                 (350, 253), (263, 313), (381, 442), (402, 442), (402, 513),
                 (496, 516), (496, 880), (101, 880)))
    right = side(sheet, (518, 114, 917, 882),
                 ((915, 189), (697, 117), (673, 121), (659, 137), (659, 237),
                  (666, 253), (751, 315), (635, 443), (614, 444), (614, 513),
                  (520, 517), (520, 880), (915, 880)))
    # Rear-to-front depth UV is reversed on the opposite, outside-facing print.
    right = right.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    images = {
        "left": left, "right": right,
        "marquee": rectify(front, ((197, 62), (591, 60), (585, 169), (204, 170)), (160, 48)),
        "controls": rectify(front, ((140, 508), (626, 508), (688, 633), (94, 633)), (192, 64)),
        "screen": rectify(front, ((256, 322), (510, 322), (519, 453), (251, 453)), (96, 64)),
    }
    # Preserve the cyan print under the photographs' warm indoor light.
    sizes = {"left": (18, 54), "right": (18, 54),
             "marquee": (24, 6), "controls": (30, 10), "screen": (20, 10)}
    return {name: ImageEnhance.Color(image).enhance(1.2).resize(sizes[name], Image.Resampling.BOX)
            for name, image in images.items()}


def model():
    # The established four-player geometry, dressed with Simpsons materials only.
    solids = [s for s in wide.model() if s.part not in ("button", "stick", "upper")]
    solids.insert(1, base.Solid("upper", 2, 10, 1, 15, 17, 31, (((1, 0, .65), 23.75),)))
    for y, color in PLAYERS:
        solids.append(base.Solid("stick", 10.6, 11, y - .2, y + .2, 19.4, 22))
        solids.append(base.Ball((10.8, y, 22.25), .75, color))
        for x, dy in ((12.2, -1.2), (13.1, -2.0)):
            z = (148 - x) / 7
            solids.append(base.Solid("button", x - .4, x + .4, y + dy - .4,
                                     y + dy + .4, z, z + .3, color=color))
    return solids


def paint(solid, p, n, tex):
    x, y, z = p
    nx, ny, nz = n
    part = solid.part
    if part == "side":
        if abs(ny) < .9 or x > wide.side_front(z) - .65 or z > 35.35:
            return "trim", TRIM, False
        return "side art", base.sample(tex["left" if ny > 0 else "right"],
                                       (x - 2) / 9, (36 - z) / 36), False
    if part == "deck":
        if nz > .7:
            if y < -1.5 or y > 17.5 or x > 14.35:
                return "trim", TRIM, False
            return "controls", base.sample(tex["controls"], (18 - y) / 20, (x - 8) / 7), False
        if nx > .9:
            if z > 18.65 or z < 16.8 or y < -1.5 or y > 17.5:
                return "trim", TRIM, False
        return "controls", BLACK, False
    if part == "hood" and nx > .9:
        if z > 35.4 or z < 32.6:
            return "trim", (45, 49, 50), False
        return "marquee", base.sample(tex["marquee"], (15 - y) / 14, (36 - z) / 4), True
    if part == "upper" and nx > .5:
        if 2.3 < y < 13.7 and 21.1 < z < 27.7:
            return "screen", base.sample(tex["screen"], (13.7 - y) / 11.4, (27.7 - z) / 6.6), True
        # Two speaker grilles in the dark hood above the monitor.
        if 29 < z < 30.8 and (2.5 < y < 5.5 or 10.5 < y < 13.5):
            return "screen", (56, 58, 60) if int(z * 3) % 2 else BLACK, False
        return "screen", BLACK, False
    if part == "lower" and nx > .9:
        for center in (4.5, 11.5):
            if abs(y - center) < 2.7 and 3 < z < 13.5:
                border = abs(y - center) > 2.35 or z < 3.35 or z > 13.15
                if border:
                    return "front", (48, 51, 56), False
                if 11.4 < z < 12.4 and (abs(y - center - .8) < .35 or abs(y - center + .8) < .35):
                    return "front", (229, 42, 38), False
                if 10 < z < 10.5:
                    return "front", (211, 207, 183), False
                return "front", (12, 15, 18), False
        return "front", BLACK, False
    return base.paint(solid, p, n, tex)


def main():
    tex, solids, views = textures(), model(), []
    for turns, facing in enumerate(base.FACINGS):
        flat, layers = base.render(solids, tex, turns, painter=paint, width=WIDTH, height=HEIGHT)
        base.save_view(facing, flat, layers, prefix="cabinet_simpsons")
        views.append(flat)
    shell.preview(views, title="THE SIMPSONS / REFERENCE PASS",
                  subtitle="48 x 56 px | four players / cyan family-stack sides / yellow molding / twin coin doors",
                  filename="simpsons.png")


if __name__ == "__main__":
    main()
