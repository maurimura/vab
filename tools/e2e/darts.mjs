// One player at the dartboard (client/src/darts), throwing for both sides: darts score where
// they land, a turn passes after three, going below zero is a bust, and exactly zero wins.
import { check, player, run, sleep, state, use, waitFor } from "./lib.mjs";

const SHOTS = process.env.SHOTS; // a folder for screenshots, if wanted

await run("darts, alone", async () => {
  const it = await player("solo", `e2e-darts-${Date.now()}`);
  await use(it, "dartboard", "Darts");
  const darts = async () => (await state(it)).darts;
  /** Throws a dart that lands at `x`, `y` (millimetres), and waits for it to land. */
  const dart = async (x, y) => {
    const before = (await darts()).stuck.length;
    await it.page.evaluate(([x, y]) => window.vab.throwDart(x, y), [x, y]);
    return waitFor(it, (s) => s.darts.stuck.length === before + 1 || s.darts.phase === "turn_over", {
      what: "the dart to land",
    }).then((s) => s.darts);
  };
  const nextTurn = () => waitFor(it, (s) => s.darts.phase === "aiming", { what: "the next turn" });

  let game = await waitFor(it, (s) => s.darts?.phase === "aiming", { what: "the board" }).then((s) => s.darts);
  check(game.turn === 0 && game.remaining.join() === "301,301", "red throws first, both on 301");
  if (SHOTS) await it.page.screenshot({ path: `${SHOTS}/darts-1-aiming.png` });

  await dart(0, 103); // treble 20
  await dart(0, 166); // double 20
  game = await dart(50, 0); // 6
  check(game.darts.join() === "T20,D20,6" && game.turn_points === 106, `a turn of ${game.darts.join(", ")}`);
  check(game.remaining[0] === 195 && game.phase === "turn_over", "red is on 195, and the turn is over");
  if (SHOTS) await it.page.screenshot({ path: `${SHOTS}/darts-2-turn.png` });
  game = (await nextTurn()).darts;
  check(game.turn === 1 && game.stuck.length === 0, "blue's turn, the board cleared");

  game = await dart(0, 0); // bull
  game = await dart(0, 300); // off the board
  game = await dart(-10, -10); // outer bull
  check(game.remaining[1] === 226, `blue: Bull, Miss, 25 makes 75, on 226: ${game.darts.join(", ")}`);
  await nextTurn();

  // Red on 195: 180 (three treble 20s) leaves 15.
  for (let i = 0; i < 3; i++) await dart(i - 1, 103);
  await nextTurn();
  for (let i = 0; i < 3; i++) await dart(0, 300);
  await nextTurn();
  // Red on 15: a treble 20 is a bust.
  game = await dart(0, 103);
  check(game.busted && game.remaining[0] === 15, "below zero is a bust, red stays on 15");
  await nextTurn();
  for (let i = 0; i < 3; i++) await dart(0, 300);
  await nextTurn();
  // 15 exactly: a treble 5 (5 is left of the 20).
  const fiveAngle = (-18 * Math.PI) / 180;
  game = await dart(Math.sin(fiveAngle) * 103, Math.cos(fiveAngle) * 103);
  check(game.win === 0 && game.remaining[0] === 0, `exactly zero wins: ${game.darts.join(", ")}`);
  await sleep(300);
  if (SHOTS) await it.page.screenshot({ path: `${SHOTS}/darts-3-won.png` });
  check(it.errors.length === 0, `no errors in the page: ${it.errors}`);
});
