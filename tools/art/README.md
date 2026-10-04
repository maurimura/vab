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
brightness adjustment, patches and photographed white-logo ink extraction. A reviewed renderer
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

Select `cabinet_mk2_v2_*` in the editor's object palette to try it. These are an
independent alternative: the existing assets and map are not replaced. The PNG
layers can be edited in Draw mode; rebuilding replaces only this alternative's
layers and sprites. This custom geometry is generated by the Python script, not
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
