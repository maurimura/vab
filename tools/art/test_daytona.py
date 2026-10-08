"""Daytona's linked seat count is independent of one physical driving station."""
from copy import deepcopy
from unittest.mock import patch
import json
import unittest
from PIL import Image
import cabinet_pipeline as pipeline
import cabinet_racing
import test_build_mk2 as common


class DaytonaTests(common.CabinetTests):
    SIZE = (48, 56)

    @classmethod
    def setUpClass(cls):
        cls.recipe = pipeline.load_recipe('daytona')
        cls.views = pipeline.render_recipe(cls.recipe)

    def test_one_driver_eight_linked_catalog_seats(self):
        self.assertEqual(self.recipe['players'], 8)
        self.assertTrue(self.recipe['linked'])
        self.assertEqual(self.recipe['control_stations'], 1)
        self.assertEqual(self.recipe['control_kind'], 'wheel')
        _, solids, _ = pipeline.prepare(self.recipe)
        for part, count in [('wheel', 1), ('shifter', 1), ('pedal', 2), ('spoke', 2),
                            ('button', 5), ('stick', 0)]:
            self.assertEqual(sum(s.part == part for s in solids), count)

    def test_controls_are_not_an_upright_joystick_substitute(self):
        r = deepcopy(self.recipe)
        r['buttons_per_player'] = 4
        with self.assertRaisesRegex(ValueError, 'controls disagree'):
            pipeline.prepare(r)
        r = deepcopy(self.recipe)
        r['pedals'].pop()
        with self.assertRaisesRegex(ValueError, 'controls disagree'):
            pipeline.prepare(r)

    def test_exports_match_recipe_and_layers(self):
        for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
            name = f'cabinet_daytona_{facing}'
            folder = pipeline.ROOT / 'art/objects' / name
            self.assertIn('width: 48', (folder / 'layers.ron').read_text())
            self.assertIn('height: 56', (folder / 'layers.ron').read_text())
            composite = Image.new('RGBA', self.SIZE)
            for i in range(7):
                with Image.open(folder / f'{i}.png') as layer:
                    composite.alpha_composite(layer)
            self.assertEqual(composite.tobytes(), flat.tobytes())
            with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as saved:
                self.assertEqual(saved.tobytes(), flat.tobytes())

    def test_real_display_and_honest_reference_records(self):
        sources = self.recipe['sources']
        self.assertEqual(sources['screen']['kind'], 'local-game-capture')
        self.assertEqual(sources['cabinet']['kind'], 'cabinet-photo-unverified')
        self.assertIn('not publisher authorization', sources['cabinet']['rights'])
        with Image.open(pipeline.reference(self.recipe, sources['screen'])) as image:
            self.assertEqual(image.size, (496, 384))
        self.assertEqual(self.recipe['textures']['screen']['fit'], 'contain')
        self.assertEqual(self.recipe['textures']['left'], self.recipe['textures']['right'])
        self.assertTrue(self.recipe['uncertainties'])

    def test_side_color_identity_and_seat_in_all_views(self):
        for _, layers in self.views:
            colors = [c for _, c in layers['side art'].getcolors() if c[3]]
            self.assertTrue(any(c[0] > 100 and c[0] > c[1] * 1.5 for c in colors))
            self.assertTrue(any(c[0] > 90 and c[1] > 65 and c[2] < c[1] for c in colors))
        _, solids, _ = pipeline.prepare(self.recipe)
        self.assertTrue(any(s.part == 'seat_back' for s in solids))
        self.assertTrue(any(s.part == 'platform' for s in solids))

    def test_catalog_keeps_existing_core_link_options_and_players(self):
        catalog = (pipeline.ROOT / 'assets/games.ron').read_text()
        self.assertIn('(rom: "daytona", core: "daytona", title: "Daytona USA", arcade: true, players: 8, options: {"cabinets": "1", "link_topology": "star", "link_pace": "1"}, cabinets: ["daytona"])', catalog)


class RacingValidationTests(unittest.TestCase):
    def load_modified(self, **changes):
        recipe = deepcopy(pipeline.load_recipe('daytona'))
        recipe.update(changes)
        with patch.object(pipeline.Path, 'read_text', return_value=json.dumps(recipe)):
            return pipeline.load_recipe('daytona')

    def test_eight_seats_require_linked_racing(self):
        for changes in ({'linked': False}, {'renderer': 'upright'}, {'players': 9}):
            with self.assertRaisesRegex(ValueError, 'player count'):
                self.load_modified(**changes)

    def test_wheel_requires_single_racing_station(self):
        for changes in ({'control_stations': 2}, {'renderer': 'upright', 'players': 2},
                        {'control_kind': 'joystick'}):
            with self.assertRaisesRegex(ValueError, 'renderer'):
                self.load_modified(**changes)

    def test_bad_scale_is_rejected(self):
        for value in (0, -1, 1.1, '0.6'):
            with self.assertRaisesRegex(ValueError, 'footprint scale'):
                self.load_modified(footprint_scale=value)

    def test_wheel_is_hollow_and_hits_rim_not_center(self):
        wheel = cabinet_racing.Wheel((0, 0, 0), 2, .4, 0)
        self.assertIsNone(wheel.hit((-10, 0, 0), (1, 0, 0)))
        self.assertIsNone(wheel.hit((-10, 3, 0), (1, 0, 0)))
        distance, normal = wheel.hit((-10, 1.8, 0), (1, 0, 0))
        self.assertAlmostEqual(distance, 10.2)
        self.assertEqual(normal, (1, 0, 0))
        self.assertIsNotNone(wheel.hit((0, -10, 0), (0, 1, 0)))


if __name__ == '__main__':
    unittest.main()
