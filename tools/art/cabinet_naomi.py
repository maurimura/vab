"""Reusable NAOMI Universal standing shell; shared projection and control geometry."""
import build_mk2 as base
import cabinet_upright as upright


def model(recipe):
    solids = [
        base.Solid('base', 1, 12, 1, 15, 0, 7),
        base.Solid('crt_back', 1, 8, 2, 14, 18, 30),
        base.Solid('monitor', 7, 11, 2, 14, 18, 31, (((1, 0, .16), 13.6),)),
        base.Solid('deck', 8, 14, .5, 15.5, 15.5, 18.5, (((1, 0, 6), 122),)),
        base.Solid('billboard', 6.5, 8.5, 2, 14, 32, 36),
    ]
    for lo, hi in ((1, 2), (14, 15)):
        solids.append(base.Solid('rail', 7, 11, lo, hi, 0, 31,
                                 (((1, 0, .16), 13.6),)))
    # Reuse the parameterized sticks/buttons, not an unrelated upright shell.
    solids.extend(s for s in upright.model(recipe)
                  if s.part in ('stick', 'button'))
    return solids


def paint(solid, p, n, tex, *, recipe):
    x, y, z = p
    nx, ny, nz = n
    colors = {k: tuple(v) for k, v in recipe['colors'].items()}
    part = solid.part
    if part in ('stick', 'button'):
        return upright.paint(solid, p, n, tex, recipe=recipe)
    if part == 'deck':
        if nz > .7:
            if x > 13.5 or y < .9 or y > 15.1:
                return 'trim', colors['body'], False
            return 'controls', base.sample(tex['controls'], (15.5 - y) / 15, (x - 8) / 6), False
        return 'controls', colors['body'], False
    if part == 'billboard':
        if nx > .9:
            if 2.4 < y < 13.6 and 32.4 < z < 35.6:
                return 'marquee', base.sample(tex['marquee'], (13.6 - y) / 11.2,
                                             (35.6 - z) / 3.2), True
            return 'trim', colors['dark'], False
        return 'body', colors['dark'], False
    if part == 'monitor' and nx > .9:
        if 30 < z < 30.7:
            return 'trim', colors['lamp'], True
        if 3 < y < 13 and 19.8 < z < 29.3:
            if 3.4 < y < 12.6 and 21 < z < 28.3:
                return 'screen', base.sample(tex['screen'], (12.6 - y) / 9.2,
                                            (28.3 - z) / 7.3), True
            return 'screen', colors['dark'], False
        if 18.5 < z < 19.3 and (2.7 < y < 5.3 or 10.7 < y < 13.3):
            return 'body', colors['dark'], False
        return 'body', colors['body'], False
    if part == 'rail':
        if abs(ny) > .9:
            # Plain black side rails and narrow red seam corroborated by the flyer;
            # no invented tennis-player side prints or mirrored lettering.
            if x > 10.6 - .16 * z:
                return 'trim', colors['body'], False
            if 9.6 - .16 * z < x < 9.9 - .16 * z:
                return 'side art', colors['seam'], False
            return 'side art', colors['dark'], False
        return 'trim', colors['body'], False
    if part == 'base':
        if nx > .9:
            if 2.8 < y < 13.2 and 1 < z < 6.2:
                edge = y < 3.1 or y > 12.9 or z < 1.3 or z > 5.9 or 7.8 < y < 8.1
                return 'front', colors['door_edge'] if edge else colors['body'], False
            return 'front', colors['body'], False
        return 'body', colors['body'], False
    if part == 'crt_back':
        if nx < -.9 and 4 < y < 12 and 23 < z < 28 and int(z) % 2 == 0:
            return 'body', (18, 22, 25), False
        return 'body', colors['dark'], False
    return 'body', colors['body'], False
