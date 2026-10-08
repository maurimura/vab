//! Bar layout editor: paint floor tiles and place objects on the isometric grid, then save
//! to assets/maps/bar.ron. A desktop dev tool (`make editor`) that also builds for the web
//! (`make editor-web`, README: Editor on the web), where it saves to the editor Worker instead
//! and draw mode is left out.
//!
//! The palette lists every PNG in assets/tiles/floor and assets/tiles/objects (on the web, as
//! they were when it was built), and on the desktop images reload when their files change, so
//! art edited in draw mode or a pixel-art app shows up right away. Clicking an object already
//! on the map selects it rather than painting over it, in the Move tool (also M): drag it
//! elsewhere, turn a cabinet with R, or delete it.

#[cfg(not(target_arch = "wasm32"))]
mod draw;
mod history;
#[cfg(not(target_arch = "wasm32"))]
mod scaffold;
mod store;

use bevy::asset::io::AssetReaderError;
use bevy::asset::{AssetLoadError, AssetMetaCheck, LoadState};
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use crossbeam_channel::Receiver;
#[cfg(not(target_arch = "wasm32"))]
use draw::Studio;
use history::History;
use store::{Saved, tiles_in};
use world::{
    Game, Map, MapPlugin, MapSprite, Placed, TILE_HEIGHT, TILE_WIDTH, cell_to_world, footprint,
    games_from_ron, map_sprite, world_to_cell,
};

/// The repo's assets/ folder (this crate lives in tools/editor).
#[cfg(not(target_arch = "wasm32"))]
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");
/// The assets the editor Worker serves next to the page (tools/editor/web/assets).
#[cfg(target_arch = "wasm32")]
const ASSETS: &str = "assets";
/// The repo's art/ folder: layer sources and cabinet skins, kept out of the web build.
#[cfg(not(target_arch = "wasm32"))]
const ART: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../art");
const MAP_FILE: &str = "maps/bar.ron";
/// The games cabinets can run.
const GAMES: &str = include_str!("../../../assets/games.ron");
/// A cabinet's four views, each a quarter turn from the last, as scaffold.rs renders them and
/// objects/cabinet_<skin>_<facing> tiles are named. R turns the brush through them.
const FACINGS: [&str; 4] = ["down_right", "down_left", "up_left", "up_right"];
/// Grid lines drawn around the origin, in cells.
const GRID_RADIUS: i32 = 16;
/// Largest brush, in cells per side.
const MAX_BRUSH_SIZE: i32 = 9;
const MIN_ZOOM: f32 = 1.0;
const MAX_ZOOM: f32 = 8.0;
/// Trackpad pinch (in magnification) that changes the zoom by one step.
const PINCH_PER_ZOOM_STEP: f32 = 0.2;

fn main() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: ASSETS.into(),
                // On the desktop, images reload when their files change.
                watch_for_changes_override: Some(cfg!(not(target_arch = "wasm32"))),
                // The Worker only has the files; don't ask it for a .meta next to each.
                meta_check: AssetMetaCheck::Never,
                ..default()
            })
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Bar editor".into(),
                    // On the web, <canvas id="bevy"> in tools/editor/web/index.html.
                    canvas: Some("#bevy".into()),
                    fit_canvas_to_parent: true,
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins((EguiPlugin::default(), MapPlugin))
    .insert_resource(ClearColor(Color::srgb(0.08, 0.08, 0.1)))
    .init_resource::<Editor>()
    .init_resource::<Mode>()
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (
            (move_camera, tool_keys, paint, draw_grid, shortcuts)
                .run_if(resource_equals(Mode::Map)),
            receive_map,
            finish_save,
            redraw_map,
        ),
    )
    .add_systems(EguiPrimaryContextPass, panel);
    #[cfg(not(target_arch = "wasm32"))]
    app.init_resource::<Studio>();
    app.run();
}

/// Whether the editor shows the map or, on the desktop, draws an asset.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Map,
    #[cfg(not(target_arch = "wasm32"))]
    Draw,
}

#[derive(Resource)]
struct Editor {
    map: Map,
    floor_tiles: Vec<String>,
    /// Objects other than cabinets.
    object_tiles: Vec<String>,
    /// Cabinet skins with views to paint (`None` is the plain one), and each one's game.
    cabinets: Vec<Cabinet>,
    /// What left click paints.
    brush: Brush,
    /// Cells per side of the square the brush paints and erases.
    brush_size: i32,
    /// The games cabinets can run (assets/games.ron).
    games: Vec<Game>,
    /// ROM set given to plain cabinets as they are painted; empty for none. A skin's cabinets
    /// get the skin's game.
    game: String,
    hovered: Option<IVec2>,
    zoom: f32,
    /// Pinch not yet turned into a whole zoom step.
    pinch: f32,
    map_changed: bool,
    pointer_over_panel: bool,
    /// The map before the current paint, erase or move stroke; `None` when no stroke is under
    /// way or the press began on the panel.
    stroke_start: Option<Map>,
    /// What the Move tool has picked, if anything.
    selection: Option<Selection>,
    /// While the selection is dragged: the hovered cell minus the cell it stands on, so it
    /// keeps its place under the pointer.
    grab: Option<IVec2>,
    /// The brush M put aside for the Move tool, to go back to.
    previous_brush: Option<Brush>,
    history: History<Map>,
    typing_in_panel: bool,
    status: String,
    /// The map file, until it has loaded (or failed to).
    loading: Option<Handle<Map>>,
    /// A save under way; its result arrives here.
    saving: Option<Receiver<Saved>>,
}

/// What left click paints: a tile, or a cabinet in the view R has turned it to. Or the Move
/// tool, which picks up what is there instead; a click on an object switches to it.
#[derive(Clone, PartialEq, Eq)]
enum Brush {
    None,
    Tile(String),
    Cabinet { skin: Option<String>, facing: usize },
    Move,
}

impl Brush {
    /// The tile it paints: for a cabinet, the view it faces. Empty for none.
    fn tile(&self) -> String {
        match self {
            Self::None | Self::Move => String::new(),
            Self::Tile(tile) => tile.clone(),
            Self::Cabinet { skin, facing } => cabinet_tile(skin.as_deref(), FACINGS[*facing]),
        }
    }
}

/// A cabinet skin in the palette: the game its cabinets run, from assets/games.ron.
#[derive(Clone, PartialEq)]
struct Cabinet {
    skin: Option<String>,
    game: Option<Game>,
}

impl Cabinet {
    /// Show the game title, not its internal ROM/skin identifier.
    fn label(&self) -> &str {
        match (&self.skin, &self.game) {
            (Some(_), Some(game)) => &game.title,
            (Some(skin), None) => skin,
            (None, _) => "plain",
        }
    }
}

/// The tile of a cabinet skin (`None` for the plain one) facing one way.
fn cabinet_tile(skin: Option<&str>, facing: &str) -> String {
    match skin {
        Some(skin) => format!("objects/cabinet_{skin}_{facing}"),
        None => format!("objects/cabinet_{facing}"),
    }
}

/// The skin and the view (an index into `FACINGS`) of a cabinet tile: `None` as the skin for
/// the plain one, and no parts at all for any other tile, including the old single-view
/// objects/cabinet.
fn cabinet_parts(tile: &str) -> Option<(Option<String>, usize)> {
    let rest = tile.strip_prefix("objects/cabinet")?;
    let (facing, name) = FACINGS
        .iter()
        .enumerate()
        .find(|(_, facing)| rest.ends_with(&format!("_{facing}")))?;
    let skin = match &rest[..rest.len() - name.len() - 1] {
        "" => None,
        skin => Some(skin.strip_prefix('_')?.to_owned()),
    };
    Some((skin, facing))
}

/// The two layers of the map.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Floor,
    Objects,
}

/// What the Move tool has picked. It is found again each time by the cell it stands on, so an
/// undo that puts something else there selects that, rather than pointing at the wrong thing.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Selection {
    layer: Layer,
    at: IVec2,
}

fn layer_of(map: &Map, layer: Layer) -> &Vec<Placed> {
    match layer {
        Layer::Floor => &map.floor,
        Layer::Objects => &map.objects,
    }
}

fn layer_of_mut(map: &mut Map, layer: Layer) -> &mut Vec<Placed> {
    match layer {
        Layer::Floor => &mut map.floor,
        Layer::Objects => &mut map.objects,
    }
}

/// What the Move tool picks at a cell: the object covering it, else the floor tile there.
fn pick(map: &Map, cell: IVec2) -> Option<Selection> {
    [Layer::Objects, Layer::Floor]
        .into_iter()
        .find_map(|layer| {
            let placed = layer_of(map, layer).iter().find(|p| p.covers(cell))?;
            Some(Selection {
                layer,
                at: placed.cell(),
            })
        })
}

fn placed(map: &Map, selection: Selection) -> Option<&Placed> {
    layer_of(map, selection.layer)
        .iter()
        .find(|p| p.cell() == selection.at)
}

/// The selected thing, while something stands where it was picked.
fn selected(editor: &Editor) -> Option<&Placed> {
    placed(&editor.map, editor.selection?)
}

fn selected_mut(editor: &mut Editor) -> Option<&mut Placed> {
    let selection = editor.selection?;
    layer_of_mut(&mut editor.map, selection.layer)
        .iter_mut()
        .find(|p| p.cell() == selection.at)
}

/// Moves the selection to stand on `to`, unless something else in its layer is in the way: a
/// drag across other things leaves them be.
fn move_selection(editor: &mut Editor, to: IVec2) {
    let Some(selection) = editor.selection else {
        return;
    };
    let layer = layer_of_mut(&mut editor.map, selection.layer);
    let Some(index) = layer.iter().position(|p| p.cell() == selection.at) else {
        return;
    };
    let moved = Placed {
        x: to.x,
        y: to.y,
        ..layer[index].clone()
    };
    let blocked = layer
        .iter()
        .enumerate()
        .any(|(i, other)| i != index && moved.cells().any(|cell| other.covers(cell)));
    if blocked {
        return;
    }
    layer[index] = moved;
    editor.selection = Some(Selection {
        at: to,
        ..selection
    });
    editor.map_changed = true;
}

/// R with the Move tool: turns the selected cabinet a quarter turn. Its game stays.
fn turn_selection(editor: &mut Editor) {
    let Some((skin, facing)) = selected(editor).and_then(|p| cabinet_parts(&p.tile)) else {
        return;
    };
    let tile = cabinet_tile(skin.as_deref(), FACINGS[(facing + 1) % FACINGS.len()]);
    let before = editor.map.clone();
    if let Some(placed) = selected_mut(editor) {
        placed.tile = tile;
    }
    editor.map_changed = true;
    record_undo(editor, before);
}

/// Delete or Backspace with the Move tool: removes the selection.
fn delete_selection(editor: &mut Editor) {
    let Some(selection) = editor.selection.take() else {
        return;
    };
    let before = editor.map.clone();
    layer_of_mut(&mut editor.map, selection.layer).retain(|p| p.cell() != selection.at);
    editor.map_changed = true;
    record_undo(editor, before);
}

/// Gives the selected cabinet a game (none for an empty `rom`), as one undo step.
fn assign_game(editor: &mut Editor, rom: String) {
    let before = editor.map.clone();
    if let Some(placed) = selected_mut(editor) {
        placed.game = (!rom.is_empty()).then_some(rom);
    }
    editor.map_changed = true;
    record_undo(editor, before);
}

/// M, or Move in the palette: the Move tool, or back to the brush it replaced.
fn toggle_move_tool(editor: &mut Editor) {
    if editor.brush == Brush::Move {
        editor.brush = editor
            .previous_brush
            .take()
            .unwrap_or_else(|| first_floor_tile(editor));
    } else {
        editor.previous_brush = Some(std::mem::replace(&mut editor.brush, Brush::Move));
    }
}

/// The brush to start with, or fall back to.
fn first_floor_tile(editor: &Editor) -> Brush {
    editor
        .floor_tiles
        .first()
        .map_or(Brush::None, |tile| Brush::Tile(tile.clone()))
}

/// The title of a game in games.ron, or the ROM set's name when it isn't there.
fn game_title<'a>(games: &'a [Game], rom: &'a str) -> &'a str {
    games
        .iter()
        .find(|game| game.rom == rom)
        .map_or(rom, |game| game.title.as_str())
}

/// A placed cabinet's game title, without internal tile/ROM identifiers.
fn placed_label<'a>(games: &'a [Game], placed: &'a Placed) -> &'a str {
    placed
        .game
        .as_deref()
        .and_then(|rom| games.iter().find(|game| game.rom == rom))
        .map_or(placed.tile.as_str(), |game| game.title.as_str())
}

/// A dropdown of the games in games.ron, "(none)" first, that writes the pick into `rom`.
fn game_combo(ui: &mut egui::Ui, id: &str, games: &[Game], rom: &mut String) {
    let selected = games
        .iter()
        .find(|game| game.rom == *rom)
        .map_or("(none)", |game| game.title.as_str());
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(rom, String::new(), "(none)");
            for game in games {
                ui.selectable_value(rom, game.rom.clone(), &game.title);
            }
        });
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            map: Map::default(),
            floor_tiles: Vec::new(),
            object_tiles: Vec::new(),
            cabinets: Vec::new(),
            brush: Brush::None,
            brush_size: 1,
            games: Vec::new(),
            game: String::new(),
            hovered: None,
            zoom: 3.0,
            pinch: 0.0,
            map_changed: true,
            pointer_over_panel: false,
            stroke_start: None,
            selection: None,
            grab: None,
            previous_brush: None,
            history: History::default(),
            typing_in_panel: false,
            status: String::new(),
            loading: None,
            saving: None,
        }
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut editor: ResMut<Editor>) {
    commands.spawn(Camera2d);
    match games_from_ron(GAMES) {
        Ok(games) => editor.games = games,
        Err(error) => editor.status = format!("Could not read games.ron: {error}\n"),
    }
    load_palette(&mut editor);
    editor.brush = first_floor_tile(&editor);
    editor.loading = Some(asset_server.load(MAP_FILE));
    editor.status += &format!("Loading {MAP_FILE}");
}

/// Reads which tiles there are: floor, objects, and the cabinet skins among them, one entry for
/// a skin's four views, with the game assets/games.ron gives the skin. A tile the brush was
/// painting may be gone (deleted in draw mode); the brush then goes back to the first floor tile.
fn load_palette(editor: &mut Editor) {
    editor.floor_tiles = tiles_in("floor");
    editor.object_tiles = Vec::new();
    editor.cabinets = Vec::new();
    for tile in tiles_in("objects") {
        match cabinet_parts(&tile).map(|(skin, _)| skin) {
            // Retain legacy artwork for existing maps, but offer only its v2 replacement.
            Some(Some(skin)) if skin == "mk2" => {}
            Some(skin) => {
                if !editor.cabinets.iter().any(|cabinet| cabinet.skin == skin) {
                    let game = editor
                        .games
                        .iter()
                        .find(|game| {
                            skin.as_ref()
                                .is_some_and(|skin| game.cabinets.contains(skin))
                        })
                        .cloned();
                    editor.cabinets.push(Cabinet { skin, game });
                }
            }
            None if tile == "objects/cabinet" => {} // The single view from before skins.
            None => editor.object_tiles.push(tile),
        }
    }
    let gone = match &editor.brush {
        Brush::None | Brush::Move => false,
        Brush::Tile(tile) => {
            !editor.floor_tiles.contains(tile) && !editor.object_tiles.contains(tile)
        }
        Brush::Cabinet { skin, .. } => !editor.cabinets.iter().any(|c| &c.skin == skin),
    };
    if gone {
        editor.brush = first_floor_tile(editor);
    }
}

/// Takes the map once it has loaded: from assets/ on the desktop, from the Worker on the web,
/// which serves the one last saved there.
fn receive_map(asset_server: Res<AssetServer>, maps: Res<Assets<Map>>, mut editor: ResMut<Editor>) {
    let Some(handle) = &editor.loading else {
        return;
    };
    if let Some(map) = maps.get(handle) {
        editor.map = map.clone();
        editor.map_changed = true;
        editor.status = format!("Loaded {MAP_FILE}");
    } else if let LoadState::Failed(error) = asset_server.load_state(handle) {
        editor.status = match *error {
            AssetLoadError::AssetReaderError(AssetReaderError::NotFound(_)) => {
                format!("New map; saves to {MAP_FILE}")
            }
            _ => format!("Could not read {MAP_FILE}: {error}"),
        };
    } else {
        return;
    }
    editor.loading = None;
}

/// Shows how the save under way went, once it has.
fn finish_save(mut editor: ResMut<Editor>) {
    let Some(result) = editor.saving.as_ref().and_then(|done| done.try_recv().ok()) else {
        return;
    };
    editor.status = result.unwrap_or_else(|error| error);
    editor.saving = None;
}

/// Arrows/WASD or scrolling pan; + and - or a trackpad pinch zoom in whole steps so pixels
/// stay square.
fn move_camera(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut pinches: MessageReader<PinchGesture>,
    time: Res<Time>,
    mut editor: ResMut<Editor>,
    camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let (mut transform, mut projection) = camera.into_inner();
    let mut pan = Vec2::ZERO;
    if !editor.typing_in_panel {
        for (key, direction) in [
            (KeyCode::ArrowLeft, Vec2::NEG_X),
            (KeyCode::KeyA, Vec2::NEG_X),
            (KeyCode::ArrowRight, Vec2::X),
            (KeyCode::KeyD, Vec2::X),
            (KeyCode::ArrowUp, Vec2::Y),
            (KeyCode::KeyW, Vec2::Y),
            (KeyCode::ArrowDown, Vec2::NEG_Y),
            (KeyCode::KeyS, Vec2::NEG_Y),
        ] {
            if keys.pressed(key) {
                pan += direction * 600.0 * time.delta_secs();
            }
        }
        if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
            editor.zoom = (editor.zoom + 1.0).min(MAX_ZOOM);
        }
        if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
            editor.zoom = (editor.zoom - 1.0).max(MIN_ZOOM);
        }
    }
    if !editor.pointer_over_panel {
        let lines_to_pixels = match scroll.unit {
            MouseScrollUnit::Line => 40.0,
            MouseScrollUnit::Pixel => 1.0,
        };
        pan += Vec2::new(-scroll.delta.x, scroll.delta.y) * lines_to_pixels;
    }
    let pinched: f32 = pinches.read().map(|pinch| pinch.0).sum();
    if !editor.pointer_over_panel {
        editor.pinch += pinched;
        let steps = (editor.pinch / PINCH_PER_ZOOM_STEP).trunc();
        editor.pinch -= steps * PINCH_PER_ZOOM_STEP;
        editor.zoom = (editor.zoom + steps).clamp(MIN_ZOOM, MAX_ZOOM);
    }
    transform.translation += (pan / editor.zoom).extend(0.0);
    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = 1.0 / editor.zoom;
    }
}

/// [ and ] shrink and grow the brush; M switches to the Move tool and back; R turns a cabinet
/// a quarter turn, the brush's or the selected one; Delete removes the selection, Esc lets go.
fn tool_keys(keys: Res<ButtonInput<KeyCode>>, mut editor: ResMut<Editor>) {
    if editor.typing_in_panel {
        return;
    }
    let editor = &mut *editor;
    if keys.just_pressed(KeyCode::BracketLeft) {
        editor.brush_size = (editor.brush_size - 1).max(1);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        editor.brush_size = (editor.brush_size + 1).min(MAX_BRUSH_SIZE);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        toggle_move_tool(editor);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        if let Brush::Cabinet { facing, .. } = &mut editor.brush {
            *facing = (*facing + 1) % FACINGS.len();
        } else if editor.brush == Brush::Move {
            turn_selection(editor);
        }
    }
    if editor.brush == Brush::Move {
        if keys.any_just_pressed([KeyCode::Delete, KeyCode::Backspace]) {
            delete_selection(editor);
        }
        if keys.just_pressed(KeyCode::Escape) {
            editor.selection = None;
        }
    }
}

/// The cells a brush of `size` covers when centered on `center`.
fn brush_cells(center: IVec2, size: i32) -> impl Iterator<Item = IVec2> {
    let start = center - IVec2::splat((size - 1) / 2);
    (0..size).flat_map(move |x| (0..size).map(move |y| start + IVec2::new(x, y)))
}

/// Where the brush places tiles: every brush cell, except that an object covering several
/// cells is placed once, on the hovered cell.
fn brush_anchors(center: IVec2, brush: &str, size: i32) -> Vec<IVec2> {
    if footprint(brush) == IVec2::ONE {
        brush_cells(center, size).collect()
    } else {
        vec![center]
    }
}

/// Left click paints the brush on the hovered cells, unless an object is there: that is
/// selected instead, in the Move tool, where a click picks up what is there and drags it.
/// Right click erases the hovered cells (objects first).
fn paint(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut editor: ResMut<Editor>,
) {
    let (camera, camera_transform) = *camera;
    editor.hovered = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        .map(world_to_cell);
    let editor = &mut *editor;
    let mouse = [MouseButton::Left, MouseButton::Right];
    // A stroke is one undo step.
    if !buttons.any_pressed(mouse)
        && let Some(before) = editor.stroke_start.take()
    {
        record_undo(editor, before);
    }
    if !buttons.pressed(MouseButton::Left) {
        editor.grab = None;
    }
    if buttons.any_just_pressed(mouse) && editor.stroke_start.is_none() {
        editor.stroke_start = (!editor.pointer_over_panel).then(|| editor.map.clone());
    }
    let Some(center) = editor.hovered else { return };
    if editor.pointer_over_panel || editor.stroke_start.is_none() || editor.loading.is_some() {
        return;
    }

    // A click over in one frame is pressed and released before this runs: paint on the press too.
    let held = |button| buttons.pressed(button) || buttons.just_pressed(button);
    // Whatever the brush, a click on an object selects it, and the same press can drag it.
    // Floor is painted over as always, or there would be no painting over it.
    if buttons.just_pressed(MouseButton::Left)
        && editor.brush != Brush::Move
        && editor.map.objects.iter().any(|p| p.covers(center))
    {
        toggle_move_tool(editor);
    }
    if editor.brush == Brush::Move {
        if buttons.just_pressed(MouseButton::Left) {
            editor.selection = pick(&editor.map, center);
            editor.grab = editor.selection.map(|selection| center - selection.at);
        }
        if held(MouseButton::Left)
            && let (Some(selection), Some(grab)) = (editor.selection, editor.grab)
            && center - grab != selection.at
        {
            move_selection(editor, center - grab);
        }
    }
    let tile = editor.brush.tile();
    if held(MouseButton::Left) && !tile.is_empty() {
        let is_object = tile.starts_with("objects/");
        // A skin's cabinets run the skin's game; a plain one, the game picked in the panel.
        let game = match &editor.brush {
            Brush::Cabinet { skin: Some(_), .. } => brush_game(editor).map(|game| game.rom.clone()),
            Brush::Cabinet { skin: None, .. } => {
                (!editor.game.is_empty()).then(|| editor.game.clone())
            }
            _ => None,
        };
        let layer = if is_object {
            &mut editor.map.objects
        } else {
            &mut editor.map.floor
        };
        for cell in brush_anchors(center, &tile, editor.brush_size) {
            let placed = Placed {
                x: cell.x,
                y: cell.y,
                tile: tile.clone(),
                game: game.clone(),
            };
            if !layer.contains(&placed) {
                // Whatever it would overlap goes, objects covering several cells included.
                let covered: Vec<IVec2> = placed.cells().collect();
                layer.retain(|p| !covered.iter().any(|&c| p.covers(c)));
                layer.push(placed);
                editor.map_changed = true;
            }
        }
    }

    if held(MouseButton::Right) {
        for cell in brush_cells(center, editor.brush_size) {
            let at_cell = |p: &Placed| p.covers(cell);
            for layer in [&mut editor.map.objects, &mut editor.map.floor] {
                if layer.iter().any(at_cell) {
                    layer.retain(|p| !at_cell(p));
                    editor.map_changed = true;
                    break;
                }
            }
        }
    }
}

/// Redraws every map sprite after an edit; maps are small, so this stays simple.
fn redraw_map(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut editor: ResMut<Editor>,
    drawn: Query<Entity, With<MapSprite>>,
) {
    if !editor.map_changed {
        return;
    }
    editor.map_changed = false;
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    for placed in &editor.map.floor {
        commands.spawn(map_sprite(&asset_server, placed, false));
    }
    for placed in &editor.map.objects {
        commands.spawn(map_sprite(&asset_server, placed, true));
    }
}

fn draw_grid(mut gizmos: Gizmos, editor: Res<Editor>) {
    let diamond = |x: i32, y: i32| {
        let c = cell_to_world(x, y);
        let (w, h) = (TILE_WIDTH / 2.0, TILE_HEIGHT / 2.0);
        [
            c + Vec2::new(0.0, h),
            c + Vec2::new(w, 0.0),
            c + Vec2::new(0.0, -h),
            c + Vec2::new(-w, 0.0),
            c + Vec2::new(0.0, h),
        ]
    };
    let faint = Color::srgba(1.0, 1.0, 1.0, 0.06);
    for x in -GRID_RADIUS..=GRID_RADIUS {
        for y in -GRID_RADIUS..=GRID_RADIUS {
            gizmos.linestrip_2d(diamond(x, y), faint);
        }
    }
    let yellow = Color::srgb(1.0, 0.85, 0.2);
    if let Some(center) = editor.hovered {
        if editor.brush == Brush::Move {
            // What a click would pick up.
            let under = pick(&editor.map, center).and_then(|s| placed(&editor.map, s));
            for cell in under.into_iter().flat_map(Placed::cells) {
                gizmos.linestrip_2d(diamond(cell.x, cell.y), yellow.with_alpha(0.5));
            }
        } else {
            let tile = editor.brush.tile();
            let size = footprint(&tile);
            for anchor in brush_anchors(center, &tile, editor.brush_size) {
                for dx in 0..size.x {
                    for dy in 0..size.y {
                        let cell = anchor + IVec2::new(dx, dy);
                        gizmos.linestrip_2d(diamond(cell.x, cell.y), yellow);
                    }
                }
            }
        }
    }
    if editor.brush == Brush::Move
        && let Some(placed) = selected(&editor)
    {
        for cell in placed.cells() {
            gizmos.linestrip_2d(diamond(cell.x, cell.y), Color::srgb(0.4, 0.9, 1.0));
        }
    }
    // A dot above each cabinet that has a game.
    for placed in editor.map.objects.iter().filter(|p| p.game.is_some()) {
        let above = cell_to_world(placed.x, placed.y) + Vec2::Y * 44.0;
        gizmos.circle_2d(above, 2.5, Color::srgb(0.3, 1.0, 0.4));
    }
}

/// Cmd/Ctrl+S saves, Cmd/Ctrl+Z undoes, Cmd/Ctrl+Shift+Z or Cmd/Ctrl+Y redoes.
fn shortcuts(keys: Res<ButtonInput<KeyCode>>, mut editor: ResMut<Editor>) {
    let modifier = [
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
    ];
    if !keys.any_pressed(modifier) {
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::KeyS) {
        save(&mut editor);
    }
    if keys.just_pressed(KeyCode::KeyZ) && !shift {
        undo(&mut editor);
    }
    if (keys.just_pressed(KeyCode::KeyZ) && shift) || keys.just_pressed(KeyCode::KeyY) {
        redo(&mut editor);
    }
}

/// Clears a deleted tile from the map, as one undo step, and from the palette.
#[cfg(not(target_arch = "wasm32"))]
fn forget_tile(editor: &mut Editor, tile: &str) {
    let before = editor.map.clone();
    editor.map.floor.retain(|placed| placed.tile != tile);
    editor.map.objects.retain(|placed| placed.tile != tile);
    if before != editor.map {
        editor.map_changed = true;
        editor.status = format!("Cleared {tile} from the map; save to keep that");
    }
    record_undo(editor, before);
    load_palette(editor);
}

/// The game a skin's cabinets get: the brush's skin's, from assets/games.ron.
fn brush_game(editor: &Editor) -> Option<&Game> {
    let Brush::Cabinet { skin, .. } = &editor.brush else {
        return None;
    };
    editor
        .cabinets
        .iter()
        .find(|cabinet| &cabinet.skin == skin)?
        .game
        .as_ref()
}

/// Keeps `before` for undo if the map has changed since.
fn record_undo(editor: &mut Editor, before: Map) {
    editor.history.record(before, &editor.map);
}

fn undo(editor: &mut Editor) {
    if editor.history.undo(&mut editor.map) {
        editor.map_changed = true;
    }
}

fn redo(editor: &mut Editor) {
    if editor.history.redo(&mut editor.map) {
        editor.map_changed = true;
    }
}

/// Saves the map, unless a save is still under way (`finish_save` shows how it went).
fn save(editor: &mut Editor) {
    if editor.saving.is_some() || editor.loading.is_some() {
        return;
    }
    let (done, result) = crossbeam_channel::bounded(1);
    store::save(&editor.map, done);
    editor.saving = Some(result);
    editor.status = "Saving...".into();
}

fn panel(
    mut contexts: EguiContexts,
    mut editor: ResMut<Editor>,
    #[cfg(not(target_arch = "wasm32"))] mut studio: ResMut<Studio>,
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))] mut mode: ResMut<Mode>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let editor = &mut *editor;
    // egui 0.36 panels are shown inside a Ui; this one covers the window (see bevy_egui's
    // side_panel example).
    let mut window_ui = egui::Ui::new(
        ctx.clone(),
        "window".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let palette = egui::Panel::left("palette")
        .resizable(false)
        .default_size(190.0)
        .show(&mut window_ui, |ui| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.horizontal(|ui| {
                    if ui.selectable_label(*mode == Mode::Map, "Map").clicked()
                        && *mode != Mode::Map
                    {
                        *mode = Mode::Map;
                        // Pick up assets made in draw mode.
                        load_palette(editor);
                    }
                    if ui.selectable_label(*mode == Mode::Draw, "Draw").clicked()
                        && *mode != Mode::Draw
                    {
                        *mode = Mode::Draw;
                        studio.enter(&editor.brush.tile());
                    }
                });
                ui.separator();
            }
            match *mode {
                Mode::Map => map_panel(ui, editor),
                #[cfg(not(target_arch = "wasm32"))]
                Mode::Draw => {
                    if let Some(tile) = studio.side_panel(ui, &editor.map) {
                        forget_tile(editor, &tile);
                    }
                }
            }
        })
        .response
        .rect;
    #[cfg(not(target_arch = "wasm32"))]
    if *mode == Mode::Draw {
        studio.canvas(&mut window_ui);
        studio.shortcuts(ctx);
    }
    // Panels in a background Ui aren't areas egui hit-tests, so is_pointer_over_egui() misses
    // the palette; check its rect too.
    let over_palette = ctx
        .pointer_hover_pos()
        .is_some_and(|pos| palette.contains(pos));
    editor.pointer_over_panel =
        over_palette || ctx.is_pointer_over_egui() || ctx.egui_wants_pointer_input();
    editor.typing_in_panel = ctx.egui_wants_keyboard_input();
    Ok(())
}

fn map_panel(ui: &mut egui::Ui, editor: &mut Editor) {
    if ui
        .selectable_label(editor.brush == Brush::Move, "Move things (M)")
        .clicked()
        && editor.brush != Brush::Move
    {
        toggle_move_tool(editor);
    }
    ui.add_space(8.0);
    for (heading, tiles) in [
        ("Floor", &editor.floor_tiles),
        ("Objects", &editor.object_tiles),
    ] {
        ui.heading(heading);
        for tile in tiles {
            let name = tile.split_once('/').map_or(tile.as_str(), |(_, name)| name);
            let selected = editor.brush == Brush::Tile(tile.clone());
            if ui.selectable_label(selected, name).clicked() {
                editor.brush = Brush::Tile(tile.clone());
            }
        }
        ui.add_space(8.0);
    }
    ui.heading("Cabinets");
    // Picking a skin keeps the way the brush faces.
    let facing = match editor.brush {
        Brush::Cabinet { facing, .. } => facing,
        _ => 0,
    };
    for cabinet in &editor.cabinets {
        let name = cabinet.label();
        let selected =
            matches!(&editor.brush, Brush::Cabinet { skin, .. } if skin == &cabinet.skin);
        // Variants can share a title; retain the skin only as the widget's identity.
        if ui
            .push_id(&cabinet.skin, |ui| ui.selectable_label(selected, name))
            .inner
            .clicked()
        {
            editor.brush = Brush::Cabinet {
                skin: cabinet.skin.clone(),
                facing,
            };
        }
    }
    if let Brush::Cabinet { facing, .. } = editor.brush {
        ui.small(format!("Facing {} (R turns it)", FACINGS[facing]));
    }
    ui.add_space(8.0);
    ui.add(egui::Slider::new(&mut editor.brush_size, 1..=MAX_BRUSH_SIZE).text("Brush size"));
    ui.separator();
    match &editor.brush {
        Brush::Cabinet { skin: Some(_), .. } => {
            let title = brush_game(editor).map_or("none in games.ron", |game| &game.title);
            ui.label(format!("Game: {title}"));
            ui.small("The skin's, from assets/games.ron.");
        }
        Brush::Cabinet { skin: None, .. } => {
            ui.label("Cabinet game");
            game_combo(ui, "game", &editor.games, &mut editor.game);
            ui.small("Paint a cabinet to place or reassign it.");
        }
        Brush::Move => match selected(editor).cloned() {
            Some(placed) => {
                ui.label(format!(
                    "Selected: {}",
                    placed_label(&editor.games, &placed)
                ));
                let mut rom = placed.game.clone().unwrap_or_default();
                match cabinet_parts(&placed.tile) {
                    // A skin's cabinets keep the skin's game.
                    Some((Some(_), _)) => {
                        let title = if rom.is_empty() {
                            "none"
                        } else {
                            game_title(&editor.games, &rom)
                        };
                        ui.label(format!("Game: {title}"));
                    }
                    Some((None, _)) => {
                        ui.label("Cabinet game");
                        game_combo(ui, "selected game", &editor.games, &mut rom);
                        if rom != placed.game.unwrap_or_default() {
                            assign_game(editor, rom);
                        }
                    }
                    // Not a cabinet with views (the old objects/cabinet, say): just its game.
                    None if !rom.is_empty() => {
                        ui.label(format!("Game: {}", game_title(&editor.games, &rom)));
                    }
                    None => {}
                }
                ui.small("Drag it to move it. R turns a cabinet, Delete removes it, Esc lets go.");
            }
            None => {
                ui.small("Click something to pick it up; it goes where there is room.");
            }
        },
        _ => {}
    }
    ui.separator();
    if let Some(cell) = editor.hovered {
        let object = editor.map.objects.iter().find(|p| p.covers(cell));
        let text = match object {
            Some(placed) => placed_label(&editor.games, placed).to_owned(),
            None => "empty".into(),
        };
        ui.label(format!("Cell ({}, {}): {text}", cell.x, cell.y));
    }
    ui.horizontal(|ui| {
        let save_button = egui::Button::new("Save (Cmd+S)");
        let can_save = editor.saving.is_none() && editor.loading.is_none();
        if ui.add_enabled(can_save, save_button).clicked() {
            save(editor);
        }
        if ui.button("Clear").clicked() {
            let before = std::mem::take(&mut editor.map);
            record_undo(editor, before);
            editor.map_changed = true;
            editor.status = format!("Cleared; {MAP_FILE} is unchanged until you save");
        }
    });
    ui.horizontal(|ui| {
        let undo_button = egui::Button::new("Undo (Cmd+Z)");
        if ui
            .add_enabled(editor.history.can_undo(), undo_button)
            .clicked()
        {
            undo(editor);
        }
        let redo_button = egui::Button::new("Redo (Cmd+Shift+Z)");
        if ui
            .add_enabled(editor.history.can_redo(), redo_button)
            .clicked()
        {
            redo(editor);
        }
    });
    ui.label(&editor.status);
    ui.separator();
    ui.small(
                "Left click: paint, or select what is there\nRight click: erase\n[ / ]: brush size\nR: turn a cabinet\nM: move things (drag; Delete removes)\nScroll, arrows, WASD: pan\n+ / -, pinch: zoom",
            );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cabinet_labels_use_only_the_game_title() {
        for game in games_from_ron(GAMES).unwrap() {
            for skin in &game.cabinets {
                let cabinet = Cabinet {
                    skin: Some(skin.clone()),
                    game: Some(game.clone()),
                };
                assert_eq!(cabinet.label(), game.title);
            }
        }
    }

    #[test]
    fn palette_offers_only_mk2_v2_named_mkii() {
        let mut editor = Editor {
            games: games_from_ron(GAMES).unwrap(),
            ..Default::default()
        };
        load_palette(&mut editor);
        assert!(
            editor
                .cabinets
                .iter()
                .all(|cabinet| cabinet.skin.as_deref() != Some("mk2"))
        );
        let cabinets: Vec<_> = editor
            .cabinets
            .iter()
            .filter(|cabinet| cabinet.game.as_ref().is_some_and(|game| game.rom == "mk2"))
            .collect();
        assert_eq!(cabinets.len(), 1);
        assert_eq!(cabinets[0].skin.as_deref(), Some("mk2_v2"));
        assert_eq!(cabinets[0].label(), "Mortal Kombat II");
    }

    #[test]
    fn moved_and_hovered_cabinets_use_only_the_game_title() {
        let games = games_from_ron(GAMES).unwrap();
        for game in &games {
            for skin in &game.cabinets {
                let placed = Placed {
                    x: 0,
                    y: 0,
                    tile: cabinet_tile(Some(skin), FACINGS[0]),
                    game: Some(game.rom.clone()),
                };
                assert_eq!(placed_label(&games, &placed), game.title);
            }
        }
    }

    #[test]
    fn unassigned_objects_keep_their_asset_label() {
        let placed = Placed {
            x: 0,
            y: 0,
            tile: "objects/air_hockey".into(),
            game: None,
        };
        assert_eq!(placed_label(&[], &placed), "objects/air_hockey");
    }

    #[test]
    fn cabinet_variants_share_a_title_but_keep_distinct_identity() {
        let game = games_from_ron(GAMES)
            .unwrap()
            .into_iter()
            .find(|game| game.rom == "mk2")
            .unwrap();
        let original = Cabinet {
            skin: Some("mk2".into()),
            game: Some(game.clone()),
        };
        let alternative = Cabinet {
            skin: Some("mk2_v2".into()),
            game: Some(game),
        };
        assert_eq!(original.label(), alternative.label());
        assert_ne!(original.skin, alternative.skin);
    }
}
