// Ultimate Mortal Kombat 3's hidden fighters for the bar: Mileena, Ermac and Classic Sub-Zero
// on the select screen from the first game, made before emulator/snapshot.mjs saves the start-up
// state:
//
//   SETUP=emulator/umk3-unlock.mjs node emulator/snapshot.mjs \
//     emulator/dist/midway/fbneo.mjs ~/Downloads/umk3.zip emulator/dist/umk3.state 9
//
// On the board they take an Ultimate Kombat Kode each, entered on the game-over screen after a
// lost game (Mileena 22264 22264, Ermac 12344 44321, Classic Sub-Zero 81835 81835). The game
// keeps what a kode unlocks in its CMOS, so an operator's cabinet has them for good: it writes
// 0xED, 0xB0 to the fighter's two words below and takes their sum off the checksum of the block
// they're in, and nothing else (entering each kode here showed just that; FBNeo's cheat.dat has
// the same words as "Unlock Mileena" etc., 0x2810FC on in its byte addresses, without the
// checksum). Here the same is written into the CMOS in the machine's state (FBNeo's "NV RAM":
// the CMOS's 16-bit words, the byte in the low half), then the board is powered on again: it
// finds its CMOS OK and reads them, as after an operator's power cycle. They show on the select
// screen in the three dragon squares on Kano's row, for both players. Human Smoke has no kode
// and no flag: a player picks Smoke and holds Back + High Punch + Block + High Kick + Run until
// the fight starts.

/** Each fighter's two words, as byte offsets into FBNeo's NV RAM (CMOS word index * 2). */
const FIGHTERS = { mileena: 0x10fc, ermac: 0x1104, classicSubZero: 0x110c };
const UNLOCKED = [0xed, 0xb0];
/** Their block's checksum: 0xFFFF minus the sum of the block's bytes, high byte first. */
const CHECKSUM = [0x1160, 0x1162];
/** The game's own revision, which it keeps in the CMOS too, past the checksum: finds the CMOS. */
const REVISION = { at: 0x1164, text: "REVISION 1.2" };

export default function setup({ core, run }) {
  const state = core.serialize();
  const cmos = state.subarray(findCmos(state));
  let taken = 0;
  for (const at of Object.values(FIGHTERS)) {
    UNLOCKED.forEach((value, i) => {
      taken += value - cmos[at + i * 2];
      cmos[at + i * 2] = value;
    });
  }
  const [high, low] = CHECKSUM;
  const checksum = (((cmos[high] << 8) | cmos[low]) - taken) & 0xffff;
  cmos[high] = checksum >> 8;
  cmos[low] = checksum & 0xff;
  core.unserialize(state);
  core.reset();
  run(10); // power-on ("CMOS OK" this time), on to the attract mode
}

/** Where the CMOS starts in a state: the one place the revision is, in its words' low bytes. */
function findCmos(state) {
  const pattern = [...REVISION.text].flatMap((c) => [c.charCodeAt(0), 0]);
  const found = [];
  for (let i = 0; i + pattern.length <= state.length; i++) {
    if (pattern.every((b, j) => state[i + j] === b)) found.push(i - REVISION.at);
  }
  if (found.length !== 1) throw new Error(`UMK3's CMOS: ${found.length} places look like it in the state`);
  return found[0];
}
