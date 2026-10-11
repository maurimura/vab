// The bar's staff (client/src/staff.rs): two players in a room see the same bartender and cleaner
// at the same stop of their shift, worked out from the room's clock alone; the bartender keeps
// behind the counter (in the bar's map, cells -7 to 3 along x and -16 to -13 along y) and the
// cleaner to the floor; and a shift later, the next of each takes over for both.
import { check, player, run, sleep, waitFor } from "./lib.mjs";

const behindBar = ([x, y]) => x >= -7 && x <= 3 && y >= -16 && y <= -13;

await run("The staff", async () => {
  const room = `e2e-staff-${Date.now()}`;
  const [one, two] = [await player("one", room), await player("two", room)];
  for (const it of [one, two]) await waitFor(it, (s) => s.staff?.length === 2, { what: "the staff" });
  const both = () => Promise.all([one, two].map((it) => it.page.evaluate(() => window.vab.state().staff)));
  const byJob = (staff, job) => staff.find((member) => member.job === job);

  // A stop changes every few seconds: the two may look either side of a change, so a few tries.
  const alike = async () => {
    for (let tries = 0; tries < 10; tries++) {
      const [a, b] = await both();
      const same = ["Bartender", "Cleaner"].every((job) => {
        const [x, y] = [byJob(a, job), byJob(b, job)];
        return x.name === y.name && x.shift === y.shift && x.stop === y.stop && x.to.join() === y.to.join();
      });
      if (same) return a;
      await sleep(300);
    }
    throw new Error(`the two see different staff: ${JSON.stringify(await both())}`);
  };
  const before = await alike();
  const names = (staff) => staff.map((member) => `${member.job} ${member.name}`).join(", ");
  check(true, `both see the same staff at the same stops: ${names(before)}`);

  for (let i = 0; i < 20; i++) {
    const [staff] = await both();
    const [bartender, cleaner] = [byJob(staff, "Bartender"), byJob(staff, "Cleaner")];
    if (!behindBar(bartender.cell)) throw new Error(`the bartender left the bar: ${bartender.cell}`);
    if (behindBar(cleaner.cell)) throw new Error(`the cleaner went behind the bar: ${cleaner.cell}`);
    await sleep(250);
  }
  check(true, "over 5 seconds the bartender keeps behind the counter and the cleaner out of it");

  // Past the end of both shifts, and short of the end of the next.
  const ahead = Math.max(...before.map((member) => member.secondsLeft)) + 5;
  await Promise.all([one, two].map((it) => it.page.evaluate((s) => window.vab.clockAhead(s), ahead)));
  await sleep(500);
  const after = await alike();
  for (const job of ["Bartender", "Cleaner"]) {
    const [was, is] = [byJob(before, job), byJob(after, job)];
    check(is.shift === was.shift + 1 && is.name !== was.name, `a shift later, ${was.name} hands over to ${is.name}`);
  }
  check(one.errors.length === 0 && two.errors.length === 0, `no errors in the pages: ${one.errors} ${two.errors}`);
});
