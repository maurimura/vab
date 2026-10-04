"""Validate the three-player Cadillacs and Dinosaurs cabinet."""
import unittest
from PIL import Image
import test_build_mk2 as common
import build_dino as cabinet


class DinoTests(common.CabinetTests):
    SIZE = (cabinet.WIDTH, cabinet.HEIGHT)

    @classmethod
    def setUpClass(cls):
        tex, solids = cabinet.textures(), cabinet.model()
        cls.views = [cabinet.base.render(solids, tex, turns, painter=cabinet.paint,
                                        width=cabinet.WIDTH, height=cabinet.HEIGHT)
                     for turns in range(4)]

    def test_three_two_button_stations(self):
        solids = cabinet.model()
        balls = [s for s in solids if isinstance(s, cabinet.base.Ball)]
        buttons = [s for s in solids if s.part == 'button'
                   and isinstance(s, cabinet.base.Solid)]
        self.assertEqual(len(balls), 3)
        self.assertEqual(len({s.center[1] for s in balls}), 3)
        self.assertEqual(sum(s.part == 'stick' for s in solids), 3)
        self.assertEqual(len(buttons), 6)
        for _, _, color in cabinet.BUTTONS:
            self.assertEqual(sum(s.color == color for s in buttons), 3)

    def test_opposite_decals_and_reference_screen(self):
        tex = cabinet.textures()
        self.assertNotEqual(tex['left'].tobytes(), tex['right'].tobytes())
        snow = cabinet.shell.textures()
        for name in ('left', 'right', 'marquee', 'bezel', 'controls', 'screen'):
            self.assertNotEqual(tex[name].tobytes(), snow[name].tobytes(), name)

    def test_export_matches_render_and_editable_layers(self):
        for facing, (flat, _) in zip(cabinet.base.FACINGS, self.views):
            name = f'cabinet_dino_{facing}'
            folder = cabinet.ROOT / 'art/objects' / name
            text = (folder / 'layers.ron').read_text()
            self.assertIn('width: 40', text)
            self.assertIn('height: 52', text)
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
