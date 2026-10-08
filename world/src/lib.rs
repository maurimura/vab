//! The bar's map: what is placed on the isometric grid and how it is drawn. Shared by the
//! editor (tools/editor), the game client and, as the map format alone (no `bevy` feature),
//! the Worker that stores the map.

use std::collections::BTreeMap;
use std::sync::OnceLock;

#[cfg(feature = "bevy")]
use bevy::asset::{AssetLoader, AsyncReadExt, LoadContext, io::Reader};
#[cfg(feature = "bevy")]
use bevy::prelude::*;
#[cfg(feature = "bevy")]
use bevy::sprite::Anchor;
use glam::{IVec2, Vec2};
use serde::{Deserialize, Serialize};

/// Cells are 2:1 isometric diamonds.
pub const TILE_WIDTH: f32 = 32.0;
pub const TILE_HEIGHT: f32 = 16.0;

/// Objects that cover more than one cell, built in like the game list.
const OBJECTS: &str = include_str!("../../assets/objects.ron");

/// Everything placed on the grid. Tile names are PNG paths under assets/tiles/ without the
/// extension, e.g. "floor/wood" or "objects/cabinet".
///
/// Also an asset: `asset_server.load::<Map>("maps/bar.ron")` with [`MapPlugin`] added.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "bevy", derive(Asset, TypePath))]
pub struct Map {
    pub floor: Vec<Placed>,
    pub objects: Vec<Placed>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Placed {
    pub x: i32,
    pub y: i32,
    pub tile: String,
    /// The ROM set a cabinet runs, e.g. "mk2".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
}

impl Placed {
    /// The cell it stands on; an object covering several extends along +x and +y from it.
    pub fn cell(&self) -> IVec2 {
        IVec2::new(self.x, self.y)
    }

    /// Every cell it covers: its own, and more along +x and +y for an object in
    /// assets/objects.ron.
    pub fn cells(&self) -> impl Iterator<Item = IVec2> + use<> {
        let (x, y, size) = (self.x, self.y, footprint(&self.tile));
        (0..size.x).flat_map(move |dx| (0..size.y).map(move |dy| IVec2::new(x + dx, y + dy)))
    }

    pub fn covers(&self, cell: IVec2) -> bool {
        let size = footprint(&self.tile);
        let offset = cell - IVec2::new(self.x, self.y);
        offset.cmpge(IVec2::ZERO).all() && offset.cmplt(size).all()
    }

    /// The middle of the area it covers, in world pixels.
    pub fn center(&self) -> Vec2 {
        let size = footprint(&self.tile);
        let front = IVec2::new(self.x, self.y) + size - IVec2::ONE;
        (cell_to_world(self.x, self.y) + cell_to_world(front.x, front.y)) / 2.0
    }
}

#[derive(Deserialize)]
struct Object {
    tile: String,
    size: (i32, i32),
}

/// How many cells a tile covers along +x and +y: one by one unless assets/objects.ron says
/// otherwise.
pub fn footprint(tile: &str) -> IVec2 {
    static OBJECT_SIZES: OnceLock<Vec<Object>> = OnceLock::new();
    OBJECT_SIZES
        .get_or_init(|| ron::from_str(OBJECTS).expect("assets/objects.ron is a valid list"))
        .iter()
        .find(|object| object.tile == tile)
        .map_or(IVec2::ONE, |object| {
            IVec2::new(object.size.0, object.size.1)
        })
}

/// A game a cabinet can run, from assets/games.ron.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Game {
    /// The ROM set, served at /roms/<rom>.zip.
    pub rom: String,
    /// The core that runs it: an FBNeo core, served at /fbneo/<core>/fbneo.mjs, or a core built
    /// on its own and served at /<core>/<core>.mjs: "supermodel" (Sega Model 3, supermodel/),
    /// "daytona" (Daytona USA's Sega Model 2, daytona/) or "mame" (MAME, mame/).
    pub core: String,
    pub title: String,
    /// A BIOS set loaded next to the ROM, e.g. "neogeo".
    #[serde(default)]
    pub bios: Option<String>,
    /// Players take turns on player 1's controls (Pac-Man, Wonder Boy), as on an upright
    /// cabinet, so online player 2 plays through them too.
    #[serde(default)]
    pub turns: bool,
    /// How many can play at once, each in their own seat (up to 4, or 8 for an `arcade` game).
    #[serde(default = "two")]
    pub players: u32,
    /// Online, the players' machines run in lockstep with a few frames of input delay instead
    /// of rolling back: for cores whose save state is too big to take every frame (Supermodel's
    /// is 32 MB).
    #[serde(default)]
    pub lockstep: bool,
    /// A lightgun game (Time Crisis II): the player aims with the mouse or a finger on the screen,
    /// which shows a crosshair, and the aim goes to the core with the buttons.
    #[serde(default)]
    pub gun: bool,
    /// Two players play on two linked boards, one each, as a twin cabinet (Time Crisis II): each
    /// browser runs its player's own board, and the boards talk over the game's own link
    /// (web/emulator/linked.js) instead of sharing one machine. Both start over when the second
    /// player sits down, the link being set at power-on.
    #[serde(default)]
    pub linked: bool,
    /// Linked cabinets, as in an arcade (Daytona USA): every player's browser runs only their
    /// own cabinet, from a state for their seat (/roms/<rom>.seat<n>.state), and the cabinets'
    /// link boards talk to each other through the players' browsers, so the game's own rules
    /// say who plays whom; no rollback, no lockstep, no handover (web/emulator/worker.js).
    #[serde(default)]
    pub arcade: bool,
    /// Settings for the core, made before the game loads (the `_<core>_set` its module
    /// exports; FBNeo has none), e.g. Daytona USA's one cabinet on a star link. With a `view`
    /// among them the machine has a screen per seat, and each player's shows their own; an
    /// `arcade` game's machine also gets `seat`, the player's (web/emulator/worker.js).
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    /// The cabinet skins drawn for it (tiles/objects/cabinet_<skin>_<facing>.png), which the
    /// editor gives this game. None yet: it runs on a plain cabinet the editor sets it on.
    #[serde(default)]
    pub cabinets: Vec<String>,
    /// A driving game steered with a wheel (Out Run, Cruis'n USA): the arrows turn it, a little
    /// for a tap and all the way for a hold, and the core reads it as its analog stick
    /// (web/emulator/wheel.js, web/emulator/libretro.js).
    #[serde(default)]
    pub wheel: Option<Wheel>,
}

fn two() -> u32 {
    2
}

/// How the arrows turn a driving game's wheel (web/emulator/wheel.js), and where the core reads
/// it on its analog stick. `/wheel` in the chat tries other ramps while playing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Wheel {
    /// Seconds from the middle to full lock, an arrow held.
    pub lock: f64,
    /// Seconds from full lock back to the middle, let go.
    pub back: f64,
    /// How the turn goes while held: after a fraction f of `lock`, the wheel is at f^curve of
    /// full lock. 1 turns evenly; 2 barely turns for a tap and speeds up toward the lock.
    pub curve: f64,
    /// Where on the core's analog stick (0 to 32767 each way) the game's wheel turns: the core
    /// ignores the stick up to the first and is at full lock from the second. A property of the
    /// core, not of the feel: Out Run's FBNeo core has dead zones, (10600, 23500); MAME reads
    /// all of it, the default. Every machine running the game reads the wheel through it, a
    /// watcher's too, so `/wheel` leaves it as it is here.
    #[serde(default = "whole_stick")]
    pub span: (u16, u16),
}

fn whole_stick() -> (u16, u16) {
    (0, 32767)
}

impl Wheel {
    /// The longest `lock` or `back` and the steepest `curve` `/wheel` takes.
    pub const MOST: f64 = 10.0;
    /// The gentlest `curve` it takes.
    pub const LEAST_CURVE: f64 = 0.1;

    /// The ramp `/wheel` asks for: `lock=0.5 back=0.1 curve=2`, any of them, each in place of
    /// this one's. An error says what's wrong with the first that won't do.
    pub fn tuned(self, settings: &str) -> Result<Wheel, String> {
        let mut wheel = self;
        for setting in settings.split_whitespace() {
            let Some((name, value)) = setting.split_once('=') else {
                return Err(format!("{setting}: name=number, e.g. lock=0.5"));
            };
            let (field, least) = match name {
                "lock" => (&mut wheel.lock, 0.0),
                "back" => (&mut wheel.back, 0.0),
                "curve" => (&mut wheel.curve, Self::LEAST_CURVE),
                _ => return Err(format!("{name}: lock, back or curve")),
            };
            match value.parse::<f64>() {
                Ok(value) if (least..=Self::MOST).contains(&value) => *field = value,
                _ => return Err(format!("{name}: a number from {least} to {}", Self::MOST)),
            }
        }
        Ok(wheel)
    }

    /// The ramp's numbers, as `/wheel` takes them.
    pub fn ramp(&self) -> String {
        format!("lock={} back={} curve={}", self.lock, self.back, self.curve)
    }

    /// The wheel as assets/games.ron has it, to paste there.
    pub fn to_ron(&self) -> String {
        let span = if self.span == whole_stick() {
            String::new()
        } else {
            format!(", span: ({}, {})", self.span.0, self.span.1)
        };
        format!(
            "wheel: Some((lock: {:?}, back: {:?}, curve: {:?}{span}))",
            self.lock, self.back, self.curve
        )
    }
}

pub fn games_from_ron(text: &str) -> Result<Vec<Game>, ron::error::SpannedError> {
    ron::from_str(text)
}

impl Map {
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .expect("a map always serializes")
    }
}

/// Loads `.ron` maps through the asset server: from assets/ on the desktop, over HTTP on the
/// web, where the Worker serves the map the web editor last saved.
#[cfg(feature = "bevy")]
pub struct MapPlugin;

#[cfg(feature = "bevy")]
impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Map>().init_asset_loader::<MapLoader>();
    }
}

#[cfg(feature = "bevy")]
#[derive(Default, TypePath)]
pub struct MapLoader;

#[cfg(feature = "bevy")]
impl AssetLoader for MapLoader {
    type Asset = Map;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Map, BevyError> {
        let mut text = String::new();
        reader.read_to_string(&mut text).await?;
        Ok(Map::from_ron(&text)?)
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}

/// Center of a cell's diamond. +x runs down-right on screen, +y down-left.
pub fn cell_to_world(x: i32, y: i32) -> Vec2 {
    Vec2::new(
        (x - y) as f32 * TILE_WIDTH / 2.0,
        -(x + y) as f32 * TILE_HEIGHT / 2.0,
    )
}

/// The cell whose diamond contains a world position.
pub fn world_to_cell(position: Vec2) -> IVec2 {
    let across = position.x / (TILE_WIDTH / 2.0);
    let down = -position.y / (TILE_HEIGHT / 2.0);
    IVec2::new(
        ((across + down) / 2.0).round() as i32,
        ((down - across) / 2.0).round() as i32,
    )
}

/// Marks entities drawn from a map.
#[cfg(feature = "bevy")]
#[derive(Component)]
pub struct MapSprite;

/// The sprite for a placed floor tile or object.
///
/// Floor tiles are centered on their cell. Objects stand on the cells they cover: the image's
/// bottom is the area's bottom point and its middle the area's, so one wider than the area
/// (a cabinet whose deck or canvas overhangs its cell) overhangs both sides alike, and objects
/// nearer the viewer (by their front cell) draw on top.
#[cfg(feature = "bevy")]
pub fn map_sprite(
    asset_server: &AssetServer,
    placed: &Placed,
    object: bool,
) -> (MapSprite, Sprite, Anchor, Transform) {
    let image = asset_server.load(format!("tiles/{}.png", placed.tile));
    let (anchor, position) = if object {
        let size = footprint(&placed.tile);
        let front = IVec2::new(placed.x, placed.y) + size - IVec2::ONE;
        let depth = (front.x + front.y) as f32 * 0.001;
        let left = cell_to_world(placed.x, front.y).x - TILE_WIDTH / 2.0;
        let middle = left + (size.x + size.y) as f32 * TILE_WIDTH / 4.0;
        let bottom = cell_to_world(front.x, front.y).y - TILE_HEIGHT / 2.0;
        (
            Anchor::BOTTOM_CENTER,
            Vec3::new(middle, bottom, 1.0 + depth),
        )
    } else {
        let depth = (placed.x + placed.y) as f32 * 0.001;
        (
            Anchor::CENTER,
            cell_to_world(placed.x, placed.y).extend(depth),
        )
    };
    (
        MapSprite,
        Sprite::from_image(image),
        anchor,
        Transform::from_translation(position),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_positions_map_back_to_their_cell() {
        for (x, y) in [(0, 0), (3, -2), (-5, 7)] {
            let center = cell_to_world(x, y);
            // Anywhere inside the diamond, a little short of its corners.
            for offset in [
                Vec2::ZERO,
                Vec2::new(14.0, 0.0),
                Vec2::new(-14.0, 0.0),
                Vec2::new(0.0, 7.0),
                Vec2::new(0.0, -7.0),
            ] {
                assert_eq!(world_to_cell(center + offset), IVec2::new(x, y));
            }
        }
    }

    #[test]
    fn the_game_catalog_parses() {
        let games = games_from_ron(include_str!("../../assets/games.ron")).unwrap();
        let mslug = games.iter().find(|g| g.rom == "mslug").unwrap();
        assert_eq!(mslug.bios.as_deref(), Some("neogeo"));
        assert_eq!(mslug.cabinets, ["mslug"]);
        assert_eq!(mslug.core, "neogeo");
        assert_eq!(mslug.players, 2);
        let mk2 = games.iter().find(|g| g.rom == "mk2").unwrap();
        assert_eq!(mk2.title, "Mortal Kombat II");
        assert_eq!(mk2.cabinets, ["mk2_v2"]);
        for (rom, core, players, skin, bios) in [
            ("dinou", "capcom", 3, "dino", None),
            ("simpsons", "konami", 4, "simpsons", None),
            ("nbajam", "midway", 4, "nbajam", None),
            ("umk3", "midway", 2, "umk3", None),
            ("kof98", "neogeo", 2, "kof98", Some("neogeo")),
        ] {
            let game = games.iter().find(|g| g.rom == rom).unwrap();
            assert_eq!(game.core, core);
            assert_eq!(game.players, players);
            assert_eq!(game.cabinets, [skin]);
            assert!(!game.turns);
            assert_eq!(game.bios.as_deref(), bios);
        }
        assert!(
            games
                .iter()
                .find(|g| g.rom == "mk2")
                .unwrap()
                .bios
                .is_none()
        );
        assert!(mk2.options.is_empty() && !mk2.arcade);
        let daytona = games.iter().find(|g| g.rom == "daytona").unwrap();
        assert_eq!(daytona.core, "daytona");
        assert_eq!(daytona.players, 8);
        assert!(daytona.arcade && !daytona.lockstep && !daytona.turns);
        assert_eq!(daytona.options["cabinets"], "1");
        assert_eq!(daytona.options["link_topology"], "star");
        assert_eq!(daytona.options["link_pace"], "1");
        // One cabinet a browser: no screen per seat, and the seat is the worker's to set.
        assert!(!daytona.options.contains_key("view") && !daytona.options.contains_key("seat"));
        // Only arcade games take more than 4.
        for game in &games {
            assert!(game.players >= 1 && game.players <= if game.arcade { 8 } else { 4 });
        }
    }

    #[test]
    fn shooter_catalog_preserves_hardware_and_sequential_seats() {
        let games = games_from_ron(include_str!("../../assets/games.ron")).unwrap();
        for (rom, core, title, turns) in [
            ("invaders", "classics", "Space Invaders", true),
            ("asteroid", "classics", "Asteroids", true),
            ("s1945", "psikyo", "Strikers 1945", false),
            ("term2", "midway", "Terminator 2: Judgment Day", false),
        ] {
            let game = games.iter().find(|g| g.rom == rom).unwrap();
            assert_eq!(game.core, core);
            assert_eq!(game.title, title);
            assert_eq!(game.players, 2);
            assert_eq!(game.turns, turns);
            assert_eq!(game.cabinets, [rom]);
            assert!(game.bios.is_none());
        }
    }

    /// Games whose cabinet skin isn't drawn yet: they stand in plain cabinets for now.
    const SKIN_TO_COME: [&str; 0] = [];

    #[test]
    fn out_run_is_one_player_on_its_own_core() {
        let games = games_from_ron(include_str!("../../assets/games.ron")).unwrap();
        let outrun = games.iter().find(|g| g.rom == "outrun").unwrap();
        assert_eq!(outrun.core, "outrun");
        assert_eq!(outrun.title, "Out Run");
        assert_eq!(outrun.cabinets, ["outrun"]);
        // One player; whoever presses E next watches.
        assert_eq!(outrun.players, 1);
        assert!(outrun.bios.is_none() && outrun.options.is_empty());
        assert!(
            !outrun.turns && !outrun.lockstep && !outrun.gun && !outrun.linked && !outrun.arcade
        );
        // The one-player games: Out Run and Cruis'n USA.
        assert_eq!(games.iter().filter(|g| g.players == 1).count(), 2);
        // Steered with a wheel the arrows turn evenly, full lock in 0.3 s and back as fast,
        // over the part of the stick FBNeo's dead zones leave.
        assert_eq!(
            outrun.wheel,
            Some(Wheel {
                lock: 0.3,
                back: 0.3,
                curve: 1.0,
                span: (10600, 23500)
            })
        );
    }

    #[test]
    fn the_driving_games_have_wheels() {
        let games = games_from_ron(include_str!("../../assets/games.ron")).unwrap();
        let steered: Vec<&str> = games
            .iter()
            .filter(|game| game.wheel.is_some())
            .map(|game| game.rom.as_str())
            .collect();
        assert_eq!(steered, ["outrun", "crusnusa41"]);
        for wheel in games.iter().filter_map(|game| game.wheel) {
            assert!(wheel.lock > 0.0 && wheel.back > 0.0 && wheel.curve > 0.0);
            assert!(wheel.span.0 < wheel.span.1 && wheel.span.1 <= 32767);
            // What `/wheel` prints for games.ron reads back as the same wheel.
            let line = format!(
                "(rom: \"x\", core: \"x\", title: \"x\", {})",
                wheel.to_ron()
            );
            let game: Game = ron::from_str(&line).unwrap();
            assert_eq!(game.wheel, Some(wheel));
        }
    }

    #[test]
    fn wheel_tunes_any_of_its_numbers() {
        let wheel = Wheel {
            lock: 0.6,
            back: 0.1,
            curve: 2.0,
            span: (0, 32767),
        };
        assert_eq!(wheel.tuned(""), Ok(wheel));
        let tuned = wheel.tuned("curve=1.5  lock=0.45").unwrap();
        assert_eq!((tuned.lock, tuned.back, tuned.curve), (0.45, 0.1, 1.5));
        assert_eq!(tuned.span, wheel.span);
        assert_eq!(tuned.ramp(), "lock=0.45 back=0.1 curve=1.5");
        assert_eq!(
            tuned.to_ron(),
            "wheel: Some((lock: 0.45, back: 0.1, curve: 1.5))"
        );
        assert_eq!(wheel.tuned("back=0").unwrap().back, 0.0);
        // Nothing changes when anything is wrong.
        for wrong in [
            "lock",
            "lock=",
            "lock=fast",
            "lock=-1",
            "lock=11",
            "curve=0",
            "lock=NaN",
            "speed=2",
        ] {
            assert!(wheel.tuned(wrong).is_err(), "{wrong}");
        }
        assert!(wheel.tuned("lock=0.5 curve=0").is_err());
    }

    #[test]
    fn mame_catalog_has_tekken_and_the_lightgun_game() {
        let games = games_from_ron(include_str!("../../assets/games.ron")).unwrap();
        let tekken = games.iter().find(|g| g.rom == "tekken3je1").unwrap();
        assert_eq!(tekken.core, "mame");
        assert_eq!(tekken.title, "Tekken 3");
        assert_eq!(tekken.players, 2);
        assert!(!tekken.gun && !tekken.linked && !tekken.turns);
        let crisis = games.iter().find(|g| g.rom == "timecrs2").unwrap();
        assert_eq!(crisis.core, "mame");
        assert_eq!(crisis.title, "Time Crisis II");
        // Two players, each at their own linked cabinet with a gun.
        assert_eq!(crisis.players, 2);
        assert!(crisis.gun && crisis.linked && !crisis.lockstep && !crisis.turns);
        // Only a lightgun game says so, and only the twin cabinet is linked.
        assert_eq!(games.iter().filter(|g| g.gun).count(), 1);
        assert_eq!(games.iter().filter(|g| g.linked).count(), 1);
    }

    #[test]
    fn cruisn_usa_is_one_player_on_mame() {
        let games = games_from_ron(include_str!("../../assets/games.ron")).unwrap();
        let cruisn = games.iter().find(|g| g.rom == "crusnusa41").unwrap();
        assert_eq!(cruisn.core, "mame");
        assert_eq!(cruisn.title, "Cruis'n USA");
        // One driver; the next to press E watches.
        assert_eq!(cruisn.players, 1);
        assert!(
            !cruisn.gun && !cruisn.linked && !cruisn.lockstep && !cruisn.turns && !cruisn.arcade
        );
        assert!(cruisn.bios.is_none() && cruisn.options.is_empty());
        // Steered with a wheel that a tap barely turns, full lock in 0.6 s held, back in 0.1 s,
        // over the part of the stick the game answers to (MAME reads all of it, the game not).
        assert_eq!(
            cruisn.wheel,
            Some(Wheel {
                lock: 0.6,
                back: 0.1,
                curve: 2.0,
                span: (5200, 28900)
            })
        );
        assert_eq!(cruisn.cabinets, ["crusnusa"]);
        // Existing plain-cabinet placements remain valid; art doesn't replace the map.
        let map = Map::from_ron(include_str!("../../assets/maps/bar.ron")).unwrap();
        assert!(map.objects.iter().any(|object| {
            object.tile == "objects/cabinet" && object.game.as_deref() == Some("crusnusa41")
        }));
    }

    #[test]
    fn catalog_cabinet_skins_have_all_four_views() {
        let games = games_from_ron(include_str!("../../assets/games.ron")).unwrap();
        let tiles =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tiles/objects");
        // Every current game has a skin; future gaps must be recorded explicitly.
        for game in games {
            let to_come = SKIN_TO_COME.contains(&game.rom.as_str());
            assert_eq!(
                game.cabinets.is_empty(),
                to_come,
                "{}: a skin, or SKIN_TO_COME, but not both",
                game.title
            );
            for skin in game.cabinets {
                for facing in ["down_right", "down_left", "up_left", "up_right"] {
                    assert!(
                        tiles.join(format!("cabinet_{skin}_{facing}.png")).is_file(),
                        "{skin} is missing {facing}"
                    );
                }
            }
        }
    }

    #[test]
    fn objects_cover_their_footprint() {
        let table = Placed {
            x: 3,
            y: -1,
            tile: "objects/pool_table".into(),
            game: None,
        };
        assert_eq!(footprint("objects/pool_table"), IVec2::new(2, 1));
        assert_eq!(
            table.cells().collect::<Vec<_>>(),
            [IVec2::new(3, -1), IVec2::new(4, -1)]
        );
        assert!(table.covers(IVec2::new(4, -1)));
        assert!(!table.covers(IVec2::new(3, 0)));
        let counter = Placed {
            tile: "objects/counter".into(),
            ..table
        };
        assert_eq!(counter.cells().collect::<Vec<_>>(), [IVec2::new(3, -1)]);
    }

    #[test]
    fn maps_round_trip_through_ron() {
        let map = Map {
            floor: vec![Placed {
                x: 1,
                y: 2,
                tile: "floor/wood".into(),
                game: None,
            }],
            objects: vec![Placed {
                x: 0,
                y: 0,
                tile: "objects/cabinet".into(),
                game: Some("mk2".into()),
            }],
        };
        let back = Map::from_ron(&map.to_ron()).unwrap();
        assert_eq!(back.floor, map.floor);
        assert_eq!(back.objects, map.objects);
    }
}
