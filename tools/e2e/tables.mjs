// One player visits each table in the bar: pool, air hockey and shuffleboard open, and say
// what they're doing; Esc goes back to the bar from each.
import { check, frameRate, player, run, sleep, state, use, waitFor } from "./lib.mjs";

await run("the tables, alone", async () => {
  const it = await player("solo", `e2e-tables-${Date.now()}`);
  console.log(`  (${(await frameRate(it, 1)).toFixed(0)} frames a second)`);
  const leave = async () => {
    await it.page.keyboard.press("Escape");
    // Air hockey's first Esc lets the pointer go; a second leaves.
    await sleep(500);
    if ((await state(it)).mode !== "Walking") await it.page.keyboard.press("Escape");
    await waitFor(it, (s) => s.mode === "Walking", { what: "the bar again" });
  };

  await use(it, "pool_table", "Pool");
  const pool = await waitFor(it, (s) => s.pool, { what: "the pool game" }).then((s) => s.pool);
  check(pool.balls.length === 16 && pool.my_turn, "pool: a rack of 16, and the break is ours");
  await leave();

  await use(it, "air_hockey", "Hockey");
  const hockey = await waitFor(it, (s) => s.hockey, { what: "the rink" }).then((s) => s.hockey);
  check(hockey.seat === null && hockey.net === "none", "air hockey: playing the bot");
  await leave();

  await use(it, "shuffleboard", "Shuffleboard");
  await it.page.evaluate(() => window.vab.throw(0, 400));
  const board = await waitFor(it, (s) => s.shuffleboard?.thrown === 1 && s.shuffleboard.phase === "throwing", {
    what: "the throw to stop",
  }).then((s) => s.shuffleboard);
  check(board.pucks.length === 1 && board.pucks[0].points > 0, `shuffleboard: a throw scores, ${JSON.stringify(board.pucks)}`);
  await leave();
  check(it.errors.length === 0, `no errors in the page: ${it.errors}`);
});
