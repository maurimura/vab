"""Offline pipeline smoke tests, source safeguards and approved-art regression."""
from copy import deepcopy
from hashlib import sha256
import io
import unittest

from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common


class MetalSlugTests(common.CabinetTests):
    SIZE = (40, 56)

    @classmethod
    def setUpClass(cls):
        cls.recipe = pipeline.load_recipe('mslug')
        cls.views = pipeline.render_recipe(cls.recipe)

    def test_physical_four_buttons_but_three_game_actions(self):
        self.assertEqual(self.recipe['players'], 2)
        self.assertEqual(self.recipe['buttons_per_player'], 4)
        self.assertEqual(self.recipe['game_actions'], ['Shoot', 'Jump', 'Grenade'])
        solids = pipeline.cabinet_mvs.model(self.recipe)
        buttons = [s for s in solids if s.part == 'button' and isinstance(s, pipeline.base.Solid)]
        self.assertEqual(len(buttons), 8)
        self.assertEqual({s.color for s in buttons}, {tuple(b['color']) for b in self.recipe['buttons']})

    def test_original_art_with_honest_source_classification(self):
        sources = self.recipe['sources']
        self.assertEqual(sources['marquee']['kind'], 'original-art-scan')
        self.assertIn('1996 NAZCA', sources['marquee']['rights'])
        self.assertIn('not publisher permission', sources['marquee']['rights'])
        self.assertEqual(sources['cabinet']['author'], 'Beeblebrox (Wikipedia)')
        self.assertEqual(sources['screen']['kind'], 'local-game-capture')
        self.assertGreater(len(self.recipe['uncertainties']), 0)

    def test_exports_match_recipe_render(self):
        for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
            name = f'cabinet_mslug_{facing}'
            path = pipeline.ROOT / 'art/objects' / name
            self.assertIn('width: 40', (path / 'layers.ron').read_text())
            self.assertIn('height: 56', (path / 'layers.ron').read_text())
            composite = Image.new('RGBA', self.SIZE)
            for i in range(7):
                with Image.open(path / f'{i}.png') as layer:
                    composite.alpha_composite(layer)
            self.assertEqual(composite.tobytes(), flat.tobytes())
            with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as saved:
                self.assertEqual(saved.tobytes(), flat.tobytes())


class PipelineTests(unittest.TestCase):
    def test_approved_simpsons_sprites_are_byte_identical(self):
        recipe = pipeline.load_recipe('simpsons')
        for facing, (flat, _) in zip(pipeline.base.FACINGS, pipeline.render_recipe(recipe)):
            encoded = io.BytesIO()
            flat.save(encoded, format='PNG')
            expected = recipe['approved_sha256'][facing]
            self.assertEqual(sha256(encoded.getvalue()).hexdigest(), expected)
            path = pipeline.ROOT / f'assets/tiles/objects/cabinet_simpsons_{facing}.png'
            self.assertEqual(sha256(path.read_bytes()).hexdigest(), expected)

    def test_source_checksum_changes_block_build(self):
        recipe = deepcopy(pipeline.load_recipe('mslug'))
        recipe['sources']['marquee']['sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'checksum changed'):
            pipeline.prepare(recipe)

    def test_missing_source_provenance_blocks_build(self):
        recipe = deepcopy(pipeline.load_recipe('mslug'))
        del recipe['sources']['marquee']['rights']
        with self.assertRaisesRegex(ValueError, 'missing source classification'):
            pipeline.prepare(recipe)

    def test_reference_paths_cannot_escape(self):
        recipe = pipeline.load_recipe('mslug')
        with self.assertRaisesRegex(ValueError, 'inside their cabinet folder'):
            pipeline.reference(recipe, {'file': '../simpsons/front.png'})

    def test_recipe_paths_cannot_escape(self):
        with self.assertRaisesRegex(ValueError, 'identifier'):
            pipeline.load_recipe('../mslug')

    def test_crops_cannot_include_unrelated_page_margins(self):
        recipe = pipeline.load_recipe('mslug')
        with self.assertRaisesRegex(ValueError, 'beyond its source'):
            pipeline.extract(recipe, {'source': 'controls', 'crop': [-1, 0, 735, 230]})

    def test_control_count_mismatch_blocks_build(self):
        recipe = deepcopy(pipeline.load_recipe('mslug'))
        recipe['buttons_per_player'] = 3
        with self.assertRaisesRegex(ValueError, 'controls disagree'):
            pipeline.prepare(recipe)

    def test_validate_does_not_edit_map_or_catalog(self):
        paths = [pipeline.ROOT / 'assets/maps/bar.ron', pipeline.ROOT / 'assets/games.ron']
        before = [p.read_bytes() for p in paths]
        pipeline.render_recipe(pipeline.load_recipe('mslug'))
        self.assertEqual(before, [p.read_bytes() for p in paths])


if __name__ == '__main__':
    unittest.main()
