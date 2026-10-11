"""Run: python3 -m unittest discover -s tools/art -p 'test_*.py'."""
import json
import unittest

from PIL import Image

import build_avatar as avatar


class AvatarTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog = json.loads((avatar.OUT / "catalog.json").read_text())

    def test_catalog_keys_are_the_drawings(self):
        keys = [[f"#{r:02x}{g:02x}{b:02x}" for r, g, b in shades] for shades in avatar.KEYS.values()]
        self.assertEqual(self.catalog["keys"], keys)
        self.assertEqual(self.catalog["size"], [avatar.WIDTH, avatar.HEIGHT])

    def test_every_item_has_its_picture_and_no_more(self):
        listed = {f"{layer['id']}/{item['id']}.png" for layer in self.catalog["layers"]
                  for item in layer["items"]}
        on_disk = {str(path.relative_to(avatar.OUT)) for path in avatar.OUT.glob("*/*.png")}
        self.assertEqual(listed, on_disk)
        drawn = {f"{layer}/{item}.png" for layer, items in avatar.ITEMS.items() for item in items}
        self.assertEqual(listed, drawn)

    def test_items_use_the_slots_they_list(self):
        keys = {shade: slot for slot, shades in avatar.KEYS.items() for shade in shades}
        palettes = self.catalog["palettes"]
        for layer in self.catalog["layers"]:
            for item in layer["items"]:
                with self.subTest(f"{layer['id']}/{item['id']}"):
                    image = Image.open(avatar.OUT / layer["id"] / f"{item['id']}.png").convert("RGBA")
                    self.assertEqual(image.size, (avatar.WIDTH, avatar.HEIGHT))
                    data = image.tobytes()
                    pixels = (tuple(data[i:i + 4]) for i in range(0, len(data), 4))
                    used = {keys[(r, g, b)] for r, g, b, a in pixels if a and (r, g, b) in keys}
                    slots = item.get("slots", [])
                    self.assertEqual(used, set(range(1, len(slots) + 1)))
                    self.assertEqual(len(item.get("colours", [])), len(slots))
                    for palette in slots:
                        self.assertIn(palette, palettes)

    def test_layers_and_tabs_agree(self):
        layers = [layer["id"] for layer in self.catalog["layers"]]
        self.assertEqual(sorted(self.catalog["tabs"]), sorted(layers))
        for layer in self.catalog["layers"]:
            items = [item["id"] for item in layer["items"]]
            if layer.get("required"):
                self.assertIn(layer.get("default"), items)
            elif "default" in layer:
                self.assertIn(layer["default"], items)
            top, bottom = layer["view"]
            self.assertTrue(0 <= top < bottom <= avatar.HEIGHT)
            for hiding in layer.get("without", []):
                self.assertIn(hiding, layers)

    def test_the_default_character_is_the_default_look(self):
        drawn = Image.open(avatar.ROOT / "assets/characters/player.png").convert("RGBA")
        self.assertEqual(drawn.tobytes(), avatar.compose(self.catalog, avatar.default_look(self.catalog)).tobytes())


if __name__ == "__main__":
    unittest.main()
