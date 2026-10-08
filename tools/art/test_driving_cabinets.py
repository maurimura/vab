"""Reference-driven Out Run / Cruis'n USA art; no emulator or map changes."""
from copy import deepcopy
import json
import unittest
from unittest.mock import patch
from PIL import Image
import cabinet_pipeline as pipeline
import cabinet_racing


class DrivingCabinetTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.recipes = {skin: pipeline.load_recipe(skin) for skin in ('outrun', 'crusnusa')}
        cls.views = {skin: pipeline.render_recipe(recipe) for skin, recipe in cls.recipes.items()}

    def test_exports_fit_and_seven_layers_recompose(self):
        for skin, recipe in self.recipes.items():
            for turns, (facing, (flat, layers)) in enumerate(zip(pipeline.base.FACINGS, self.views[skin])):
                with self.subTest(skin=skin, facing=facing):
                    pipeline.check_view(flat, layers, recipe['size'], turns)
                    name = f'cabinet_{skin}_{facing}'
                    with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as exported:
                        self.assertEqual(exported.tobytes(), flat.tobytes())
                    folder = pipeline.ROOT / 'art/objects' / name
                    self.assertIn('width: 48', (folder / 'layers.ron').read_text())
                    self.assertIn('height: 56', (folder / 'layers.ron').read_text())
                    composite = Image.new('RGBA', tuple(recipe['size']))
                    for index in range(7):
                        with Image.open(folder / f'{index}.png') as layer:
                            composite.alpha_composite(layer)
                    self.assertEqual(composite.tobytes(), flat.tobytes())

    def test_one_physical_wheel_shifter_and_two_pedals(self):
        for skin, recipe in self.recipes.items():
            with self.subTest(skin=skin):
                self.assertEqual(recipe['players'], 1)
                self.assertEqual(recipe['control_stations'], 1)
                _, solids, _ = pipeline.prepare(recipe)
                for part, count in [('wheel', 1), ('shifter', 1), ('pedal', 2),
                                    ('spoke', 2), ('stick', 0), ('button', recipe['buttons_per_player'])]:
                    self.assertEqual(sum(s.part == part for s in solids), count)
                wrong = deepcopy(recipe)
                wrong['buttons_per_player'] += 1
                with self.assertRaisesRegex(ValueError, 'controls disagree'):
                    pipeline.prepare(wrong)

    def test_distinct_supported_shells_not_renamed_daytona(self):
        out = self.recipes['outrun']
        self.assertEqual(out['shell'], 'deluxe-car')
        self.assertEqual(out['buttons_per_player'], 1)
        _, solids, _ = pipeline.prepare(out)
        for part, count in [('tire', 2), ('hubcap', 2), ('exhaust', 4),
                            ('spoiler', 1), ('coin_tower', 1), ('speaker', 2)]:
            self.assertEqual(sum(s.part == part for s in solids), count)
        crus = self.recipes['crusnusa']
        self.assertNotIn('shell', crus)
        self.assertTrue(crus['ribbed_seat'])
        self.assertEqual(crus['dimensions']['seat_height'], 23)
        self.assertEqual(crus['buttons_per_player'], 5)
        self.assertNotIn('seat_back', crus['textures'])  # No invented rear decal.

    def test_sources_and_game_screens_are_honest(self):
        out = self.recipes['outrun']
        self.assertEqual(out['sources']['screen']['kind'], 'local-game-capture')
        with Image.open(pipeline.reference(out, out['sources']['screen'])) as image:
            self.assertEqual(image.size, (320, 224))
        crus = self.recipes['crusnusa']
        self.assertEqual(crus['textures']['screen']['source'], 'upright')
        self.assertEqual(crus['sources']['upright']['kind'], 'cabinet-photo-unverified')
        for recipe in self.recipes.values():
            self.assertTrue(recipe['full_side_art'])
            self.assertEqual(recipe['textures']['left'], recipe['textures']['right'])
            self.assertEqual(recipe['textures']['screen']['fit'], 'contain')
            self.assertIn('not publisher authorization', recipe['sources']['cabinet']['rights'])
            self.assertTrue(recipe['uncertainties'])

    def test_catalog_selects_skins_without_changing_game_configuration(self):
        catalog = (pipeline.ROOT / 'assets/games.ron').read_text()
        for skin, core in [('outrun', 'outrun'), ('crusnusa', 'mame')]:
            recipe = self.recipes[skin]
            row = next(line for line in catalog.splitlines() if f'rom: "{recipe["rom"]}"' in line)
            self.assertIn(f'core: "{core}"', row)
            self.assertIn('players: 1', row)
            self.assertIn('wheel: Some(', row)
            self.assertIn(f'cabinets: ["{skin}"]', row)

    def test_both_keep_photographed_color_identity(self):
        for skin in self.recipes:
            for _, layers in self.views[skin]:
                colors = [color for _, color in layers['side art'].getcolors() if color[3]]
                self.assertTrue(any(c[0] > 90 and c[0] > c[1] * 1.35 for c in colors))


class DecorativeCylinderTests(unittest.TestCase):
    def test_filled_caps_and_cardinal_axes(self):
        for axis, origin, ray in [((1, 0, 0), (-10, 0, 0), (1, 0, 0)),
                                  ((0, 1, 0), (0, -10, 0), (0, 1, 0))]:
            cylinder = cabinet_racing.Cylinder('tire', (0, 0, 0), 2, .4, axis)
            for _ in range(2):
                distance, normal = cylinder.hit(origin, ray)
                self.assertAlmostEqual(distance, 10.2)
                self.assertEqual(normal, axis)
                self.assertEqual(cylinder.center, (0, 0, 0))
                self.assertEqual(cylinder.axis, axis)
            self.assertIsNone(cylinder.hit((0, 0, 3), ray))

    def test_radial_surface_and_unsupported_axis(self):
        cylinder = cabinet_racing.Cylinder('tire', (0, 0, 0), 2, .4, (0, 1, 0))
        distance, normal = cylinder.hit((-10, 0, 0), (1, 0, 0))
        self.assertAlmostEqual(distance, 12)
        self.assertEqual(normal, (1, 0, 0))
        with self.assertRaisesRegex(ValueError, 'cardinal axis'):
            cabinet_racing.Cylinder('tire', (0, 0, 0), 2, .4, (1, 1, 0))

    def test_unknown_racing_shell_is_rejected(self):
        recipe = deepcopy(pipeline.load_recipe('outrun'))
        recipe['shell'] = 'unknown'
        with patch.object(pipeline.Path, 'read_text', return_value=json.dumps(recipe)):
            with self.assertRaisesRegex(ValueError, 'Unknown racing shell'):
                pipeline.load_recipe('outrun')


if __name__ == '__main__':
    unittest.main()
