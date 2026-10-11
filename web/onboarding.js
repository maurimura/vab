// The screen a player sees first, while the bar loads behind it (web/index.html): their name and
// how they look, an item and its colours for each layer of the catalog (web/avatar.js). It is
// plain HTML, up before the client has downloaded; Enter goes into the bar once it has. The
// chat's /look opens it again from the bar (client/src/chat.rs). Both are remembered in cookies
// for next time, and a first visit starts from a look picked at random. Someone who has been in
// before (their look is remembered) doesn't pick again: the screen only says how the bar's
// loading goes, and they're in as they were as soon as it has; /look still opens the picker.

import { compose, decode, encode, loadCatalog, randomLook } from "./avatar.js";
import { cookie, remember } from "./cookies.js";

const screen = document.getElementById("onboarding");
const form = document.getElementById("look");
const nameInput = document.getElementById("look-name");
const preview = document.getElementById("look-preview");
const tabs = document.getElementById("look-tabs");
const items = document.getElementById("look-items");
const colours = document.getElementById("look-colours");
/** Where the items and colours scroll, the screen staying the same size. */
const choices = document.querySelector("#look .choices");
const enter = document.getElementById("look-enter");
const status = document.getElementById("look-status");
const close = document.getElementById("look-close");

/** The longest name; the room cuts it there too. */
const MAX_NAME = 20;

let catalog;
/** The look on the screen, and the one last gone in with. */
let look;
let worn;
let tab;
/** Whether the bar has loaded, and whether the player is waiting for it to go in. */
let barLoaded = false;
let waiting = false;

/** Been in before: straight in, without picking. */
const returning = cookie("look") !== undefined;
if (returning) screen.classList.add("returning");

let resolveEntered;
/** The player's first pick, { name, look } (as text), once they went in. */
export const entered = new Promise((resolve) => (resolveEntered = resolve));
/** What Enter (or Done) does with the player's pick, and what closing without one does. */
let done = resolveEntered;
let cancel;

nameInput.value = cookie("name") ?? "";
if (!matchMedia("(pointer: coarse)").matches) nameInput.focus();

loadCatalog().then((loaded) => {
  catalog = loaded;
  look = cookie("look") ? decode(catalog, cookie("look")) : randomLook(catalog);
  tab = catalog.tabs[0];
  render();
  // In before: in as they were, once the bar is up (as an Enter pressed now would).
  if (returning) {
    waiting = true;
    if (barLoaded) go();
  }
});

/** How much of the client has downloaded, in bytes (`total` is 0 when not known). */
export function loading(loadedBytes, total) {
  const percent = total ? `${Math.min(99, Math.floor((loadedBytes / total) * 100))}%` : `${(loadedBytes / 1e6).toFixed(1)} MB`;
  const downloaded = total && loadedBytes >= total;
  const text = downloaded ? "Opening the bar…" : `Loading the bar… ${percent}`;
  status.textContent = text;
  if (waiting) enter.textContent = text;
}

/** The bar is up: Enter goes in. */
export function loaded() {
  barLoaded = true;
  status.textContent = "The bar is open.";
  if (waiting) go();
}

/** Opens the screen again in the bar, with the player's name now; resolves with their new pick,
 * or nothing if they close it. */
export function open(name) {
  screen.classList.remove("returning");
  nameInput.value = name;
  // Not what was tried on and closed without.
  look = structuredClone(worn);
  enter.textContent = "Done";
  enter.disabled = false;
  status.textContent = "";
  close.hidden = false;
  screen.hidden = false;
  render();
  // The keyboard here, for Esc to close it.
  enter.focus();
  return new Promise((resolve) => {
    done = resolve;
    cancel = () => resolve(null);
  });
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  if (!catalog) return;
  if (barLoaded) go();
  else {
    waiting = true;
    enter.disabled = true;
    enter.textContent = status.textContent;
  }
});

function go() {
  waiting = false;
  const name = nameInput.value.split(/\s+/).filter(Boolean).join(" ").slice(0, MAX_NAME);
  if (name) remember("name", name);
  const text = encode(look);
  remember("look", text);
  worn = structuredClone(look);
  hide();
  done({ name, look: text });
}

function hide() {
  screen.hidden = true;
  close.hidden = true;
  cancel = undefined;
}

close.addEventListener("click", () => {
  const closed = cancel;
  hide();
  closed?.();
});
screen.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && cancel) close.click();
});

document.getElementById("look-random").addEventListener("click", () => {
  if (!catalog) return;
  look = randomLook(catalog);
  render();
});

/** Draws a look, or the rows `[top, bottom)` of it, into a canvas. */
function draw(canvas, pixels, [top, bottom] = [0, catalog.size[1]]) {
  const [width] = catalog.size;
  canvas.width = width;
  canvas.height = bottom - top;
  const image = new ImageData(pixels.slice(top * width * 4, bottom * width * 4), width, bottom - top);
  canvas.getContext("2d").putImageData(image, 0, 0);
}

function render() {
  draw(preview, compose(catalog, look));
  renderTabs();
  renderItems();
  renderColours();
}

function renderTabs() {
  tabs.replaceChildren(
    ...catalog.tabs.map((id) => {
      const layer = catalog.layers.find((layer) => layer.id === id);
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = layer.label;
      button.setAttribute("aria-pressed", id === tab);
      button.onclick = () => {
        tab = id;
        render();
        choices.scrollTop = 0;
      };
      return button;
    }),
  );
}

/** The layer's items, each worn by the player as they are (but for what would hide it), and
 * nothing if it can be left off. */
function renderItems() {
  const layer = catalog.layers.find((layer) => layer.id === tab);
  // A layer of one item that must be worn (the skin) has only colours to pick.
  if (layer.required && layer.items.length === 1) {
    items.replaceChildren();
    return;
  }
  const choices = [...(layer.required ? [] : [null]), ...layer.items];
  items.replaceChildren(
    ...choices.map((item) => {
      const button = document.createElement("button");
      button.type = "button";
      button.setAttribute("aria-pressed", (look[layer.id]?.item ?? null) === (item?.id ?? null));
      const canvas = document.createElement("canvas");
      const trying = { ...look };
      if (item) trying[layer.id] = { item: item.id, colours: coloursFor(layer, item) };
      else delete trying[layer.id];
      const shown = { ...trying };
      for (const hiding of layer.without ?? []) delete shown[hiding];
      draw(canvas, compose(catalog, shown), layer.view);
      const label = document.createElement("span");
      label.textContent = item?.label ?? "None";
      button.append(canvas, label);
      button.onclick = () => {
        if (item) look[layer.id] = trying[layer.id];
        else delete look[layer.id];
        render();
      };
      return button;
    }),
  );
}

/** An item's colours when picked: the ones already worn on this layer where their palettes
 * match, so a new shirt keeps the shirt's colour, its own otherwise. */
function coloursFor(layer, item) {
  const worn = look[layer.id];
  const before = worn && layer.items.find((other) => other.id === worn.item);
  return item.slots.map((palette, slot) => (before?.slots[slot] === palette ? worn.colours[slot] : item.colours[slot]));
}

/** A row of swatches for each of the worn item's colours, and one to pick any colour. */
const SLOT_NAMES = ["Colour", "Second colour"];
function renderColours() {
  const layer = catalog.layers.find((layer) => layer.id === tab);
  const worn = look[layer.id];
  const item = worn && layer.items.find((item) => item.id === worn.item);
  colours.replaceChildren(
    ...(item?.slots ?? []).map((palette, slot) => {
      const row = document.createElement("div");
      row.className = "swatches";
      if (item.slots.length > 1) row.dataset.name = SLOT_NAMES[slot] ?? "";
      const set = (colour) => {
        worn.colours[slot] = colour;
        render();
      };
      for (const colour of catalog.palettes[palette]) {
        const swatch = document.createElement("button");
        swatch.type = "button";
        swatch.style.background = colour;
        swatch.title = colour;
        swatch.setAttribute("aria-label", colour);
        swatch.setAttribute("aria-pressed", colour === worn.colours[slot]);
        swatch.onclick = () => set(colour);
        row.append(swatch);
      }
      const any = document.createElement("input");
      any.type = "color";
      any.title = "Any colour";
      any.value = worn.colours[slot];
      any.classList.toggle("picked", !catalog.palettes[palette].includes(worn.colours[slot]));
      any.oninput = () => {
        worn.colours[slot] = any.value;
        draw(preview, compose(catalog, look));
      };
      any.onchange = () => set(any.value);
      row.append(any);
      return row;
    }),
  );
}
