// Makes a start-up save state for a game: boots it, presses a button to get past any
// "settings restored" screen, inserts coins and saves the machine. The page loads this state
// right after the game, so it starts ready to play with credits.
//
//   node emulator/snapshot.mjs <core.mjs> <rom.zip> <out.state> [coins] [file ...]
//   node emulator/snapshot.mjs emulator/dist/midway/fbneo.mjs ~/Downloads/mk2.zip emulator/dist/mk2.state 9
//   node emulator/snapshot.mjs emulator/dist/neogeo/fbneo.mjs ~/Downloads/mslug.zip emulator/dist/mslug.state 9 ~/Downloads/neogeo.zip
//   BOOT=175 node emulator/snapshot.mjs flycast/dist/flycast.mjs flycast/.cache/roms/vtennisg.zip flycast/dist/vtennisg.state 0 flycast/.cache/roms/vtennisg/gds-0011.chd
//
// The files after the coins go with the ROM set as the page puts them (romPath in libretro.js):
// a BIOS set next to it, or at their path after a `roms/` folder (a NAOMI game's disc,
// roms/vtennisg/gds-0011.chd, in the folder named after its set). BOOT is the seconds of game
// time the boot takes (10; a NAOMI's BIOS and GD-ROM take about 175). The state is written
// deflated, marked as the worker marks states ("vabz"), which it unpacks: a NAOMI's is 73 MB raw.
// States only load into the same core build: rerun this after rebuilding the cores.
import { readFileSync, writeFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { Core, romPath as pathUnderRoms } from "../web/emulator/libretro.js";
import { actionButtons } from "./action-buttons.mjs";
import { packState } from "./packed-state.mjs";

const [corePath, romPath, outPath, coins = "9", ...filePaths] = process.argv.slice(2);
const BOOT = Number(process.env.BOOT ?? 10);
const { default: createFBNeo } = await import(resolve(corePath));
const core = await Core.create(createFBNeo, { onFrame() {}, onAudio() {} });
for (const path of filePaths) core.addFile(pathUnderRoms(resolve(path)), readFileSync(path));
const { fps } = core.loadGame(basename(romPath), readFileSync(romPath));

// RetroPad SELECT inserts a coin; use the driver's first action to confirm boot.
// UMK3's CMOS warning checks High Punch (Y), not Low Punch (B).
const SELECT = 1 << 2;
const run = (seconds, mask = 0) => {
  core.inputs[0] = mask;
  for (let i = 0; i < Math.round(seconds * fps); i++) core.run();
};

run(BOOT); // boot; action descriptors are now available
const confirm = actionButtons(core.buttons)[0]?.[0] ?? 0;
run(0.2, 1 << confirm); // "any button to continue"
run(5);
for (let i = 0; i < Number(coins); i++) {
  run(0.2, SELECT);
  run(0.3);
}
run(2);
const state = core.serialize();
const packed = packState(state);
writeFileSync(outPath, packed);
console.log(`Saved ${outPath}: ${(packed.length / 1e6).toFixed(1)} MB (${(state.length / 1e6).toFixed(1)} MB unpacked)`);
