"""Validate the photo-referenced Simpsons four-player cabinet."""
import unittest
from PIL import Image
import test_build_mk2 as common
import build_simpsons as cabinet


class SimpsonsTests(common.CabinetTests):
    SIZE = (cabinet.WIDTH, cabinet.HEIGHT)

    @classmethod
    def setUpClass(cls):
        tex, solids = cabinet.textures(), cabinet.model()
        cls.views = [cabinet.base.render(solids, tex, turns, painter=cabinet.paint,
                                        width=cabinet.WIDTH, height=cabinet.HEIGHT)
                     for turns in range(4)]

    def test_four_colored_two_button_stations(self):
        solids = cabinet.model()
        balls = [s for s in solids if isinstance(s, cabinet.base.Ball)]
        buttons = [s for s in solids if s.part == 'button'
                   and isinstance(s, cabinet.base.Solid)]
        self.assertEqual(len(balls), 4)
        self.assertEqual(len({s.center[1] for s in balls}), 4)
        self.assertEqual(len({s.color for s in balls}), 4)
        self.assertEqual(sum(s.part == 'stick' for s in solids), 4)
        self.assertEqual(len(buttons), 8)
        for _, color in cabinet.PLAYERS:
            self.assertEqual(sum(s.color == color for s in buttons), 2)

    def test_simpsons_art_not_sunsetriders(self):
        tex = cabinet.textures()
        sunset = cabinet.wide.textures()
        for name in ('marquee', 'controls', 'screen'):
            self.assertNotEqual(tex[name].tobytes(), sunset[name].tobytes(), name)

    def test_both_sides_use_their_family_decals(self):
        tex = cabinet.textures()
        self.assertEqual(tex['left'].size, (18, 54))
        self.assertEqual(tex['right'].size, tex['left'].size)
        self.assertNotEqual(tex['left'].tobytes(), tex['right'].tobytes())
        side = next(s for s in cabinet.model() if s.part == 'side')
        for normal, name in [((0, 1, 0), 'left'), ((0, -1, 0), 'right')]:
            layer, color, lit = cabinet.paint(side, (5, 16, 10), normal, tex)
            self.assertEqual(layer, 'side art')
            self.assertEqual(color, cabinet.base.sample(tex[name], 3 / 9, 26 / 36))
            self.assertFalse(lit)
            self.assertGreater(len(tex[name].getcolors(maxcolors=18 * 54)), 100)

    def test_cutout_margins_continue_cyan_not_white(self):
        tex = cabinet.textures()
        for name in ('left', 'right'):
            for point in ((0, 0), (17, 0), (17, 27), (0, 53), (17, 53)):
                r, g, b = tex[name].getpixel(point)
                self.assertLess(r, 160, (name, point))
                self.assertGreater(g, r, (name, point))
                self.assertGreater(b, r, (name, point))

    def test_side_and_deck_molding_are_yellow(self):
        self.assertEqual(cabinet.TRIM, (247, 212, 43))
        solids, tex = cabinet.model(), cabinet.textures()
        side = next(s for s in solids if s.part == 'side')
        deck = next(s for s in solids if s.part == 'deck')
        for solid, point, normal in [
            (side, (11, 16, 10), (0, 1, 0)),
            (side, (5, 16, 36), (0, 1, 0)),
            (deck, (14.5, 8, 19), (0, 0, 1)),
            (deck, (12, 17.8, 19), (0, 0, 1)),
            (deck, (15, 8, 18.8), (1, 0, 0)),
        ]:
            self.assertEqual(cabinet.paint(solid, point, normal, tex),
                             ('trim', cabinet.TRIM, False))

    def test_exports_match_render_and_layers(self):
        for facing, (flat, _) in zip(cabinet.base.FACINGS, self.views):
            name = f'cabinet_simpsons_{facing}'
            folder = cabinet.ROOT / 'art/objects' / name
            text = (folder / 'layers.ron').read_text()
            self.assertIn('width: 48', text)
            self.assertIn('height: 56', text)
            composite = Image.new('RGBA', self.SIZE)
            for index in range(len(cabinet.base.LAYERS)):
                with Image.open(folder / f'{index}.png') as layer:
                    self.assertEqual(layer.size, self.SIZE)
                    composite.alpha_composite(layer)
            self.assertEqual(composite.tobytes(), flat.tobytes())
            with Image.open(cabinet.ROOT / f'assets/tiles/objects/{name}.png') as exported:
                self.assertEqual(exported.tobytes(), flat.tobytes())


if __name__ == '__main__':
    unittest.main()
