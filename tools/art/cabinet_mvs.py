"""Reusable two-player Neo Geo 'big red' shell for recipe-driven artwork.

Artwork is supplied by cabinet_pipeline; this module only describes geometry and
material placement. A cartridge game need not have its own dedicated side print.
"""
import build_mk2 as base


def model(recipe):
    solids = [
        base.Solid('lower', 2, 11, 0, 16, 0, 19),
        base.Solid('upper', 2, 9, 1, 15, 19, 31, (((1, 0, .65), 23.75),)),
        base.Solid('hood', 2, 10.5, 1, 15, 29, 36, (((1, 0, -2), -49.5),)),
        base.Solid('deck', 8, 14, 0, 16, 18.5, 21.5, (((1, 0, 6), 134),)),
    ]
    for ymin, ymax in ((0, 1), (15, 16)):
        solids += [
            base.Solid('side', 2, 11, ymin, ymax, 0, 19),
            base.Solid('side', 2, 9.5, ymin, ymax, 19, 30, (((8, 0, 1), 100),)),
            base.Solid('side', 2, 10.5, ymin, ymax, 30, 36, (((1, 0, -2), -49.5),)),
        ]
    for y in recipe['stations']:
        solids.append(base.Solid('stick', 10.4, 10.8, y - .2, y + .2, 20.5, 23))
        solids.append(base.Ball((10.6, y, 23.25), .7, tuple(recipe['colors']['stick'])))
        for button in recipe['buttons']:
            x, dy = button['position']
            z = (134 - x) / 6
            solids.append(base.Solid('button', x - .35, x + .35, y + dy - .35,
                                     y + dy + .35, z, z + .25, color=tuple(button['color'])))
    return solids


def side_front(z):
    if z < 19:
        return 11
    if z < 30:
        return min(9.5, (100 - z) / 8)
    return min(10.5, 2 * z - 49.5)


def paint(solid, p, n, tex, *, recipe):
    x, y, z = p
    nx, ny, nz = n
    part = solid.part
    colors = {name: tuple(color) for name, color in recipe['colors'].items()}
    if part == 'side':
        if abs(ny) < .9 or x > side_front(z) - .65 or z > 35.35:
            return 'trim', colors['trim'], False
        # Use a separately oriented source for each outside-facing panel.
        u = (x - 2) / 9 if ny > 0 else (11 - x) / 9
        return 'side art', base.sample(tex['left' if ny > 0 else 'right'], u, (36 - z) / 36), False
    if part == 'deck':
        if nz > .7:
            if x > 13.5 or y < .4 or y > 15.6:
                return 'trim', colors['trim'], False
            return 'controls', base.sample(tex['controls'], (16 - y) / 16, (x - 8) / 6), False
        return 'controls', colors['trim'], False
    if part == 'hood' and nx > .9:
        if z > 35.5 or z < 31.5:
            return 'trim', colors['trim'], False
        return 'marquee', base.sample(tex['marquee'], (15 - y) / 14, (36 - z) / 5), True
    if part == 'upper' and nx > .5:
        if 2.2 < y < 13.8 and 23 < z < 29:
            return 'screen', base.sample(tex['screen'], (13.8 - y) / 11.6, (29 - z) / 6), True
        return 'screen', colors['trim'], False
    if part == 'lower' and nx > .9:
        if 4.3 < y < 11.7 and 3.5 < z < 14:
            if y < 4.7 or y > 11.3 or z < 3.9 or z > 13.6:
                return 'front', (48, 49, 52), False
            if 12 < z < 13 and (5.5 < y < 6.5 or 9.5 < y < 10.5):
                return 'front', (217, 112, 42), False
            return 'front', (14, 16, 19), False
        if 16 < z < 17.5:
            return 'front', (234, 230, 212), False
        return 'front', colors['body'], False
    # Physical controls and unprinted rear vents share the common material helper.
    return base.paint(solid, p, n, tex)
