"""Sit-down racing station using the shared cabinet raycaster and layer exporter.

One half of the original Daytona twin: CRT tower, low platform, bucket seat,
annular steering wheel, three spokes, gated shifter and two independent pedals.
Dimensions are recipe world-space estimates, not factory measurements.
"""
import math
import build_mk2 as base


class Wheel:
    """Finite hollow cylinder; rays through its center hit the separate spokes."""
    part = 'wheel'

    def __init__(self, center, radius, thickness, tilt):
        self.center = tuple(center)
        self.radius, self.inner, self.thickness = radius, radius - thickness, thickness
        self.axis = base.unit((1, 0, tilt))
        self.up = base.unit((-tilt, 0, 1))

    def hit(self, origin, ray):
        delta = base.add(origin, base.mul(self.center, -1))
        a, da = base.dot(delta, self.axis), base.dot(ray, self.axis)
        u, du = delta[1], ray[1]
        v, dv = base.dot(delta, self.up), base.dot(ray, self.up)
        hits = []
        half = self.thickness / 2
        if abs(da) > 1e-9:
            for end in (-half, half):
                t = (end - a) / da
                r2 = (u + t * du) ** 2 + (v + t * dv) ** 2
                if self.inner ** 2 <= r2 <= self.radius ** 2:
                    hits.append((t, base.mul(self.axis, 1 if end > 0 else -1)))
        aa, bb = du * du + dv * dv, 2 * (u * du + v * dv)
        if aa > 1e-9:
            for radius, sign in ((self.radius, 1), (self.inner, -1)):
                disc = bb * bb - 4 * aa * (u * u + v * v - radius * radius)
                if disc < 0:
                    continue
                for t in ((-bb - math.sqrt(disc)) / (2 * aa), (-bb + math.sqrt(disc)) / (2 * aa)):
                    if abs(a + t * da) <= half:
                        radial = base.add((0, u + t * du, 0), base.mul(self.up, v + t * dv))
                        hits.append((t, base.mul(base.unit(radial), sign)))
        return max(hits, key=lambda h: h[0]) if hits else None


def model(recipe):
    d = recipe['dimensions']
    rear, face, seat = d['tower_rear'], d['tower_front'], d['seat_back']
    lo, hi, top = d['side_min'], d['side_max'], d['tower_height']
    solids = [
        base.Solid('platform', rear, seat + 1, lo, hi, 0, 2),
        base.Solid('lower', rear, face, lo + .7, hi - .7, 2, 14),
        base.Solid('upper', rear, face, lo + .7, hi - .7, 14, top - 4,
                   (((1, 0, .22), face + .22 * 14),)),
        base.Solid('hood', rear, face + .7, lo + .5, hi - .5, top - 4, top),
        base.Solid('deck', face - 1, face + 4, lo, hi, 12.8, 15.7,
                   (((1, 0, 3), face + 4 + 3 * 14),)),
        base.Solid('dashboard', face + 1, face + 3, lo + .6, hi - .6, 13, 17.4),
        base.Solid('seat_base', seat - 7, seat, lo + .8, hi - .8, 2, 6),
        base.Solid('cushion', seat - 6.5, seat - .6, lo + 1.4, hi - 1.4, 6, 8),
        base.Solid('seat_back', seat - 1.8, seat, lo + 1, hi - 1, 6, 19,
                   (((-1, 0, .12), -(seat - 1.8) + .12 * 19),)),
    ]
    for a, b in ((lo, lo + .7), (hi - .7, hi)):
        solids.extend([
            base.Solid('side', rear, face, a, b, 2, 10),
            base.Solid('side', rear, face, a, b, 10, top - 3,
                       (((1, 0, .22), face + .22 * 14),)),
        ])
    for a, b in ((lo + 1, lo + 1.8), (hi - 1.8, hi - 1)):
        solids.append(base.Solid('seat_wing', seat - 6.5, seat, a, b, 7, 12,
                                 (((-1, 0, 1), 12 - (seat - 3)),)))
    w = recipe['wheel']
    cx, cy, cz = w['center']
    solids.append(Wheel(w['center'], w['radius'], w['thickness'], w['tilt']))
    solids.extend([
        base.Solid('wheel_hub', cx - .45, cx + .35, cy - .65, cy + .65, cz - .65, cz + .65),
        base.Solid('spoke', cx - .2, cx + .15, cy - 2, cy + 2, cz - .23, cz + .23),
        base.Solid('spoke', cx - .2, cx + .15, cy - .28, cy + .28, cz - 2, cz),
    ])
    sx, sy, sz = recipe['shifter']['position']
    solids.extend([
        base.Solid('gear_gate', sx - 1, sx + 1, sy - .9, sy + .9, sz - .3, sz),
        base.Solid('shifter', sx - .15, sx + .15, sy - .15, sy + .15, sz, sz + 2),
        base.Solid('gear_knob', sx - .4, sx + .4, sy - .4, sy + .4, sz + 1.7, sz + 2.4),
    ])
    for pedal in recipe['pedals']:
        x, y, z = pedal['position']
        solids.append(base.Solid('pedal', x - .9, x + .9, y - .6, y + .6, z, z + .6,
                                 (((1, 0, 2), x + .9 + 2 * (z + .3)),)))
    for button in recipe['buttons']:
        x, y, z = button['position']
        solids.append(base.Solid('button', x - .2, x + .2, y - .35, y + .35,
                                 z - .3, z + .3, color=tuple(button['color'])))
    return solids


def paint(solid, p, n, tex, *, recipe):
    x, y, z = p
    nx, ny, nz = n
    part = solid.part
    colors = {key: tuple(value) for key, value in recipe['colors'].items()}
    d = recipe['dimensions']
    rear, face, seat = d['tower_rear'], d['tower_front'], d['seat_back']
    lo, hi, top = d['side_min'], d['side_max'], d['tower_height']
    if part == 'button':
        return 'controls', solid.color, False
    if part == 'wheel':
        return 'controls', colors['rubber'], False
    if part in ('spoke', 'wheel_hub', 'pedal', 'shifter'):
        return 'controls', colors['metal'], False
    if part == 'gear_knob':
        return 'controls', colors['rubber'], False
    if part == 'gear_gate':
        if abs(y - recipe['shifter']['position'][1]) < .18 or abs(x - recipe['shifter']['position'][0]) < .18:
            return 'controls', colors['rubber'], False
        return 'controls', colors['trim'], False
    if part == 'platform':
        if abs(ny) > .9 and 1 < z < 1.65:
            return 'trim', colors['trim'], False
        if nx > .9 and 3.8 < y < 12.2 and int(y * 2) % 2 == 0:
            return 'body', colors['vent'], False
        return 'body', colors['platform'], False
    if part == 'side':
        front = min(face, face + .22 * (14 - z)) if z >= 14 else face
        if abs(ny) < .9 or x > front - .4:
            return 'trim', colors['trim'], False
        if z >= 10:
            u = (x - rear) / (face - rear) if ny > 0 else (face - x) / (face - rear)
            return 'side art', base.sample(tex['left' if ny > 0 else 'right'], u, (top - 3 - z) / (top - 13)), False
        return 'side art', colors['trim'], False
    if part == 'hood' and nx > .9:
        if z > top - .4 or z < top - 3.6:
            return 'trim', colors['metal'], False
        return 'marquee', base.sample(tex['marquee'], (hi - .5 - y) / (hi - lo - 1), (top - .4 - z) / 3.2), True
    if part == 'upper' and nx > .5:
        if lo + 1.25 < y < hi - 1.25 and 19 < z < top - 4.8:
            return 'screen', base.sample(tex['screen'], (hi - 1.25 - y) / (hi - lo - 2.5),
                                       (top - 4.8 - z) / (top - 23.8)), True
        return 'body', colors['rubber'], False
    if part == 'dashboard':
        if nx > .9:
            return 'controls', base.sample(tex['dashboard'], (hi - .6 - y) / (hi - lo - 1.2), (17.4 - z) / 4.4), False
        return 'controls', colors['body'], False
    if part == 'deck':
        if nz > .6:
            return 'controls', colors['body'], False
        if nx > .9:
            if z < 13.2:
                return 'trim', colors['trim'], False
            return 'controls', base.sample(tex['dashboard'], (hi - y) / (hi - lo), (15.7 - z) / 2.5), False
        return 'controls', colors['body'], False
    if part == 'lower' and nx > .9:
        if z > 8:
            return 'front', colors['metal'], False
        return 'front', colors['body'], False
    if part == 'seat_back' and nx > .9:
        return 'side art', base.sample(tex['seat_back'], (hi - 1 - y) / (hi - lo - 2), (19 - z) / 13), False
    if part in ('cushion', 'seat_back', 'seat_wing'):
        if abs(ny) > .9:
            return 'trim', colors['seat_edge'], False
        return 'body', colors['seat'], False
    if nx < -.9 and part in ('upper', 'lower'):
        if lo + 2 < y < hi - 2 and (4 < z < 10 or 21 < z < 25) and int(z * 2) % 2 == 0:
            return 'body', colors['vent'], False
    return 'body', colors['body'], False
