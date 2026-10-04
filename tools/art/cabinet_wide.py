"""Recipe-driven wide four-station cabinet, sharing the existing Sunset Riders shell.

NBA Jam uses its own sourced textures/materials and three-button blue/red controls;
the approved Sunset Riders generator, projection and sprite outputs are unchanged.
"""
import build_mk2 as base
import build_sunsetriders as wide


def model(recipe):
    solids = [s for s in wide.model() if s.part not in ('stick', 'button')]
    for y, color in zip(recipe['stations'], recipe['station_colors']):
        solids.append(base.Solid('stick', 10.6, 11, y - .2, y + .2, 19.4, 22))
        solids.append(base.Ball((10.8, y, 22.25), .75, tuple(color)))
        for button in recipe['buttons']:
            x, dy = button['position']
            z = (148 - x) / 7
            solids.append(base.Solid('button', x - .35, x + .35, y + dy - .35,
                                     y + dy + .35, z, z + .3, color=tuple(button['color'])))
    return solids


def paint(solid, p, n, tex, *, recipe):
    x, y, z = p
    nx, ny, nz = n
    colors = {name: tuple(color) for name, color in recipe['colors'].items()}
    if solid.part == 'side':
        if abs(ny) < .9 or x > wide.side_front(z) - .65 or z > 35.35:
            return 'trim', colors['trim'], False
        return 'side art', base.sample(tex['left' if ny > 0 else 'right'],
                                       (x - 2) / 9, (36 - z) / 36), False
    if solid.part == 'lower' and nx > .9:
        # The photographed dedicated cabinet has one two-slot coin-door assembly.
        if 4 < y < 12 and 5.5 < z < 12:
            edge = y < 4.4 or y > 11.6 or z < 5.9 or z > 11.6
            if edge:
                return 'front', (49, 52, 57), False
            if 10 < z < 11 and any(abs(y - slot) < .5 for slot in (6, 10)):
                return 'front', (225, 188, 57), False
            return 'front', (15, 18, 23), False
        return 'front', colors['body'], False
    layer, color, lit = wide.paint(solid, p, n, tex)
    if layer == 'trim':
        color = colors['trim']
    return layer, color, lit
