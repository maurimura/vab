// Two players at the dartboard (client/src/darts): seats, the thrower's hand seen by the other,
// darts scored alike on both boards, turns passing, a reload mid-game, and someone leaving.
import { check, player, run, sleep, state, use, waitFor } from "./lib.mjs";

const room = `e2e-darts-${Date.now()}`;
const game = async (it) => (await state(it)).darts;
const at = (it) => use(it, "dartboard", "Darts");
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const view = (g) => ({ remaining: g.remaining, turn: g.turn, darts: g.darts, stuck: g.stuck });

/** Whoever's turn it is throws a dart landing at `x`, `y`; waits for both boards to have it. */
async function dart(players, [x, y]) {
  const [a, b] = players;
  const thrower = (await game(a)).my_turn ? a : b;
  const thrown = (await game(thrower)).darts.length;
  await thrower.page.evaluate(([x, y]) => window.vab.throwDart(x, y), [x, y]);
  const landed = (s) => s.darts.darts.length === thrown + 1 && s.darts.phase !== "flying";
  await waitFor(a, landed, { what: "the dart on a's board" });
  await waitFor(b, landed, { what: "the dart on b's board" });
  return thrower;
}

/** Waits for both boards to be on the next turn, `turn`'s. */
async function nextTurn(players, turn) {
  for (const it of players) {
    await waitFor(it, (s) => s.darts.turn === turn && s.darts.phase === "aiming" && s.darts.darts.length === 0, {
      what: `${it.name} on the next turn`,
    });
  }
}

await run("darts, two players", async () => {
  const a = await player("a", room);
  await at(a);
  check((await game(a)).seat === null, "alone, a throws for both sides");

  const b = await player("b", room);
  await at(b);
  let ga = await waitFor(a, (s) => s.darts.opponent, { what: "a to see b" }).then((s) => s.darts);
  let gb = await waitFor(b, (s) => s.darts.opponent && !s.darts.waiting_for_start, {
    what: "b to get the game",
  }).then((s) => s.darts);
  check(ga.seat === 0 && gb.seat === 1 && same(ga.names, gb.names), `seats and names: ${ga.names.join(", ")}`);
  check(ga.my_turn && !gb.my_turn, "a throws first");

  // a's hand, as a aims: b sees it.
  await a.page.mouse.click(640, 360); // takes aim, locking the pointer
  await a.page.mouse.move(680, 300, { steps: 10 });
  gb = await waitFor(b, (s) => s.darts.their_hand, { what: "b to see a's hand" }).then((s) => s.darts);
  check(true, `b sees a's hand at ${gb.their_hand.at.map(Math.round)}`);

  const players = [a, b];
  check((await dart(players, [0, 103])) === a, "a throws a treble 20");
  await dart(players, [0, 50]);
  await dart(players, [50, 0]);
  [ga, gb] = [await game(a), await game(b)];
  check(same(view(ga), view(gb)), `both boards alike: ${ga.darts.join(", ")}, a on ${ga.remaining[0]}`);
  await nextTurn(players, 1);
  check((await game(b)).my_turn, "then it's b's turn");

  check((await dart(players, [0, 0])) === b, "b throws a bull");
  // b reloads mid-turn, and gets the game back from a.
  await b.page.reload({ waitUntil: "load" });
  await waitFor(b, (s) => s.mode === "Walking", { what: "b's bar to load", timeout: 60000 });
  await b.page.mouse.click(640, 360);
  await b.page.keyboard.press("ShiftLeft");
  await at(b);
  gb = await waitFor(b, (s) => s.darts.opponent && !s.darts.waiting_for_start, {
    what: "b to get the game again",
  }).then((s) => s.darts);
  ga = await game(a);
  check(same(view(ga), view(gb)), `after reloading, b has the game as it was: ${gb.darts.join(", ")}`);
  check(gb.seat === 1 && gb.my_turn, "the same seat, and still b's turn");
  await dart(players, [-10, -10]);
  await dart(players, [0, 300]);
  [ga, gb] = [await game(a), await game(b)];
  check(same(ga.remaining, gb.remaining) && gb.remaining[1] === 226, `b on ${gb.remaining[1]} on both`);

  // b leaves: a's game waits for b.
  await nextTurn(players, 0);
  await b.page.keyboard.press("Escape");
  await sleep(400);
  if ((await state(b)).mode !== "Walking") await b.page.keyboard.press("Escape");
  ga = await waitFor(a, (s) => !s.darts.opponent, { what: "a to see b leave" }).then((s) => s.darts);
  check(ga.seat === 0, "a keeps the seat, the game waiting for b");
  for (const it of players) check(it.errors.length === 0, `no errors in ${it.name}'s page: ${it.errors}`);
});
