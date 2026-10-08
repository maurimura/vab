"""Original Virtua Tennis art: NAOMI shell, honest sources and offline exports."""
import unittest
from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common


class VirtuaTennisCabinetTests(common.CabinetTests):
    SIZE = (40, 60)

    @classmethod
    def setUpClass(cls):
        cls.recipe = pipeline.load_recipe('vtennis')
        cls.views = pipeline.render_recipe(cls.recipe)

    def test_exported_sprites_and_editable_layers_match(self):
        for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
            name = f'cabinet_vtennis_{facing}'
            folder = pipeline.ROOT / 'art/objects' / name
            manifest = (folder / 'layers.ron').read_text()
            self.assertIn('width: 40', manifest)
            self.assertIn('height: 60', manifest)
            composite = Image.new('RGBA', self.SIZE)
            for index in range(7):
                with Image.open(folder / f'{index}.png') as layer:
                    self.assertEqual(layer.size, self.SIZE)
                    composite.alpha_composite(layer)
            self.assertEqual(composite.tobytes(), flat.tobytes())
            with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as sprite:
                self.assertEqual(sprite.tobytes(), flat.tobytes())

    def test_two_stations_three_physical_buttons_but_two_game_actions(self):
        _, solids, _ = pipeline.prepare(self.recipe)
        self.assertEqual(self.recipe['players'], 2)
        self.assertEqual(self.recipe['control_stations'], 2)
        self.assertEqual(self.recipe['game_actions_per_player'], 2)
        self.assertEqual(len([s for s in solids if s.part == 'stick']), 2)
        self.assertEqual(len([s for s in solids if isinstance(s, pipeline.base.Ball)]), 2)
        self.assertEqual(len([s for s in solids if s.part == 'button'
                              and isinstance(s, pipeline.base.Solid)]), 6)

    def test_naomi_has_separate_rails_pod_base_billboard_and_open_knee_space(self):
        tex, solids, painter = pipeline.prepare(self.recipe)
        self.assertEqual(self.recipe['renderer'], 'naomi')
        parts = [s.part for s in solids]
        self.assertEqual(parts.count('rail'), 2)
        for part in ('crt_back', 'base', 'monitor', 'billboard', 'deck'):
            self.assertIn(part, parts)
        # A horizontal ray below the deck must pass through the center opening.
        self.assertTrue(all(s.hit((20, 8, 12), (-1, 0, 0)) is None for s in solids))
        monitor = next(s for s in solids if s.part == 'monitor')
        self.assertEqual(painter(monitor, (8.7, 8, 30.4), (1, 0, 0), tex),
                         ('trim', (255, 160, 30), True))

    def test_catalog_selects_skin_without_changing_gd_rom_settings(self):
        self.assertEqual(self.recipe['rom'], 'vtennisg')
        catalog = (pipeline.ROOT / 'assets/games.ron').read_text()
        self.assertIn('(rom: "vtennisg", core: "flycast", title: "Virtua Tennis", '
                      'lockstep: true, files: ["vtennisg/gds-0011.chd"], '
                      'cabinets: ["vtennis"])', catalog)

    def test_flyer_art_not_conversion_panel_or_another_games_screen(self):
        pipeline.validate_sources(self.recipe)
        self.assertEqual(self.recipe['sources']['flyer']['kind'], 'original-art-scan')
        self.assertTrue(all(s['source'] == 'flyer' for s in self.recipe['textures'].values()))
        self.assertEqual(self.recipe['textures']['screen']['fit'], 'contain')
        self.assertNotIn('left', self.recipe['textures'])
        self.assertNotIn('right', self.recipe['textures'])
        self.assertIn('not publisher authorization', self.recipe['sources']['flyer']['rights'])
        self.assertTrue(self.recipe['uncertainties'])


if __name__ == '__main__':
    unittest.main()
