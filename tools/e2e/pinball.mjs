// The Saloon pinball machine (client/src/pinball): it opens, the plunger launches the ball, the
// flippers swing, and a drained ball brings the next one.
import { check, player, run, sleep, state, use, waitFor } from "./lib.mjs";

await run("pinball", async () => {
  const it = await player("solo", `e2e-pinball-${Date.now()}`);
  await use(it, "pinball_saloon", "Pinball");
  let machine = await waitFor(it, (s) => s.pinball?.on_plunger, { what: "the ball on the plunger" }).then((s) => s.pinball);
  check(machine.ball === 1 && machine.score === 0, "ball 1 waits on the plunger");

  await it.page.keyboard.down("Space");
  await sleep(1000);
  machine = (await state(it)).pinball;
  check(machine.plunger > 0.5, `holding Space pulls the plunger back (${machine.plunger.toFixed(2)} in)`);
  await it.page.keyboard.up("Space");
  machine = await waitFor(it, (s) => s.pinball.balls[0]?.at[1] < 20, { what: "the ball up the table" }).then((s) => s.pinball);
  check(!machine.on_plunger, `letting go launches it, up to y ${machine.balls[0].at[1].toFixed(1)}`);

  await it.page.keyboard.down("ShiftLeft");
  await it.page.keyboard.down("ShiftRight");
  machine = await waitFor(it, (s) => s.pinball.flippers.every((f) => f === 1), { what: "the flippers up" }).then((s) => s.pinball);
  check(true, "Shift swings both flippers up");
  await it.page.keyboard.up("ShiftLeft");
  await it.page.keyboard.up("ShiftRight");

  // Off the plunger, the display says what to go for.
  machine = await waitFor(it, (s) => s.pinball.screen.includes("SHOOT") || s.pinball.screen.includes("HIT "), { what: "a goal on the display" }).then((s) => s.pinball);
  check(true, `the display says what to go for: ${machine.screen}`);

  // Hands off: the ball drains in time (a ball saved comes back to the plunger: plunged again),
  // and the next one comes.
  machine = await waitFor(
    it,
    async (s) => {
      if (s.pinball.ball === 1 && s.pinball.on_plunger) {
        await it.page.keyboard.down("Space");
        await sleep(400);
        await it.page.keyboard.up("Space");
      }
      return s.pinball.ball === 2 && s.pinball.on_plunger;
    },
    { what: "ball 2", timeout: 120000 },
  ).then((s) => s.pinball);
  check(machine.score > 0, `ball 2 comes after the first drains, the score at ${machine.score}`);
  check(it.errors.length === 0, `no errors in the page: ${it.errors}`);
});
