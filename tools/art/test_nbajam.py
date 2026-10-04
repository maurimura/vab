"""NBA Jam: sourced four-player art and reproducible layer exports."""
import unittest
from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common


class NbaJamTests(common.CabinetTests):
    SIZE = (48, 56)

    @classmethod
    def setUpClass(cls):
        cls.recipe = pipeline.load_recipe('nbajam')
        cls.views = pipeline.render_recipe(cls.recipe)

    def test_four_stations_twelve_buttons_and_team_colors(self):
        _, solids, _ = pipeline.prepare(self.recipe)
        self.assertEqual(self.recipe['players'], 4)
        self.assertEqual(self.recipe['control_stations'], 4)
        sticks = [s for s in solids if s.part == 'stick']
        balls = [s for s in solids if isinstance(s, pipeline.base.Ball)]
        buttons = [s for s in solids if s.part == 'button' and isinstance(s, pipeline.base.Solid)]
        self.assertEqual(len(sticks), 4)
        self.assertEqual(len(balls), 4)
        self.assertEqual(len(buttons), 12)
        self.assertEqual([b.color for b in balls],
                         [(34, 137, 221)] * 2 + [(226, 37, 35)] * 2)
        self.assertEqual(len({b.center[1] for b in balls}), 4)
        self.assertEqual({b.color for b in buttons},
                         {(226, 37, 35), (34, 137, 221), (239, 237, 218)})
        for button in buttons:
            # Every raised cap stays within the wide deck's footprint.
            self.assertLessEqual(button.planes[0][1], 15)
            self.assertGreaterEqual(-button.planes[1][1], 8)
            self.assertLessEqual(button.planes[2][1], 18)
            self.assertGreaterEqual(-button.planes[3][1], -2)

    def test_exported_layers_and_sprites_match_recipe(self):
        for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
            name = f'cabinet_nbajam_{facing}'
            folder = pipeline.ROOT / 'art/objects' / name
            self.assertIn('width: 48', (folder / 'layers.ron').read_text())
            self.assertIn('height: 56', (folder / 'layers.ron').read_text())
            composite = Image.new('RGBA', self.SIZE)
            for i in range(7):
                with Image.open(folder / f'{i}.png') as image:
                    self.assertEqual(image.size, self.SIZE)
                    composite.alpha_composite(image)
            self.assertEqual(composite.tobytes(), flat.tobytes())
            with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as image:
                self.assertEqual(image.tobytes(), flat.tobytes())

    def test_side_badges_have_nba_white_and_blue_in_every_facing(self):
        for _, layers in self.views:
            colors = [c for _, c in layers['side art'].getcolors() if c[3]]
            self.assertTrue(any(min(c[:3]) > 160 for c in colors))
            self.assertTrue(any(c[2] > c[0] * 1.3 and c[2] > 60 for c in colors))
            self.assertTrue(any(c[0] > 90 and c[0] > c[2] * 1.4 for c in colors))

    def test_real_screen_and_documented_print_uncertainties(self):
        sources = self.recipe['sources']
        self.assertEqual(sources['screen']['kind'], 'local-game-capture')
        with Image.open(pipeline.reference(self.recipe, sources['screen'])) as image:
            self.assertEqual(image.size, (400, 254))
        self.assertEqual(sources['flyer']['kind'], 'original-print-scan')
        self.assertEqual(sources['marquee']['kind'], 'archive-artwork-unverified')
        self.assertIn('scan versus redrawn', sources['marquee']['purpose'])
        self.assertTrue(any('opposite' in text for text in self.recipe['uncertainties']))
        self.assertTrue(any('2 coins' in text for text in self.recipe['uncertainties']))


if __name__ == '__main__':
    unittest.main()
