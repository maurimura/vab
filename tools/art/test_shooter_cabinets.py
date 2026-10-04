"""Shooter cabinets: reproducible four-view exports and actual physical controls."""
from copy import deepcopy
import unittest
from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common

SKINS = ('invaders', 'asteroid', 's1945', 'term2')
CAPTURES = {'invaders': (224, 260), 'asteroid': (640, 480),
            's1945': (224, 320), 'term2': (400, 255)}
COUNTS = {'invaders': (1, 3, 1, 0), 'asteroid': (1, 5, 0, 0),
          's1945': (2, 4, 2, 0), 'term2': (2, 2, 0, 2)}


def shooter_tests(skin):
    class ShooterCabinetTests(common.CabinetTests):
        SIZE = (32, 48)

        @classmethod
        def setUpClass(cls):
            cls.recipe = pipeline.load_recipe(skin)
            cls.views = pipeline.render_recipe(cls.recipe)

        def test_exports_and_all_seven_layers_recompose_exactly(self):
            for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
                name = f'cabinet_{skin}_{facing}'
                folder = pipeline.ROOT / 'art/objects' / name
                text = (folder / 'layers.ron').read_text()
                self.assertIn('width: 32', text)
                self.assertIn('height: 48', text)
                composite = Image.new('RGBA', self.SIZE)
                for i in range(7):
                    with Image.open(folder / f'{i}.png') as layer:
                        self.assertEqual(layer.size, self.SIZE)
                        composite.alpha_composite(layer)
                self.assertEqual(composite.tobytes(), flat.tobytes())
                with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as saved:
                    self.assertEqual(saved.tobytes(), flat.tobytes())

        def test_physical_shared_buttons_or_independent_guns_match_reference(self):
            stations, caps, sticks, guns = COUNTS[skin]
            _, solids, _ = pipeline.prepare(self.recipe)
            self.assertEqual(self.recipe['players'], 2)
            self.assertEqual(self.recipe['control_stations'], stations)
            self.assertEqual(self.recipe['buttons_per_player'], caps)
            self.assertEqual(sum(s.part == 'stick' for s in solids), sticks)
            self.assertEqual(sum(isinstance(s, pipeline.base.Ball) for s in solids), sticks)
            self.assertEqual(sum(s.part == 'gun' for s in solids), guns)
            buttons = [s for s in solids if s.part == 'button' and isinstance(s, pipeline.base.Solid)]
            self.assertEqual(len(buttons), stations * caps)
            for button in buttons:
                self.assertGreaterEqual(-button.planes[1][1], 8)
                self.assertLessEqual(button.planes[0][1], 14)
                self.assertGreaterEqual(-button.planes[3][1], 3)
                self.assertLessEqual(button.planes[2][1], 13)
            if guns:
                for part in ('gun_grip', 'gun_mount', 'gun_barrel'):
                    self.assertEqual(sum(s.part == part for s in solids), guns)

        def test_archived_photo_provenance_and_distinct_actual_capture(self):
            sources = self.recipe['sources']
            self.assertEqual(sources['cabinet']['kind'], 'cabinet-photo-unverified')
            self.assertEqual(sources['marquee']['kind'], 'cabinet-photo-unverified')
            self.assertIn('not publisher authorization', sources['cabinet']['rights'])
            self.assertEqual(sources['screen']['kind'], 'local-game-capture')
            with Image.open(pipeline.reference(self.recipe, sources['screen'])) as capture:
                self.assertEqual(capture.size, CAPTURES[skin])
                self.assertGreater(len(capture.convert('RGB').getcolors(100000)), 2)
            if skin != 's1945':
                self.assertFalse(any(self.recipe['textures'][side].get('flip_x')
                                     for side in ('left', 'right')))

        def test_side_and_trim_identity_in_every_rotation(self):
            for _, layers in self.views:
                side = [c for _, c in layers['side art'].getcolors() if c[3]]
                if skin == 's1945':
                    self.assertLess(max(max(c[:3]) for c in side), 50)
                else:
                    self.assertGreater(len(side), 8)
                    self.assertTrue(any(c[2] > c[0] for c in side))
                if skin == 'term2':
                    trim = [c for _, c in layers['trim'].getcolors() if c[3]]
                    self.assertTrue(any(c[0] > 100 and c[1] < 70 and c[2] < 70 for c in trim))

    ShooterCabinetTests.__name__ = f'{skin.title()}CabinetTests'
    return ShooterCabinetTests


InvadersCabinetTests = shooter_tests('invaders')
AsteroidCabinetTests = shooter_tests('asteroid')
S1945CabinetTests = shooter_tests('s1945')
Term2CabinetTests = shooter_tests('term2')


class ShooterRecipeTests(unittest.TestCase):
    def test_supplied_rom_counts_and_distinct_screen_hashes(self):
        hashes = set()
        for skin, count in [('invaders', 4), ('asteroid', 5), ('s1945', 10), ('term2', 17)]:
            recipe = pipeline.load_recipe(skin)
            validation = recipe['rom_validation']
            self.assertEqual(validation['required_roms'], count)
            self.assertEqual(validation['matching_required_roms'], count)
            self.assertEqual(validation['coins'], 9)
            self.assertIsNone(validation['bios'])
            for key in ('zip_sha256', 'state_sha256', 'core_sha256'):
                self.assertEqual(len(validation[key]), 64)
            hashes.add(recipe['sources']['screen']['sha256'])
        self.assertEqual(len(hashes), 4)

    def test_space_invaders_discloses_missing_audio_and_uses_consistent_taito_panel(self):
        r = pipeline.load_recipe('invaders')
        self.assertEqual(r['shell'], 'flat-upright')
        self.assertEqual(r['textures']['controls']['source'], 'cabinet')
        self.assertEqual(r['audio']['status'], 'missing-samples')
        self.assertEqual(r['audio']['sample_set'], 'invaders')
        self.assertEqual(len(r['audio']['required_files']), 10)
        self.assertFalse(r['audio']['downloaded'])
        self.assertEqual([b['name'] for b in r['buttons']], ['Fire', '1P Start', '2P Start'])

    def test_asteroids_is_five_button_shared_panel_with_visible_vector_processing(self):
        r = pipeline.load_recipe('asteroid')
        self.assertEqual(r['control_kind'], 'buttons')
        self.assertTrue(r['turns'])
        self.assertEqual(r['game_actions'], [b['name'] for b in r['buttons']])
        self.assertEqual(r['textures']['screen']['fit'], 'contain')
        self.assertEqual(r['textures']['screen']['line_boost'], 3)
        pixels = [p for _, p in pipeline.textures(r)['screen'].getcolors()]
        self.assertTrue(any(min(p) > 80 for p in pixels))
        self.assertIn((0, 0, 0), pixels)

    def test_strikers_has_portrait_monitor_duplicated_shoot_bomb_and_no_fake_side_print(self):
        r = pipeline.load_recipe('s1945')
        self.assertTrue(r['portrait_monitor'])
        self.assertEqual(r['game_actions'], ['Shoot / hold to charge', 'Bomb'])
        self.assertEqual(r['buttons_per_player'], 4)
        self.assertEqual(r['rom_validation']['optional_absent'], ['4-u59.bin'])
        self.assertNotIn('left', r['textures'])
        self.assertNotIn('right', r['textures'])
        for b in r['buttons']:
            self.assertEqual(b['color_by_station'], [[35, 132, 210], [37, 208, 72]])
            for _, dy in b['position_by_station']:
                if b['name'].endswith('left'):
                    self.assertGreater(dy, 0)
                else:
                    self.assertLess(dy, 0)

    def test_t2_uses_two_machine_guns_not_four_ordinary_panel_caps(self):
        r = pipeline.load_recipe('term2')
        self.assertEqual(r['control_kind'], 'mounted-guns')
        self.assertEqual(r['game_actions'], ['Machine-gun trigger', 'Grenade'])
        self.assertIn('Mouse aiming is not added', r['input_note'])
        self.assertTrue(all(b['height'] > 19 for b in r['buttons']))
        self.assertFalse(r['turns'])

    def test_stroke_boost_rejects_invalid_kernels_or_artwork_modification(self):
        r = pipeline.load_recipe('asteroid')
        for value in (True, 0, 2, 7, 3.0):
            altered = deepcopy(r)
            altered['textures']['screen']['line_boost'] = value
            with self.assertRaisesRegex(ValueError, 'screen line boost'):
                pipeline.textures(altered)
        r['textures']['marquee']['line_boost'] = 3
        with self.assertRaisesRegex(ValueError, 'thin gameplay strokes'):
            pipeline.textures(r)


if __name__ == '__main__':
    unittest.main()
