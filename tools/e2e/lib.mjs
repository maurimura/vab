// Players in headless Chrome for browser tests of the bar (README.md here). Each runs the site
// from `make dev` in its own browser, and the client's test hooks (client/src/testing.rs) say
// what the game is doing and skip what a headless browser does badly: walking, and pushing a
// puck by hand.
import puppeteer from "puppeteer-core";

export const BASE_URL = process.env.BASE_URL ?? "http://localhost:8787";
const CHROME =
  process.env.CHROME ??
  (process.platform === "darwin"
    ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    : "/usr/bin/google-chrome");
// On a Mac, headless Chrome draws with the GPU (Metal): 60 frames a second, as a player sees it.
// Elsewhere, or with RENDER=software, it draws in software, which works anywhere but manages
// 5 to 15 a second, so anything timed by hand (walking, pushing) comes out uneven.
const RENDER = process.env.RENDER ?? (process.platform === "darwin" ? "gpu" : "software");
const RENDER_ARGS = {
  software: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
  gpu: process.platform === "darwin" ? ["--use-angle=metal"] : ["--use-angle=vulkan"],
}[RENDER];

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const browsers = [];
/** Closes every browser, whatever state the test is in. */
export async function closeAll() {
  await Promise.all(browsers.map((browser) => browser.close().catch(() => {})));
}

/** A player in `room`, the bar loaded and the welcome closed. */
export async function player(name, room) {
  const browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: "new",
    // A fake microphone that beeps, allowed without asking: players at a cabinet talk
    // (voice.mjs).
    args: [
      ...RENDER_ARGS,
      "--autoplay-policy=no-user-gesture-required",
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
    ],
    defaultViewport: { width: 1280, height: 720 },
  });
  browsers.push(browser);
  const page = await browser.newPage();
  const errors = [];
  page.on("console", (message) => {
    const text = message.text();
    // ROMs aren't in a local bucket unless uploaded, and software rendering complains.
    if (/panic|error/i.test(text) && !/roms\/|GL Driver|software rendering/.test(text)) {
      errors.push(text.slice(0, 300));
    }
  });
  page.on("pageerror", (error) => {
    if (!/roms\//.test(error.message)) errors.push(`page: ${error.message}`);
  });
  const it = { name, browser, page, errors };
  await it.page.goto(`${BASE_URL}/?room=${encodeURIComponent(room)}`, { waitUntil: "load" });
  await waitFor(it, (state) => state.mode === "Walking", { what: "the bar to load", timeout: 60000 });
  // A click gives the page the keyboard; a key closes the welcome.
  await page.mouse.click(640, 360);
  await page.keyboard.press("ShiftLeft");
  return it;
}

/** The client's state (client/src/testing.rs), or null until the hooks are up. */
export async function state(it) {
  return it.page.evaluate(() => window.vab?.state() ?? null);
}

/** Waits until `test(state)` holds, and returns that state. */
export async function waitFor(it, test, { what = "the game", timeout = 20000 } = {}) {
  const until = Date.now() + timeout;
  let last = null;
  while (Date.now() < until) {
    last = await state(it).catch(() => null);
    if (last && test(last)) return last;
    await sleep(100);
  }
  throw new Error(`${it.name}: gave up waiting for ${what}; last state: ${JSON.stringify(last)}`);
}

/** Puts the player next to the object `name` (testing.rs) and presses E there. */
export async function use(it, name, mode) {
  await it.page.evaluate((name) => window.vab.goTo(name), name);
  await waitFor(it, (state) => state.go_to?.name === name && state.go_to.cell, {
    what: `the player to reach ${name}`,
  });
  // A frame for the hint to see the player there.
  await sleep(300);
  await it.page.keyboard.press("KeyE");
  await waitFor(it, (state) => state.mode === mode, { what: `${mode} to open` });
}

/** Frames drawn a second, over `seconds`. */
export async function frameRate(it, seconds = 2) {
  const before = await state(it);
  await sleep(seconds * 1000);
  const after = await state(it);
  return (after.frame - before.frame) / (after.seconds - before.seconds);
}

/** Fails the test with `message` unless `ok`. */
export function check(ok, message) {
  if (!ok) throw new Error(message);
  console.log(`  ok: ${message}`);
}

/** Runs `test`, then closes every browser; a failure says what and exits non-zero. */
export async function run(name, test) {
  console.log(`${name}:`);
  try {
    await test();
    console.log("passed");
  } catch (error) {
    console.error(`FAILED: ${error.message}`);
    process.exitCode = 1;
  } finally {
    await closeAll();
    // Something of puppeteer's can keep node waiting after the browsers have gone.
    process.exit(process.exitCode ?? 0);
  }
}
