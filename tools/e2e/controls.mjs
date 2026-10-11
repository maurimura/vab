// The controls panel (client/src/controls.rs), for every game and for one. In the bar, the gear
// (top right) opens every game's, and a second click closes it; there the arrows turn a wheel of the panel's own there (and don't
// walk), + slows it and Steer left goes on K, and Mortal Kombat II and the driving game both
// have K. At the driving game, the gear opens the game's own, over those (Tab too): the arrows still turn the
// game's wheel, + slows it more, Steer left goes on J (not on Y, the chat's), Esc closes the
// panel only, and Mortal Kombat II keeps every game's K. It's all kept over a reload; R at the
// game puts back every game's, and R in the bar (/controls this time) the usual ones. On top of
// the panel, the player's name: Enter opens the page's name box with it, Esc there leaves it,
// and a new one typed and Enter is the player's, kept in the cookie.
// GAME picks the driving game (Out Run unless said; crusnusa41 for Cruis'n USA).
import { check, player, run, sleep, state, use, waitFor } from "./lib.mjs";

const GAME = process.env.GAME ?? "outrun";
const OTHER = "mk2";
const round = (seconds) => Math.round(seconds * 100) / 100;
const sameRamp = (a, b) => a.lock === b.lock && a.back === b.back && a.curve === b.curve;

/** Presses Up or Down until `row` (as the panel reports it: "Lock", "Key(6)") is picked. */
async function pick(it, row) {
  const s = await state(it);
  const to = s.controls.rows.indexOf(row);
  if (to < 0) throw new Error(`no ${row} among ${s.controls.rows}`);
  const key = to > s.controls.selected ? "ArrowDown" : "ArrowUp";
  for (let i = 0; i < Math.abs(to - s.controls.selected); i++) await it.page.keyboard.press(key);
  await waitFor(it, (s) => s.controls.selected === to, { what: `${row} picked` });
}

/** Holds `key` until the wheel is past `past` (-1..1 toward the key's side), then lets go. */
async function steer(it, key, past) {
  await it.page.keyboard.down(key);
  await waitFor(it, (s) => (past > 0 ? s.controls.turned > past : s.controls.turned < past), {
    what: `${key} turning the wheel`,
    timeout: 5000,
  });
  await it.page.keyboard.up(key);
  return waitFor(it, (s) => s.controls.turned === 0, { what: "the wheel straight again", timeout: 5000 });
}

/** Puts button `id` on `key` in the panel up. */
async function bind(it, id, key) {
  await pick(it, `Key(${id})`);
  await it.page.keyboard.press("Enter");
  await waitFor(it, (s) => s.controls.binding, { what: "the panel waiting for a key" });
  await it.page.keyboard.press(key);
  return waitFor(it, (s) => !s.controls.binding, { what: `${key} taken` });
}

/** Sits at `game`, and waits for it to say what its buttons are. */
async function sit(it, game) {
  await use(it, game, "Playing");
  return waitFor(it, (s) => s.controls?.named > 0, { what: `${game}'s buttons`, timeout: 60000 });
}

/** Closes the panel if it's up, and stands up. */
async function standUp(it) {
  if ((await state(it)).controls.open) {
    await it.page.keyboard.press("Escape");
    await waitFor(it, (s) => !s.controls.open, { what: "the panel closed" });
    await sleep(200);
  }
  await it.page.keyboard.press("Escape");
  await waitFor(it, (s) => s.mode === "Walking", { what: "the bar" });
}

/** Clicks the gear, top right (1280 by 720: lib.mjs), until the panel is `open` or not. */
async function gear(it, open = true) {
  await it.page.mouse.click(1280 - 8 - 18, 8 + 18);
  return waitFor(it, (s) => s.controls.open === open, { what: `the gear ${open ? "opening" : "closing"} the panel` });
}

/** `/controls` in the chat. */
async function controlsCommand(it) {
  await it.page.keyboard.press("KeyY");
  await sleep(200);
  await it.page.keyboard.type("/controls");
  await it.page.keyboard.press("Enter");
  return waitFor(it, (s) => s.controls.open, { what: "the panel" });
}

await run("The controls panel, for every game and for one", async () => {
  const one = await player("one", `e2e-controls-${Date.now()}`);
  await one.page.evaluate(() => localStorage.removeItem("controls"));
  let s = await sit(one, GAME);
  const usual = s.controls.wheel;
  check(!s.controls.open && s.controls.keys["6"] === "Left", `${GAME}: Left steers left`);
  await standUp(one);

  // In the bar: every game's.
  s = await gear(one);
  check(s.controls.scope === "every" && s.controls.rows.length === 17, "the gear in the bar: your name, every game's buttons and wheel");
  s = await gear(one, false);
  check(s.mode === "Walking", "the gear again closes it");
  s = await gear(one);

  // The name, first in the panel.
  const nameBox = () => one.page.evaluate(() => !document.getElementById("name").hidden);
  const was = s.controls.name;
  await pick(one, "Name");
  await one.page.keyboard.press("Enter");
  await waitForBox(true);
  check(await one.page.evaluate((was) => document.activeElement.value === was, was), `Enter on Your name opens the name box with "${was}"`);
  await one.page.keyboard.type("Nobody");
  await one.page.keyboard.press("Escape");
  await waitForBox(false);
  await sleep(300);
  s = await state(one);
  check(s.controls.name === was && s.controls.open, "Esc in the box leaves the name, and the panel up");
  await one.page.keyboard.press("Enter");
  await waitForBox(true);
  await one.page.keyboard.type("Racer X");
  await one.page.keyboard.press("Enter");
  s = await waitFor(one, (s) => s.controls.name === "Racer X", { what: "the new name" });
  const cookie = await one.page.evaluate(() => document.cookie);
  check(!(await nameBox()) && cookie.includes("name=Racer%20X"), "a name typed and Enter is the player's, kept for next time");
  async function waitForBox(open) {
    for (let i = 0; i < 50 && (await nameBox()) !== open; i++) await sleep(100);
    if ((await nameBox()) !== open) throw new Error(`the name box never ${open ? "opened" : "closed"}`);
  }
  const cell = JSON.stringify(s.player.cell);
  s = await steer(one, "ArrowRight", 0.95);
  check(JSON.stringify(s.player.cell) === cell, "Right turns the panel's wheel to full lock and back, and doesn't walk");
  const first = s.controls.wheel;
  await pick(one, "Lock");
  await one.page.keyboard.press("Equal");
  await one.page.keyboard.press("Equal");
  const everyLock = round(first.lock + 0.1);
  s = await waitFor(one, (s) => s.controls.wheel.lock === everyLock, { what: "every game's wheel slower" });
  const every = s.controls.wheel;
  check(true, `+ twice: every game's wheel to full lock in ${everyLock} s`);
  s = await bind(one, 6, "KeyK");
  check(s.controls.keys["6"] === "K", "every game's Steer left on K");
  await one.page.keyboard.press("Escape");
  await waitFor(one, (s) => !s.controls.open, { what: "the panel closed" });
  check((await state(one)).mode === "Walking", "Esc closes it too, in the bar");

  s = await sit(one, OTHER);
  check(s.controls.keys["6"] === "K", `${OTHER} has every game's K for left`);
  await standUp(one);

  // At the driving game: every game's, then its own over them.
  s = await sit(one, GAME);
  check(s.controls.keys["6"] === "K" && sameRamp(s.controls.wheel, every), `${GAME} has every game's K and wheel`);
  s = await gear(one);
  check(s.controls.scope === GAME, "the gear at the game opens the game's own");
  await steer(one, "KeyK", -0.95);
  check(true, "with the panel up, K turns the game's wheel to full left lock, and let go it's straight");
  await pick(one, "Lock");
  await one.page.keyboard.press("Equal");
  const gameLock = round(everyLock + 0.05);
  s = await waitFor(one, (s) => s.controls.wheel.lock === gameLock, { what: "the game's wheel slower" });
  check(true, `+: ${GAME}'s own wheel to full lock in ${gameLock} s`);
  await pick(one, "Key(6)");
  await one.page.keyboard.press("Enter");
  await waitFor(one, (s) => s.controls.binding, { what: "the panel waiting for a key" });
  await one.page.keyboard.press("KeyY");
  s = await waitFor(one, (s) => s.controls.note !== "", { what: "Y refused" });
  check(s.controls.binding && s.controls.keys["6"] === "K", `Y is the bar's: "${s.controls.note}"`);
  await one.page.keyboard.press("KeyJ");
  s = await waitFor(one, (s) => !s.controls.binding, { what: "J taken" });
  check(s.controls.keys["6"] === "J", `${GAME}'s own Steer left on J`);
  await one.page.keyboard.press("Escape");
  await waitFor(one, (s) => !s.controls.open, { what: "the panel closed" });
  await sleep(300);
  check((await state(one)).mode === "Playing", "Esc closes the panel, and the player is still playing");
  await steer(one, "KeyJ", -0.5);
  check(true, "J turns the wheel left, the panel closed");
  await standUp(one);

  s = await sit(one, OTHER);
  check(s.controls.keys["6"] === "K", `${OTHER} still has every game's K`);
  await standUp(one);

  // Kept for next time.
  await one.page.reload({ waitUntil: "load" });
  await waitFor(one, (s) => s.mode === "Walking", { what: "the bar again", timeout: 60000 });
  await one.page.mouse.click(640, 360);
  s = await sit(one, GAME);
  check(s.controls.keys["6"] === "J" && s.controls.wheel.lock === gameLock, "after a reload the game's own are as left");

  // R at the game: every game's again.
  await one.page.keyboard.press("Tab");
  await waitFor(one, (s) => s.controls.open, { what: "the game's panel" });
  await one.page.keyboard.press("KeyR");
  s = await waitFor(one, (s) => s.controls.keys["6"] === "K", { what: "every game's K back" });
  check(sameRamp(s.controls.wheel, every), "R at the game puts back every game's K and wheel");
  await standUp(one);

  // R in the bar: the usual.
  await controlsCommand(one);
  await one.page.keyboard.press("KeyR");
  s = await waitFor(one, (s) => s.controls.keys["6"] === "Left", { what: "the usual keys" });
  check(sameRamp(s.controls.wheel, first), "R in the bar puts back the usual keys");
  await one.page.keyboard.press("Escape");
  await waitFor(one, (s) => !s.controls.open, { what: "the panel closed" });
  s = await sit(one, GAME);
  check(s.controls.keys["6"] === "Left" && sameRamp(s.controls.wheel, usual), `${GAME} as it comes, its own wheel too`);
  const kept = await one.page.evaluate(() => localStorage.getItem("controls"));
  check(!kept.includes(GAME) && !kept.includes("every"), `the browser keeps nothing of it (${kept})`);
});
