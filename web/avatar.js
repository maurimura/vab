// What players wear (assets/avatar/catalog.json, drawn by tools/art/build_avatar.py). A look is
// an item and its colours for each layer, put together here into the picture the bar draws for
// a player (client/src/room.rs) and the onboarding screen shows (web/onboarding.js).
//
// The room passes looks on as text, "layer=item/colour/colour ..." with colours in hex without
// the #, and a layer left out wears nothing. Whatever it doesn't know (an item from a newer
// catalog, a colour that isn't one) is read as the layer's default, so looks keep working as
// the catalog grows.

const BASE = "./assets/avatar/";

/** The catalog, with each item's pixels: loaded once. */
let loading;
export function loadCatalog() {
  loading ??= (async () => {
    const catalog = await (await fetch(`${BASE}catalog.json`)).json();
    catalog.keys = catalog.keys.map((shades) => shades.map(rgb));
    await Promise.all(
      catalog.layers.flatMap((layer) =>
        layer.items.map(async (item) => {
          item.slots ??= [];
          item.colours ??= [];
          item.pixels = await pixels(`${BASE}${layer.id}/${item.id}.png`, catalog.size);
        }),
      ),
    );
    return catalog;
  })();
  return loading;
}

async function pixels(url, [width, height]) {
  const image = await createImageBitmap(await (await fetch(url)).blob());
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  context.drawImage(image, 0, 0);
  return context.getImageData(0, 0, width, height).data;
}

const HEX = /^#?[0-9a-f]{6}$/i;
/** "#rrggbb" as [r, g, b]. */
const rgb = (colour) => [1, 3, 5].map((i) => parseInt(colour.slice(i, i + 2), 16));

/** The look a newcomer starts with: each layer's default. */
export function defaultLook(catalog) {
  const look = {};
  for (const layer of catalog.layers) {
    const item = layer.items.find((item) => item.id === layer.default);
    if (item) look[layer.id] = { item: item.id, colours: [...item.colours] };
  }
  return look;
}

/** A look as the room passes it on. */
export function encode(look) {
  return Object.entries(look)
    .map(([layer, { item, colours }]) => [`${layer}=${item}`, ...colours.map((c) => c.slice(1))].join("/"))
    .join(" ");
}

/** A look from the room, or a cookie, as far as this catalog can tell. */
export function decode(catalog, text) {
  const said = new Map(
    (text ?? "")
      .split(" ")
      .filter(Boolean)
      .map((part) => {
        const [layer, rest = ""] = part.split("=");
        const [item, ...colours] = rest.split("/");
        return [layer, { item, colours }];
      }),
  );
  if (!said.size) return defaultLook(catalog);
  const look = {};
  for (const layer of catalog.layers) {
    const wanted = said.get(layer.id);
    const byId = (id) => layer.items.find((item) => item.id === id);
    const item = wanted || layer.required ? (byId(wanted?.item) ?? byId(layer.default)) : undefined;
    if (!item) continue;
    look[layer.id] = {
      item: item.id,
      colours: item.slots.map((_, slot) => {
        const colour = item.id === wanted?.item ? wanted.colours[slot] : undefined;
        return colour && HEX.test(colour) ? `#${colour.replace("#", "").toLowerCase()}` : item.colours[slot] ?? "#808080";
      }),
    };
  }
  return look;
}

/** Something to start from: an item for each layer (most of the time, for those that can be
 * left off) in colours from its palettes. */
export function randomLook(catalog) {
  const pick = (list) => list[Math.floor(Math.random() * list.length)];
  const look = {};
  for (const layer of catalog.layers) {
    if (!layer.required && Math.random() > (layer.default ? 0.9 : 0.35)) continue;
    const item = pick(layer.items);
    look[layer.id] = { item: item.id, colours: item.slots.map((palette) => pick(catalog.palettes[palette])) };
  }
  return look;
}

/**
 * A colour's light, mid and dark shades: lighter towards yellow and darker towards blue, as
 * pixel art shades (tools/art/build_avatar.py does the same).
 */
export function ramp(colour) {
  const [h, l, s] = hls(rgb(colour));
  const towards = (target, degrees) => {
    const delta = ((target - h + 540) % 360) - 180;
    return (h + Math.max(-degrees, Math.min(degrees, delta)) + 360) % 360;
  };
  return [fromHls(towards(60, 8), l + 0.12, s * 0.9), rgb(colour), fromHls(towards(250, 12), l - 0.16, s * 0.8)];
}

/** As Python's colorsys: hue in degrees, lightness and saturation 0 to 1. */
function hls([r, g, b]) {
  [r, g, b] = [r / 255, g / 255, b / 255];
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, l, 0];
  const d = max - min;
  const s = l <= 0.5 ? d / (max + min) : d / (2 - max - min);
  const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, l, s];
}

function fromHls(h, l, s) {
  l = Math.max(0, Math.min(1, l));
  s = Math.max(0, Math.min(1, s));
  if (!s) return [l * 255, l * 255, l * 255];
  const m2 = l <= 0.5 ? l * (1 + s) : l + s - l * s;
  const m1 = 2 * l - m2;
  const v = (hue) => {
    hue = ((hue % 360) + 360) % 360;
    if (hue < 60) return m1 + (m2 - m1) * (hue / 60);
    if (hue < 180) return m2;
    if (hue < 240) return m1 + (m2 - m1) * ((240 - hue) / 60);
    return m1;
  };
  return [v(h + 120) * 255, v(h) * 255, v(h - 120) * 255].map(Math.round);
}

/** The look's layers on top of each other: RGBA, the catalog's size. */
export function compose(catalog, look) {
  const [width, height] = catalog.size;
  const out = new Uint8ClampedArray(width * height * 4);
  for (const layer of catalog.layers) {
    const chosen = look[layer.id];
    const item = chosen && layer.items.find((item) => item.id === chosen.item);
    if (!item) continue;
    // Each slot's reserved greys, as one number, to the chosen colour's shades.
    const swaps = new Map();
    chosen.colours.forEach((colour, slot) => {
      const keys = catalog.keys[slot];
      if (!keys) return;
      ramp(colour).forEach((shade, i) => swaps.set(key(keys[i]), shade));
    });
    const from = item.pixels;
    for (let i = 0; i < out.length; i += 4) {
      const alpha = from[i + 3] / 255;
      if (!alpha) continue;
      const [r, g, b] = swaps.get(key([from[i], from[i + 1], from[i + 2]])) ?? [from[i], from[i + 1], from[i + 2]];
      // Over what's there already, in straight alpha.
      const under = (out[i + 3] / 255) * (1 - alpha);
      const total = alpha + under;
      out[i] = (r * alpha + out[i] * under) / total;
      out[i + 1] = (g * alpha + out[i + 1] * under) / total;
      out[i + 2] = (b * alpha + out[i + 2] * under) / total;
      out[i + 3] = total * 255;
    }
  }
  return out;
}

const key = ([r, g, b]) => (r << 16) | (g << 8) | b;
