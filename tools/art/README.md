# Reference-based cabinet assets

## Autonomous cabinet recipes

The project skill is `.agents/skills/create-cabinet/SKILL.md`. In Pi, `/reload`
then `/skill:create-cabinet <game>` loads the research/build/review workflow.
The skill does not install web search tools: use available search/browser tools,
or public archive searches and `curl` as a limited fallback. Rebuilds are offline.

```sh
python3 tools/art/cabinet_pipeline.py validate mslug
python3 tools/art/cabinet_pipeline.py build mslug
python3 tools/art/cabinet_pipeline.py validate simpsons
python3 -m unittest discover -s tools/art -p 'test_*.py'
```

Recipes live in `tools/art/recipes/`. They record the cabinet variant, source
page/image URLs, hashes, rights/uncertainties, dimensions, control layout, and
texture crops/rectification. The pipeline shares the established raycaster,
lighting and editable-layer export. Its texture loader handles crops, perspective
corners, polygon masks, quarter-turn rotation, aspect-preserving letterboxing,
brightness adjustment, thin gameplay-stroke boosting, patches and photographed
white-logo ink extraction. A reviewed renderer
registry keeps recipe metadata from selecting arbitrary Python code. All four
views are validated before writing files. Neither validate nor build edits the
map/catalog or performs network access, ROM uploads, commits or deployment.

The Simpsons recipe is a legacy adapter: the approved renderer is unchanged and
its four sprite PNG hashes are regression-tested. This is not yet a full migration
of every old generator to declarative recipes.

### Remaining catalog: online-reference batch

```sh
for skin in mvsc sf2ce pacman atetris wboy; do
  python3 tools/art/cabinet_pipeline.py build "$skin"
done
python3 tools/art/preview_catalog.py
```

The `upright` renderer handles both ordinary **32×48** cabinets and a **40×56**
Capcom pedestal. All five include four sprites, seven editable layers per facing,
a separate preview and a JSON recipe with archived sources and uncertainties.
Combined review: `art/previews/catalog-batch.png`.

- **Marvel vs. Capcom**: silver Capcom conversion pedestal, dark monitor housing,
  separate neck/foot, original-design crossover marquee and six-button stations.
  Exact pedestal model/region is unverified; no invented character sides.
- **Street Fighter II' Champion Edition**: brown-sided conversion upright,
  granite operator overlay/bezel and six-button stations.
- **Pac-Man**: yellow Midway-style upright with photographed character sides and
  front; red molding follows the restoration photo, not a universal factory claim.
  One documented side supports both views; unseen art is not invented. Two Start
  buttons and one joystick serve two sequential players. The vertical gameplay
  display is letterboxed and brightened at tiny resolution to keep maze lines.
- **Tetris**: Atari Games operator conversion, Soviet skyline marquee, printed
  bezel/deck fascia and two modeled buttons per station. The supplied cabinet photo
  adds the rectified cathedral side decal, surrounded by black, and red molding.
  One documented decal is repeated with readable lettering on both sides.
- **Wonder Boy**: off-white-sided Sega conversion, landscape original-design
  marquee and blue photographed overlay. The supplied photo guides plain sides
  and light edging; the front/bezel stay black, the deck fascia blue. The different
  archive restoration artwork is not mixed into the operator kit. One joystick,
  duplicated left/right Speed/Jump
  pairs and two Start buttons serve two sequential players.

`players` remains the game's catalog seat count; optional `control_stations`
records actual shared/independent joysticks. `buttons_per_player` is the count of
modeled buttons per physical station; fighter start buttons remain photographed
in the overlay rather than extra raised geometry. Neither field alters emulation.

Marquees/overlays come from [Arcade Artwork](https://www.arcadeartwork.org/), not
publisher-hosted masters. Original-design scan classifications are not publisher
authentication or redistribution permission; restoration/photo uncertainties and
exact URLs/hashes are in each recipe. The later supplied Tetris/Wonder Boy photos
are retained separately as `user-supplied-reference`, not classified as original
publisher artwork. Large marquee working copies are reduced to
2048px; original download hashes/dimensions are retained. Screen images are real
captures from already supplied local ROMs and matching existing startup states:

```sh
node tools/art/capture_screen.mjs emulator/dist/classics/fbneo.mjs \
  /path/to/atetris.zip art/references/atetris/screen.png \
  emulator/dist/atetris.state --confirm --frames=1200
```

This confirms Tetris's difficulty selection and waits through the tutorial to
actual play. Default capture is Start for 15 frames then 600 neutral frames.
Optional trailing local ZIPs provide BIOS/parents; this tool never fetches ROMs.
After replacing a reference, update its recipe SHA-256 before rebuilding.

All catalog games now have selectable skins. The editor displays game titles only,
with internal skin identity retained to distinguish same-title variants. No map
placement, ROM upload, commit or deployment is part of this batch.

### NBA Jam — four players

`python3 tools/art/cabinet_pipeline.py build nbajam` builds four **48×56** views,
seven editable layers per facing and `art/previews/nbajam.png`. The `wide` renderer
reuses the established four-player shell without changing Sunset Riders outputs.
The 1993-style cabinet has orange basketball-textured NBA side logos, black trim,
a blue/red court deck, two blue and two red joysticks and twelve raised action
buttons (red Shoot/Block, blue Pass/Steal, white Turbo). Start buttons stay in the
photographed overlay. One documented side supplies both sides with readable
branding; the opposite original print remains unverified.

Archive cabinet/panel photos and marquee artwork are corroborated by a printed
1993 Midway flyer. Its cabinet already carries NBA Jam Session fascia branding;
this is not Tournament Edition or a substituted game. The marquee is a PSD preview
of uncertain scan-versus-restoration provenance, not a publisher-hosted master.
All URLs, hashes, rights and adaptations are in `tools/art/recipes/nbajam.json`.
The display is a real **400×254** tip-off/tutorial capture from the supplied ROM.

The game uses the **existing `midway` FBNeo WASM core**, four simultaneous seats,
no separate BIOS and the unchanged browser controls/rollback/watch flow. The
provided `nbajam.zip` has all 21 ROMs matching pinned FBNeo revision 3.01 by CRC/size;
legacy short filenames are accepted. ZIP/state are staged only in ignored
`emulator/dist/`, never in tracked art or web assets. For reproducible setup:

```sh
node emulator/snapshot.mjs emulator/dist/midway/fbneo.mjs \
  /path/to/nbajam.zip emulator/dist/nbajam.state 2
node tools/art/capture_screen.mjs emulator/dist/midway/fbneo.mjs \
  /path/to/nbajam.zip art/references/nbajam/screen.png \
  emulator/dist/nbajam.state --confirm=12 --action=2 --frames=600
node emulator/rollback-check.mjs emulator/dist/midway/fbneo.mjs \
  /path/to/nbajam.zip emulator/dist/nbajam.state
PLAYERS=4 node emulator/netplay-check.mjs emulator/dist/midway/fbneo.mjs \
  /path/to/nbajam.zip emulator/dist/nbajam.state
```

The **two-coin** startup gets one quarter ready to play; the generic nine-coin
startup instead leaves this revision asking to use remaining credits during team
selection. Players can insert more coins with the usual controls. `--action=2`
uses the second reported action rather than Turbo to confirm selection; repeated
confirms progress through initials/birthday/team menus. Update the reference hash
if recapturing. Serving the ROM/state from R2, placing a cabinet and deploying
remain separate permissioned steps.

### The King of Fighters '98 and Ultimate Mortal Kombat 3

```sh
for skin in kof98 umk3; do
  python3 tools/art/cabinet_pipeline.py validate "$skin"
  python3 tools/art/cabinet_pipeline.py build "$skin"
done
python3 tools/art/preview_catalog.py kof98 umk3 \
  --filename=kof98-umk3.png --title="KOF '98 + ULTIMATE MORTAL KOMBAT 3"
node --test emulator/action-buttons.test.mjs
```

Both games use the existing browser-local emulators, two simultaneous seats and
unchanged join/watch/input flow. No map placement or ROM upload is included.
Four views and seven editable layers per view are exported for each skin;
individual previews are `art/previews/{kof98,umk3}.png` and the separate batch
preview is `art/previews/kof98-umk3.png` (earlier catalog previews stay intact).

- **KOF '98**: **40×56** red Neo Geo MVS interpretation, not an invented
  factory-dedicated cabinet. The English *The Slugfest* mini-marquee occupies one
  of four bays; the others are blank. Generic Neo Geo/MVS/SNK sides, black trim,
  photographed SNK-striped deck, two black sticks and four red/yellow/green/blue
  A/B/C/D actions per player follow the documented hardware. Start buttons remain
  photographed. Uses `neogeo` core and the already supplied `neogeo` BIOS;
  **all 16 ROMs** match pinned FBNeo by CRC/size.
- **UMK3**: **32×48** documented MK3-family upgrade cabinet, with Ultimate's
  purple-skeleton marquee above retained MK3 character sides and blue operator
  deck/fascia. Black body, red molding/sticks and six MK-layout actions per player:
  red punches, blue kicks, white Block, yellow Run. This is not a Street Fighter
  two-row layout or fabricated UMK3 skeleton side art. Start buttons remain
  photographed. Uses the existing `midway` core without BIOS; **all 26 required
  revision 1.2 ROMs** match. The missing security PIC is optional for FBNeo.

All artwork URLs/hashes/classifications, adaptations, supplied ZIP/state hashes
and remaining rights uncertainties are in the recipes. Archive hosting is not
publisher authorization; photo restoration/print authenticity remains unverified.
The MVS photo retains Beeblebrox's CC BY-SA 3.0 attribution. One photographed side
is repeated readably on the opposite side of each cabinet; unseen prints are not
invented. UMK3's photographed side retains glare/limited detail. Real captures show
Kyo versus Chris (**304×224**) and Kitana versus Reptile (**400×254**), never another
game's screen.

Reproducible startup/capture, using only supplied local files:

```sh
node emulator/snapshot.mjs emulator/dist/neogeo/fbneo.mjs \
  /path/to/kof98.zip emulator/dist/kof98.state 9 /path/to/neogeo.zip
node tools/art/capture_screen.mjs emulator/dist/neogeo/fbneo.mjs \
  /path/to/kof98.zip art/references/kof98/screen.png emulator/dist/kof98.state \
  --confirm=10 --frames=900 /path/to/neogeo.zip
node emulator/snapshot.mjs emulator/dist/midway/fbneo.mjs \
  /path/to/umk3.zip emulator/dist/umk3.state 9
node tools/art/capture_screen.mjs emulator/dist/midway/fbneo.mjs \
  /path/to/umk3.zip art/references/umk3/screen.png emulator/dist/umk3.state \
  --confirm=3 --frames=600
```

UMK3's initial CMOS warning requires **High Punch (RetroPad Y)**, not Low Punch
(B). `emulator/action-buttons.mjs` identifies actual action IDs, independent of
English labels; snapshot boot uses the first reported action and capture uses
that same order. The shared fix does not rewrite existing startup states.
Both new states contain nine credits and leave players to start/select normally.

Both games pass deterministic rollback and two-seat joins, departure and
spectators with zero mismatches. KOF '98 fits eight-frame rollbacks (7.4 ms p99
versus 16.9 ms budget in this Node check). UMK3's eight-frame stress test remains
in sync but exceeds budget (35.2 ms p99 versus 18.3 ms); the **existing** worker
benchmark automatically selects three rollback frames / three-frame input delay
on this machine, with 16.6 ms p99 for three-frame stress. Browser/hardware results
can vary; no streaming fallback or new emulator is introduced.

### Space Invaders, Asteroids, Strikers 1945 and Terminator 2

Four **32×48** facings and seven editable layers per facing are in the usual
asset/art folders, with recipes/references for `invaders`, `asteroid`, `s1945`
and `term2`. Review `art/previews/shooters-batch.png`; individual previews remain
separate. All marquees are rectified from archived cabinet photographs, not
misrepresented publisher masters. Exact URLs, download/reference hashes,
classifications, variant choices and rights uncertainties are in each recipe.

- **Space Invaders**: blue Taito upright, alien/moonscape sides/front,
  integrated title/bezel, brown edging and one white shared two-way joystick.
  Yellow Fire plus two red Start buttons follow this photograph; the separately
  researched red/yellow Taito America conversion deck is not mixed in. The
  `flat-upright` shell keeps the tall glass/display near the cabinet front.
  **Four supplied program ROMs** match; two alternating seats use `classics`.
  **Audio is missing:** FBNeo requires separate `invaders` samples `1`–`9.wav`
  and `18.wav`, absent from the supplied program ZIP and local files. No samples
  were downloaded. A supplied sample pack plus frontend sample mounting will
  be needed to enable sound; the current game is silent.
- **Asteroids**: black Atari upright, spaceship/orange-blue planet sides,
  original-design title and blue/red panel, **no joystick**. Five shared white
  actions are Rotate Left/Right, Hyperspace, Thrust and Fire; two metal/LED Start
  buttons stay photographed. **Five supplied ROMs** match, two alternating seats,
  `classics`. The real 640×480 vector capture receives a 3×3 stroke maximum
  filter then 4× brightness only in the tiny letterboxed sprite texture; its
  reference is untouched. Descriptor-driven routing maps seat-two Start to
  FBNeo's player-one R3 **2P Start** and lets both seats use the shared coin chute.
  `emulator/asteroid-controls-check.mjs` verifies seat-two Start matches native
  two-player selection (not one-player), plus shared coins and rotation in game RAM.
- **Strikers 1945**: black JAMMA conversion, plane/title marquee, portrait screen
  and gray Fabtek panel, without fabricated aircraft sides. The gray deck is an
  explicitly selected operator alternative to the photographed red deck, not a
  claimed factory-dedicated cabinet. Two black sticks and duplicated blue/green
  Shoot/Bomb pairs follow the panel; white Start buttons stay photographed.
  **Ten supplied ROMs** match; the absent MCU is undumped and simulated by FBNeo.
  Two simultaneous seats run the new `psikyo` core, without BIOS. Its low-resolution
  cabinet/marquee photo limits fine print detail.
- **Terminator 2: Judgment Day**: Midway blue Arnold/T2 sides, white title,
  red molding and **two fixed-base machine guns**, separately modeled receiver,
  barrel, grip and mount. Trigger/grenade details belong to the guns, not a
  four-button joystick panel. Start stays photographed. The LA3-indexed panel
  artwork is common to the hardware; the game remains supplied **LA4** with all
  **17 required ROMs** matching. Two simultaneous seats use existing `midway`.
  Arrow keys/touch directions aim via FBNeo's digital fallback; the two actions
  fire/throw grenades. Mouse aiming is not added. Independent guns and saved aim
  replay have been verified by `emulator/term2-controls-check.mjs`.

The four real screen captures are respectively 224×260, 640×480, 224×320 and
400×255. Capture accepts the one-row TMS34010 geometry correction, not arbitrary
sizes. One photographed side is repeated readably when an unseen print is
unverified; the plain Strikers sides stay plain. No other game's display is used.

```sh
./emulator/build.sh classics psikyo
for skin in invaders asteroid s1945 term2; do
  python3 tools/art/cabinet_pipeline.py build "$skin"
done
python3 tools/art/preview_catalog.py invaders asteroid s1945 term2 \
  --filename=shooters-batch.png \
  --title='SPACE INVADERS / ASTEROIDS / STRIKERS 1945 / TERMINATOR 2'
node --test emulator/action-buttons.test.mjs emulator/controller-routing.test.mjs
node emulator/asteroid-controls-check.mjs emulator/dist/classics/fbneo.mjs \
  /path/to/asteroid.zip emulator/dist/asteroid.state
node emulator/term2-controls-check.mjs emulator/dist/midway/fbneo.mjs \
  /path/to/term2.zip emulator/dist/term2.state
```

Each ignored startup state was made with `snapshot.mjs` and nine coins. To
recapture, use `capture_screen.mjs <core> <zip> <screen.png> <state>` with
`--frames=300` for Invaders/Asteroids, `--confirm --frames=600` for Strikers and
`--confirm --frames=2000` for T2. `TURNS=1` enables the shared-panel routing in
both rollback/netplay checks for Invaders/Asteroids. Four two-seat checks pass
joins, departure and spectators without final errors or mismatches. Asteroids'
watcher logged temporary missing initial frames while loading, then recovered
with the periodic full state; this is not a claim of flawless browser startup.
Eight-frame rollback stays in sync for all four (p99 1.4 / 5.7 / 11.2 / 15.3 ms
against 16.7 / 16.7 / 16.9 / 18.3 ms budgets on this Node host). Browser results
still require actual served-core/ROM testing after permissioned upload.

**Core/state compatibility:** the rebuilt classics core includes the existing
replay fixes, adding 20 bytes to earlier Pac-Man/Tetris/Wonder Boy state layouts.
Their ignored local startup states were regenerated for this build; earlier
states and the earlier core are backed up in `/tmp/vab-shooters/`. All three
refreshed states load/run and pass rollback/two-seat/spectator regression checks;
their tracked art remains unchanged. When serving
the rebuilt classics core, upload those three matching states too, together with
the four new ROMs/states and the Psikyo core. Uploads require separate permission;
no ROM/core/state has been uploaded, committed or deployed by this batch, and no
cabinet has been placed on the user's map.

### Out Run and Cruis'n USA — driving cabinet art

```sh
for skin in outrun crusnusa; do
  python3 tools/art/cabinet_pipeline.py validate "$skin"
  python3 tools/art/cabinet_pipeline.py build "$skin"
done
python3 tools/art/preview_catalog.py outrun crusnusa \
  --filename=outrun-crusnusa.png --title="OUT RUN + CRUIS'N USA"
python3 -m unittest discover -s tools/art -p 'test_*.py'
```

Review `art/previews/outrun-crusnusa.png` or the individual previews. Each skin
has four **48×56** sprites and seven editable layers per facing. The reusable
`racing` renderer now supports full-height artwork, taller/ribbed bucket seats
and a photographed deluxe car shell; approved Daytona exports remain unchanged.
Footprints are compressed to 70% around the established cell anchor, with height
unchanged. Geometry is a pixel-scale approximation, not factory dimensions.

- **Out Run**: 1986 Sega **Deluxe Moving Cabinet**, explicitly distinguished from
  the standard moving seat/upright by Sega's printed flyer. Red car body, two
  black tires/light hubs, rear spoiler/logo, four exhausts, seat speakers, gray
  motion base and separate coin pedestal are modeled. One yellow Start,
  three-spoke steering rim, Low/High lever and two pedals are separate controls;
  no Daytona view buttons or four-speed gate. Motion is represented by a stationary
  platform. Hood, outer side, seat-back and spoiler art are photograph extractions.
  The display is a real **320×224** attract racing capture from the already supplied
  local ROM and the parallel integration's local FBNeo Out Run core:
  `node tools/art/capture_screen.mjs emulator/dist/outrun/fbneo.mjs /path/to/outrun.zip art/references/outrun/screen.png - --frames=1200`.
  No startup state or ROM repacking is required; source/core/ROM hashes are recorded.
- **Cruis'n USA**: original-design 1994 Midway **single sit-down**, full-height
  purple/sunset/red-car sides, independent title marquee, black tall bucket,
  black dashboard/platform. Three red/white/blue view buttons, green Start and
  orange Radio follow the photographs, with a steering wheel, four-speed lever
  and two pedals. The chrome control photo corroborates the layout but its finish
  is not mixed into the selected black panel. Compact twin/shared leader marquee
  and upright remain research variants. Only the upright photo's actual same-game
  racing screen is rectified for the display; it is not a claimed emulator capture.
  The supplied ZIP contains revision 4.1, reflected by recipe `rom: crusnusa41`;
  the skin stays `crusnusa`. The parent-ROM capture attempt failed validation, so
  this art change does not repack/repair/download ROMs to obtain a screen.

References: [Out Run deluxe](https://www.arcadeartwork.org/picture.php?/96513),
[dashboard](https://www.arcadeartwork.org/picture.php?/5479),
[1986 flyer](https://www.arcadeartwork.org/picture.php?/87272),
[Cruis'n USA single](https://www.arcadeartwork.org/picture.php?/93302),
[controls](https://www.arcadeartwork.org/picture.php?/4549) and
[same-game screen photo](https://www.arcadeartwork.org/picture.php?/93304).
Original downloads, SHA-256s, measured extractions and uncertainties are retained
under `art/references/{outrun,crusnusa}/` and `tools/art/recipes/`. One photographed
outer side is repeated readably; unseen artwork is not invented. The Cruis'n USA
seat has no fabricated rear decal. Sega/Midway/Nintendo art and photo rights
remain unresolved; archive hosting is not publisher authorization.

The game integration's entries in `assets/games.ron` now select these skins:
`outrun` uses `cabinets: ["outrun"]`; `crusnusa41` uses `cabinets: ["crusnusa"]`.
The editor assigns their games automatically; existing plain-cabinet placements
remain valid and are not replaced.
No existing ROM/core/seat configuration, map, emulator code or binaries are
changed by this art work. Leave floor clearance when placing later. No ROM
upload, commit or deployment is included; visual approval is still needed.

### Daytona USA — original Sega sit-down

```sh
python3 tools/art/cabinet_pipeline.py validate daytona
python3 tools/art/cabinet_pipeline.py build daytona
python3 -m unittest discover -s tools/art -p 'test_*.py'
```

Review `art/previews/daytona.png`: four **48×56** sprites and seven editable layers
per facing. The reusable `racing` renderer models a CRT tower, low red-edged
platform, black bucket seat, hollow steering rim/silver three-spoke assembly,
right-hand gated shifter, two pedals and five dashboard buttons (yellow Start,
green/yellow/blue/red views). The catalog retains the existing Daytona core,
eight linked seats and link options; `control_stations: 1` describes the art,
not the number of online players. No map placement, ROM upload or deployment.

Selected variant is **one station adapted from the original red/yellow twin**,
not a claimed factory single-seat cabinet or Daytona 2/deluxe motion hardware.
The shared photographed marquee is reduced above one tower. One documented
outer side print is repeated readably; the unseen opposite print is unverified.
Seat-back numbering/flag lettering comes from the same cabinet photo. The long
footprint is compressed to 60% around the existing cell center, height unchanged,
so all views fit the existing bottom-center anchor. Leave room around it when
placing. Geometry/curvature and rear service details are pixel-scale estimates.

References: [archive search](https://www.arcadeartwork.org/qsearch.php?q=daytona),
[original-design twin photo](https://www.arcadeartwork.org/galleries/arcadecabs168/daytona.png),
[dashboard photo](https://www.arcadeartwork.org/galleries/arcadecpanels/daytona.png),
[Sega/AM2 flyer](https://www.arcadeartwork.org/galleries/arcadeflyers168/daytona.png)
and [1994 instruction strip](https://www.arcadeartwork.org/galleries/arcartmore168/daytona_instructions.png).
The sharp instruction file may be restored/recreated and is research-only.
Photo authors/restoration status and Sega artwork redistribution rights are
unresolved; archive hosting is not publisher authorization. Original decoded
files, measured corners, SHA-256s and adaptations are in
`art/references/daytona/` and `tools/art/recipes/daytona.json`.

The screen is a real **496×384 Daytona Revision A Hornet/oval frame** from the
already supplied local ROM and existing Daytona web core, not another game's
screen. The original ZIP uses ten legacy numeric suffixes; only a temporary
copy was repacked with the required `.icNN` names, with unchanged ROM contents.
The supplied ZIP was not modified. Exact rename list and both ZIP hashes are
in the recipe. Reproduce against that prepared local directory:

```sh
ROMS=/path/to/prepared-local-roms node daytona/harness/serve.mjs
node daytona/harness/run.mjs '?rom=daytona&cabinets=1&warmup=600&play&frames=600' \
  art/references/daytona/screen.png --canvas
```

Update the source hash after recapturing. No ROM was fetched or stored in tracked
art/assets; online integration and deployment are separate from this cabinet.

### Sega Rally Championship — original 1995 twin station

```sh
python3 tools/art/cabinet_pipeline.py validate srallyc
python3 tools/art/cabinet_pipeline.py build srallyc
python3 -m unittest discover -s tools/art -p 'test_*.py'
```

Review `art/previews/srallyc.png`: four **48×56** sprites in
`assets/tiles/objects/cabinet_srallyc_<facing>.png` and seven editable layers per
facing in `art/objects/cabinet_srallyc_<facing>/`. The shared `racing` renderer
supports optional shell/platform colors and a Race Leader header; existing
Daytona output remains unchanged. One station is adapted from the original
**white/blue 1995 twin**, with orange/blue shared marquee, striped white seat back,
black cushion/dashboard, blue platform edges, wheel, four-speed shifter, two
pedals and **two** raised buttons: yellow Start and red View Change. The long
footprint uses the same 60% compression and bottom-center anchor as Daytona.

References: [original twin photo](https://www.arcadeartwork.org/galleries/arcadecabs168/srallyc.png)
and [control photo](https://www.arcadeartwork.org/galleries/arcadecpanels/srallyc.png),
located through [archive search](https://www.arcadeartwork.org/qsearch.php?q=srally).
The archive's `srallycb` cabinet and `srallyc` flyer actually depict **Sega Rally 2**;
these are explicitly excluded research references, not used for textures or
hardware claims. Decoded files, URLs, SHA-256s, measured corners and remaining
uncertainties are in `art/references/srallyc/` and
`tools/art/recipes/srallyc.json`. No authenticated original operator decal scan
was located. Photo authors/restoration status and Sega redistribution rights are
unknown; archive hosting is not publisher authorization. Tower sides and unseen
seat sides stay plain rather than receiving invented decals.

The display is the original game's **photographed Championship Top 10 table**,
not a fresh gameplay capture or another game's screen. Replace its texture with
a real local capture when the in-progress Sega Rally core is ready. This art task
does not run or modify that integration. `players: 4` records the original game's
linked capability; `control_stations: 1` describes the art only. No Sega Rally
catalog entry existed when these assets were made: the eventual game entry should
select `cabinets: ["srallyc"]`, independently of its ROM/core/network setup. No
catalog/map edits, ROM downloads/uploads, commit or deployment are included.

### Virtua Striker 2 '98 — Sega sports upright

```sh
python3 tools/art/cabinet_pipeline.py validate vs298
python3 tools/art/cabinet_pipeline.py build vs298
python3 -m unittest discover -s tools/art -p 'test_*.py'
```

Four **40×56** sprites and seven editable layers per facing use the reusable
`upright` renderer's `sega-sports` shell. Review `art/previews/vs298.png`.
The existing `vs298` catalog entry now selects this skin; its Supermodel core,
two seats and lockstep mode are unchanged. No map placement, upload or deployment.

Selected variant: the white Sega **Virtua Striker 2 sports upright running the
'98 update**, retaining the base-game stadium/player side print and marquee.
White stepped sides/molding, a broad black monitor hood/bezel, teal overhanging
deck, white fascia, gray front recesses and dark foot follow the photos. Two green
sticks and six raised green/blue/red actions are modeled; two yellow Start buttons
remain in the photographed deck. The alternative camouflaged-green panel indexed
as `vs298` is retained as research only, not mixed into this teal-deck variant.

Sources are archived from [Arcade Artwork's vs2 search](https://www.arcadeartwork.org/qsearch.php?q=vs2):
[front cabinet](https://www.arcadeartwork.org/galleries/arcadecabs168/vs298.png),
[three-quarter side](https://www.arcadeartwork.org/galleries/arcadecabs168/vs2.png),
[alternative panel](https://www.arcadeartwork.org/galleries/arcadecpanels/vs298.png)
and [1997 Sega flyer](https://www.arcadeartwork.org/galleries/arcadeflyers168/vs2.png).
The flyer corroborates Model 3 and joystick/three-button controls; it is not a
'98-specific flyer or publisher-hosted master. Photos' authors/restoration status
are unknown, and Sega artwork/photograph redistribution rights are unresolved.
Archive hosting is not authorization. Exact region/model is unverified; one
low-resolution side photo supports the print repeated readably on both sides,
not invented opposite-side art or a fabricated dedicated '98 decal kit. Measured
image corners, polygon masking, dimensions and SHA-256 hashes are in
`tools/art/recipes/vs298.json`; original downloads are retained without resizing.

The screen is a real **496×384 '98 field/players attract frame**, not another
game or a synthetic soccer image. Capture uses only the supplied local ROM:

```sh
./supermodel/build.sh web
node supermodel/harness/serve.mjs  # local server, default ROMS=~/Downloads
# In another terminal, using our own headless Chrome:
node supermodel/harness/run.mjs '?rom=vs298&warmup=600&play&frames=1800' \
  art/references/vs298/screen.png --canvas
```

`--canvas` exports only the native canvas; ordinary harness screenshots retain
their existing full-page behavior. Update the recipe hash after recapturing.
Screen filtering preserves aspect with `fit: contain`. No ROM is stored in art,
committed or downloaded by this workflow.

### Tekken 3 and Time Crisis II — Namco cabinets

```sh
for skin in tekken3 timecrs2; do
  python3 tools/art/cabinet_pipeline.py validate "$skin"
  python3 tools/art/cabinet_pipeline.py build "$skin"
done
python3 tools/art/preview_catalog.py tekken3 timecrs2 \
  --filename=tekken3-timecrs2.png --title='TEKKEN 3 + TIME CRISIS II'
python3 -m unittest discover -s tools/art -p 'test_*.py'
```

Review `art/previews/tekken3-timecrs2.png` or the individual previews. Each
skin has four sprites and seven editable layers per facing. `assets/games.ron`
selects them without changing ROM, MAME core, seats, lockstep or linked/gun flags.
No map placement, local ROM capture, ROM download/upload, commit or deployment.

- **Tekken 3**: **32×48** gray Namco operator conversion, orange/black title,
  brown photographed deck, plain dark-gray sides and gray coin-door front.
  Two black/red sticks and four punch/kick actions each: the photographed left
  blue/red punches, right red punches and yellow kicks are modeled. White Start
  buttons remain photographed. This is not a claimed factory-dedicated Jin cabinet;
  the separate lightning-overlay panel and Jin flyer are research only. The
  original-design Namco instruction strip supplies narrow upper/lower bezel bands.
  Shared upright geometry approximates the shell. System 12 identification is
  corroborated by [MAME's hardware list](https://github.com/mamedev/mame/blob/master/src/mame/namco/namcos12.cpp).
- **Time Crisis II**: **64×64** red/blue CRT twin with one shared yellow title,
  two independent monitor bays, white edging, a central coin/instruction tower,
  cyan/pink holstered pistols, simplified cords and two metal-tread floor pedals.
  The new reusable `twin-gun` renderer shares the existing raycaster/export and
  texture loader; it does not turn free feedback pistols into T2 mounted guns.
  Only the separate control photograph's center plaque is extracted, since its
  pistol colors are swapped relative to the selected cabinet. The Namco Europe
  flyer corroborates System 23, twin screens, feedback guns and pedals. Floor depth
  is compressed 0.55× and width 0.9× around the cell center to avoid clipping the
  pedals with the established anchor; proportions and tread are sprite adaptations,
  not dimensionally exact. Leave neighboring floor space when placing it later.

The screens are **actual game/attract displays rectified from the selected cabinet
photos**, not local emulator captures or another game's imagery. Both preserve
aspect ratio and receive modest brightness boosts at sprite scale; reflections and
fine detail remain uncertain. Time Crisis II repeats the one photographed blue
side readably on both sides, not an invented unseen opposite print. The alternative
projection twin and flyer's red side/black guns are not silently mixed in.

Sources: [Tekken cabinet](https://www.arcadeartwork.org/picture.php?/98203),
[instruction strip](https://www.arcadeartwork.org/picture.php?/100922),
[Time Crisis II cabinet](https://www.arcadeartwork.org/picture.php?/98329),
[control plaque](https://www.arcadeartwork.org/picture.php?/6129) and
[Namco Europe flyer](https://www.arcadeartwork.org/picture.php?/88485).
Original downloads, measured corners, source purposes, hashes and unused research
variants are retained under `art/references/{tekken3,timecrs2}/` and their recipes.
Namco/licensor artwork and photographs retain their underlying rights; archive
hosting is not publisher authorization. Scan/photograph authors, replacement-print
status and redistribution permissions remain unknown. Visual approval and rights
review are still needed before deployment.

### Virtua Tennis — original white NAOMI Universal

```sh
python3 tools/art/cabinet_pipeline.py validate vtennis
python3 tools/art/cabinet_pipeline.py build vtennis
python3 -m unittest discover -s tools/art -p 'test_*.py'
```

Review `art/previews/vtennis.png`: four **40×60** sprites and seven editable
layers per facing. The reusable `naomi` renderer shares projection, lighting,
control geometry and export with the existing pipeline. It models the standing
white frame, plain dark rails, deep CRT pod, open knee space, low service/cash
base, projecting white deck, orange lamp and separate green-title billboard.
Two green/pink ball-top sticks and six raised physical actions follow the flyer;
the game uses only Shot/Lob. Start remains in the photographed panel.

Selected variant is the original **1999 Virtua Tennis / Sega Professional Tennis**
in the white standing NAOMI Universal pictured in Sega Amusements Europe's
[operator flyer](https://www.arcadeartwork.org/picture.php?/88783).
The title, deck and **actual photographed tennis display** are rectified from that
scan; this is not a local capture or another game's display. The scan limits
fine detail; screen aspect is preserved with a 1.15× sprite-only brightness gain.
The flyer's yellow-looking space below the deck is its background through the
open frame, not yellow cabinet artwork. Proportions, lower doors and rear vents
are small-scale geometry approximations. No unseen tennis side print is invented.

[Arcade Otaku's NAOMI Universal documentation/photo](https://wiki.arcadeotaku.com/w/Sega_Naomi_Universal)
corroborates the shell only: its different game's panel/loading display and
NAOMI side lettering are not extracted. The separately archived
[red conversion pedestal](https://www.arcadeartwork.org/picture.php?/98807) and
[blue conversion deck](https://www.arcadeartwork.org/picture.php?/6300) are
research-only alternatives, not mixed into the selected white machine.
Decoded originals, measured corners, SHA-256s, classifications and uncertainties
are in `art/references/vtennis/` and `tools/art/recipes/vtennis.json`.
Sega/photograph rights remain unresolved; archive/wiki hosting is not permission.

The existing `vtennisg` GD-ROM entry in `assets/games.ron` selects `vtennis`.
Its Flycast core, disc path, player count and lockstep settings are unchanged.
The original game's flyer guides the cabinet, not a claimed GD-ROM-specific
art kit. No emulator work, ROM download/capture/upload, map placement, commit
or deployment is included.

### Metal Slug: online-source trial

`mslug` uses a **40×56**, two-player Neo Geo MVS big-red shell with red sides,
black molding and original Neo Geo/MVS/SNK logo strokes extracted from a cabinet
photo. A scan of the original Metal Slug mini-marquee fills the first of four
marquee bays; the others remain blank, not advertisements for unrelated games.
The SNK-striped panel has red/yellow/green/blue A/B/C/D buttons even though Metal
Slug uses only three actions. The display is captured from the user's existing
local game and pinned Neo Geo core, not another game's photo or synthetic art.

This is an MVS cabinet interpretation, **not** a unique factory-dedicated Metal
Slug machine. The photographed red cabinet has a custom deck, deliberately not
reused; a separate SNK panel photograph guides the original-style deck. One side
photo supports the generic branding repeated on the opposite side. Preview:
`art/previews/mslug.png`. Sources: `art/references/mslug/` and the recipe JSON.

The mini-marquee is a third-party archive scan carrying 1996 NAZCA/SNK credits,
not a publisher-hosted master or permission to redistribute. Panel photo rights
are unknown. The MVS cabinet photograph is by Beeblebrox under
[CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/), from
[Wikimedia Commons](https://commons.wikimedia.org/wiki/File:Neo_Geo_full_on.png);
logo extractions/render adaptations acknowledge that license, with underlying
printed artwork/trademark rights retained. No ROMs are committed. Review rights
and the chosen variant before deployment.

This trial exercises one online-researched cabinet plus an approved-art
regression; it is not an independent with/without-skill quality benchmark.

`build_mk2.py` makes an alternative cabinet from the three supplied photos:
left and right side prints, the left photo's gameplay display, and the straight-on
photo's marquee, deck, control-panel fascia, and lower front. A shared solid model
keeps all four facings consistent. Red T-molding, the recessed monitor, raised
controls, coin door, and rear service/vent panels are modeled separately.

```sh
python3 -m pip install Pillow  # if not already installed
python3 tools/art/build_mk2.py
python3 -m unittest discover -s tools/art -p 'test_*.py'
```

Outputs:

- `assets/tiles/objects/cabinet_mk2_v2_<facing>.png`: transparent 32×48 game sprites.
- `art/objects/cabinet_mk2_v2_<facing>/`: seven editable PNG layers and `layers.ron`.
- `art/previews/mk2-v2.png`: four views, native sizes, and the old cabinet at the same zoom.

The editor offers only this v2 design, labeled `Mortal Kombat II`; the internal skin remains
`mk2_v2` and the ROM remains `mk2`. Legacy `cabinet_mk2_*` assets are retained for
existing maps but hidden from the cabinet palette. No map is replaced. The PNG
layers can be edited in Draw mode; rebuilding replaces only the v2 layers and
sprites. This custom geometry is generated by the Python script, not
the editor's existing **Make views** skin renderer.

The photos and comparison-only copies of the previous sprites are retained in
`art/references/mk2/` for reproducible builds, including in a fresh checkout.
Details such as title lettering are impressions at this tiny native size, not
legible text.

## Snow Bros.

`python3 tools/art/build_snowbros.py` builds `cabinet_snowbros_<facing>.png` in
`assets/tiles/objects/`, matching editable layers in `art/objects/`, and the
four-view preview `art/previews/snowbros.png`. The decal sheet supplies distinct
blue/red snowmen, cyan side panels, yellow marquee, bezel, and control artwork;
the assembled-machine photo supplies the monitor image and guides the white
molding, black front, reclined display, and cyan joysticks. Both references are
kept in `art/references/snowbros/`.

Select `cabinet_snowbros_*` in the editor palette and set **Cabinet game** to
`snowbrosb` (the existing ROM-set name). No map or existing cabinet is replaced.
The generator shares projection and export helpers with `build_mk2.py`, while
keeping its own paint and two-button-per-player model. Rebuilding overwrites only
this cabinet's generated layers and sprites; use Draw mode for manual touch-ups.

## Sunset Riders — four-player cabinet

`python3 tools/art/build_sunsetriders.py` builds four **48×56** sprites named
`cabinet_sunsetriders_<facing>.png`, editable layers in `art/objects/`, and
`art/previews/sunsetriders.png`. Select `cabinet_sunsetriders_*` in the editor
palette and set **Cabinet game** to `ssriders`.

The larger model has a 16-unit-wide body (versus 10 for the small cabinets), a
20-unit-wide projecting deck (twice the small deck's width), a wider monitor and
marquee, four color-coded joysticks, eight buttons, and four coin-slot lights.
White molding follows the supplied cabinet photo; the new decal sheet provides
the sunset sky, riders, cliffs, marquee, bezel, and deck/front artwork. References
are retained in `art/references/sunsetriders/`. The screen is a hand-drawn western
placeholder, not captured Sunset Riders gameplay or the reference's Snow Bros.
display.

The sprite keeps the existing bottom-center cell anchor. The body fits a cell,
while the wider deck visibly overhangs it: leave room beside it when placing it.
This changes cabinet art only, not emulator input handling or multiplayer support.
Shared rendering/export helpers support the larger canvas without changing the
MKII or Snow Bros. sprites. No map is replaced.

## Cadillacs and Dinosaurs — three players

`python3 tools/art/build_dino.py` builds four **40×52** sprites named
`cabinet_dino_<facing>.png`, matching editable layers, and `art/previews/dino.png`.
The supplied decal sheet provides yellow T-rex/car sides, marquee, green bezel,
control deck and lower-front artwork. The assembled-machine photo supplies the
monitor and guides the black trim, front and three red/blue two-button stations.
References are retained in `art/references/dino/`.

The editor automatically assigns this skin `dinou` from `assets/games.ron`, using
the existing Capcom core and three seats. A split `dinou.zip` needs the shared
files from its parent `dino` set: combine them into a standalone `dinou.zip`
without replacing the USA program ROMs. All files must match pinned FBNeo.

## The Simpsons — four players

`python3 tools/art/build_simpsons.py` builds four **48×56** sprites named
`cabinet_simpsons_<facing>.png`, editable layers, and `art/previews/simpsons.png`.
The first photo supplies the cyan marquee, character deck and monitor; its
red/blue/green/yellow controls are modeled as four two-button stations. The clean
side decal sheet supplies both cyan family-stack prints and the title lettering;
the side photos guide yellow T-molding on the cabinet and deck. Cut-out margins
continue the cyan background instead of importing white page margins. Black front
panels retain two paired coin doors. All references are in `art/references/simpsons/`.

The editor assigns `simpsons` automatically, using the existing Konami core and
four seats. Both new cabinets keep the bottom-center cell anchor; leave room for
the overhanging decks. Neither changes the map. Rebuilding overwrites only each
cabinet's own generated sprites and layers; use Draw mode for manual touch-ups.
