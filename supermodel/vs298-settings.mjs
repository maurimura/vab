// Virtua Striker 2 '98's operator settings for the bar, made in the game's own test menu before
// emulator/snapshot.mjs saves the start-up state:
//
//   SETUP=supermodel/vs298-settings.mjs BOOT=15 node emulator/snapshot.mjs \
//     supermodel/dist/headless/supermodel.mjs ~/Downloads/vs298.zip supermodel/dist/vs298.state 9
//
// Sega ships the board with GAME ASSIGNMENTS' V GOAL SYSTEM and PK SYSTEM off, so a level match
// just ends in a draw. Here, for 1P and VS play, a level match goes to golden-goal extra time,
// 45 seconds of it ("ON [TIME 0'45"]": the 2'00" match stands for 90 minutes, so about France
// '98's 30), then to penalties, five kickers a side ("ON [MEMBER 5]"). A shootout still level
// after five each is a draw: there's no sudden death (V GOAL's "TIME FREE" would play on until
// a goal instead, and never reach penalties). The board keeps its settings in its EEPROM, which
// a reset leaves alone and the save state carries, so every cabinet started from the state has
// them.
//
// The test menu takes the operator's buttons, which the shim puts on port 0's L3 (Test: open,
// pick, change) and R3 (Service: next line); the bar's keys never press them.
const TEST = 1 << 14;
const SERVICE = 1 << 15;

/** Walks the menu blind (the headless core draws nothing), then powers the board back on. */
export default function setup({ core, run }) {
  const press = (button, times = 1) => {
    for (let i = 0; i < times; i++) {
      run(0.1, button);
      run(0.2);
    }
  };
  press(TEST); // the test menu, on EXIT
  run(2);
  press(SERVICE, 8); // down to GAME ASSIGNMENTS
  press(TEST);
  run(1);
  press(SERVICE, 7); // down (from EXIT) to 1P SETTING's V GOAL SYSTEM
  press(TEST, 4); // OFF -> 0'15" -> 0'20" -> 0'30" -> 0'45"
  press(SERVICE); // its PK SYSTEM
  press(TEST, 3); // OFF -> MEMBER 3 -> 4 -> 5
  press(SERVICE, 2); // VS SETTING's V GOAL SYSTEM
  press(TEST, 4);
  press(SERVICE); // its PK SYSTEM
  press(TEST, 3);
  press(SERVICE); // EXIT, back to the test menu, on GAME ASSIGNMENTS
  press(TEST);
  run(1);
  press(SERVICE, 6); // the test menu's EXIT, where the game writes the settings to the EEPROM
  press(TEST);
  run(1);
  // That EXIT doesn't take the game out of the test menu here; a power cycle does.
  core.reset();
  run(20); // to the attract mode
}
