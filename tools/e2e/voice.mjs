// Two players at a cabinet talk (client/src/voice.rs, Voice in web/index.html). Chrome's fake
// microphone beeps, so each hears the other "talking". Then muting: Shift with a player number,
// /unmute in the chat, M for your own microphone, and clicking your own line in the panel.
// GAME picks the cabinet (Snow Bros. unless said).
import { check, player, run, sleep, use, waitFor } from "./lib.mjs";

const GAME = process.env.GAME ?? "snowbrosb";
const other = (state) => state.voice?.find((person) => !person.flags.includes("y"));
const me = (state) => state.voice?.find((person) => person.flags.includes("y"));

await run("Voice at a cabinet", async () => {
  const room = `e2e-voice-${Date.now()}`;
  const one = await player("one", room);
  const two = await player("two", room);
  await use(one, GAME, "Playing");
  await use(two, GAME, "Playing");

  for (const it of [one, two]) {
    await waitFor(it, (s) => s.voice?.length === 2, { what: "both players in the voice panel", timeout: 30000 });
  }
  check(true, "each sees both players in the voice panel");

  // Each hears the other's (fake) microphone. Their own isn't checked: both beep alike, and
  // echo cancellation can take one machine's beep for an echo of the other's.
  for (const [it, by] of [[one, "two"], [two, "one"]]) {
    await waitFor(it, (s) => other(s)?.flags.includes("t"), { what: `player ${by} talking` });
  }
  check(true, "each sees the other talking");

  // Player one mutes player two (seat 2) with Shift+2, Shift held over a few frames as a
  // hand would...
  await one.page.keyboard.down("ShiftLeft");
  await sleep(100);
  await one.page.keyboard.press("Digit2", { delay: 100 });
  await one.page.keyboard.up("ShiftLeft");
  let state = await waitFor(one, (s) => other(s)?.flags.includes("m"), { what: "player two muted" });
  check(!other(state).flags.includes("t"), "Shift+2 mutes player two, who no longer shows talking");

  // ...and hears them again with /unmute in the chat.
  await one.page.keyboard.press("KeyY");
  await sleep(200);
  await one.page.keyboard.type(`/unmute ${other(state).name}`);
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

  // Clicking your own line (player one's, the first under the status line) does the same.
  await one.page.mouse.click(60, 49);
  await waitFor(one, (s) => me(s)?.flags.includes("o"), { what: "a click to turn player one's mic off" });
  await one.page.mouse.click(60, 49);
  await waitFor(one, (s) => !me(s)?.flags.includes("o"), { what: "a click to turn it on again" });
  check(true, "clicking your own line turns your microphone off and on");

  for (const it of [one, two]) check(it.errors.length === 0, `no errors in ${it.name}'s page: ${it.errors}`);
});
