# Cabinet settings presets

Daytona USA keeps its operator settings (cabinet type, LINK ID master/slave, CAR NUMBER,
difficulty, coins/free play, ...) in the I/O board's 128-byte settings EEPROM, and records in its
16 KB backup RAM. The recomp saves both as `ioboard_eeprom.bin` and `backup_ram.bin`
(`tools/common/nvram.h`, the app's data folder). Revision A's factory settings are a **linked
twin cabinet, LINK ID master, car 1**: alone it waits for a second cabinet at its settings
screen, and two cabinets on factory settings are both masters, so the link never comes up.

So every cabinet of a machine needs a preset:

| directory | machine | cabinet | settings |
|---|---|---|---|
| `1/0/` | `cabinets=1` | the only one | single cabinet (no link), free play |
| `2/0/` | `cabinets=2` | port 0, the master | twin, LINK ID master, car 1, free play |
| `2/1/` | `cabinets=2` | port 1, the slave | twin, LINK ID slave, car 2, free play |

Both twin presets must match in everything but LINK ID and CAR NUMBER: the game cancels the
link ("CANCELLED") between cabinets whose settings differ.

## Where the core looks

At power-on (load and every `retro_reset`), cabinet *k* of an *N*-cabinet machine takes its two
files from the first of:

1. `<nvram_dir>/<k>/` in the module's file system (`daytona_set("nvram_dir", ...)`, default
   `/nvram`), for a frontend that brings its own;
2. `/daytona/nvram/<N>/<k>/`: these presets, embedded by `build.sh` when `daytona/nvram/1/` or
   `daytona/nvram/2/` exist (`daytona_set("presets", "0")` skips them);
3. the factory's (the log says so, and that the link will not come up).

A file of the wrong size is reported and skipped.

## Making them (needs the ROM set)

The presets are made by the game itself, headless: `make-nvram.mjs` boots one cabinet from
factory settings with the Node build, drives its test menu from an input script
(`scripts/*.txt`, the recomp's `scripts/inputs` format, the same `m2run --inputs` reads), then
saves the EEPROM and backup RAM, as `m2run --save-nvram` does. Revision A's menus, as found
with `explore` on 2026-10-06: TEST opens TEST MODE (BOOKKEEPING, GAME SYSTEM, COIN ASSIGNMENT,
INPUT TEST, OUTPUT TEST, DRIVE BD TEST, SOUND TEST, TGP TEST, MEMORY TEST, BACKUP RAM CLEAR,
EXIT; the cursor starts on EXIT). GREEN (VR4) moves the cursor up, RED (VR1) down, YELLOW (VR3)
and BLUE (VR2) change the value, START chooses. GAME SYSTEM: LINK ID (MASTER, SLAVE, SINGLE),
CAR NUMBER, CABINET (TWIN, UPLIGHT, DELUXE), COUNTRY (JPN), DIFFICULTY, ADVERTISE SOUND, GAME
MODE, RIVAL ARROW. COIN ASSIGNMENT: CREDIT TO START, COIN/CREDIT SETTING (#1 1 coin 1 credit;
one value down from #1 is #27 FREE PLAY). The scripts set:

| preset | script | settings (all else factory: TWIN, NORMAL, ...) | after the script |
|---|---|---|---|
| `1/0/` | `single.txt` | LINK ID SINGLE, COUNTRY USA, #27 FREE PLAY | the attract mode, FREEPLAY |
| `2/0/` | `twin-master.txt` | LINK ID MASTER, CAR NUMBER 1, COUNTRY USA, #27 FREE PLAY | NETWORK CHECKING, THIS IS MASTER CONTROLLER |
| `2/1/` | `twin-slave.txt` | LINK ID SLAVE, CAR NUMBER 2, COUNTRY USA, #27 FREE PLAY | NETWORK CHECKING, THIS IS SLAVE MACHINE |

With `2/0` and `2/1` a fresh two-cabinet core comes up linked (cabinet 1 of 2 and 2 of 2) and
shows the linked attract mode in English (COUNTRY USA, set 2026-10-07: the factory JPN showed
通信システム, 2人まで対戦できます; YELLOW cycles JPN, EXPORT, USA), FREEPLAY on both screens. To make them again (another recomp, other settings):

1. Build the core with the ROM set: `./daytona/build.sh headless`.
2. Check or change a script by watching it:

       node daytona/make-nvram.mjs explore daytona/nvram/scripts/twin-slave.txt --every 100 --shots /tmp/slave

   writes a PNG of the screen every 100 frames and a contact sheet of them (`sheet.png`).
3. `node daytona/make-nvram.mjs make` writes `1/0/`, `2/0/` and `2/1/` here (a script still
   holding a `# TODO` line is refused).
4. `./daytona/build.sh` again (embeds them), then `node daytona/check.mjs`: its link check wants
   both cabinets of a `cabinets=2` machine linked, 1 of 2 and 2 of 2.

## A ring of eight (not built in)

`8/0`-`8/7` for `cabinets=8` (harness/ring-lab.mjs, ../ring-notes.md): `8/0` is `twin-master.txt`
(MASTER, car 1), `8/1` `twin-slave.txt` (SLAVE, car 2), `8/2`-`8/7` `ring-car3.txt`-`ring-car8.txt`
(SLAVE, cars 3-8: as `twin-slave.txt` with CAR NUMBER pressed up 2-7 times, 30 frames apart, and
the rest 200 frames later). Checked with `explore` (CAR NUMBER 8, COUNTRY USA, #27 FREE PLAY,
NETWORK CHECKING / THIS IS SLAVE MACHINE); `8/0` and `8/1` come out byte for byte as `2/0` and
`2/1`, the others differ from `8/1` in EEPROM byte 13 (car number - 1) and its checksum (bytes
9-10). They are not embedded (`core.mk` takes `nvram/1` and `nvram/2`): make them elsewhere and
hand them over with `nvram_dir`:

    node daytona/make-nvram.mjs make --only 8/ --out /some/dir     # writes /some/dir/8/<k>/

The arcade mode's seat states (`../make-states.mjs --nvram=/some/dir`) are made from a ring on
these: each seat's state carries its cabinet's EEPROM and backup RAM, so a seat needs no preset.

Whether to commit the `.bin` files is the user's call: they are settings the game wrote, not ROM
data, but the backup RAM is the game's own record keeping (its ranking table included). They are
not git-ignored, so `git status` shows them.
