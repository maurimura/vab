#!/usr/bin/env python3
"""The avatar's items, drawn pixel by pixel: one 16x32 PNG per item under assets/avatar/.

Run: python3 tools/art/build_avatar.py (requires Pillow).

Each item is a layer of the same frame as the body, feet at the bottom middle. What a player
picks recolours it: pixels in a colour slot's reserved greys (KEYS, also in
assets/avatar/catalog.json) become three shades of the colour they chose, everything else is
drawn as it is. The page puts the layers together (web/avatar.js); so does `compose` below, for
the default character (assets/characters/player.png) and a preview sheet of every item
(art/previews/avatar.png).

In the drawings below, `.` is clear, `o` the outline, `#` and `%` fill colour slots 1 and 2 (lit
on a run's left, shaded on its right), `1 2 3` and `4 5 6` are slot 1's and slot 2's light, mid
and dark shades by hand, and other letters are fixed colours (COLOURS).
"""
import colorsys
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets/avatar"
WIDTH, HEIGHT = 16, 32

# A colour slot's light, mid and dark shades, as drawn.
KEYS = {
    1: ((224, 224, 224), (168, 168, 168), (112, 112, 112)),
    2: ((200, 200, 200), (144, 144, 144), (88, 88, 88)),
}
COLOURS = {
    "o": (20, 18, 28, 255),  # outline, as the rest of the bar's art
    "e": (26, 22, 30, 255),  # eyes
    "w": (246, 246, 240, 255),  # white: teeth, soles, collars
    "k": (40, 40, 48, 255),  # near black: lenses, mouths
    "l": (255, 255, 255, 150),  # a lens's shine
    "g": (30, 30, 40, 215),  # a dark lens
    "m": (110, 40, 40, 150),  # mouth, over any skin
    "M": (70, 20, 24, 255),  # an open mouth
    "b": (240, 90, 100, 90),  # blush
    "f": (120, 60, 30, 110),  # freckles
    "d": (30, 20, 20, 110),  # brows, over any skin
    "s": (0, 0, 0, 80),  # the shadow under the feet
    "y": (240, 196, 64, 255),  # gold: buttons, buckles, a crown
    "Y": (186, 132, 36, 255),  # gold's shade
    "r": (220, 50, 60, 255),  # a crown's ruby
    "a": (160, 168, 180, 255),  # steel: zips
}
SHADE = {"1": (1, 0), "2": (1, 1), "3": (1, 2), "4": (2, 0), "5": (2, 1), "6": (2, 2)}
# Slot 1's mid shade, see-through: `,` faintly (stubble), `~` more (a visor).
SEE_THROUGH = {",": (1, 1, 110), "~": (1, 1, 170)}


def blank():
    return ["." * WIDTH] * HEIGHT


def grid(top, rows):
    """A drawing whose first row is row `top` of the frame."""
    frame = blank()
    for offset, row in enumerate(rows):
        assert len(row) == WIDTH, f"row {top + offset} is {len(row)} wide: {row!r}"
        frame[top + offset] = row
    return frame


def over(*drawings):
    """Drawings on top of each other, later ones winning where they aren't clear."""
    frame = [list(row) for row in blank()]
    for drawing in drawings:
        for y, row in enumerate(drawing):
            for x, c in enumerate(row):
                if c != ".":
                    frame[y][x] = c
    return ["".join(row) for row in frame]


def shade_runs(row):
    """`#`/`%` runs lit on their left and shaded on their right; a 2-pixel run (an arm) is
    lit or shaded by the side of the body it's on."""
    out = list(row)
    x = 0
    while x < len(row):
        c = row[x]
        if c not in "#%":
            x += 1
            continue
        end = x
        while end < len(row) and row[end] == c:
            end += 1
        light, mid, dark = ("1", "2", "3") if c == "#" else ("4", "5", "6")
        run = end - x
        for i in range(x, end):
            out[i] = mid
        if run >= 3:
            out[x], out[end - 1] = light, dark
        elif run == 2:
            if x + 1 < WIDTH / 2:
                out[x] = light
            else:
                out[end - 1] = dark
        x = end
    return "".join(out)


def paint(drawing):
    image = Image.new("RGBA", (WIDTH, HEIGHT))
    for y, row in enumerate(drawing):
        for x, c in enumerate(shade_runs(row)):
            if c == ".":
                continue
            if c in SHADE:
                slot, shade = SHADE[c]
                image.putpixel((x, y), (*KEYS[slot][shade], 255))
            elif c in SEE_THROUGH:
                slot, shade, alpha = SEE_THROUGH[c]
                image.putpixel((x, y), (*KEYS[slot][shade], alpha))
            else:
                image.putpixel((x, y), COLOURS[c])
    return image


# The body: skin in slot 1, with the shadow under the feet. Everything else is drawn on it.
BODY = grid(0, [
    "................",
    "................",
    "....oooooooo....",
    "...o########o...",
    "..o##########o..",
    "..o##########o..",
    "..o##########o..",
    ".o############o.",
    ".o############o.",
    "..o##########o..",
    "..o##########o..",
    "...o32222223o...",
    "....ooo33ooo....",
    "...oooo33oooo...",
    "..o##########o..",
    ".o############o.",
    ".o############o.",
    ".o############o.",
    ".o############o.",
    ".o##o######o##o.",
    ".o##o######o##o.",
    "..oo########oo..",
    "...o###oo###o...",
    "...o###oo###o...",
    "...o###oo###o...",
    "...o###oo###o...",
    "...o###oo###o...",
    "...o###oo###o...",
    "...o###oo###o...",
    "..o####oo####o..",
    "...oooo..oooo...",
    "....ssssssss....",
])

# Faces: eyes on rows 7-8 (columns 5 and 10), the mouth on 9-10.
EYES = grid(7, [".....e....e.....", ".....e....e....."])
FACES = {
    "smile": over(EYES, grid(9, ["......m..m......", ".......mm......."])),
    "calm": over(EYES, grid(6, ["....dd....dd....", "................", "................",
                                "................", ".......mm......."])),
    "grin": over(EYES, grid(9, ["......MwwM......", ".......MM......."])),
    "wink": over(grid(7, ["..........e.....", "....ee....e....."]),
                 grid(9, ["......m..m......", ".......mm......."])),
    "surprised": over(EYES, grid(5, ["....d......d....", "................", "................",
                                     "................", ".......MM.......", ".......MM......."])),
    "sleepy": grid(6, ["....dd....dd....", "................", "....ee....ee....",
                       "................", ".......mm......."]),
    "determined": over(EYES, grid(5, ["....d......d....", ".....d....d.....", "................",
                                      "................", "................",
                                      "......mmmm......"])),
    "blush": over(EYES, grid(9, ["...bbm..m.bb....", ".......mm......."])),
    "freckles": over(EYES, grid(9, ["...fmf..fmf.....", ".......mm......."])),
}

# Facial hair, in the hair colours.
BEARDS = {
    "stubble": grid(9, [
        "...,,......,,...",
        "...,,,....,,,...",
        "....,,,,,,,,....",
        "......,,,,......",
    ]),
    "mustache": grid(9, [
        "......####......",
        ".....#....#.....",
    ]),
    "goatee": grid(9, [
        "......#..#......",
        "................",
        "......####......",
        "......o##o......",
        ".......oo.......",
    ]),
    "beard": grid(7, [
        "...#........#...",
        "...##......##...",
        "...###.##.###...",
        "...###....###...",
        "...o########o...",
        "....o######o....",
        ".....oooooo.....",
    ]),
}

# Hair, over the top of the head.
HAIR = {
    "short": grid(1, [
        ".....oooooo.....",
        "...oo######oo...",
        "..o##########o..",
        "..o##########o..",
        "..o#33#33#33#o..",
        "..o#3......3#o..",
    ]),
    "buzz": grid(2, [
        "....oooooooo....",
        "...o#3#3#3#3o...",
        "..o3#3#3#3#3#o..",
        "..o3........3o..",
    ]),
    "parted": grid(1, [
        "....ooooooo.....",
        "...o#####o##o...",
        "..o#####3####o..",
        "..o####3#####o..",
        "..o###3.3####o..",
        "..o##3....3##o..",
    ]),
    "spiky": grid(0, [
        "...o..o..o..o...",
        "..o#oo#oo#oo#o..",
        "..o##########o..",
        ".o############o.",
        "..o##########o..",
        "..o#3#33#33#3o..",
        "..o3........3o..",
    ]),
    "mohawk": grid(0, [
        "......oooo......",
        ".....o####o.....",
        ".....o####o.....",
        ".....o####o.....",
        "......o33o......",
    ]),
    "curly": grid(0, [
        "....oo.oo.oo....",
        "...o##o##o##o...",
        "..o#3#3#3#3#3o..",
        ".o#3#3#3#3#3#3o.",
        ".o3#3#3#3#3#3#o.",
        ".o#3#......3#3o.",
        ".o3#........#3o.",
    ]),
    "afro": grid(0, [
        "...oooooooooo...",
        "..o##########o..",
        ".o############o.",
        "o##############o",
        "o##############o",
        "o##3333333333##o",
        "o###........###o",
        "o##3........3##o",
        ".o#3........3#o.",
        "..oo........oo..",
    ]),
    "bob": grid(1, [
        ".....oooooo.....",
        "...oo######oo...",
        "..o##########o..",
        ".o############o.",
        ".o##33333333##o.",
        ".o##3......3##o.",
        ".o##........##o.",
        ".o##........##o.",
        ".o#3........3#o.",
        ".o#3........3#o.",
        "..oo........oo..",
    ]),
    "long": grid(1, [
        ".....oooooo.....",
        "...oo######oo...",
        "..o##########o..",
        ".o############o.",
        ".o##3#3333#3##o.",
        ".o##3......3##o.",
        ".o##........##o.",
        ".o##........##o.",
        ".o##........##o.",
        ".o##........##o.",
        ".o##........##o.",
        ".o##........##o.",
        ".o##o......o##o.",
        ".o##o......o##o.",
        ".o##o......o##o.",
        ".o#3o......o3#o.",
        "..oo........oo..",
    ]),
    "bun": grid(0, [
        "......oooo......",
        ".....o####o.....",
        "...ooo####ooo...",
        "..o##########o..",
        "..o##########o..",
        "..o##3333333#o..",
        "..o#3......3#o..",
    ]),
    "pigtails": grid(1, [
        ".....oooooo.....",
        "...oo######oo...",
        "..o##########o..",
        "..o##########o..",
        "..o#33#33#33#o..",
        ".oo#3......3#oo.",
        "o##o........o##o",
        "o##o........o##o",
        "o##o........o##o",
        "o#3o........o3#o",
        ".oo..........oo.",
    ]),
}

# Tops, over the body from the neck to the hips: arms on rows 14-18, hands on 19-20.
TOPS = {
    "tee": grid(13, [
        "...oooo..oooo...",
        "..o####oo####o..",
        ".o############o.",
        ".oooo######oooo.",
        "....o######o....",
        "....o######o....",
        "....o######o....",
        "....o######o....",
        "....o333333o....",
    ]),
    "tank": grid(14, [
        "....o#o..o#o....",
        "....o##oo##o....",
        "....o######o....",
        "....o######o....",
        "....o######o....",
        "....o######o....",
        "....o######o....",
        "....o333333o....",
    ]),
    "longsleeve": grid(13, [
        "...oooo..oooo...",
        "..o####oo####o..",
        ".o############o.",
        ".o##o######o##o.",
        ".o##o######o##o.",
        ".o33o######o33o.",
        "....o######o....",
        "....o######o....",
        "....o333333o....",
    ]),
    "striped": grid(13, [
        "...oooo..oooo...",
        "..o%%%%oo%%%%o..",
        ".o############o.",
        ".oooo%%%%%%oooo.",
        "....o######o....",
        "....o%%%%%%o....",
        "....o######o....",
        "....o%%%%%%o....",
        "....o666666o....",
    ]),
    "polo": grid(13, [
        "...ooowwwwooo...",
        "..o####3w####o..",
        ".o#####3w#####o.",
        ".oooo######oooo.",
        "....o######o....",
        "....o######o....",
        "....o######o....",
        "....o######o....",
        "....o333333o....",
    ]),
    "shirt": grid(13, [
        "...oowo..owoo...",
        "..o####ww####o..",
        ".o######w#####o.",
        ".o##o###3##o##o.",
        ".o##o###w##o##o.",
        ".o33o###3##o33o.",
        "....o###w##o....",
        "....o###3##o....",
        "....o333w33o....",
    ]),
    "hoodie": grid(12, [
        "..o%o......o%o..",
        "..o%%oo..oo%%o..",
        "..o###w##w###o..",
        ".o####w##w####o.",
        ".o##o######o##o.",
        ".o##o#6666#o##o.",
        ".o66o#6##6#o66o.",
        "....o######o....",
        "....o######o....",
        "....o666666o....",
    ]),
    "sweater": grid(13, [
        "...oooo..oooo...",
        "..o###5555###o..",
        ".o############o.",
        ".o%%o%%%%%%o%%o.",
        ".o##o#5##5#o##o.",
        ".o55o######o55o.",
        "....o######o....",
        "....o######o....",
        "....o555555o....",
    ]),
}

# Jackets, over the top, open down the middle.
JACKETS = {
    "jacket": grid(13, [
        "...oooo..oooo...",
        "..o###3..3###o..",
        ".o#####..#####o.",
        ".o##o#3..3#o##o.",
        ".o##o#3..3#o##o.",
        ".o33o#3..3#o33o.",
        "....o#3..3#o....",
        "....o#3..3#o....",
        "....o33..33o....",
    ]),
    "leather": grid(13, [
        "...oooo..oooo...",
        "..o##o#..#o##o..",
        ".o###o#..#o###o.",
        ".o##o#a..3#o##o.",
        ".o##o#a..3#o##o.",
        ".o33o#a..3#o33o.",
        "....o#a..3#o....",
        "....o#a..3#o....",
        "....o33..33o....",
    ]),
    "varsity": grid(13, [
        "...oooo..oooo...",
        "..o%%#w..w#%%o..",
        ".o%%%#w..w#%%%o.",
        ".o%%o#w..w#o%%o.",
        ".o%%o#w..w#o%%o.",
        ".owwo#w..w#owwo.",
        "....o#w..w#o....",
        "....o#w..w#o....",
        "....owwwwwwo....",
    ]),
    "vest": grid(14, [
        "....o#o..o#o....",
        "....o##..##o....",
        "....o##..##o....",
        "....o##..##o....",
        "....o##..##o....",
        "....o##..##o....",
        "....o##..##o....",
        "....o3o..o3o....",
    ]),
    "blazer": grid(13, [
        "...oooo..oooo...",
        "..o###%..%###o..",
        ".o####%..%####o.",
        ".o##o#3%%3#o##o.",
        ".o##o##3###o##o.",
        ".o33o##y###o33o.",
        "....o##3###o....",
        "....o##y###o....",
        "....o333333o....",
    ]),
}

# Legs, from the hips down.
LEGS = {
    "jeans": grid(21, [
        "....o######o....",
        "...o###oo###o...",
        "...o###oo###o...",
        "...o###oo###o...",
        "...o###oo###o...",
        "...o###oo###o...",
        "...o###oo###o...",
        "...o111oo111o...",
    ]),
    "trousers": grid(21, [
        "....o######o....",
        "...o#3#oo#3#o...",
        "...o#3#oo#3#o...",
        "...o#3#oo#3#o...",
        "...o#3#oo#3#o...",
        "...o#3#oo#3#o...",
        "...o#3#oo#3#o...",
        "...o#3#oo#3#o...",
    ]),
    "shorts": grid(21, [
        "....o######o....",
        "...o###oo###o...",
        "...o###oo###o...",
        "...o333oo333o...",
    ]),
    "skirt": grid(21, [
        "....o######o....",
        "...o########o...",
        "...o########o...",
        "..o##########o..",
        "..o#3#3##3#3#o..",
        "..oooooooooooo..",
    ]),
    "cargo": grid(21, [
        "....o######o....",
        "...o###oo###o...",
        "...o###oo###o...",
        "...o%%#oo#%%o...",
        "...o%%#oo#%%o...",
        "...o###oo###o...",
        "...o###oo###o...",
        "...o333oo333o...",
    ]),
}

# Shoes, over the feet.
SHOES = {
    "sneakers": grid(29, [
        "..o####oo####o..",
        "..owwwwoowwwwo..",
    ]),
    "hightops": grid(27, [
        "...o###oo###o...",
        "...o%%%oo%%%o...",
        "..o####oo####o..",
        "..owwwwoowwwwo..",
    ]),
    "boots": grid(26, [
        "...o111oo111o...",
        "...o###oo###o...",
        "...o###oo###o...",
        "..o####oo####o..",
        "..ooooo..ooooo..",
    ]),
    "cowboy": grid(25, [
        "...o###oo###o...",
        "...o#1#oo#1#o...",
        "...o###oo###o...",
        "...o###oo###o...",
        ".o#####oo#####o.",
        "..oooo3oo3oooo..",
    ]),
}

# Glasses, around the eyes.
GLASSES = {
    "round": grid(6, [
        "....###..###....",
        "..##l..##l..##..",
        "...#...##...#...",
        "....###..###....",
    ]),
    "square": grid(6, [
        "...####..####...",
        "..##l..##l..##..",
        "...#...##...#...",
        "...####..####...",
    ]),
    "sunglasses": grid(6, [
        "...##########...",
        "..##ggg##ggg##..",
        "...#lgg##lgg#...",
        "....###..###....",
    ]),
    "visor": grid(6, [
        "..oooooooooooo..",
        "..o~l~~~~~~l~o..",
        "..o~~~~~~~~~~o..",
        "..oooooooooooo..",
    ]),
}

# Hats, on top of everything.
HATS = {
    "cap": grid(0, [
        ".....oooooo.....",
        "...oo######oo...",
        "..o##########o..",
        "..o##########o..",
        "..o3333333333o..",
        ".o%%%%%%%%%%%%o.",
        "..oooooooooooo..",
    ]),
    "beanie": grid(0, [
        "......o%%o......",
        "....oo####oo....",
        "...o########o...",
        "..o##########o..",
        "..o3#3#3#3#3#o..",
        "..o3#3#3#3#3#o..",
        "..oooooooooooo..",
    ]),
    "cowboy": grid(0, [
        "....oooooooo....",
        "...o###3####o...",
        "...o###3####o...",
        "...o%%%%%%%%o...",
        ".oo##########oo.",
        "o#333333333333#o",
        ".oooooooooooooo.",
    ]),
    "bucket": grid(0, [
        ".....oooooo.....",
        "....o######o....",
        "...o########o...",
        "...o%%%%%%%%o...",
        "..o##########o..",
        ".o#3333333333#o.",
        ".oooooooooooooo.",
    ]),
    "headband": grid(4, [
        "..oooooooooooo..",
        "..o##########o##",
        "..oooooooooooo.#",
        "...............3",
    ]),
    "headphones": grid(1, [
        "...oooooooooo...",
        "..o##########o..",
        ".o#o........o#o.",
        ".o#o........o#o.",
        ".o#o........o#o.",
        "o%%o........o%%o",
        "o%%o........o%%o",
        "o%%o........o%%o",
        ".oo..........oo.",
    ]),
    "crown": grid(0, [
        "...y...yy...y...",
        "...yy.yyyy.yy...",
        "...yyyyyyyyyy...",
        "...yryyrryyry...",
        "...YYYYYYYYYY...",
    ]),
}

ITEMS = {
    "body": {"body": BODY},
    "face": FACES,
    "beard": BEARDS,
    "legs": LEGS,
    "shoes": SHOES,
    "top": TOPS,
    "jacket": JACKETS,
    "hair": HAIR,
    "glasses": GLASSES,
    "hat": HATS,
}


def ramp(colour):
    """A colour's light, mid and dark shades: lighter towards yellow, darker towards blue, as
    pixel art shades (web/avatar.js does the same)."""
    r, g, b = (int(colour[i:i + 2], 16) / 255 for i in (1, 3, 5))
    h, l, s = colorsys.rgb_to_hls(r, g, b)

    def towards(target, degrees):
        delta = ((target - h * 360 + 540) % 360) - 180
        step = max(-degrees, min(degrees, delta))
        return ((h * 360 + step) % 360) / 360

    def rgb(h, l, s):
        return tuple(round(v * 255) for v in colorsys.hls_to_rgb(h, max(0, min(1, l)), max(0, min(1, s))))

    return (
        rgb(towards(60, 8), l + 0.12, s * 0.9),
        (round(r * 255), round(g * 255), round(b * 255)),
        rgb(towards(250, 12), l - 0.16, s * 0.8),
    )


def recolour(image, colours):
    """Slot pixels in the shades of the chosen colours (a slot without one stays grey)."""
    out = image.copy()
    swaps = {}
    for slot, colour in enumerate(colours, start=1):
        for key, shade in zip(KEYS[slot], ramp(colour)):
            swaps[key] = shade
    pixels = out.load()
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = pixels[x, y]
            if a and (r, g, b) in swaps:
                pixels[x, y] = (*swaps[(r, g, b)], a)
    return out


def compose(catalog, look):
    """The layers of a look {layer: (item, [colours])} on top of each other."""
    frame = Image.new("RGBA", (WIDTH, HEIGHT))
    for layer in catalog["layers"]:
        chosen = look.get(layer["id"])
        if not chosen:
            continue
        item, colours = chosen
        image = Image.open(OUT / layer["id"] / f"{item}.png").convert("RGBA")
        frame.alpha_composite(recolour(image, colours))
    return frame


def default_look(catalog):
    look = {}
    for layer in catalog["layers"]:
        item = layer.get("default")
        if item:
            entry = next(i for i in layer["items"] if i["id"] == item)
            look[layer["id"]] = (item, entry.get("colours", []))
    return look


def preview(catalog, scale=6):
    """Every item on the default character, a row per layer, in a few colours."""
    base = default_look(catalog)
    rows = []
    for layer in catalog["layers"]:
        row = []
        for item in layer["items"]:
            look = dict(base)
            look[layer["id"]] = (item["id"], item.get("colours", []))
            row.append(compose(catalog, look))
        rows.append(row)
    columns = max(len(row) for row in rows)
    sheet = Image.new("RGBA", (columns * (WIDTH + 4) * scale, len(rows) * (HEIGHT + 4) * scale),
                      (58, 52, 70, 255))
    for y, row in enumerate(rows):
        for x, image in enumerate(row):
            big = image.resize((WIDTH * scale, HEIGHT * scale), Image.Resampling.NEAREST)
            sheet.alpha_composite(big, (x * (WIDTH + 4) * scale + 2 * scale, y * (HEIGHT + 4) * scale + 2 * scale))
    return sheet


def main():
    catalog = json.loads((OUT / "catalog.json").read_text())
    for layer, items in ITEMS.items():
        (OUT / layer).mkdir(parents=True, exist_ok=True)
        for item, drawing in items.items():
            paint(drawing).save(OUT / layer / f"{item}.png")
    compose(catalog, default_look(catalog)).save(ROOT / "assets/characters/player.png")
    preview(catalog).save(ROOT / "art/previews/avatar.png")


if __name__ == "__main__":
    main()
