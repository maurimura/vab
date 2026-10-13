"""Tecmo World Cup '98 reference-based conversion art and bat-grip regression."""
from copy import deepcopy
import unittest
from unittest.mock import patch

from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common


class TecmoWorldCup98Tests(common.CabinetTests):
    @classmethod
    def setUpClass(cls):
        cls.recipe = pipeline.load_recipe('twcup98')
        cls.views = pipeline.render_recipe(cls.recipe)

    def test_photographed_physical_controls(self):
        self.assertEqual(self.recipe['players'], 2)
        self.assertEqual(self.recipe['control_stations'], 2)
        self.assertEqual(self.recipe['buttons_per_player'], 3)
        solids = pipeline.cabinet_upright.model(self.recipe)
        self.assertEqual(sum(s.part == 'stick' for s in solids), 2)
        self.assertEqual(sum(s.part == 'grip' for s in solids), 2)
        self.assertEqual(sum(isinstance(s, pipeline.base.Ball) for s in solids), 0)
        buttons = [s for s in solids if s.part == 'button']
        self.assertEqual(len(buttons), 6)
        self.assertEqual({s.color for s in buttons},
                         {tuple(b['color']) for b in self.recipe['buttons']})

    def test_no_invented_sides_or_false_capture_claim(self):
        textures = self.recipe['textures']
        self.assertNotIn('left', textures)
        self.assertNotIn('right', textures)
        self.assertEqual(textures['screen']['source'], 'flyer_back')
        self.assertEqual(textures['screen']['fit'], 'contain')
        self.assertEqual(self.recipe['sources']['flyer_back']['kind'], 'original-print-scan')
        self.assertNotIn('operator_art', {t.get('source') for t in textures.values()})
        self.assertIn('not a fresh local-ROM capture', ' '.join(self.recipe['uncertainties']))
        self.assertTrue(all(s['sha256'] == s['download_sha256']
                            for s in self.recipe['sources'].values()))

    def test_bat_grip_count_is_validated(self):
        solids = pipeline.cabinet_upright.model(self.recipe)
        without_grips = [s for s in solids if s.part != 'grip']
        with patch.object(pipeline.cabinet_upright, 'model', return_value=without_grips):
            with self.assertRaisesRegex(ValueError, 'controls disagree'):
                pipeline.prepare(self.recipe)

    def test_default_ball_grips_remain_supported(self):
        changed = deepcopy(self.recipe)
        del changed['joystick_grip']
        _, solids, _ = pipeline.prepare(changed)
        self.assertEqual(sum(isinstance(s, pipeline.base.Ball) for s in solids), 2)
        self.assertEqual(sum(s.part == 'grip' for s in solids), 0)
        changed['buttons_per_player'] = 4
        with self.assertRaisesRegex(ValueError, 'controls disagree'):
            pipeline.prepare(changed)

    def test_saved_sprites_and_editable_layers_match_render(self):
        for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
            name = f'cabinet_twcup98_{facing}'
            folder = pipeline.ROOT / 'art/objects' / name
            composite = Image.new('RGBA', self.SIZE)
            for index in range(7):
                with Image.open(folder / f'{index}.png') as layer:
                    composite.alpha_composite(layer)
            self.assertEqual(composite.tobytes(), flat.tobytes())
            metadata = (folder / 'layers.ron').read_text()
            self.assertIn('width: 32', metadata)
            self.assertIn('height: 48', metadata)
            with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as saved:
                self.assertEqual(saved.tobytes(), flat.tobytes())


if __name__ == '__main__':
    unittest.main()
