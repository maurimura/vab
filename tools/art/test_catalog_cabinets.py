"""Remaining catalog skins: deterministic exports and original-source records."""
import unittest
from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common

GAMES = ('mvsc', 'sf2ce', 'pacman', 'atetris', 'wboy')


def cabinet_tests(game):
    class CatalogCabinetTests(common.CabinetTests):
        SIZE = tuple(pipeline.load_recipe(game)['size'])

        @classmethod
        def setUpClass(cls):
            cls.recipe = pipeline.load_recipe(game)
            cls.views = pipeline.render_recipe(cls.recipe)

        def test_exported_sprite_and_layers_match_render(self):
            for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
                name = f'cabinet_{game}_{facing}'
                folder = pipeline.ROOT / 'art/objects' / name
                text = (folder / 'layers.ron').read_text()
                self.assertIn(f'width: {self.SIZE[0]}', text)
                self.assertIn(f'height: {self.SIZE[1]}', text)
                composite = Image.new('RGBA', self.SIZE)
                for i in range(7):
                    with Image.open(folder / f'{i}.png') as image:
                        self.assertEqual(image.size, self.SIZE)
                        composite.alpha_composite(image)
                self.assertEqual(composite.tobytes(), flat.tobytes())
                with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as image:
                    self.assertEqual(image.tobytes(), flat.tobytes())

        def test_sources_distinguish_scans_from_captures_and_unknown_photos(self):
            sources = self.recipe['sources']
            self.assertEqual(sources['marquee']['kind'], 'original-art-scan')
            self.assertIn('not publisher authorization', sources['marquee']['rights'])
            self.assertEqual(sources['screen']['kind'], 'local-game-capture')
            self.assertEqual(sources['cabinet']['kind'], 'cabinet-photo-unverified')
            self.assertTrue(self.recipe['uncertainties'])
            self.assertIn('download_sha256', sources['marquee'])

        def test_station_and_modeled_button_counts(self):
            _, solids, _ = pipeline.prepare(self.recipe)
            stations = self.recipe['control_stations']
            self.assertEqual(sum(s.part == 'stick' for s in solids), stations)
            buttons = [s for s in solids if s.part == 'button' and isinstance(s, pipeline.base.Solid)]
            self.assertEqual(len(buttons), stations * self.recipe['buttons_per_player'])

    CatalogCabinetTests.__name__ = f'{game.title()}CabinetTests'
    return CatalogCabinetTests


for _game in GAMES:
    globals()[f'{_game.title()}CabinetTests'] = cabinet_tests(_game)


class CatalogRecipeTests(unittest.TestCase):
    def test_turn_based_games_share_one_station(self):
        for game in ('pacman', 'wboy'):
            recipe = pipeline.load_recipe(game)
            self.assertEqual(recipe['players'], 2)
            self.assertEqual(recipe['control_stations'], 1)

    def test_game_captures_are_not_reused_between_games(self):
        hashes = {pipeline.load_recipe(game)['sources']['screen']['sha256'] for game in GAMES}
        self.assertEqual(len(hashes), len(GAMES))

    def test_wonderboy_marquee_is_rotated_to_landscape(self):
        recipe = pipeline.load_recipe('wboy')
        image = pipeline.extract(recipe, recipe['textures']['marquee'])
        self.assertGreater(image.width, image.height * 2)

    def test_pacman_screen_remains_vertical_and_letterboxed(self):
        recipe = pipeline.load_recipe('pacman')
        image = pipeline.textures(recipe)['screen']
        self.assertEqual(image.getpixel((0, 6)), (0, 0, 0))
        self.assertGreater(max(image.getpixel((6, 6))), 0)
        self.assertEqual(recipe['textures']['screen']['fit'], 'contain')

    def test_tetris_side_decals_and_red_trim_in_all_views(self):
        recipe = pipeline.load_recipe('atetris')
        self.assertEqual(recipe['sources']['side_reference']['kind'], 'user-supplied-reference')
        for _, layers in pipeline.render_recipe(recipe):
            side = [c for _, c in layers['side art'].getcolors() if c[3]]
            trim = [c for _, c in layers['trim'].getcolors() if c[3]]
            self.assertGreater(len(side), 4, 'Cathedral decal missing from this side')
            self.assertTrue(any(c[0] > 70 and c[0] > c[1] * 1.25 for c in side))
            self.assertTrue(any(c[0] > 140 and c[1] < 80 and c[2] < 90 for c in trim))
        # Both UV orientations place the decal on the same upper/rear region.
        tex, solids, painter = pipeline.prepare(recipe)
        side = next(s for s in solids if s.part == 'side')
        for normal in ((0, 1, 0), (0, -1, 0)):
            layer, color, _ = painter(side, (5.3, 3, 23), normal, tex)
            self.assertEqual(layer, 'side art')
            self.assertGreater(max(color), 60)
            _, lower, _ = painter(side, (5.3, 3, 5), normal, tex)
            self.assertEqual(lower, tuple(recipe['colors']['side']))

    def test_wonderboy_has_plain_offwhite_sides_in_all_views(self):
        recipe = pipeline.load_recipe('wboy')
        self.assertEqual(recipe['sources']['white_cabinet']['kind'], 'user-supplied-reference')
        for _, layers in pipeline.render_recipe(recipe):
            side = [c for _, c in layers['side art'].getcolors() if c[3]]
            self.assertTrue(side)
            self.assertTrue(all(min(c[:3]) > 100 for c in side))
            self.assertTrue(all(max(c[:3]) - min(c[:3]) < 32 for c in side))

    def test_white_molding_does_not_whiten_wonderboy_bezel_or_deck(self):
        recipe = pipeline.load_recipe('wboy')
        tex, solids, painter = pipeline.prepare(recipe)
        upper = next(s for s in solids if s.part == 'upper')
        _, bezel, _ = painter(upper, (9, 4.3, 22), (1, 0, 0), tex)
        self.assertEqual(bezel, (15, 18, 23))
        deck = next(s for s in solids if s.part == 'deck')
        _, fascia, _ = painter(deck, (14, 8, 17), (1, 0, 0), tex)
        self.assertEqual(fascia, (45, 46, 83))

    def test_pedestal_has_an_actual_separate_neck(self):
        recipe = pipeline.load_recipe('mvsc')
        self.assertEqual(recipe['shell'], 'pedestal')
        solids = pipeline.cabinet_upright.model(recipe)
        self.assertEqual(sum(s.part == 'neck' for s in solids), 1)


if __name__ == '__main__':
    unittest.main()
