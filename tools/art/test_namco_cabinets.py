"""Tekken conversion and linked gun twin: provenance, controls and exports."""
from copy import deepcopy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from PIL import Image
import cabinet_pipeline as pipeline
import test_build_mk2 as common


def cabinet_tests(skin):
    class NamcoCabinetTests(common.CabinetTests):
        SIZE = tuple(pipeline.load_recipe(skin)['size'])

        @classmethod
        def setUpClass(cls):
            cls.recipe = pipeline.load_recipe(skin)
            cls.views = pipeline.render_recipe(cls.recipe)

        def test_exported_sprites_and_seven_layers_match_render(self):
            for facing, (flat, _) in zip(pipeline.base.FACINGS, self.views):
                name = f'cabinet_{skin}_{facing}'
                folder = pipeline.ROOT / 'art/objects' / name
                manifest = (folder / 'layers.ron').read_text()
                self.assertIn(f'width: {self.SIZE[0]}', manifest)
                self.assertIn(f'height: {self.SIZE[1]}', manifest)
                composite = Image.new('RGBA', self.SIZE)
                for i in range(7):
                    with Image.open(folder / f'{i}.png') as layer:
                        self.assertEqual(layer.size, self.SIZE)
                        composite.alpha_composite(layer)
                self.assertEqual(composite.tobytes(), flat.tobytes())
                with Image.open(pipeline.ROOT / f'assets/tiles/objects/{name}.png') as sprite:
                    self.assertEqual(sprite.tobytes(), flat.tobytes())

        def test_photographed_screens_are_not_claimed_local_captures(self):
            pipeline.validate_sources(self.recipe)
            sources = self.recipe['sources']
            self.assertEqual(sources['cabinet']['kind'], 'cabinet-photo-unverified')
            self.assertIn('not publisher authorization', sources['cabinet']['rights'])
            self.assertIn('unknown', sources['cabinet']['rights'])
            screens = [v for k, v in self.recipe['textures'].items() if k.startswith('screen')]
            self.assertEqual(len(screens), 2 if skin == 'timecrs2' else 1)
            for screen in screens:
                self.assertEqual(screen['source'], 'cabinet')
                self.assertEqual(screen['fit'], 'contain')
            self.assertTrue(self.recipe['uncertainties'])
            self.assertNotIn('rom_validation', self.recipe)

        def test_front_views_keep_the_game_color_identity(self):
            for _, layers in self.views[:2]:
                colors = [color for _, color in layers['marquee'].getcolors() if color[3]]
                self.assertTrue(any(c[0] > 120 and c[1] > 50 and c[2] < 100 for c in colors))
                if skin == 'timecrs2':
                    controls = [c for _, c in layers['controls'].getcolors() if c[3]]
                    self.assertTrue(any(c[0] > 120 and c[0] > 2 * c[2] for c in controls))
                    self.assertTrue(any(c[2] > 100 and c[2] > 2 * c[0] for c in controls))

    return NamcoCabinetTests


TekkenCabinetTests = cabinet_tests('tekken3')
TimeCrisisCabinetTests = cabinet_tests('timecrs2')


class NamcoControlTests(unittest.TestCase):
    def test_tekken_has_four_actions_per_player_not_six(self):
        r = pipeline.load_recipe('tekken3')
        _, solids, _ = pipeline.prepare(r)
        self.assertEqual(r['players'], 2)
        self.assertEqual(r['buttons_per_player'], 4)
        self.assertEqual(sum(s.part == 'stick' for s in solids), 2)
        self.assertEqual(sum(isinstance(s, pipeline.base.Ball) for s in solids), 2)
        self.assertEqual(sum(s.part == 'button' and isinstance(s, pipeline.base.Solid) for s in solids), 8)
        self.assertNotIn('left', r['textures'], 'Do not invent dedicated character side art')
        self.assertNotIn('controls_alternative', {t.get('source') for t in r['textures'].values()})

    def test_twin_has_two_pistols_and_pedals_not_machine_guns_or_sticks(self):
        r = pipeline.load_recipe('timecrs2')
        _, solids, _ = pipeline.prepare(r)
        self.assertEqual(r['control_kind'], 'holstered-guns')
        for part in ('gun', 'gun_grip', 'holster', 'pedal', 'pedal_guard', 'upper', 'button'):
            self.assertEqual(sum(s.part == part for s in solids), 2, part)
        self.assertFalse(any(s.part in ('stick', 'gun_mount', 'gun_barrel') for s in solids))
        self.assertEqual(sum(s.part == 'hood' for s in solids), 1)
        self.assertEqual({s.color for s in solids if s.part == 'gun'}, {tuple(c) for c in r['gun_colors']})
        for side in ('left', 'right'):
            self.assertEqual(r['textures'][side]['source'], 'cabinet')
            self.assertFalse(r['textures'][side].get('flip_x', False))

    def test_missing_pedal_or_gun_blocks_build(self):
        r = pipeline.load_recipe('timecrs2')
        solids = pipeline.cabinet_twin.model(r)
        for part in ('pedal', 'gun'):
            with patch.object(pipeline.cabinet_twin, 'model', return_value=[s for s in solids if s.part != part]):
                with self.assertRaisesRegex(ValueError, 'controls disagree'):
                    pipeline.prepare(r)

    def test_station_metadata_mismatch_blocks_build(self):
        r = deepcopy(pipeline.load_recipe('timecrs2'))
        r['stations'].append(8)
        with self.assertRaisesRegex(ValueError, 'stations disagree'):
            pipeline.prepare(r)

    def test_unsupported_twin_control_combinations_are_rejected(self):
        original = pipeline.load_recipe('timecrs2')
        for key, value in (('renderer', 'upright'), ('control_stations', 1),
                           ('pedals', 1), ('buttons_per_player', 2),
                           ('control_kind', 'mounted-guns')):
            r = deepcopy(original)
            r[key] = value
            with tempfile.TemporaryDirectory() as folder:
                (Path(folder) / 'timecrs2.json').write_text(json.dumps(r))
                with patch.object(pipeline, 'RECIPES', Path(folder)):
                    with self.assertRaises(ValueError):
                        pipeline.load_recipe('timecrs2')

    def test_catalog_preserves_emulation_and_link_flags(self):
        catalog = (pipeline.ROOT / 'assets/games.ron').read_text()
        self.assertIn('(rom: "tekken3je1", core: "mame", title: "Tekken 3", lockstep: true, cabinets: ["tekken3"])', catalog)
        self.assertIn('(rom: "timecrs2", core: "mame", title: "Time Crisis II", players: 2, gun: true, linked: true, cabinets: ["timecrs2"])', catalog)

    def test_validate_does_not_touch_map_or_catalog(self):
        paths = [pipeline.ROOT / 'assets/games.ron', pipeline.ROOT / 'assets/maps/bar.ron']
        before = [p.read_bytes() for p in paths]
        for skin in ('tekken3', 'timecrs2'):
            pipeline.render_recipe(pipeline.load_recipe(skin))
        self.assertEqual(before, [p.read_bytes() for p in paths])

    def test_batch_preview_fits_taller_wider_canvas(self):
        with Image.open(pipeline.ROOT / 'art/previews/tekken3-timecrs2.png') as image:
            self.assertEqual(image.size, (1152, 830))


if __name__ == '__main__':
    unittest.main()
