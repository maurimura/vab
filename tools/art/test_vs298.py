"""Sega sports upright: physical controls, provenance and reproducible exports."""
import unittest
from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common


class VirtuaStrikerCabinetTests(common.CabinetTests):
    SIZE = (40, 56)

    @classmethod
    def setUpClass(cls):
        cls.recipe = pipeline.load_recipe('vs298')
        cls.views = pipeline.render_recipe(cls.recipe)

    def test_exported_sprites_and_seven_layers_match_recipe(self):
        for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
            name = f'cabinet_vs298_{facing}'
            folder = pipeline.ROOT / 'art/objects' / name
            manifest = (folder / 'layers.ron').read_text()
            self.assertIn('width: 40', manifest)
            self.assertIn('height: 56', manifest)
            composite = Image.new('RGBA', self.SIZE)
            for index in range(7):
                with Image.open(folder / f'{index}.png') as layer:
                    self.assertEqual(layer.size, self.SIZE)
                    composite.alpha_composite(layer)
            self.assertEqual(composite.tobytes(), flat.tobytes())
            with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as sprite:
                self.assertEqual(sprite.tobytes(), flat.tobytes())

    def test_two_green_sticks_and_three_actions_not_fighter_controls(self):
        _, solids, _ = pipeline.prepare(self.recipe)
        self.assertEqual(self.recipe['players'], 2)
        self.assertEqual(self.recipe['control_stations'], 2)
        self.assertEqual(self.recipe['buttons_per_player'], 3)
        sticks = [s for s in solids if s.part == 'stick']
        balls = [s for s in solids if isinstance(s, pipeline.base.Ball)]
        actions = [s for s in solids if s.part == 'button' and isinstance(s, pipeline.base.Solid)]
        self.assertEqual(len(sticks), 2)
        self.assertEqual(len(balls), 2)
        self.assertEqual(len(actions), 6)
        self.assertTrue(all(s.color[1] > s.color[0] * 2 for s in balls))
        self.assertEqual({s.color for s in actions}, {(38, 187, 76), (31, 152, 211), (235, 62, 49)})

    def test_wide_white_shell_with_dark_foot_and_separate_deck_trim(self):
        tex, solids, painter = pipeline.prepare(self.recipe)
        self.assertEqual(self.recipe['shell'], 'sega-sports')
        self.assertFalse(any(s.part == 'neck' for s in solids))
        foot = next(s for s in solids if s.part == 'foot')
        self.assertEqual(painter(foot, (10, 8, .5), (1, 0, 0), tex)[1], (23, 29, 29))
        deck = next(s for s in solids if s.part == 'deck')
        self.assertEqual(painter(deck, (13.8, 8, 18), (0, 0, 1), tex)[1], (23, 29, 29))
        self.assertEqual(painter(deck, (14, 8, 17), (1, 0, 0), tex)[1], (227, 224, 211))

    def test_photo_art_and_actual_98_screen_are_honestly_classified(self):
        sources = self.recipe['sources']
        pipeline.validate_sources(self.recipe)
        self.assertEqual(sources['cabinet']['kind'], 'cabinet-photo-unverified')
        self.assertEqual(sources['side_cabinet']['kind'], 'cabinet-photo-unverified')
        self.assertIn('not publisher authorization', sources['cabinet']['rights'])
        self.assertEqual(sources['screen']['kind'], 'local-game-capture')
        with Image.open(pipeline.reference(self.recipe, sources['screen'])) as screen:
            self.assertEqual(screen.size, (496, 384))
            self.assertGreater(len(screen.convert('RGB').getcolors(1000000)), 1000)
        self.assertEqual(self.recipe['textures']['screen']['fit'], 'contain')
        self.assertEqual(self.recipe['textures']['controls']['source'], 'cabinet')
        self.assertNotIn('controls_alternative', {s.get('source') for s in self.recipe['textures'].values()})
        for side in ('left', 'right'):
            self.assertEqual(self.recipe['textures'][side]['source'], 'side_cabinet')
            self.assertFalse(self.recipe['textures'][side].get('flip_x', False))
        self.assertTrue(self.recipe['uncertainties'])

    def test_catalog_maps_skin_without_changing_core_or_online_mode(self):
        catalog = (pipeline.ROOT / 'assets/games.ron').read_text()
        self.assertIn('(rom: "vs298", core: "supermodel", title: "Virtua Striker 2 \'98", lockstep: true, cabinets: ["vs298"])', catalog)


if __name__ == '__main__':
    unittest.main()
