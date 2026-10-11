"""Original Sega Rally art stays distinct from Daytona and the misindexed sequel."""
from copy import deepcopy
import unittest
from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common


class SegaRallyTests(common.CabinetTests):
    SIZE = (48, 56)

    @classmethod
    def setUpClass(cls):
        cls.recipe = pipeline.load_recipe('srallyc')
        cls.views = pipeline.render_recipe(cls.recipe)

    def test_one_physical_station_with_two_buttons(self):
        self.assertEqual(self.recipe['players'], 4)
        self.assertTrue(self.recipe['linked'])
        self.assertEqual(self.recipe['control_stations'], 1)
        self.assertEqual(self.recipe['control_kind'], 'wheel')
        self.assertEqual(self.recipe['buttons_per_player'], 2)
        _, solids, _ = pipeline.prepare(self.recipe)
        for part, count in [('wheel', 1), ('shifter', 1), ('pedal', 2),
                            ('button', 2), ('stick', 0)]:
            self.assertEqual(sum(s.part == part for s in solids), count)
        wrong = deepcopy(self.recipe)
        wrong['buttons_per_player'] = 5
        with self.assertRaisesRegex(ValueError, 'controls disagree'):
            pipeline.prepare(wrong)

    def test_exports_match_recipe_and_seven_layers(self):
        for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
            name = f'cabinet_srallyc_{facing}'
            folder = pipeline.ROOT / 'art/objects' / name
            metadata = (folder / 'layers.ron').read_text()
            self.assertIn('width: 48', metadata)
            self.assertIn('height: 56', metadata)
            composite = Image.new('RGBA', self.SIZE)
            for i in range(7):
                with Image.open(folder / f'{i}.png') as layer:
                    composite.alpha_composite(layer)
            self.assertEqual(composite.tobytes(), flat.tobytes())
            with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as image:
                self.assertEqual(image.tobytes(), flat.tobytes())

    def test_no_sequel_art_or_substituted_game_screen(self):
        # These misleadingly indexed files are deliberately research-only.
        for spec in self.recipe['textures'].values():
            for extract in [spec, *spec.get('patches', [])]:
                if 'source' in extract:
                    self.assertIn(extract['source'], ('cabinet', 'controls'))
        screen = self.recipe['textures']['screen']
        self.assertEqual(screen['source'], 'cabinet')
        self.assertEqual(screen['fit'], 'contain')
        self.assertNotIn('left', self.recipe['textures'])
        self.assertNotIn('right', self.recipe['textures'])
        self.assertIn('photographed', self.recipe['preview_note'])
        source = self.recipe['sources']['cabinet']
        self.assertEqual(source['kind'], 'cabinet-photo-unverified')
        self.assertIn('not publisher authorization', source['rights'])
        self.assertTrue(self.recipe['uncertainties'])

    def test_white_shell_blue_platform_and_striped_seat(self):
        for _, layers in self.views:
            pixels = [color for _, color in layers['side art'].getcolors() if color[3]]
            self.assertTrue(any(min(c[:3]) > 110 for c in pixels))
            body = [color for _, color in layers['body'].getcolors() if color[3]]
            self.assertTrue(any(c[2] > c[0] * 2 and c[2] > 80 for c in body))
        for _, layers in self.views[:2]:
            pixels = [color for _, color in layers['side art'].getcolors() if color[3]]
            self.assertTrue(any(c[0] > 80 and c[0] > c[1] * 1.4 for c in pixels))
            self.assertTrue(any(c[2] > c[0] * 1.4 and c[2] > 60 for c in pixels))


if __name__ == '__main__':
    unittest.main()
