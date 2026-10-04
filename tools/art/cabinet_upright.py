"""Recipe-driven upright and Capcom pedestal shells for the remaining catalog.

Projection, editable layers and lighting are shared with the approved cabinets.
Physical stations are independent from the game's simultaneous/turn-based seats.
"""
import build_mk2 as base


def model(recipe):
    if recipe.get('shell') == 'pedestal':
        solids = [
            base.Solid('lower', 3, 10, 3, 13, 0, 15),
            base.Solid('neck', 3, 7, 5, 11, 15, 24),
            base.Solid('deck', 7, 14, 0, 16, 15.5, 19, (((1, 0, 6), 122),)),
            base.Solid('upper', 2, 9, 1, 15, 22, 34, (((1, 0, .35), 19.8),)),
            base.Solid('hood', 2, 9, 1, 15, 34, 38),
        ]
        for ymin, ymax in ((0, 1), (15, 16)):
            solids.append(base.Solid('side', 2, 9.5, ymin, ymax, 22, 38))
    else:
        solids = [s for s in base.model() if s.part not in ('button', 'stick', 'upper')]
        solids.insert(1, base.Solid('upper', 2, 9, 4, 12, 16, 28, (((1, 0, .65), 20.75),)))
    for i, y in enumerate(recipe['stations']):
        color = recipe.get('station_colors', [recipe['colors']['stick']] * len(recipe['stations']))[i]
        solids.append(base.Solid('stick', 10.4, 10.8, y - .2, y + .2, 17, 19.1))
        solids.append(base.Ball((10.6, y, 19.4), .7, tuple(color)))
        for button in recipe['buttons']:
            x, dy = button['position']
            z = (122 - x) / 6
            solids.append(base.Solid('button', x - .35, x + .35, y + dy - .35,
                                     y + dy + .35, z, z + .25, color=tuple(button['color'])))
    return solids


def paint(solid, p, n, tex, *, recipe):
    x, y, z = p
    nx, ny, nz = n
    part = solid.part
    colors = {name: tuple(color) for name, color in recipe['colors'].items()}
    pedestal = recipe.get('shell') == 'pedestal'
    if part == 'side':
        front = 9.5 if pedestal else base.side_front(z)
        top = 38 if pedestal else 32
        if abs(ny) < .9 or x > front - .65 or z > top - .65:
            return 'trim', colors.get('hood_trim', colors['trim']) if pedestal else colors['trim'], False
        if 'left' in tex:
            u = (x - 2) / 9 if ny > 0 else (11 - x) / 9
            return 'side art', base.sample(tex['left' if ny > 0 else 'right'], u, (top - z) / top), False
        return 'side art', colors.get('hood', colors['side']) if pedestal else colors['side'], False
    if part == 'deck':
        lo, hi = (0, 16) if pedestal else (3, 13)
        if nz > .7:
            if x > 13.5 or y < lo + .4 or y > hi - .4:
                return 'trim', colors['trim'], False
            return 'controls', base.sample(tex['controls'], (hi - y) / (hi - lo), (x - 8) / 6), False
        if nx > .9 and 'panel' in tex:
            return 'controls', base.sample(tex['panel'], (hi - y) / (hi - lo), (18.5 - z) / 3), False
        return 'controls', colors.get('deck', colors['trim']), False
    if part == 'hood' and nx > .9:
        top = 38 if pedestal else 32
        lo, hi = (1, 15) if pedestal else (4, 12)
        if z > top - .4 or z < top - 3.4:
            return 'trim', colors.get('hood_trim', colors['trim']), False
        return 'marquee', base.sample(tex['marquee'], (hi - y) / (hi - lo), (top - z) / 4), True
    if part == 'upper' and nx > .5:
        lo, hi = (1, 15) if pedestal else (4, 12)
        bottom, top = (23, 32.5) if pedestal else (18.8, 25.1)
        if lo + .6 < y < hi - .6 and bottom + .6 < z < top - .6:
            return 'screen', base.sample(tex['screen'], (hi - .6 - y) / (hi - lo - 1.2),
                                       (top - .6 - z) / (top - bottom - 1.2)), True
        if 'bezel' in tex:
            return 'screen', base.sample(tex['bezel'], (hi - y) / (hi - lo), (top - z) / (top - bottom)), False
        return 'screen', colors.get('bezel', colors['trim']), False
    if part == 'lower' and nx > .9:
        if 6 < y < 10 and 5 < z < 11.5:
            if y < 6.4 or y > 9.6 or z < 5.4 or z > 11.1:
                return 'front', (55, 58, 61), False
            if 9.5 < z < 10.5 and (6.6 < y < 7.4 or 8.6 < y < 9.4):
                return 'front', (218, 55, 42), False
            return 'front', (15, 18, 21), False
        if 'front' in tex:
            return 'front', base.sample(tex['front'], (13 - y) / 10, (16 - z) / 16), False
        return 'front', colors['body'], False
    if part == 'neck':
        return 'body', colors['body'], False
    if part == 'button':
        return 'controls', solid.color, False
    if part == 'stick':
        return 'controls', (57, 57, 64), False
    if nx < -.9:
        if 5 < y < 11 and 22 < z < 28 and int(z) % 2 == 0:
            return 'body', (9, 11, 16), False
        if 5 < y < 11 and 4 < z < 15:
            edge = y < 5.35 or y > 10.65 or z < 4.35 or z > 14.65
            return 'body', (42, 44, 53) if edge else (24, 27, 35), False
    if pedestal and part in ('upper', 'hood'):
        return 'body', colors.get('hood', colors['body']), False
    return 'body', colors.get('back', colors['body']), False
