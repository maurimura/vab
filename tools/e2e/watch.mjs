// Watching the tables (client/src/seats.rs): a second player presses F by a table someone plays
// at and sees the same game, hands off: the pool rack and where a shot ends, the air hockey rink
// as the player has it, where a shuffleboard throw stops, and a dart scored on the board.
import { check, player, run, sleep, state, use, waitFor } from "./lib.mjs";

const room = `e2e-watch-${Date.now()}`;
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const near = (p, q, within = 30) => Math.hypot(p[0] - q[0], p[1] - q[1]) < within;

/** Puts the player next to the object `name` (testing.rs) and presses F there. */
async function watch(it, name, mode) {
  await it.page.evaluate((name) => window.vab.goTo(name), name);
  await waitFor(it, (s) => s.go_to?.name === name && s.go_to.cell, { what: `the player to reach ${name}` });
  // A frame for the hint to see the player there, and the room's seats to be known.
  await sleep(300);
  await it.page.keyboard.press("KeyF");
  await waitFor(it, (s) => s.mode === mode, { what: `${mode} to open for watching` });
}

/** Esc back to the bar (air hockey's first Esc may only let the pointer go). */
async function leave(it) {
  await it.page.keyboard.press("Escape");
  await sleep(500);
  if ((await state(it)).mode !== "Walking") await it.page.keyboard.press("Escape");
  await waitFor(it, (s) => s.mode === "Walking", { what: "the bar again" });
}

await run("watching the tables", async () => {
  const a = await player("a", room);
  const b = await player("b", room);

  // Pool: a plays alone, b watches. b gets a's rack, and a's shot ends alike on both tables.
  await use(a, "pool_table", "Pool");
  await waitFor(a, (s) => s.pool?.cue === "aiming", { what: "a's cue" });
  await watch(b, "pool_table", "Pool");
  let pb = await waitFor(b, (s) => s.pool?.watching && !s.pool.waiting_for_start, {
    what: "b to get the pool game",
  }).then((s) => s.pool);
  let pa = await waitFor(a, (s) => s.pool.watchers === 1, { what: "a to see a watcher" }).then((s) => s.pool);
  check(same(pa.balls, pb.balls), "b has a's rack");
  // Alone, a sees "Player 1" over their own panel; b sees who that is.
  check(!pb.my_turn && /^Guest \d{4}$/.test(pb.names[0]), `b's hands are off the cue, and knows who plays: ${pb.names[0]}`);
  // a shoots: aim right of the cue ball, hold, let go.
  await a.page.mouse.move(900, 360);
  await a.page.mouse.down();
  await sleep(400);
  await a.page.mouse.up();
  await waitFor(a, (s) => s.pool.cue === "rolling", { what: "a's shot to roll" });
  pa = await waitFor(a, (s) => !["rolling", "striking"].includes(s.pool.cue), {
    what: "a's shot to stop",
    timeout: 30000,
  }).then((s) => s.pool);
  pb = await waitFor(b, (s) => same(s.pool.balls, pa.balls), {
    what: "b's table to end where a's did",
    timeout: 30000,
  }).then((s) => s.pool);
  check(pb.turn === pa.turn && pb.cue === pa.cue, `both see the same table afterwards (${pa.cue}, player ${pa.turn + 1}'s turn)`);
  await leave(b);
  await leave(a);

  // Air hockey: a plays the bot, b watches and has the rink as a has it.
  await use(a, "air_hockey", "Hockey");
  await watch(b, "air_hockey", "Hockey");
  const hb = await waitFor(b, (s) => s.hockey?.watching && !s.hockey.waiting_for_start, {
    what: "b to get the rink",
  }).then((s) => s.hockey);
  const ha = await waitFor(a, (s) => s.hockey.watchers === 1, { what: "a to see a watcher" }).then((s) => s.hockey);
  check(same(hb.score, ha.score) && near(hb.puck, ha.puck) && near(hb.paddles[1], ha.paddles[1]), `b's rink is a's: puck at ${hb.puck}`);
  check(hb.seat === null && hb.net === "none", "b is in no match of their own");
  await leave(b);
  await leave(a);

  // Shuffleboard: a throws, b sees the puck stop where a's did.
  await use(a, "shuffleboard", "Shuffleboard");
  await watch(b, "shuffleboard", "Shuffleboard");
  let sb = await waitFor(b, (s) => s.shuffleboard?.watching && !s.shuffleboard.waiting_for_start, {
    what: "b to get the shuffleboard game",
  }).then((s) => s.shuffleboard);
  check(!sb.my_turn, "b can't throw");
  await waitFor(a, (s) => s.shuffleboard.watchers === 1, { what: "a to see a watcher" });
  await a.page.evaluate(() => window.vab.throw(0, 400));
  const stopped = (s) => s.shuffleboard.thrown === 1 && s.shuffleboard.phase === "throwing";
  const sa = await waitFor(a, stopped, { what: "a's throw to stop", timeout: 30000 }).then((s) => s.shuffleboard);
  sb = await waitFor(b, stopped, { what: "b's table to stop", timeout: 30000 }).then((s) => s.shuffleboard);
  check(same(sa.pucks, sb.pucks), `b has a's puck where it stopped: ${JSON.stringify(sb.pucks)}`);
  await leave(b);
  await leave(a);

  // Darts: a throws a treble 20, b sees it in the board and scored.
  await use(a, "dartboard", "Darts");
  await watch(b, "dartboard", "Darts");
  let db = await waitFor(b, (s) => s.darts?.watching && !s.darts.waiting_for_start, {
    what: "b to get the darts game",
  }).then((s) => s.darts);
  check(!db.my_turn && db.remaining.join() === "301,301", "b has the game, and no dart of their own");
  await waitFor(a, (s) => s.darts.watchers === 1, { what: "a to see a watcher" });
  await a.page.evaluate(() => window.vab.throwDart(0, 103));
  const scored = (s) => s.darts.darts.length === 1 && s.darts.phase !== "flying";
  const da = await waitFor(a, scored, { what: "a's dart to land" }).then((s) => s.darts);
  db = await waitFor(b, scored, { what: "the dart on b's board" }).then((s) => s.darts);
  check(
    same(da.stuck, db.stuck) && same(da.remaining, db.remaining) && db.darts[0] === "T20",
    `b has a's dart where it landed, scored alike: ${db.darts[0]}, ${db.remaining[0]} left`,
  );

  // a leaves: b is told nobody plays, and goes too.
  await leave(a);
  await waitFor(b, (s) => s.darts.names[0] === "Player 1", { what: "b to see the board empty" });
  await leave(b);
  for (const it of [a, b]) check(it.errors.length === 0, `no errors in ${it.name}'s page: ${it.errors}`);
});
