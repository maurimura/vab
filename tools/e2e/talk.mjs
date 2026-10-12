// Talking to the staff (client/src/talk): on a first visit the way to the bartender shows; at the
// counter E talks to them, and they come across it; picking a game shows the way there; picking
// Español changes what they say but not the games' titles, and a reload keeps it; the cleaner
// stops to talk, and walking away ends it.
import { check, enter, player, run, sleep, waitFor } from "./lib.mjs";

await run("Talking to the staff", async () => {
  const it = await player("talker", `e2e-talk-${Date.now()}`);
  const standAt = (cell, toward) =>
    it.page.evaluate(([x, y, tx, ty]) => window.vab.standAt(x, y, tx, ty), [...cell, ...toward]);
  const press = (key) => it.page.keyboard.press(key);
  const pick = async (choice) => {
    const { talk } = await it.page.evaluate(() => window.vab.state());
    const index = talk.open.choices.indexOf(choice);
    if (index < 0) throw new Error(`no "${choice}" in ${talk.open.choices}`);
    await press(`Digit${index + 1}`);
  };
  const job = (s, name) => s.staff.find((member) => member.job === name);

  let s = await waitFor(it, (s) => s.talk?.guide, { what: "the way to the bartender" });
  check(s.talk.guide.label === `Talk to ${job(s, "Bartender").name}`, `a first visit shows the way: ${s.talk.guide.label}`);

  // In front of the counter, across it from (-2, -13).
  await standAt([-2, -11], [-2, -12]);
  s = await waitFor(it, (s) => s.nearby?.kind === "Bartender" && !s.talk.guide, { what: "the counter" });
  check(true, "at the counter, the bartender can be talked to, and the way there is done");
  await sleep(200);
  await press("KeyE");
  s = await waitFor(it, (s) => s.talk.open?.choices, { what: "the bartender to talk" });
  check(s.talk.open.line.includes(s.talk.open.name), `they say hello: "${s.talk.open.line}"`);
  s = await waitFor(it, (s) => job(s, "Bartender").cell.join() === "-2,-13", { what: "the bartender to come over" });
  check(job(s, "Bartender").aside === "talking", "and come across the counter to talk");

  await pick("What can I play?");
  await waitFor(it, (s) => s.talk.open?.choices?.includes("Racing"), { what: "the kinds of game" });
  await pick("Racing");
  await waitFor(it, (s) => s.talk.open?.choices?.includes("Daytona USA"), { what: "the racing games" });
  await pick("Daytona USA");
  s = await waitFor(it, (s) => s.talk.guide?.label === "Daytona USA", { what: "the way to Daytona USA" });
  check(s.talk.open.line.startsWith("Daytona USA?"), `picking a game shows the way there: "${s.talk.open.line}"`);

  await pick("Back");
  await waitFor(it, (s) => s.talk.open?.choices?.includes("Racing"), { what: "back to the kinds" });
  await pick("Back");
  await waitFor(it, (s) => s.talk.open?.choices?.includes("Language"), { what: "back to hello" });
  await pick("Language");
  await waitFor(it, (s) => s.talk.open?.choices?.includes("Español"), { what: "the languages" });
  await pick("Español");
  s = await waitFor(it, (s) => s.talk.language === "es" && s.talk.open?.choices, { what: "Spanish" });
  check(s.talk.open.choices[0] === "¿Qué puedo jugar?", `in Spanish: "${s.talk.open.line}"`);
  await pick("¿Qué puedo jugar?");
  await waitFor(it, (s) => s.talk.open?.choices?.includes("Carreras"), { what: "the kinds, in Spanish" });
  await pick("Carreras");
  s = await waitFor(it, (s) => s.talk.open?.line === "Esto es lo que tenemos:", { what: "the racing games, in Spanish" });
  check(s.talk.open.choices.includes("Daytona USA"), `titles stay as they are: ${s.talk.open.choices}`);

  await press("Escape");
  s = await waitFor(it, (s) => !s.talk.open && job(s, "Bartender").aside !== "talking", { what: "Esc to end it" });
  check(true, `Esc ends the talk, and the bartender goes back to work (${job(s, "Bartender").aside ?? "back"})`);

  await it.page.reload({ waitUntil: "load" });
  await enter(it);
  s = await waitFor(it, (s) => s.talk, { what: "the bar again" });
  check(s.talk.language === "es", "a reload keeps the language picked");

  // The cleaner, from the cell beside them.
  s = await waitFor(it, (s) => s.staff?.length === 2, { what: "the staff" });
  const at = job(s, "Cleaner").cell;
  await standAt([at[0] - 1, at[1]], at);
  s = await waitFor(it, (s) => s.nearby?.kind === "Cleaner", { what: "the cleaner to be nearest" });
  await press("KeyE");
  s = await waitFor(it, (s) => s.talk.open?.job === "Cleaner", { what: "the cleaner to talk" });
  const stopped = job(s, "Cleaner").cell.join();
  await sleep(2000);
  s = await it.page.evaluate(() => window.vab.state());
  check(job(s, "Cleaner").cell.join() === stopped && job(s, "Cleaner").aside === "talking", `the cleaner stops to talk: "${s.talk.open.line}"`);
  await standAt([at[0] - 4, at[1] - 4], at);
  await waitFor(it, (s) => !s.talk.open, { what: "walking away to end it" });
  check(true, "walking away ends the talk");
  check(it.errors.length === 0, `no errors in the page: ${it.errors}`);
});
