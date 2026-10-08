# Arcade Bar

The page shows the bar from `assets/maps/bar.ron` (made with the editor, see [Dev
tools](#dev-tools)) and a placeholder player: arrows or WASD walk, and floor tiles without an
object are walkable. Everyone on the page is in the same bar room and sees the others walk around
(`?room=<name>` opens a separate one). E next to a cabinet sits you at it: you start its game, or
join the one being played there (see [Online play](#online-play)); next to a table (pool, air
hockey, shuffleboard, darts, below) it sits you at that. F watches the game being played there,
at a cabinet, a table or the dartboard, as does E once every seat is taken (see
[Watching](#watching)). Players sitting together can talk (see [Voice](#voice)). Esc stands up. Y
opens the chat for everyone in the room; `/name <name>` there sets the name shown above your
head, and a cookie keeps it. The controls show on a first visit and with `/help`; when a game
starts, a card lists its buttons as the game names them (the core reports them, e.g. "Z  Low
Punch").

E next to the pool table plays 8-ball: the table seen from above, the cue following the mouse
(or a finger) around the cue ball. Holding the button pulls the cue back, further the longer
it's held, and letting go shoots. Alone at the table, you take both sides in turn; when someone
sits at the other seat, you play each other, each shooting on their own turn and watching the
other's cue on theirs (`client/src/pool/online.rs`).

E next to the air hockey table plays air hockey against a bot: the rink seen from above,
upright, your goal at the bottom. Your paddle goes where the mouse (or a finger) is, kept in your
half; the bot slides its paddle back and forth across its goal. First to 7 wins. `/settings`
tunes it too (the puck's top speed and friction, how bouncy the rails and paddles are, the bot's
speed).

When someone sits at the air hockey table's other seat, the two play each other with rollback,
as at the cabinets: each machine runs the whole game a frame at a time from both players'
inputs (their paddles), guessing the other's until it arrives and replaying the frames since
when a guess was wrong, so each player's own paddle answers at once. Before a match, player 1
times a few round trips over the link the match will use, and inputs go 1 frame late on a
quick connection, up to 3 on a slow one (`input_delay_for` in `client/src/hockey/online.rs`),
straight between the browsers once WebRTC connects. While the other player's input is on its way, their paddle is guessed to carry on as
it was going, and what a correction moves is drawn gliding there rather than jumping.
`/netstats` in the chat shows how a match is doing: how often the browser draws, the ping,
whether packets go straight or through the room (if they go through the room, see TURN below),
rollbacks, and frames skipped or waited. `?lag=120` in the URL holds table packets back about
that many milliseconds, to try it over a slow connection. `/settings` in the chat tunes how it plays
(shot speeds, friction, bounce, pocket size, how the cue pulls back) and racks the balls again.

On a phone or tablet (a screen whose main pointer is a finger) the controls are on the screen
instead (`client/src/touch.rs`): a thumb dragged anywhere walks, Play and Watch show by a cabinet,
and at one a d-pad sits under the left thumb (fixed in place, so a move's sequence can be tapped
or rolled through) and the game's buttons under the right, named as the game names them, with
Coin, Start and Leave. The chat is typed in the page's own box, since
a canvas can't bring up a phone's keyboard. Chrome's device toolbar shows all of it on a computer,
as long as its pixel ratio is left at the computer's own (an emulated one gets the canvas size wrong).

Time Crisis II is played with a lightgun (`gun` in `assets/games.ron`): the mouse aims over the
game, where a crosshair stands in for the pointer, a click shoots, and the right button or Space
works the pedal (Z and X do too). On a touch screen a finger on the game aims there and shoots
for as long as it's down, and the pedal is a button. Its cabinet is the game's twin cabinet: two
players each play their own screen and gun, linked (see [Online play](#online-play)); the next
to press E watches.

| Path | What | Built with |
| --- | --- | --- |
| `client/` | Bevy app, mounted on `<canvas id="bevy">`; its text font (Fira Mono cut to Latin-1, OFL) is in `fonts/` | `cargo` + `wasm-bindgen` → `web/pkg/` |
| `server/` | Worker + `Room` Durable Object (WebSocket Hibernation) | `workers-rs` template, `wrangler` |
| `netplay/` | Rollback for two players at a cabinet (GGRS), run by the emulator worker | `cargo` + `wasm-bindgen` → `web/netplay/` |
| `emulator/` | Per-system FBNeo libretro cores as Emscripten ES modules | emsdk + FBNeo's Makefile → `emulator/dist/<core>/` |
| `supermodel/`, `daytona/` | The Sega cores, each an Emscripten ES module behind the libretro API: Supermodel for the Model 3 (Virtua Striker 2) and Daytona USA's Model 2 (see [Daytona USA](#daytona-usa)) | their `build.sh` → `supermodel/dist/`, `daytona/dist/` |
| `mame/` | MAME (libretro's fork) with the Namco System 12 and System 23 drivers only, for Tekken 3 and Time Crisis II, as an Emscripten ES module: see [mame/README.md](mame/README.md) | emsdk + MAME's own build (`mame/build.sh`) → `mame/dist/` |
| `flycast/` | Flycast for the Sega NAOMI (Virtua Tennis), as an Emscripten ES module behind the libretro API: see [flycast/README.md](flycast/README.md) and [Virtua Tennis](#virtua-tennis) | emsdk + Flycast's CMake build (`flycast/build.sh`) → `flycast/dist/` |
| `hockey/` | Air hockey physics and the bot, without Bevy, tested natively (`cargo test -p hockey`) | |
| `darts/` | Darts scoring, a game of 301 and the throwing hand's sway, without Bevy, tested natively (`cargo test -p darts`) | |
| `shuffleboard/` | Table shuffleboard physics and scoring, without Bevy, tested natively (`cargo test -p shuffleboard`) | |
| `billiards/` | Pool physics, without Bevy: deterministic, tested natively (`cargo test -p billiards`) | |
| `world/` | Map format, isometric grid math, tile drawing (shared by the editor and, later, the client) | |
| `tools/e2e/` | Browser tests: players in headless Chrome against `make dev` (`make e2e`), reading the game through hooks only local builds have | `puppeteer-core` |
| `tools/editor/` | Bar layout editor: on the desktop (`make editor`), or on the web behind Cloudflare Access (`make editor-web`, served by the same Worker as `vab-editor`) | `cargo`, `bevy_egui` (+ `wasm-bindgen` → `tools/editor/web/pkg/`) |
| `assets/` | Tile art (`tiles/`) and maps (`maps/`) | |
| `web/` | Static assets: `index.html`, the room connection (`room.js`), Bevy's `pkg/`, the emulator worker + libretro frontend in `emulator/` | |

Routes: static files from `web/`, `GET /ws/:room` (WebSocket to that room's Durable Object), `GET /ice` (WebRTC servers), `GET /fbneo/<core>/fbneo.{mjs,wasm}` (FBNeo cores), `GET /supermodel/supermodel.{mjs,wasm}`, `GET /daytona/daytona.{mjs,wasm}`, `GET /mame/mame.{mjs,wasm}` and `GET /flycast/flycast.{mjs,wasm}` (the Sega, MAME and Flycast cores) and `GET /roms/<path>` (ROM sets, and the files a game needs next to one, folders and all: `/roms/vtennisg/gds-0011.chd`), all from R2, and `GET /assets/maps/bar.ron` (the bar's map: the one last saved from the web editor, from R2, or the one in `web/`; `PUT` saves it, from the editor Worker only).

## Setup

Needs `rustup`, Node/npm, `git` and `make`. The Rust toolchain (`rust-toolchain.toml`), emsdk and
`wasm-bindgen-cli` are pinned and installed into the project on first use.

```sh
(cd server && npm install)
(cd server && npx wrangler login)   # once: make dev reads the site's R2 bucket
make dev        # builds the client, serves everything at http://localhost:8787
```

Cores, ROM sets and start-up states live in the site's R2 bucket (see [Deploy](#deploy)), and
`make dev` reads them from there (`remote = true` on the bucket in `server/wrangler.toml`), so
games play locally with nothing built or uploaded. The map shown is `assets/maps/bar.ron`, not
the one saved from the web editor. Working on the emulator itself (`emulator/patches/`, the
FBNeo version) needs cores built here, in the local bucket, which `make dev BUCKET=local` serves
instead, map included:

```sh
make emulator   # FBNeo cores -> emulator/dist, uploaded to local R2. First run takes a while.
# Each ROM set and BIOS set (neogeo.zip for Neo Geo games), zipped and named as FBNeo expects:
make upload-rom ROM=$HOME/Downloads/mk2.zip
# A game's other `files` (assets/games.ron) at their path under /roms/, e.g. a NAOMI game's disc:
make upload-rom ROM=$HOME/Downloads/vtennisg/gds-0011.chd KEY=vtennisg/gds-0011.chd
# Optional start-up state per game: skips boot screens, inserts 9 coins. Redo after rebuilding cores.
node emulator/snapshot.mjs emulator/dist/midway/fbneo.mjs $HOME/Downloads/mk2.zip emulator/dist/mk2.state
make upload-rom ROM=emulator/dist/mk2.state
make dev BUCKET=local
```

Local builds (`make client`, `make dev`) use the `wasm-dev` profile: a change rebuilds in
seconds. The client comes out big (about 85 MB), past the 25 MiB a static asset may be, so it's
served gzipped (about 16 MB) and the page unpacks it as it loads. After switching between
`wasm-dev` and `wasm-release`, restart `make dev`: the files it serves change names. What players download is
`wasm-release`, about 15 MB but minutes to build, as it optimizes the whole program, Bevy
included, for size: `make deploy`, `make preview` and CI always build that, and
`make dev PROFILE=wasm-release` tries it locally.

## Dev tools

`make editor` opens the bar layout editor on the desktop; it also runs on the web, see [Editor on
the web](#editor-on-the-web).

- The palette is every PNG in `assets/tiles/floor/` and `assets/tiles/objects/`. Floor tiles are
  32×16 diamonds drawn centered on their cell. Objects are 32 px wide and any height: the bottom
  point of the image sits on the bottom point of the cell's diamond. An object covering several
  cells (the pool table) is listed in `assets/objects.ron` with its size, is (x + y) × 16 px wide,
  and stands on the whole area from the cell it's placed on.
- Left click paints, right click erases, scroll / arrows / WASD pan, `+` / `-` zoom,
  Cmd+S saves `assets/maps/bar.ron`.
- Clicking an object already on the map selects it instead of painting over it, and the editor
  switches to the Move tool (M, or "Move things" in the palette, which also picks up floor
  tiles). Dragging takes the selection to another cell, where nothing else is in its way; R
  turns a selected cabinet, Delete removes it, Esc lets go, and the panel changes a plain
  cabinet's game. A cabinet's game moves with it. M again goes back to the brush.
- Cabinets are one entry per skin (`assets/tiles/objects/cabinet_<skin>_<facing>.png`, made in
  draw mode), and R turns the one about to be placed through its four views. A skin's cabinets
  run the game that names the skin in `cabinets` in `assets/games.ron`; the plain cabinet runs
  whatever "Cabinet game" says, or nothing.
- Images reload when their files change, so you can edit art in a pixel-art app with the editor
  open. The current tiles are placeholders.

### Editor on the web

The map part of the editor (not draw mode) also builds for the web, and the Worker serves it as
a second deployment, `vab-editor`, from `tools/editor/web/` (`[env.editor]` in
`server/wrangler.toml`). A map saved there goes to R2 (`maps/bar.ron` in the site's bucket) and
the bar shows it from its next page load, no deploy needed; until the first save, the site shows
the map built into it. `make pull-map` copies R2's map into `assets/maps/bar.ron` to commit it,
which also keeps the built-in copy current.

Merging to `main` deploys it; `make editor-dev` runs it at <http://localhost:8788> next to `make
dev`, saving into the local bucket, which `make dev BUCKET=local` shows (built and gzipped like
the client, `wasm-dev` unless told otherwise), and `make editor-preview` deploys it as
`vab-editor-preview`, saving into the preview site's bucket. It must only be reachable by people
you let in, so once it is deployed, put Cloudflare Access in front of it (the site stays public:
the toggle is per Worker; the preview editor gets its own):

1. Dashboard → Workers & Pages → `vab-editor` → Settings → Domains & Routes → `workers.dev` →
   Enable Cloudflare Access. Edit the policy it makes to the emails allowed in (or a login
   method, under Zero Trust → Access → Applications).
2. In `server/wrangler.toml`, under `[env.editor.vars]`, set `ACCESS_TEAM` to the team name
   (Zero Trust → Settings: `<team>.cloudflareaccess.com`) and `ACCESS_AUD` to the application's
   audience tag (its overview page), and deploy again. The Worker checks the token Access signs
   onto each request against them before it saves, and refuses to save until they are set.

## Deploy

Merging to `main` deploys the site (`.github/workflows/ci.yml`): the Worker, its Room Durable
Object and everything in `web/`, and the same Worker again as `vab-editor` with the web editor
([Editor on the web](#editor-on-the-web)). Pull requests get the same build and checks without
deploying.
The workflow needs two repository secrets: `CLOUDFLARE_API_TOKEN` (a token made from the "Edit
Cloudflare Workers" template) and `CLOUDFLARE_ACCOUNT_ID`.

Cores, ROMs and start-up states live in R2 and go up by hand, when they change:

```sh
cd server && npx wrangler r2 bucket create vab && cd ..   # once
make emulator-remote                                      # cores
make supermodel-remote daytona-remote                     # the Sega cores, once built (make supermodel, make daytona)
make mame-remote flycast-remote                           # MAME and Flycast, likewise (make mame, make flycast)
make upload-rom R2_TARGET=--remote ROM=$HOME/Downloads/mk2.zip   # each ROM, BIOS and .state
make upload-rom R2_TARGET=--remote ROM=$HOME/Downloads/vtennisg/gds-0011.chd KEY=vtennisg/gds-0011.chd   # a disc, at its path
make deploy                                               # or merge to main
make editor-deploy                                        # the web editor, likewise
```

A preview Worker, `vab-preview`, runs the same site with its own rooms and its own R2 bucket, for
trying changes on several computers before they reach the site:

```sh
cd server && npx wrangler r2 bucket create vab-preview && cd ..   # once
make emulator-remote R2_BUCKET=vab-preview
make upload-rom R2_TARGET=--remote R2_BUCKET=vab-preview ROM=$HOME/Downloads/mk2.zip   # each
make upload-rom R2_TARGET=--remote R2_BUCKET=vab-preview ROM=$HOME/Downloads/vtennisg/gds-0011.chd KEY=vtennisg/gds-0011.chd
make preview    # https://vab-preview.<account>.workers.dev
make editor-preview   # the editor for it, https://vab-editor-preview.<account>.workers.dev
```

TURN (optional, per Worker): create a TURN key in the Cloudflare dashboard (Realtime → TURN
Server), then `npx wrangler secret put TURN_KEY_ID` and `npx wrangler secret put
TURN_KEY_API_TOKEN` in `server/` (add `--env preview` for the preview). Without it, players whose
browsers can't connect directly play through the room instead, which is slower.

## Pinned versions

| | Version | Where |
| --- | --- | --- |
| Rust | 1.98.1 | `rust-toolchain.toml` |
| Bevy | 0.19.1 | `client/Cargo.toml` |
| workers-rs | 0.8 | `server/Cargo.toml` |
| Emscripten | 6.0.10 | `emulator/emsdk.sh` |
| FBNeo | `aceeebed` (libretro/FBNeo) + `emulator/patches/` | `emulator/build.sh` |

ROM sets must match the FBNeo commit; bump them together. FBNeo's license is non-commercial.

## Rollback netplay

Online play re-runs frames from save states, so both players' machines must end up identical.
`emulator/patches/` adds state FBNeo keeps outside its save states (the 4-way joystick's last
direction, the YM2151's render position). After rebuilding the cores or bumping FBNeo, check each
game (a rollback on every frame, compared with playing straight through on another instance):

```sh
node emulator/rollback-check.mjs emulator/dist/midway/fbneo.mjs $HOME/Downloads/mk2.zip emulator/dist/mk2.state
```

## Online play

Players take a cabinet's free seats in order: as many as its game takes (`players` in
`assets/games.ron`, 2 unless said, up to 4 as in Sunset Riders, 8 at Daytona USA's linked
cabinets, which play another way: see [Daytona USA](#daytona-usa), or 1, when the next to press
E watches), each on their seat's controls.
The first plays alone right away. Whoever sits down later joins that game as it is: the lowest
seat among those playing captures its machine and hands it to everyone through the room, and all
of them start a new GGRS session (`netplay/`) from it. Someone leaving works the same way. Each
machine guesses the others' input and re-runs frames when the real one arrives; the worker picks
the rollback limit from how fast the machine runs the game (Mortal Kombat II gets 3 frames and 3
frames of input delay on an M-series Mac, the rest 8 and 2). GGRS compares a hash of the game's
RAM every 60 frames and reports any desync. A `lockstep` game (Virtua Striker 2 and Virtua
Tennis, whose Sega cores' states are too big to save every frame) never guesses: it runs a frame
once everyone's input for it is in, behind an input delay picked from the round trip and then tuned
to how late the others' inputs actually arrive. The worker runs one frame per slot on a precise
clock, at the game's own rate, and sends each input as soon as it exists, so both machines run
the same slot and swap one input per frame.

Game packets go through the room's WebSocket at first and straight between the browsers over
WebRTC once that connects (`web/room.js`). Turn-based games (`turns`) use player 1's controls for
both players, like an upright cabinet.

A `linked` game (Time Crisis II's co-op) is played differently: the real one is a twin cabinet,
two boards joined by a serial link, and so is the bar's. Each player's browser runs their own
board (seat 0 the Left/Red one, seat 1 the Right/Blue), fed only by their own gun, on its player
1 controls; nothing is shared but the link, which the MAME core emulates and the worker carries
(`web/emulator/linked.js`, [mame/README.md](mame/README.md#linked-cabinets-time-crisis-ii-co-op)):
each frame, a board is handed what the other board transmitted D frames earlier, then runs, then
sends what it transmitted, every frame. D comes from the round trip as a lockstep game's delay
does (2 to 12 frames; each board proposes one and both take the larger) and is fixed for the
session; a board runs at most D frames ahead of the other and waits, as lockstep does, when the
other's bytes are late. Over the reliable, ordered channel lockstep games use. The status line
says "linked with player 2, 4 frames of delay", and "Link lost" if a second goes by with nothing
from the other board (linked boards talk every frame or so, even in attract mode). The link is
set at power-on, so a game in progress can't take a second player: when someone sits at the
other seat, both boards start over from their side's start-up state (`timecrs2-link0.state`,
`timecrs2-link1.state` in R2, made by `mame/link-states.mjs`: the two boards linked, at the mode
select with 4 credits each, saved after the same frame, with the link bytes that were in flight
between them), and when one player leaves, the other's board starts over alone from
`timecrs2.state`. Whatever was being played is lost both times: the price of a cabinet whose
link is set at power-on, for now.

A player's input for a frame, what GGRS sends and confirms and the watchers get, is 32 bits: the
RetroPad's buttons in the low 16 (bit `1 << id`, ids from libretro.h), and at a lightgun game
where the gun points in the high 16, 8 bits across the screen (0 its left edge, 255 its right)
and 8 down (0 the top). Other games leave those 0. The emulator worker answers the core's
RetroPad from the low half and, at a lightgun game only, its lightgun from both
(`web/emulator/libretro.js`).
## Daytona USA

Daytona USA (Sega Model 2, 1994) runs on its own core, `daytona/`, built and put in the local
bucket with `make daytona` (`make daytona-remote` for the site's), served from R2 at
`/daytona/`, with MAME's `daytona` ROM set: `make upload-rom ROM=$HOME/Downloads/daytona.zip`
(add `R2_TARGET=--remote` for the site), and its seat states (below): `make upload-daytona-states`.

It is the bar's arcade mode (`arcade: true` in `assets/games.ron`): one cabinet in the bar seats
up to 8, like an arcade's row of linked sit-downs, and every player's browser runs only their
own cabinet (the core's `cabinets=1`, `link_topology=star` and `seat`, which the worker sets from
the seat). Sitting down at seat k loads `/roms/daytona.seat<k>.state`: that cabinet (link id
k + 1, car k + 1) with its link board up, in the linked attract mode, so nothing ever boots over
the network, the fragile part (`daytona/ring-notes.md`). From then on the cabinet runs at the
Model 2's own 57.524 Hz whoever comes and goes: no handover, no rollback, no lockstep, nobody
waits for anybody (`web/emulator/worker.js`, `web/index.html`'s `Arcade`). After each frame the
worker takes the cabinet's 448-byte block of link data (`_daytona_link_out`) and, when it
changed, sends it to every other player seated there; before the next frame the newest block from
each seat goes in (`_daytona_link_in`), and a seat whose player stands up goes off the link
(`_daytona_link_absent`: their car leaves the track, and a race goes on to its end without
them). The core turns the blocks into what each
cabinet's link board would have heard on the arcade's cable, so the game's own rules do the
rest: Start opens a circuit select with a 15-second count, anyone who presses Start before it
runs out races too (POSITION n/2), the idle cabinets show WAITING FOR YOUR ENTRY meanwhile, and
a Start after that opens a race of its own. The status line says how many cabinets are on the
link and how far the farthest is.

One thing the game leaves to us: someone who stands up after pressing Start but before the race
begins leaves the others in that session waiting for them for good (WAITING FOR OTHER
CHALLENGERS, no timeout). So each worker watches its own cabinet's mode in RAM (0x5010a0: 18 from
its Start until its race, 3 to 11 in the attract mode, 16 while another's session takes entries,
22 racing). A cabinet still at 18 37.5 s after its Start, someone having left meanwhile, is past
anything the game takes when nobody leaves (32.4 s, with no choice made and every count run
out), and goes back to its seat's state, the attract mode: "Race cancelled: a player left before
the start". That's 30 to 32 s after the leaver stood up.

The blocks go straight between the browsers: every two players at the cabinet have a WebRTC link
(`web/room.js`, the lower seat offering, as at any cabinet: 28 links for 8 players, 7 each),
data channels unordered and never resent (a lost block is replaced by the next, and an unchanged
one goes out again every 60 frames anyway). Until a link connects, or if it never does (no TURN
and strict NATs), the blocks for all such players go through the room as one message, which the
room passes on to each (`SEVERAL` in `server/src/lib.rs`). A block is ~26 KB/s a peer.
Measured with `daytona/harness/arcade-lab.mjs` (headless Chromes on one M-series Mac, the site
from `wrangler dev`): with 3 or 4 players and a watcher, every cabinet runs at 57.5 frames a
second and a block takes 0.4 ms from one page to another (p50; p99 4.3 ms), or 2.9 ms through
the room (p99 94 ms, the Durable Object on a busy laptop). With 100 ms added to every send the
game plays the same: the join window, a race of two, a late Start's race of its own. 8 players
make 28 direct links, every cabinet taking ~360 blocks a second with all 7 others present, at
50-53 frames a second only because one machine ran all 8 browsers.

A watcher sees one player's cabinet at a time, the lowest seat's to start with; Left and Right go
to the next. The watcher asks that player, whose worker streams them its cabinet's state with the
blocks it had then, and then each frame's controls and the blocks that went in before it (as
changes from the seat's last block: about a kilobyte and a half a second for two others), so the
watcher's machine sees exactly what the player's did. Voice runs over the links as at any cabinet,
7 others at most; Shift+1-8 mutes.

The arrows steer, accelerate and brake, Z and X shift down and up, and A S D C are the view
buttons: the help card lists them as the core names them. The worker paces frames at the rate
the core reports, so its sound (834 or 835 samples a frame at 48 kHz) neither runs dry nor piles
up. The core can also be the arcade's twin sit-down in one machine (`cabinets: 2` with a `view`,
each player shown their own cabinet, played online in lockstep); the bar no longer uses that.
Its cabinet skin is still to come: for now it stands in a plain cabinet next to Virtua Striker's
in `assets/maps/bar.ron`, and in the editor a plain cabinet can be given the game.
`daytona/harness/arcade-lab.mjs` tries the arcade mode end to end in headless Chromes.

## Virtua Tennis

Virtua Tennis (Sega NAOMI, 1999; Power Smash in Japan) runs on its own core, `flycast/`
([flycast/README.md](flycast/README.md)), built and put in the local bucket with `make flycast`
(`make flycast-remote` for the site's), served from R2 at `/flycast/`. It's a GD-ROM game, MAME's
`vtennisg`, so it comes as two files: the ROM set, with the NAOMI BIOS and the game's security
chip, and the disc, a CHD, which Flycast looks for in a folder named after the ROM set. In R2
they sit as they would on disk, `/roms/vtennisg.zip` and `/roms/vtennisg/gds-0011.chd`:

```sh
make upload-rom ROM=$HOME/Downloads/vtennisg.zip
make upload-rom ROM=$HOME/Downloads/vtennisg/gds-0011.chd KEY=vtennisg/gds-0011.chd
```

(add `R2_TARGET=--remote` for the site, and `R2_BUCKET=vab-preview` for the preview's bucket).
The disc is the game's `files` in `assets/games.ron`: paths under `/roms/` that the page has the
worker download along with the ROM set (and its BIOS set, for a game with one), each written at
the same path in the core's file system, folders made as needed (`romPath` and `Core.addFile` in
`web/emulator/libretro.js`). It is about 45 MiB, fetched like the ROM sets: a browser that has
it only asks the server whether it changed.

The frontend sets the core's options (`OPTIONS` in `web/emulator/libretro.js`, keys `reicast_*`,
Flycast's old name): one thread with the frame drawn within `retro_run`, no frame skipping, the
NAOMI's own 640x480, nothing the board didn't have, the USA BIOS so the text is English, the disc
read at its own pace, no network, free play. Two players, each a stick and two shot buttons,
Coin and Start; the help card names the buttons as the core does.

A NAOMI boots for almost three minutes (the BIOS, then the GD-ROM), and the BIOS's own screens
stay black in this build, so the cabinet starts from a state saved in the attract mode, made
with the other games' tool and a longer boot:

```sh
BOOT=175 node emulator/snapshot.mjs flycast/dist/flycast.mjs $HOME/Downloads/vtennisg.zip flycast/dist/vtennisg.state 0 $HOME/Downloads/vtennisg/gds-0011.chd
make upload-rom ROM=flycast/dist/vtennisg.state
```

(no coins: the board is on free play; the disc's path must have a `roms/` folder in it for the
tool to put it in the set's folder, as `flycast/.cache/roms/vtennisg/gds-0011.chd` has). The
state is 76 MB, deflated to 20 MB in the file. Remade after `make flycast`, like the others.

A frame costs 10-13 ms in Chrome, between Tekken 3's and Virtua Striker 2's, too much to replay
for rollback with a state that size, so online it plays in lockstep (`lockstep: true`), as
Virtua Striker 2 does. Flycast compiles the game's code as it runs (an SH4 to WebAssembly JIT),
about 2600 blocks over the first seconds with frames of 100-300 ms among them, and in lockstep
each such hitch is a wait for the other player: the compiled code survives the power-on and
state load of a handover (flycast/patches/0007), and a joiner, whose machine hasn't run the game
yet, plays 300 frames blind from the handed-over state and goes back to it before the session
starts (`vab_warmup` in `assets/games.ron`, `web/emulator/worker.js`), so both machines start
with the code compiled. `node flycast/lab.mjs` tries the cabinet end to end in headless Chromes
of its own against `make dev BUCKET=local`: one player alone, from the start-up state into a
match, then a second at the same cabinet (both in lockstep: 60 fps, 4 frames of input delay on
one machine), then a watcher; with `--delay=40 --jitter=15` the pages' direct links carry that
one-way delay, to see the game settle after a join at a chosen ping. The machine's own
determinism, Chrome against Node and through saves, rollbacks and the JIT's background
compiles, is `node flycast/check.mjs` ([flycast/README.md](flycast/README.md)).

## Voice

Players sitting together talk, wherever that is: a cabinet, a table, any place with seats in the
room. Voice follows the room's seats alone (`Voice` in `web/index.html`), so a new game gets it
without doing anything. Each player calls each other player there (`Room.call` in `web/room.js`):
a WebRTC connection of its own, audio both ways, straight between the browsers or through TURN,
whatever the game's own packets do. The microphone goes on once the browser allows it (it asks
when someone first sits with you) and off when nobody is left to talk to. Echo cancellation and
noise suppression are on; headphones still help, since the game's sound plays from the same
speakers.

A panel in the top-left corner (under a cabinet's status line) lists everyone there like a voice
channel (`client/src/voice.rs`), with a green ring while they talk. M turns your microphone off
or on. Clicking or tapping someone, Shift with their player number, or `/mute <name>` in the chat
mutes them for you only (`/unmute <name>` undoes it); mutes are remembered by name in a cookie. A
click on the panel is the panel's: the tables count it as busy (`VoicePointer`), and on a touch
screen its lines are touch buttons. Players no call can reach show "no voice".

## Watching

Anyone can watch a cabinet's game. Each watcher's browser runs the game itself, a little behind
the players: the lowest seat playing sends them a state, then every player's input (32 bits a
controller port) for each frame once GGRS has confirmed it, so no rollback can change it. Inputs
go out about ten times a second, to everyone watching through one message to the room (address
0), and each watcher keeps a few frames in hand so they play evenly. When the players change, the new session starts a new stream
with a fresh state. Watchers cost the players nothing: their game never pauses for one. At a
`linked` game the watcher sees the lowest seat's own board (the red one while both play): its
state, marked with the side it's linked on, then for each frame its player's input and the link
bytes the board was handed before it (`WATCH_LINK` in `web/room.js`), which the watcher's board,
linked on the same side, is handed in turn. At Daytona USA's linked cabinets a watcher
follows one player's cabinet, streamed by that player ([Daytona USA](#daytona-usa)).

The tables and the dartboard can be watched the same way (F, or E once both seats are taken),
through the room, which passes a message sent to address 0 on to everyone watching the sender's
table (`client/src/seats.rs`). At the pool and shuffleboard tables and the dartboard the
watcher's game is the players' own protocol: the lowest seat gives each new watcher the whole
game between shots, as it does a player who sits down, and from then on whatever the player
whose turn it is sends the other (where the cue, the puck or the hand is, the shot, where
everything ended) goes to the watchers too, so their table plays each shot out as the players'
do and ends where the shooter's did. Air hockey
is too quick for that without rollback of its own, so the lowest seat playing sends the watchers
the rink itself, about 20 times a second; in between, a watcher's rink plays on as it was going,
and what each state puts right is drawn gliding there, as online play's corrections are. A
watcher's hands are off the cue, the puck, the paddles and the darts, and they see who plays
over the table. `tools/e2e/watch.mjs` tries all four with two players in headless Chrome.

`mame/linked-check.mjs` does the same for Time Crisis II's twin cabinet: two workers as the two
seats, one alone first, then both starting over linked, shooting their way into a linked game,
then one leaving, with a watcher on the first; `mame/link-states.mjs` makes the start-up states
it needs (redo them whenever the MAME core is rebuilt, then `make upload-rom` each of the three),
and `mame/linked-lab.mjs` plays it in three headless Chromes against `make dev BUCKET=local`
(a cabinet set to `timecrs2` on the map):

```sh
node mame/link-states.mjs $HOME/Downloads/timecrs2.zip          # -> mame/.cache/roms/timecrs2{,-link0,-link1}.state
node mame/linked-check.mjs $HOME/Downloads/timecrs2.zip
```

`emulator/netplay-check.mjs` plays a game between workers in Node over a simulated network:
player 1 alone, the others dropping in one by one, then player 2 leaving. Player 1 streams to a
watcher the whole time, and a machine in the script checks the stream against fresh states from
player 1 every 1.5 s (needs `make netplay`):

```sh
PLAYERS=4 node emulator/netplay-check.mjs emulator/dist/konami/fbneo.mjs $HOME/Downloads/ssriders.zip emulator/dist/ssriders.state
```

## Sizes

Players download each file once (compressed sizes; Workers static assets cap files at 25 MiB):

| File | Raw | gzip |
| --- | --- | --- |
| Bevy client (lean features, logs below `warn` compiled out, `wasm-opt -Oz`) | ~13.8 MiB | ~4.6 MB |
| Bar editor on the web (Bevy's `2d` profile + egui, `wasm-opt -Oz`; only editors load it) | ~19.9 MiB | ~6.8 MB |
| One FBNeo core (neogeo, midway, snowbros, capcom, konami, classics) | ~5–6 MiB | ~3–3.3 MB |

A cabinet loads only its system's core. To add a system, add a line to `CORES` in
`emulator/build.sh` (driver files live under `src/burn/drv` in the FBNeo checkout).
