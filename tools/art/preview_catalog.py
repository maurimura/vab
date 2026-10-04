#!/usr/bin/env python3
"""Native/4x contact sheet for a cabinet batch; no rebuilds or map changes."""
import argparse
from PIL import Image, ImageDraw
import cabinet_pipeline as pipeline


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('skins', nargs='*', default=['mvsc', 'sf2ce', 'pacman', 'atetris', 'wboy'])
    args = parser.parse_args()
    width, row_height = 896, 340
    sheet = Image.new('RGB', (width, 70 + row_height * len(args.skins)), (22, 26, 37))
    draw = ImageDraw.Draw(sheet)
    draw.text((24, 18), 'REMAINING CATALOG / ONLINE REFERENCE BATCH', fill=(233, 237, 247))
    draw.text((24, 40), 'Original-design operator art | four facings at 4x | native row | sources and caveats in recipes', fill=(151, 164, 184))
    for row, skin in enumerate(args.skins):
        recipe = pipeline.load_recipe(skin)
        top = 70 + row * row_height
        draw.text((24, top), recipe['title'].split(' /')[0], fill=(233, 237, 247))
        for column, facing in enumerate(pipeline.base.FACINGS):
            x = 24 + column * 218
            draw.text((x, top + 22), facing.replace('_', ' ').upper(), fill=(151, 164, 184))
            path = pipeline.ROOT / f'assets/tiles/objects/cabinet_{skin}_{facing}.png'
            with Image.open(path) as image:
                enlarged = image.resize((image.width * 4, image.height * 4), Image.Resampling.NEAREST)
                sheet.paste(enlarged, (x + 16, top + 46 + (56 - image.height) * 4), enlarged)
                sheet.paste(image, (x + 72, top + 278 + 56 - image.height), image)
    path = pipeline.ROOT / 'art/previews/catalog-batch.png'
    sheet.save(path)
    print(path.relative_to(pipeline.ROOT))


if __name__ == '__main__':
    main()
