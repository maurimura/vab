// The first screen (web/onboarding.js) is for the first visit: a new player picks a name and a
// look there and goes in; back again (a reload), they go straight in as they were, without the
// picker, the screen only saying how the loading goes; /look in the chat still opens the picker.
import { check, player, run, sleep, waitFor } from "./lib.mjs";

const screen = (it) =>
  it.page.evaluate(() => {
    const onboarding = document.getElementById("onboarding");
    const items = document.getElementById("look-items");
    return {
      up: !onboarding.hidden,
      returning: onboarding.classList.contains("returning"),
      picker: items.getBoundingClientRect().height > 0,
    };
  });
const cookies = (it) =>
  it.page.evaluate(() => Object.fromEntries(document.cookie.split("; ").map((part) => part.split("="))));

await run("The first screen, once", async () => {
  // A first visit: the picker, then in (lib.mjs enter waits for its items and clicks Enter the bar).
  const one = await player("one", `e2e-onboarding-${Date.now()}`);
  const first = await cookies(one);
  check(first.look !== undefined, "a first visit goes in through the picker, and the look is remembered");

  // Back again: straight in, as they were.
  await one.page.reload({ waitUntil: "load" });
  const back = await screen(one);
  check(back.returning && !back.picker, "back again, the first screen has no picker, only the loading");
  await one.page.waitForSelector("#onboarding", { hidden: true, timeout: 60000 });
  const s = await waitFor(one, (s) => s.mode === "Walking", { what: "the bar", timeout: 60000 });
  const again = await cookies(one);
  check(s.mode === "Walking" && again.look === first.look && again.name === first.name, "and in on its own, with the same name and look");

  // /look still opens the picker (a click first gives the page the keyboard).
  await one.page.mouse.click(640, 360);
  await one.page.keyboard.press("KeyY");
  await sleep(200);
  await one.page.keyboard.type("/look");
  await one.page.keyboard.press("Enter");
  await one.page.waitForSelector("#onboarding", { visible: true, timeout: 5000 });
  const reopened = await screen(one);
  check(reopened.up && !reopened.returning && reopened.picker, "/look opens the picker again");
});
