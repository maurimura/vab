// Next to both a cabinet and a table, E uses whichever the player stands nearer
// (client/src/nearby.rs). In the bar's map, the cell (-4, 3) is between the shuffleboard table
// (row 4) and a cabinet at (-4, 2).
import { check, player, run, sleep, waitFor } from "./lib.mjs";

await run("E uses the nearest thing", async () => {
  const it = await player("solo", `e2e-nearest-${Date.now()}`);
  const standAt = (cell, toward) =>
    it.page.evaluate(([x, y, tx, ty]) => window.vab.standAt(x, y, tx, ty), [...cell, ...toward]);

  await standAt([-4, 3], [-4, 2]);
  let state = await waitFor(it, (s) => s.nearby?.kind === "Cabinet", { what: "the cabinet to be nearest" });
  check(state.player.cell.join() === "-4,3", "leaning toward the cabinet, it's the one offered");

  await standAt([-4, 3], [-4, 4]);
  state = await waitFor(it, (s) => s.nearby?.kind === "Shuffleboard", { what: "the table to be nearest" });
  check(state.player.cell.join() === "-4,3", "in the same cell, leaning toward the table, the table is");
  await sleep(200);
  await it.page.keyboard.press("KeyE");
  await waitFor(it, (s) => s.mode === "Shuffleboard", { what: "E to open the table" });
  check(true, "and E opens the table, not the cabinet");
  check(it.errors.length === 0, `no errors in the page: ${it.errors}`);
});
