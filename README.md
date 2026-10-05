# Arcade Bar

The page shows the bar from `assets/maps/bar.ron` (made with the editor, see [Dev
tools](#dev-tools)) and a placeholder player: arrows or WASD walk, and floor tiles without an
object are walkable. Everyone on the page is in the same bar room and sees the others walk around
(`?room=<name>` opens a separate one). E next to a cabinet sits you at it: you start its game, or
join the one being played there (see [Online play](#online-play)). F watches the game being
played there, as does E once every seat is taken (see [Watching](#watching)). Players at a
cabinet can talk (see [Voice](#voice)). Esc stands up. Y opens the chat for everyone in the room; `/name <name>` there sets the name shown above your
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

| Path | What | Built with |
| --- | --- | --- |
| `client/` | Bevy app, mounted on `<canvas id="bevy">`; its text font (Fira Mono cut to Latin-1, OFL) is in `fonts/` | `cargo` + `wasm-bindgen` → `web/pkg/` |
| `server/` | Worker + `Room` Durable Object (WebSocket Hibernation) | `workers-rs` template, `wrangler` |
| `netplay/` | Rollback for two players at a cabinet (GGRS), run by the emulator worker | `cargo` + `wasm-bindgen` → `web/netplay/` |
| `emulator/` | Per-system FBNeo libretro cores as Emscripten ES modules | emsdk + FBNeo's Makefile → `emulator/dist/<core>/` |
| `hockey/` | Air hockey physics and the bot, without Bevy, tested natively (`cargo test -p hockey`) | |
| `shuffleboard/` | Table shuffleboard physics and scoring, without Bevy, tested natively (`cargo test -p shuffleboard`) | |
| `billiards/` | Pool physics, without Bevy: deterministic, tested natively (`cargo test -p billiards`) | |
| `world/` | Map format, isometric grid math, tile drawing (shared by the editor and, later, the client) | |
| `tools/e2e/` | Browser tests: players in headless Chrome against `make dev` (`make e2e`), reading the game through hooks only local builds have | `puppeteer-core` |
| `tools/editor/` | Bar layout editor: on the desktop (`make editor`), or on the web behind Cloudflare Access (`make editor-web`, served by the same Worker as `vab-editor`) | `cargo`, `bevy_egui` (+ `wasm-bindgen` → `tools/editor/web/pkg/`) |
| `assets/` | Tile art (`tiles/`) and maps (`maps/`) | |
| `web/` | Static assets: `index.html`, the room connection (`room.js`), Bevy's `pkg/`, the emulator worker + libretro frontend in `emulator/` | |

Routes: static files from `web/`, `GET /ws/:room` (WebSocket to that room's Durable Object), `GET /ice` (WebRTC servers), `GET /fbneo/<core>/fbneo.{mjs,wasm}` (FBNeo cores) and `GET /roms/<file>` (ROM sets), both from R2, and `GET /assets/maps/bar.ron` (the bar's map: the one last saved from the web editor, from R2, or the one in `web/`; `PUT` saves it, from the editor Worker only).

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
make upload-rom R2_TARGET=--remote ROM=$HOME/Downloads/mk2.zip   # each ROM, BIOS and .state
make deploy                                               # or merge to main
make editor-deploy                                        # the web editor, likewise
```

A preview Worker, `vab-preview`, runs the same site with its own rooms and its own R2 bucket, for
trying changes on several computers before they reach the site:

```sh
cd server && npx wrangler r2 bucket create vab-preview && cd ..   # once
make emulator-remote R2_BUCKET=vab-preview
make upload-rom R2_TARGET=--remote R2_BUCKET=vab-preview ROM=$HOME/Downloads/mk2.zip   # each
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
`assets/games.ron`, 2 unless said, up to 4 as in Sunset Riders), each on their seat's controls.
The first plays alone right away. Whoever sits down later joins that game as it is: the lowest
seat among those playing captures its machine and hands it to everyone through the room, and all
of them start a new GGRS session (`netplay/`) from it. Someone leaving works the same way. Each
machine guesses the others' input and re-runs frames when the real one arrives; the worker picks
the rollback limit from how fast the machine runs the game (Mortal Kombat II gets 3 frames and 3
frames of input delay on an M-series Mac, the rest 8 and 2). GGRS compares a hash of the game's
RAM every 60 frames and reports any desync.

Game packets go through the room's WebSocket at first and straight between the browsers over
WebRTC once that connects (`web/room.js`). Turn-based games (`turns`) use player 1's controls for
both players, like an upright cabinet.

## Voice

Players at a cabinet talk over the same WebRTC connections as their game: each link carries
audio both ways, and the microphone goes on it once the browser allows it (it asks when someone
first plays with you). Echo cancellation and noise suppression are on; headphones still help,
since the game's sound plays from the same speakers. A panel under the status line lists
everyone at the cabinet like a voice channel (`client/src/voice.rs`), with a green ring while
they talk. M turns your microphone off or on. Clicking someone, Shift with their player number,
or `/mute <name>` in the chat mutes them for you only (`/unmute <name>` undoes it); mutes are
remembered by name in a cookie. Voice needs the browsers to reach each other directly or through
TURN: players whose game goes through the room show "no voice".

## Watching

Anyone can watch a cabinet's game. Each watcher's browser runs the game itself, a little behind
the players: the lowest seat playing sends them a state, then every player's input for each frame
once GGRS has confirmed it, so no rollback can change it. Inputs go out about ten times a second,
to everyone watching through one message to the room (address 0), and each watcher keeps a few
frames in hand so they play evenly. When the players change, the new session starts a new stream
with a fresh state. Watchers cost the players nothing: their game never pauses for one.

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
