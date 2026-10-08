"""Twin-screen lightgun shell; shared projection, texture loading and layer export.

Station colors describe molded surrounds; pistols rest in holsters, unlike the
fixed machine guns on T2. Foot pedals remain separate from trigger controls.
"""
import build_mk2 as base


def model(recipe):
    solids = [
        base.Solid('lower', 2, 11, -3, 19, 0, 17),
        base.Solid('hood', 2, 10.5, -3, 19, 31, 36),
        base.Solid('divider', 2, 11, 7.5, 8.5, 17, 31),
        base.Solid('tower', 11.8, 16.7, 6.7, 9.3, 0, 18.9),
    ]
    for low, high in ((-3, -2.2), (18.2, 19)):
        solids.extend([
            base.Solid('side', 2, 11, low, high, 0, 17),
            base.Solid('side', 2, 10.5, low, high, 17, 28, (((8, 0, 1), 101),)),
            base.Solid('side', 2, 10.5, low, high, 28, 31),
        ])
    for i, y in enumerate(recipe['stations']):
        # Two independent monitor bays and molded projecting gun cradles.
        solids.extend([
            base.Solid('upper', 2, 11, y - 4.3, y + 4.3, 17, 31,
                       (((1, 0, .2), 14.8),)),
            base.Solid('deck', 8, 16.5, y - 4.3, y + 4.3, 15.7, 18.8,
                       (((1, 0, 6), 126),)),
            base.Solid('holster', 12.8, 16.2, y - 1.1, y + 1.1, 18, 20.1),
            base.Solid('gun', 10.5, 14.9, y - .4, y + .4, 20.9, 22.1,
                       color=tuple(recipe['gun_colors'][i])),
            base.Solid('gun_grip', 14, 15.3, y - .35, y + .35, 18.8, 21.2,
                       color=tuple(recipe['gun_colors'][i])),
            base.Solid('button', 13.9, 14.2, y - .2, y + .2, 19.8, 20.2,
                       color=(34, 37, 43)),
            base.Solid('pedal_guard', 16.5, 17.5, y - 2.2, y + 2.2, 0, 3),
            base.Solid('pedal', 17, 21, y - 2.2, y + 2.2, 0, 1.1,
                       (((1, 0, 4), 23),)),
        ])
        # Cords are solid, low-resolution segments, not invented extra controls.
        for x, z in ((15.6, 17.3), (16, 14.5), (16.4, 11.7),
                     (16.4, 8.9), (15.8, 7.2), (15, 8.4)):
            solids.append(base.Solid('cord', x, x + .35, y - .2, y + .2,
                                     z, z + 2.9, color=(27, 29, 33)))
    # Keep the established cell anchor (16px from the bottom). Widen the
    # monitor pair while compressing floor depth at this tiny sprite scale;
    # otherwise the projecting pedals clip in the front rotations.
    for solid in solids:
        solid.planes = [((nx / .55, ny / .9, nz),
                         offset + (8 / .55 - 8) * nx + (8 / .9 - 8) * ny)
                        for (nx, ny, nz), offset in solid.planes]
    return solids


def paint(solid, p, n, tex, *, recipe):
    x, y, z = p
    x, y = (x - 8) / .55 + 8, (y - 8) / .9 + 8
    nx, ny, nz = base.unit((n[0] * .55, n[1] * .9, n[2]))
    part = solid.part
    colors = {key: tuple(value) for key, value in recipe['colors'].items()}
    station = min(range(2), key=lambda i: abs(y - recipe['stations'][i]))
    center = recipe['stations'][station]
    surround = tuple(recipe['station_colors'][station])
    if part == 'side':
        front = 11 if z < 17 else min(10.5, (101 - z) / 8) if z < 28 else 10.5
        if abs(ny) < .9 or x > front - .55:
            return 'trim', colors['trim'], False
        texture = tex['left' if ny > 0 else 'right']
        u = (x - 2) / 9 if ny > 0 else (11 - x) / 9
        return 'side art', base.sample(texture, u, (31 - z) / 31), False
    if part == 'hood':
        if nx > .9:
            if z < 31.5 or z > 35.6 or y < -2.6 or y > 18.6:
                return 'trim', colors['trim'], False
            return 'marquee', base.sample(tex['marquee'], (19 - y) / 22, (36 - z) / 5), True
        return 'body', colors['hood'], False
    if part == 'upper':
        if nx > .5:
            if abs(y - center) < 3.5 and 19.5 < z < 27.8:
                return 'screen', base.sample(tex[f'screen{station + 1}'],
                                            (center + 3.5 - y) / 7, (27.8 - z) / 8.3), True
            if abs(y - center) < 3.5 and 28.7 < z < 29.5:
                return 'body', (32, 35, 42), False
            return 'screen', surround, False
        return 'body', colors['body'], False
    if part == 'divider':
        return 'body', colors['body'], False
    if part == 'deck':
        if nz > .7 and x < 12.5:
            return 'controls', base.sample(tex[f'deck{station + 1}'],
                                          (center + 4.3 - y) / 8.6, (x - 8) / 4.5), False
        return 'controls', surround if z > 16.3 else colors['red'], False
    if part == 'holster':
        return 'controls', surround, False
    if part == 'tower':
        if nz > .9:
            return 'controls', base.sample(tex['instructions'], (9.3 - y) / 2.6, (x - 11.8) / 4.9), False
        if nx > .9:
            if 12 < z < 15:
                return 'front', (176, 130, 37) if z > 14 else (45, 48, 51), False
            return 'front', (19, 21, 24), False
        return 'body', colors['body'], False
    if part == 'pedal_guard':
        return 'controls', colors['red'], False
    if part == 'pedal':
        if nz > .7:
            # Metal tread is geometric adaptation, not a claimed scanned print.
            tread = (int(x * 3) + int(y * 3)) % 3 == 0
            return 'controls', (146, 144, 123) if tread else (99, 101, 94), False
        return 'controls', colors['red'], False
    if part in ('gun', 'gun_grip', 'button', 'cord'):
        return 'controls', solid.color, False
    if part == 'lower' and nx > .9:
        return 'front', colors['body'], False
    return 'body', colors['body'], False
