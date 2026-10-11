// Daytona USA on the bar's wheel (steer=wheel, assets/games.ron), in Node: one arcade seat from
// its seat state, the wheel turned frame by frame by web/emulator/wheel.js with the game's
// numbers from assets/games.ron, as the worker turns it. The circuit select follows the wheel's
// position (README "Inputs": BEGINNER in the middle, ADVANCED at +16..+64 of 96, EXPERT from +72)
// and the accelerator chooses; each circuit has its own field, so the race says which was
// chosen. RIGHT is held for a number of frames, the accelerator pressed on its last, and the
// race's cars read; the shim's own ramp (steer=ramp, the old way: a 10-frame hold picked
// ADVANCED, 30 frames EXPERT) says which field is which circuit.
//
//   ROMS=<dir> node daytona/harness/wheel-check.mjs [--states=daytona/dist/states] [--core=...]
//
// Exit status 0 when a short hold picks ADVANCED and a long one EXPERT, as the old ramp did.
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { Wheel } from "../../web/emulator/wheel.js";
import { BUTTONS, Seat } from "./seat.mjs";

const opt = Object.fromEntries(process.argv.slice(2).map((a) => a.replace(/^--/, "").split("=")));
const HERE = import.meta.dirname;
const CORE = resolve(opt.core ?? join(HERE, "../dist/headless/daytona.mjs"));
const ROM = resolve(opt.rom ?? join(process.env.ROMS ?? join(homedir(), "Downloads"), "daytona.zip"));
const STATES = resolve(opt.states ?? join(HERE, "../dist/states"));
const CARS = 0x1080; // main RAM offset of the cars on the track (arcade-check.mjs)

// Daytona USA's wheel in assets/games.ron.
const line = readFileSync(join(HERE, "../../assets/games.ron"), "utf8")
  .split("\n")
  .find((l) => l.includes('rom: "daytona"'));
const [, lock, back, curve] = line.match(/wheel: Some\(\(lock: ([\d.]+), back: ([\d.]+), curve: ([\d.]+)/).map(Number);
console.log(`Daytona USA's wheel: lock ${lock} s, back ${back} s, curve ${curve}`);

const RIGHT_FROM = 560; // in the circuit select (START at 300)
/**
 * The race's cars with RIGHT held `hold` frames from RIGHT_FROM, the accelerator pressed on its
 * last frame: on the bar's wheel, or (`ramp`) on the shim's own ramp, the pad as it is.
 */
async function race(hold, ramp = false) {
  const seat = await Seat.create({ core: CORE, rom: ROM, seat: 0, options: { steer: ramp ? "ramp" : "wheel" } });
  if (!ramp) seat.core.wheel = { lock, back, curve, span: [0, 32767] };
  await seat.load(readFileSync(join(STATES, "daytona.seat0.state")));
  const wheel = new Wheel({ lock, back, curve }, 57.524);
  const held = (f, from, n) => f >= from && f < from + n;
  const chooseAt = hold ? RIGHT_FROM + hold - 1 : 600;
  let atChoice = 0;
  for (let f = 0; f < 2400; f++) {
    let pad = 0;
    if (held(f, 300, 10)) pad |= 1 << BUTTONS.start;
    if (held(f, RIGHT_FROM, hold)) pad |= 1 << BUTTONS.right;
    if (held(f, chooseAt, 10)) pad |= 1 << BUTTONS.up;
    if (held(f, 1200, 10)) pad |= 1 << BUTTONS.start;
    if (f >= 1500) pad |= 1 << BUTTONS.up;
    const input = ramp ? pad : wheel.turn(pad);
    if (f === chooseAt) atChoice = Math.round((((input << 8) >> 24) * 96) / 127);
    seat.run(input);
  }
  return { cars: seat.core.systemRam()[CARS], atChoice };
}

// Which field is which circuit, by the old ramp's picks.
const CIRCUIT = {
  [(await race(0, true)).cars]: "BEGINNER",
  [(await race(10, true)).cars]: "ADVANCED",
  [(await race(30, true)).cars]: "EXPERT",
};
console.log(`  the old ramp's picks: ${Object.entries(CIRCUIT).map(([cars, c]) => `${c} ${cars} cars`).join(", ")}`);
const results = [];
for (const hold of [0, 5, 8, 10, 14, 18, 20, 22, 30]) {
  const { cars, atChoice } = await race(hold);
  const circuit = CIRCUIT[cars] ?? `${cars} cars?`;
  results.push({ hold, circuit });
  console.log(`  RIGHT ${String(hold).padStart(2)} frames (${(hold / 57.524).toFixed(2)} s), the wheel at +${atChoice} of 96: ${circuit}`);
}
const advanced = results.filter((r) => r.circuit === "ADVANCED").map((r) => r.hold);
const ok =
  Object.keys(CIRCUIT).length === 3 &&
  results[0].circuit === "BEGINNER" &&
  advanced.length >= 2 &&
  results.at(-1).circuit === "EXPERT";
console.log(ok ? `PASS a short hold picks ADVANCED (${advanced.join(", ")} frames), a long one EXPERT` : "FAIL");
process.exit(ok ? 0 : 1);
