#!/usr/bin/env python3
"""Three-player Cadillacs and Dinosaurs cabinet from the supplied references.

Run: python3 tools/art/build_dino.py (requires Pillow).
Yellow T-rex decals dress a black-trim shell with three two-button stations.
The monitor is sampled from the assembled reference, not another game's art.
"""
from PIL import Image, ImageDraw
import build_mk2 as base
import build_snowbros as shell

ROOT = base.ROOT
REF = ROOT / "art/references/dino"
WIDTH, HEIGHT = 40, 52
YELLOW = (255, 183, 0)
BLACK = (18, 21, 25)
PLAYERS = (12.8, 8, 3.2)
BUTTONS = ((11.4, -1.2, (226, 39, 36)), (12.3, -2.2, (35, 112, 221)))


def side(sheet, box, polygon):
    # Continue the decal's yellow field through its cut-out, never the white page.
    mask = Image.new("L", sheet.size)
    ImageDraw.Draw(mask).polygon(polygon, fill=255)
    image = Image.new("RGB", sheet.size, YELLOW)
    image.paste(sheet, mask=mask)
    return image.crop(box)


def textures():
    sheet = Image.open(REF / "decals.png").convert("RGB")
    cabinet = Image.open(REF / "cabinet.png").convert("RGB")
    left = side(sheet, (61, 118, 393, 770),
                ((61, 224), (87, 172), (140, 133), (192, 118), (232, 118),
                 (248, 137), (235, 204), (237, 276), (257, 345), (289, 411),
                 (378, 411), (393, 425), (393, 468), (359, 513), (359, 758),
                 (345, 770), (61, 770)))
    right = side(sheet, (669, 118, 1001, 770),
                 ((1001, 224), (975, 172), (922, 133), (870, 118), (830, 118),
                  (814, 137), (827, 204), (825, 276), (805, 345), (773, 411),
                  (684, 411), (669, 425), (669, 468), (703, 513), (703, 758),
                  (717, 770), (1001, 770)))
    # Both decals face forward, without mirroring lettering on the opposite side.
    right = right.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    screen = cabinet.transform((80, 64), Image.Transform.QUAD,
                               (225, 258, 244, 415, 467, 385, 441, 241),
                               Image.Resampling.BICUBIC)
    images = {
        "left": left, "right": right,
        "marquee": sheet.crop((409, 24, 655, 101)),
        "bezel": sheet.crop((407, 103, 657, 350)),
        "controls": sheet.crop((398, 357, 666, 502)),
        "panel": sheet.crop((405, 509, 657, 607)),
        "front": sheet.crop((393, 868, 667, 1053)),
        "screen": screen,
    }
    sizes = {"left": (18, 48), "right": (18, 48), "marquee": (20, 6),
             "bezel": (20, 16), "controls": (24, 9), "panel": (24, 4),
             "front": (18, 16), "screen": (16, 10)}
    return {name: image.resize(sizes[name], Image.Resampling.BOX)
            for name, image in images.items()}


def model():
    # Wider than a two-player shell, with a continuous black edge and projecting deck.
    solids = [
        base.Solid("lower", 2, 11, 1, 15, 0, 16),
        base.Solid("upper", 2, 9, 2, 14, 16, 28, (((1, 0, .65), 20.75),)),
        base.Solid("hood", 2, 10.5, 2, 14, 26, 32, (((1, 0, -2), -45.5),)),
        base.Solid("deck", 8, 14, 0, 16, 15.5, 18.5, (((1, 0, 6), 122),)),
    ]
    for ymin, ymax in ((1, 2), (14, 15)):
        solids += [
            base.Solid("side", 2, 11, ymin, ymax, 0, 16),
            base.Solid("side", 2, 9.5, ymin, ymax, 16, 27, (((8, 0, 1), 95),)),
            base.Solid("side", 2, 10.5, ymin, ymax, 27, 32, (((1, 0, -2), -45.5),)),
        ]
    for y in PLAYERS:
        solids.append(base.Solid("stick", 10.4, 10.8, y - .2, y + .2, 17, 19.1))
        solids.append(base.Ball((10.6, y, 19.4), .7, BLACK))
        for x, dy, color in BUTTONS:
            z = (122 - x) / 6
            solids.append(base.Solid("button", x - .4, x + .4, y + dy - .4,
                                     y + dy + .4, z, z + .25, color=color))
    return solids


def paint(solid, p, n, tex):
    x, y, z = p
    nx, ny, nz = n
    part = solid.part
    if part == "side":
        if abs(ny) < .9 or x > base.side_front(z) - .65 or z > 31.35:
            return "trim", BLACK, False
        return "side art", base.sample(tex["left" if ny > 0 else "right"],
                                       (x - 2) / 9, (32 - z) / 32), False
    if part == "deck":
        if nz > .7:
            return "controls", base.sample(tex["controls"], (16 - y) / 16, (x - 8) / 6), False
        if nx > .9:
            if z > 17.65 or z < 15.8 or y < .5 or y > 15.5:
                return "trim", (48, 51, 56), False
            return "controls", base.sample(tex["panel"], (16 - y) / 16, (18 - z) / 2.5), False
        return "controls", BLACK, False
    if part == "hood" and nx > .9:
        if z > 31.4 or z < 28.6:
            return "trim", BLACK, False
        return "marquee", base.sample(tex["marquee"], (14 - y) / 12, (32 - z) / 4), True
    if part == "upper" and nx > .5:
        if 3.4 < y < 12.6 and 19.2 < z < 24.2:
            return "screen", base.sample(tex["screen"], (12.6 - y) / 9.2, (24.2 - z) / 5), True
        if z < 25.2:
            return "screen", base.sample(tex["bezel"], (14 - y) / 12, (25.2 - z) / 7), False
        return "screen", BLACK, False
    if part == "lower" and nx > .9:
        if 5 < y < 11 and 8.5 < z < 14:
            if y < 5.4 or y > 10.6 or z < 8.9 or z > 13.6:
                return "front", (48, 51, 56), False
            if 12 < z < 13 and (6 < y < 7 or 9 < y < 10):
                return "front", (240, 80, 24), False
            return "front", (12, 15, 18), False
        if 3 < y < 13 and 1 < z < 7:
            return "front", base.sample(tex["front"], (13 - y) / 10, (7 - z) / 6), False
        return "front", BLACK, False
    return base.paint(solid, p, n, tex)


def main():
    tex, solids, views = textures(), model(), []
    for turns, facing in enumerate(base.FACINGS):
        flat, layers = base.render(solids, tex, turns, painter=paint, width=WIDTH, height=HEIGHT)
        base.save_view(facing, flat, layers, prefix="cabinet_dino")
        views.append(flat)
    shell.preview(views, title="CADILLACS AND DINOSAURS / REFERENCE PASS",
                  subtitle="40 x 52 px | three players / yellow T-rex decals / black trim / red and blue buttons",
                  filename="dino.png")


if __name__ == "__main__":
    main()
