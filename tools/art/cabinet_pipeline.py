#!/usr/bin/env python3
"""Offline, recipe-driven cabinet build/validation; research is the agent's job.

python3 tools/art/cabinet_pipeline.py {validate,build} {mslug,simpsons}
Never fetches references, edits the map/catalog, uploads, or deploys.
"""
import argparse
from functools import partial
from hashlib import sha256
import json
from pathlib import Path
import re

from PIL import Image, ImageDraw, ImageEnhance, ImageFilter
import build_mk2 as base
import build_snowbros as previews
import build_simpsons
import cabinet_mvs
import cabinet_upright
import cabinet_wide
import cabinet_twin

ROOT = base.ROOT
RECIPES = ROOT / 'tools/art/recipes'
RENDERERS = {'simpsons-legacy': build_simpsons, 'mvs': cabinet_mvs,
             'upright': cabinet_upright, 'wide': cabinet_wide, 'twin-gun': cabinet_twin}


def load_recipe(name):
    if not re.fullmatch(r'[a-z][a-z0-9_]*', name):
        raise ValueError('Recipe name must be a simple lowercase identifier')
    recipe = json.loads((RECIPES / f'{name}.json').read_text())
    if recipe.get('version') != 1 or recipe.get('skin') != name:
        raise ValueError('Recipe version/skin mismatch')
    if recipe.get('renderer') not in RENDERERS:
        raise ValueError('Unknown renderer; register reviewed code explicitly')
    if recipe.get('players') not in range(1, 5):
        raise ValueError('Invalid player count')
    if recipe.get('buttons_per_player') not in range(1, 7):
        raise ValueError('Invalid physical button count')
    kind = recipe.get('control_kind', 'joystick')
    if kind not in ('joystick', 'buttons', 'mounted-guns', 'holstered-guns'):
        raise ValueError('Invalid physical control kind')
    if kind == 'holstered-guns':
        if recipe['renderer'] != 'twin-gun' or recipe.get('control_stations') != 2:
            raise ValueError('Holstered guns require the reviewed two-station twin renderer')
        if recipe['buttons_per_player'] != 1 or recipe.get('pedals') != 2:
            raise ValueError('Twin controls require one trigger and one pedal per station')
    elif recipe['renderer'] == 'twin-gun':
        raise ValueError('Twin renderer requires holstered guns')
    elif kind != 'joystick' and recipe['renderer'] != 'upright':
        raise ValueError('Special controls require the reviewed upright renderer')
    if recipe.get('control_stations', recipe['players']) not in range(1, 5):
        raise ValueError('Invalid physical station count')
    if len(recipe.get('size', [])) != 2 or any(type(n) is not int or not 16 <= n <= 128 for n in recipe['size']):
        raise ValueError('Invalid sprite size')
    return recipe


def reference(recipe, source):
    folder = (ROOT / 'art/references' / recipe['skin']).resolve()
    path = (folder / source['file']).resolve()
    if not path.is_relative_to(folder) or path == folder:
        raise ValueError('References must stay inside their cabinet folder')
    return path


def validate_sources(recipe):
    sources = recipe.get('sources', {})
    if not sources:
        raise ValueError('Missing source provenance')
    for name, source in sources.items():
        path = reference(recipe, source)
        if sha256(path.read_bytes()).hexdigest() != source.get('sha256'):
            raise ValueError(f'{name}: source checksum changed')
        if not source.get('kind') or not source.get('rights') or not source.get('purpose'):
            raise ValueError(f'{name}: missing source classification, rights or purpose')
        if source['kind'] not in ('local-game-capture', 'user-supplied-reference') and not source.get('page_url', '').startswith(('https://', 'http://')):
            raise ValueError(f'{name}: missing source page URL')
        with Image.open(path) as image:
            image.verify()


def extract(recipe, spec):
    image = Image.open(reference(recipe, recipe['sources'][spec['source']])).convert('RGBA').convert('RGB')
    if 'mask_polygon' in spec:
        mask = Image.new('L', image.size)
        ImageDraw.Draw(mask).polygon([tuple(p) for p in spec['mask_polygon']], fill=255)
        background = Image.new('RGB', image.size, tuple(spec['background']))
        background.paste(image, mask=mask)
        image = background
    if 'crop' in spec:
        left, top, right, bottom = spec['crop']
        if not (0 <= left < right <= image.width and 0 <= top < bottom <= image.height):
            raise ValueError('Crop extends beyond its source')
        image = image.crop(spec['crop'])
    if 'quad' in spec:
        points = spec['quad']
        if len(points) != 4 or any(len(p) != 2 or not (0 <= p[0] < image.width and 0 <= p[1] < image.height) for p in points):
            raise ValueError('Invalid rectification corners')
        a, b, c, d = points
        image = image.transform(tuple(spec.get('rectified_size', (160, 160))),
                                Image.Transform.QUAD, (*a, *d, *c, *b), Image.Resampling.BICUBIC)
    turns = spec.get('quarter_turns', 0)
    if type(turns) is not int or turns not in range(4):
        raise ValueError('Invalid image rotation')
    for _ in range(turns):
        image = image.transpose(Image.Transpose.ROTATE_90)
    if spec.get('flip_x'):
        image = image.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    return image


def textures(recipe):
    """Crops, corner rectification and original-ink patches; no invented imagery."""
    result = {}
    for name, spec in recipe['textures'].items():
        if 'canvas' in spec:
            image = Image.new('RGB', tuple(spec['canvas']), tuple(spec['fill']))
            draw = ImageDraw.Draw(image)
            for rect in spec.get('rectangles', []):
                draw.rectangle(rect['box'], fill=tuple(rect['color']))
            for patch in spec.get('patches', []):
                source = extract(recipe, patch)
                x, y, w, h = patch['placement']
                if min(x, y) < 0 or min(w, h) <= 0 or x + w > image.width or y + h > image.height:
                    raise ValueError('Artwork patch extends beyond its texture')
                source = source.resize((w, h), Image.Resampling.BOX)
                if patch.get('white_ink'):
                    # Preserve the photographed logo strokes while removing red lighting.
                    mask = Image.new('L', source.size)
                    for row in range(h):
                        for col in range(w):
                            r, g, b = source.getpixel((col, row))
                            alpha = max(0, min(255, round((min(r, g, b) - 100) * 255 / 100)))
                            mask.putpixel((col, row), alpha)
                    image.paste(Image.new('RGB', source.size, (245, 241, 223)), (x, y), mask)
                else:
                    image.paste(source, (x, y))
        else:
            image = extract(recipe, spec)
        line_boost = spec.get('line_boost', 1)
        if type(line_boost) is not int or line_boost not in (1, 3, 5):
            raise ValueError('Invalid screen line boost')
        if line_boost != 1:
            if name != 'screen':
                raise ValueError('Line boost is only for thin gameplay strokes')
            image = image.filter(ImageFilter.MaxFilter(line_boost))
        size = tuple(spec['size'])
        if spec.get('fit') == 'contain':
            image.thumbnail(size, Image.Resampling.BOX)
            fitted = Image.new('RGB', size, tuple(spec.get('background', (0, 0, 0))))
            fitted.paste(image, ((size[0] - image.width) // 2, (size[1] - image.height) // 2))
            result[name] = fitted
        else:
            result[name] = image.resize(size, Image.Resampling.BOX)
        gain = spec.get('brightness', 1.0)
        if not isinstance(gain, (int, float)) or not .1 <= gain <= 4:
            raise ValueError('Invalid texture brightness')
        if gain != 1.0:
            result[name] = ImageEnhance.Brightness(result[name]).enhance(gain)
    return result


def prepare(recipe):
    validate_sources(recipe)
    module = RENDERERS[recipe['renderer']]
    stations = recipe.get('control_stations', recipe['players'])
    if len(recipe.get('stations', range(stations))) != stations:
        raise ValueError('Physical stations disagree with the recipe')
    if recipe['renderer'].endswith('-legacy'):
        tex, solids, painter = module.textures(), module.model(), module.paint
    else:
        tex, solids = textures(recipe), module.model(recipe)
        painter = partial(module.paint, recipe=recipe)
    sticks = sum(s.part == 'stick' for s in solids)
    balls = sum(isinstance(s, base.Ball) for s in solids)
    buttons = sum(s.part == 'button' and isinstance(s, base.Solid) for s in solids)
    kind = recipe.get('control_kind', 'joystick')
    expected_sticks = stations if kind == 'joystick' else 0
    guns = sum(s.part == 'gun' for s in solids)
    expected_guns = stations if kind in ('mounted-guns', 'holstered-guns') else 0
    pedals = sum(s.part == 'pedal' for s in solids)
    if (sticks != expected_sticks or balls != sticks or guns != expected_guns
            or pedals != recipe.get('pedals', 0)
            or buttons != stations * recipe['buttons_per_player']):
        raise ValueError('Physical controls disagree with the recipe')
    return tex, solids, painter


def check_view(flat, layers, size, turns):
    if flat.size != tuple(size) or tuple(layers) != base.LAYERS:
        raise ValueError('Wrong dimensions or layer layout')
    bounds = flat.getbbox()
    if not bounds or not (bounds[0] > 0 and bounds[1] > 0 and bounds[2] < size[0] and bounds[3] < size[1]):
        raise ValueError('Sprite is empty or clips its canvas')
    if set(flat.getchannel('A').tobytes()) != {0, 255}:
        raise ValueError('Sprite has non-pixel-art transparency')
    composite = Image.new('RGBA', tuple(size))
    for layer in layers.values():
        composite.alpha_composite(layer)
    if composite.tobytes() != flat.tobytes():
        raise ValueError('Editable layers do not recompose')
    for name in ('screen', 'marquee', 'front'):
        if (layers[name].getbbox() is not None) != (turns < 2):
            raise ValueError(f'{name}: front detail appears in the wrong view')


def render_recipe(recipe):
    tex, solids, painter = prepare(recipe)
    w, h = recipe['size']
    views = []
    for turns in range(4):
        flat, layers = base.render(solids, tex, turns, painter=painter, width=w, height=h)
        check_view(flat, layers, recipe['size'], turns)
        views.append((flat, layers))
    return views


def build(recipe):
    # Validate every view before writing any generated asset.
    views = render_recipe(recipe)
    for facing, (flat, layers) in zip(base.FACINGS, views):
        base.save_view(facing, flat, layers, prefix=f"cabinet_{recipe['skin']}")
    previews.preview([flat for flat, _ in views], title=recipe['title'],
                     subtitle=recipe['preview_note'], filename=f"{recipe['skin']}.png")
    return views


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('validate', 'build'))
    parser.add_argument('recipe')
    args = parser.parse_args()
    recipe = load_recipe(args.recipe)
    if args.action == 'build':
        build(recipe)
    else:
        render_recipe(recipe)
    print(f"{args.recipe}: {args.action} passed (sources, controls, four views, layers, clipping)")


if __name__ == '__main__':
    main()
