// Two players at the shuffleboard table (client/src/shuffleboard): seats, a throw each seen
// alike on both tables, the held puck, a reload mid-game, a whole round scored alike, and
// someone leaving.
import { check, player, run, sleep, state, use, waitFor } from "./lib.mjs";

const room = `e2e-shuffleboard-${Date.now()}`;
const game = async (it) => (await state(it)).shuffleboard;
const at = (it) => use(it, "shuffleboard", "Shuffleboard");
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** Waits until both tables are waiting for throw number `thrown` (0 to 8) of the round. */
async function bothAt(a, b, thrown, what) {
  const ready = (state) =>
    state.shuffleboard?.phase === "throwing" && state.shuffleboard.thrown === thrown;
  await waitFor(a, ready, { what, timeout: 30000 });
  return waitFor(b, ready, { what, timeout: 30000 });
}

/** Whoever's turn it is throws at `velocity`; waits for both tables to stop. */
async function throwFrom(players, velocity) {
  const [a, b] = players;
  const thrown = (await game(a)).thrown;
  const thrower = (await game(a)).my_turn ? a : b;
  await thrower.page.evaluate(([vx, vy]) => window.vab.throw(vx, vy), velocity);
  const next = thrown + 1 === 8 ? 0 : thrown + 1;
  await bothAt(a, b, next, `throw ${thrown + 1} to stop`);
  return thrower;
}

await run("shuffleboard, two players", async () => {
  const a = await player("a", room);
  await at(a);
  let ga = await game(a);
  check(ga.seat === null && ga.my_turn, "alone, a throws for both sides");

  const b = await player("b", room);
  await at(b);
  ga = await waitFor(a, (s) => s.shuffleboard.opponent, { what: "a to see b" }).then((s) => s.shuffleboard);
  let gb = await waitFor(b, (s) => s.shuffleboard.opponent && !s.shuffleboard.waiting_for_start, {
    what: "b to get the game",
  }).then((s) => s.shuffleboard);
  check(ga.seat === 0 && gb.seat === 1, "a has the first seat, b the second");
  check(same(ga.names, gb.names), `both see the same names: ${ga.names.join(", ")}`);
  check(ga.my_turn && !gb.my_turn, "a throws first");

  // a slides the waiting puck about by hand; b sees it where a has it.
  await a.page.mouse.move(640, 536);
  await a.page.mouse.down();
  await a.page.mouse.move(600, 520, { steps: 10 });
  await sleep(500);
  ga = await game(a);
  gb = await waitFor(b, (s) => same(s.shuffleboard.ready, ga.ready), { what: "b to see a's puck" }).then(
    (s) => s.shuffleboard,
  );
  check(ga.ready[0] !== 24, `b sees a's puck where a holds it, at ${ga.ready}`);
  // Put down, not thrown: let go without moving.
  await sleep(200);
  await a.page.mouse.up();

  const players = [a, b];
  check((await throwFrom(players, [0, 380])) === a, "a throws");
  [ga, gb] = [await game(a), await game(b)];
  check(same(ga.pucks, gb.pucks), `both tables have the pucks alike: ${JSON.stringify(ga.pucks)}`);
  check(!ga.my_turn && gb.my_turn, "then it's b's throw");

  check((await throwFrom(players, [2, 400])) === b, "b throws");
  [ga, gb] = [await game(a), await game(b)];
  check(same(ga.pucks, gb.pucks), `still alike: ${JSON.stringify(gb.pucks)}`);

  // b reloads mid-round, and gets the game back from a.
  await b.page.reload({ waitUntil: "load" });
  await waitFor(b, (s) => s.mode === "Walking", { what: "b's bar to load", timeout: 60000 });
  await b.page.mouse.click(640, 360);
  await b.page.keyboard.press("ShiftLeft");
  await at(b);
  gb = await waitFor(b, (s) => s.shuffleboard.opponent && !s.shuffleboard.waiting_for_start, {
    what: "b to get the game again",
  }).then((s) => s.shuffleboard);
  ga = await game(a);
  check(gb.thrown === 2 && same(ga.pucks, gb.pucks), "after reloading, b has the game as it was");
  check(gb.seat === 1 && ga.my_turn, "the same seats, and a's throw");

  // The rest of the round, then its score, alike on both.
  for (const velocity of [[-3, 390], [1, 410], [4, 370], [-2, 420], [0, 395], [3, 405]]) {
    await throwFrom(players, velocity);
  }
  [ga, gb] = [await game(a), await game(b)];
  check(ga.thrown === 0 && same(ga.scores, gb.scores), `the round is scored alike: ${ga.scores}`);
  check(ga.pucks.length === 0 && gb.pucks.length === 0, "and the table cleared");
  check(ga.first === gb.first && ga.my_turn !== gb.my_turn, "both agree who throws first next");

  // b leaves: a's game waits for b, on b's throw, or goes on, on a's.
  await b.page.keyboard.press("Escape");
  ga = await waitFor(a, (s) => !s.shuffleboard.opponent, { what: "a to see b leave" }).then(
    (s) => s.shuffleboard,
  );
  check(ga.seat === 0, "a keeps the seat, the game waiting for b");
  for (const it of players) check(it.errors.length === 0, `no errors in ${it.name}'s page: ${it.errors}`);
});
