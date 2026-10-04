"""KOF '98 and UMK3: offline sources, physical controls and deterministic exports."""
import subprocess
import sys
import unittest
from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common


def fighter_tests(skin):
    class FighterCabinetTests(common.CabinetTests):
        SIZE = tuple(pipeline.load_recipe(skin)['size'])

        @classmethod
        def setUpClass(cls):
            cls.recipe = pipeline.load_recipe(skin)
            cls.views = pipeline.render_recipe(cls.recipe)

        def test_exported_sprites_and_editable_layers_match_recipe(self):
            for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
                name = f'cabinet_{skin}_{facing}'
                folder = pipeline.ROOT / 'art/objects' / name
                self.assertIn(f'width: {self.SIZE[0]}', (folder / 'layers.ron').read_text())
                self.assertIn(f'height: {self.SIZE[1]}', (folder / 'layers.ron').read_text())
                composite = Image.new('RGBA', self.SIZE)
                for i in range(7):
                    with Image.open(folder / f'{i}.png') as image:
                        self.assertEqual(image.size, self.SIZE)
                        composite.alpha_composite(image)
                self.assertEqual(composite.tobytes(), flat.tobytes())
                with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as image:
                    self.assertEqual(image.tobytes(), flat.tobytes())

        def test_two_stations_and_all_action_caps_fit_the_deck(self):
            _, solids, _ = pipeline.prepare(self.recipe)
            self.assertEqual(self.recipe['players'], 2)
            self.assertEqual(self.recipe['control_stations'], 2)
            self.assertEqual(sum(s.part == 'stick' for s in solids), 2)
            buttons = [s for s in solids if s.part == 'button' and isinstance(s, pipeline.base.Solid)]
            self.assertEqual(len(buttons), 2 * self.recipe['buttons_per_player'])
            lo, hi = (0, 16) if skin == 'kof98' else (3, 13)
            for button in buttons:
                self.assertGreaterEqual(-button.planes[1][1], 8)
                self.assertLessEqual(button.planes[0][1], 14)
                self.assertGreaterEqual(-button.planes[3][1], lo)
                self.assertLessEqual(button.planes[2][1], hi)

        def test_sources_and_actual_game_capture(self):
            sources = self.recipe['sources']
            self.assertEqual(sources['marquee']['kind'], 'original-art-scan')
            self.assertIn('not publisher authorization', sources['marquee']['rights'])
            self.assertEqual(sources['screen']['kind'], 'local-game-capture')
            self.assertEqual(sources['controls']['kind'], 'cabinet-photo-unverified')
            size = (304, 224) if skin == 'kof98' else (400, 254)
            with Image.open(pipeline.reference(self.recipe, sources['screen'])) as image:
                self.assertEqual(image.size, size)
                self.assertGreater(len(image.convert('RGB').getcolors(100000)), 100)
            self.assertTrue(any('opposite' in note for note in self.recipe['uncertainties']))

        def test_side_identity_and_trim_in_every_rotation(self):
            for _, layers in self.views:
                side = [c for _, c in layers['side art'].getcolors() if c[3]]
                self.assertGreater(len(side), 8)
                if skin == 'kof98':
                    self.assertTrue(any(c[0] > 130 and c[0] > c[2] * 2 for c in side))
                    self.assertTrue(any(min(c[:3]) > 110 for c in side))
                else:
                    trim = [c for _, c in layers['trim'].getcolors() if c[3]]
                    self.assertTrue(any(c[0] > 130 and c[1] < 70 and c[2] < 70 for c in trim))
                    self.assertTrue(any(c[0] > 60 and c[2] > c[1] for c in side))

    FighterCabinetTests.__name__ = f'{skin.title()}CabinetTests'
    return FighterCabinetTests


Kof98CabinetTests = fighter_tests('kof98')
Umk3CabinetTests = fighter_tests('umk3')


class FighterRecipeTests(unittest.TestCase):
    def test_kof_is_disclosed_mvs_variant_with_four_actions_and_one_game_bay(self):
        recipe = pipeline.load_recipe('kof98')
        self.assertEqual(recipe['renderer'], 'mvs')
        self.assertEqual(recipe['rom_validation']['bios'], 'neogeo')
        self.assertEqual(recipe['buttons_per_player'], 4)
        bays = [p for p in recipe['textures']['marquee']['patches'] if p['source'] == 'marquee']
        self.assertEqual(len(bays), 1)
        self.assertFalse(any('flip_x' in recipe['textures'][name] for name in ('left', 'right')))

    def test_umk3_uses_mk_diamond_plus_run_not_six_button_fighter_rows(self):
        recipe = pipeline.load_recipe('umk3')
        buttons = {b['name']: b for b in recipe['buttons']}
        self.assertEqual(set(buttons), set(recipe['game_actions']))
        self.assertEqual(buttons['High Punch']['position'][0], buttons['High Kick']['position'][0])
        self.assertEqual(buttons['Low Punch']['position'][0], buttons['Low Kick']['position'][0])
        self.assertGreater(buttons['Run']['position'][0], buttons['Block']['position'][0])
        self.assertEqual(buttons['Run']['color'], [245, 209, 37])
        self.assertIsNone(recipe['rom_validation']['bios'])
        self.assertEqual(recipe['rom_validation']['optional_absent'], ['463_mk3_ultimate.u64'])
        self.assertFalse(any('flip_x' in recipe['textures'][name] for name in ('left', 'right')))

    def test_rom_validation_records_and_capture_hashes_are_distinct(self):
        captures = set()
        for skin, count in [('kof98', 16), ('umk3', 26)]:
            recipe = pipeline.load_recipe(skin)
            validation = recipe['rom_validation']
            self.assertEqual(validation['required_roms'], count)
            self.assertEqual(validation['matching_required_roms'], count)
            self.assertEqual(validation['coins'], 9)
            self.assertEqual(len(validation['zip_sha256']), 64)
            self.assertEqual(len(validation['state_sha256']), 64)
            captures.add(recipe['sources']['screen']['sha256'])
        self.assertEqual(len(captures), 2)

    def test_preview_output_rejects_path_traversal(self):
        result = subprocess.run(
            [sys.executable, str(pipeline.ROOT / 'tools/art/preview_catalog.py'),
             'kof98', 'umk3', '--filename=../escape.png'],
            capture_output=True, text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('simple lowercase PNG basename', result.stderr)


if __name__ == '__main__':
    unittest.main()
