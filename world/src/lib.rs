//! The bar's map: what is placed on the isometric grid and how it is drawn. Shared by the
//! editor (tools/editor), the game client and, as the map format alone (no `bevy` feature),
//! the Worker that stores the map.

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
    /// The FBNeo core that runs it, served at /fbneo/<core>/fbneo.mjs.
    pub core: String,
    pub title: String,
    /// A BIOS set loaded next to the ROM, e.g. "neogeo".
    #[serde(default)]
    pub bios: Option<String>,
    /// Players take turns on player 1's controls (Pac-Man, Wonder Boy), as on an upright
    /// cabinet, so online player 2 plays through them too.
    #[serde(default)]
    pub turns: bool,
    /// How many can play at once, each in their own seat (up to 4).
    #[serde(default = "two")]
    pub players: u32,
    /// The cabinet skins drawn for it (tiles/objects/cabinet_<skin>_<facing>.png), which the
    /// editor gives this game.
    #[serde(default)]
    pub cabinets: Vec<String>,
}

fn two() -> u32 {
    2
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
/// left edge is the area's left corner and its bottom the area's bottom point, and objects
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
        let bottom = cell_to_world(front.x, front.y).y - TILE_HEIGHT / 2.0;
        (Anchor::BOTTOM_LEFT, Vec3::new(left, bottom, 1.0 + depth))
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
        assert_eq!(mk2.title, "mkII");
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

    #[test]
    fn catalog_cabinet_skins_have_all_four_views() {
        let games = games_from_ron(include_str!("../../assets/games.ron")).unwrap();
        let tiles =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tiles/objects");
        for game in games {
            assert!(!game.cabinets.is_empty(), "{} has no cabinet", game.title);
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
