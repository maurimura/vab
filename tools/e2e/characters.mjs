// Picking a character at a game whose seats are characters (client/src/cabinets.rs): The
// Simpsons, or GAME (Sunset Riders: ssriders). The hint says who plays each and the number keys
// for the free ones; a free one's key sits the player as them, a taken one's does nothing, and
// the status line says who each plays.
import { check, player, run, sleep, state, waitFor } from "./lib.mjs";

const GAME = process.env.GAME ?? "simpsons";
const CHARACTERS = { simpsons: ["Marge", "Homer", "Bart", "Lisa"], ssriders: ["Steve", "Billy", "Bob", "Cormano"] }[GAME];

/** Puts the player next to the cabinet, and waits for its hint. */
async function next(it) {
  await it.page.evaluate((name) => window.vab.goTo(name), GAME);
  return waitFor(it, (s) => s.hint && s.nearby?.kind === "Cabinet", { what: `the hint at ${GAME}` });
}

await run(`Picking a character at ${GAME}`, async () => {
  const room = `e2e-characters-${Date.now()}`;
  const one = await player("one", room);
  const two = await player("two", room);
  const [first, second, third] = CHARACTERS;

  let s = await next(one);
  check(s.hint.endsWith(CHARACTERS.map((who, seat) => `${seat + 1}  ${who}`).join("    ")), `the hint lists the four with their keys: ${JSON.stringify(s.hint)}`);
  await one.page.keyboard.press("Digit3");
  s = await waitFor(one, (s) => s.mode === "Playing" && s.status?.startsWith(third), { what: `player one playing ${third}` });
  check(true, `3 sits player one as ${third}: ${JSON.stringify(s.status)}`);

  s = await next(two);
  const taken = new RegExp(`${third}: \\S`);
  await waitFor(two, (s) => taken.test(s.hint), { what: `the hint saying who plays ${third}` });
  s = await state(two);
  check(!s.hint.includes(`3  ${third}`), `player two's hint says who plays ${third}, without a key: ${JSON.stringify(s.hint)}`);
  await two.page.keyboard.press("Digit3");
  await sleep(500);
  check((await state(two)).mode === "Walking", `3 does nothing for player two: ${third} is taken`);
  await two.page.keyboard.press("Digit2");
  s = await waitFor(two, (s) => s.mode === "Playing" && s.status?.startsWith(second), { what: `player two playing ${second}` });
  check(true, `2 sits player two as ${second}: ${JSON.stringify(s.status)}`);

  // Online together, each saying who the other plays.
  s = await waitFor(two, (s) => s.status?.startsWith(`${second}, online with ${third}`), { what: "player two online", timeout: 60000 });
  check(true, `player two: ${JSON.stringify(s.status)}`);
  s = await waitFor(one, (s) => s.status?.startsWith(`${third}, online with ${second}`), { what: "player one online", timeout: 60000 });
  check(true, `player one: ${JSON.stringify(s.status)}`);

  // E still takes the first free one: a third player gets Marge.
  const three = await player("three", room);
  await next(three);
  await three.page.keyboard.press("KeyE");
  s = await waitFor(three, (s) => s.mode === "Playing" && s.status?.startsWith(first), { what: `player three playing ${first}` });
  check(true, `E sits player three as ${first}, the first free: ${JSON.stringify(s.status)}`);
});
