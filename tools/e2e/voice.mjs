// Voice wherever two players sit together (client/src/voice.rs, Voice in web/index.html): at a
// cabinet, then the pool, air hockey and shuffleboard tables and the dartboard. Chrome's fake
// microphone beeps, so each hears the other "talking". At the cabinet: muting with Shift and a
// player number, /unmute in the chat, M for your own microphone, and a click on your own line.
// At the pool table, a click on the panel is the panel's: it doesn't move or pull the cue.
// GAME picks the cabinet (Snow Bros. unless said).
import { check, player, run, sleep, state, use, waitFor } from "./lib.mjs";

const GAME = process.env.GAME ?? "snowbrosb";
const TABLES = [
  ["pool_table", "Pool"],
  ["air_hockey", "Hockey"],
  ["shuffleboard", "Shuffleboard"],
  ["dartboard", "Darts"],
];
const other = (state) => state.voice?.find((person) => !person.flags.includes("y"));
const me = (state) => state.voice?.find((person) => person.flags.includes("y"));
// Your own line in the panel: under the status line at a cabinet, in the corner at a table.
const OWN_LINE = { cabinet: [60, 49], table: [60, 21] };

/** Both in the panel, and each hears the other over a call that connected. */
async function talking(one, two, where) {
  for (const it of [one, two]) {
    await waitFor(it, (s) => s.voice?.length === 2, { what: `both in the voice panel at ${where}`, timeout: 30000 });
  }
  // Their own beep isn't checked: both beep alike, and echo cancellation can take one machine's
  // beep for an echo of the other's.
  for (const [it, by] of [[one, "two"], [two, "one"]]) {
    await waitFor(it, (s) => other(s)?.flags.includes("t") && !/[cn]/.test(other(s).flags), {
      what: `player ${by} talking at ${where}`,
    });
  }
  check(true, `at ${where}, each sees both in the panel and hears the other`);
}

/** Esc back to the bar (air hockey's and darts' first Esc may only let the pointer go). */
async function leave(it) {
  await it.page.keyboard.press("Escape");
  await sleep(500);
  if ((await state(it)).mode !== "Walking") await it.page.keyboard.press("Escape");
  await waitFor(it, (s) => s.mode === "Walking", { what: "the bar again" });
}

await run("Voice wherever players sit together", async () => {
  const room = `e2e-voice-${Date.now()}`;
  const one = await player("one", room);
  const two = await player("two", room);
  await use(one, GAME, "Playing");
  await use(two, GAME, "Playing");
  await talking(one, two, "a cabinet");

  // Player one mutes player two (seat 2) with Shift+2, Shift held over a few frames as a
  // hand would...
  await one.page.keyboard.down("ShiftLeft");
  await sleep(100);
  await one.page.keyboard.press("Digit2", { delay: 100 });
  await one.page.keyboard.up("ShiftLeft");
  let s = await waitFor(one, (s) => other(s)?.flags.includes("m"), { what: "player two muted" });
  check(!other(s).flags.includes("t"), "Shift+2 mutes player two, who no longer shows talking");

  // ...and hears them again with /unmute in the chat.
  await one.page.keyboard.press("KeyY");
  await sleep(200);
  await one.page.keyboard.type(`/unmute ${other(s).name}`);
  await one.page.keyboard.press("Enter");
  await waitFor(one, (s) => !other(s)?.flags.includes("m"), { what: "player two unmuted" });
  check(true, "/unmute <name> in the chat undoes it");

  // Player two turns their microphone off with M; player one sees it.
  await two.page.keyboard.press("KeyM");
  await waitFor(two, (s) => me(s)?.flags.includes("o"), { what: "player two's mic off on their side" });
  await waitFor(one, (s) => other(s)?.flags.includes("o"), { what: "player two's mic off for player one" });
  check(true, "M turns your microphone off, and the other player sees it");
  await two.page.keyboard.press("KeyM");
  await waitFor(one, (s) => !other(s)?.flags.includes("o"), { what: "player two's mic on again" });

  // Clicking your own line does the same.
  await one.page.mouse.click(...OWN_LINE.cabinet);
  await waitFor(one, (s) => me(s)?.flags.includes("o"), { what: "a click to turn player one's mic off" });
  await one.page.mouse.click(...OWN_LINE.cabinet);
  await waitFor(one, (s) => !me(s)?.flags.includes("o"), { what: "a click to turn it on again" });
  check(true, "clicking your own line turns your microphone off and on");

  // Leaving ends the call: the one still seated has nobody to talk to.
  await leave(two);
  await waitFor(one, (s) => s.voice?.length === 0, { what: "player one's panel to empty" });
  check((await state(two)).voice?.length === 0, "standing up ends the call, for both");
  await leave(one);

  for (const [table, mode] of TABLES) {
    await use(one, table, mode);
    await use(two, table, mode);
    await talking(one, two, `the ${table.replace("_", " ")}`);
    if (table === "pool_table") {
      // Player one's turn to shoot: a click on their own line toggles the microphone, and the
      // cue doesn't follow the click or start pulling back.
      const before = await waitFor(one, (s) => s.pool?.my_turn && s.pool.cue === "aiming", { what: "player one to aim" });
      await one.page.mouse.click(...OWN_LINE.table, { delay: 300 });
      const after = await waitFor(one, (s) => me(s)?.flags.includes("o"), { what: "a click on the panel at the pool table" });
      check(after.pool.cue === "aiming" && before.pool.balls[0].at.join() === after.pool.balls[0].at.join(), "at the pool table, a click on the panel is the panel's: the cue stays put");
      await one.page.mouse.click(...OWN_LINE.table);
      await waitFor(one, (s) => !me(s)?.flags.includes("o"), { what: "the mic on again" });
    }
    await leave(two);
    await leave(one);
  }

  for (const it of [one, two]) check(it.errors.length === 0, `no errors in ${it.name}'s page: ${it.errors}`);
});
