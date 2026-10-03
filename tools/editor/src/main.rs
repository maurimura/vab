//! Bar layout editor: paint floor tiles and place objects on the isometric grid, then save
//! to assets/maps/bar.ron. A desktop dev tool (`make editor`) that also builds for the web
//! (`make editor-web`, README: Editor on the web), where it saves to the editor Worker instead
//! and draw mode is left out.
//!
//! The palette lists every PNG in assets/tiles/floor and assets/tiles/objects (on the web, as
//! they were when it was built), and on the desktop images reload when their files change, so
//! art edited in draw mode or a pixel-art app shows up right away.

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
            (move_camera, resize_brush, paint, draw_grid, shortcuts)
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
    object_tiles: Vec<String>,
    /// The tile left click paints.
    brush: String,
    /// Cells per side of the square the brush paints and erases.
    brush_size: i32,
    /// The games cabinets can run (assets/games.ron).
    games: Vec<Game>,
    /// ROM set given to cabinets as they are painted; empty for none.
    game: String,
    hovered: Option<IVec2>,
    zoom: f32,
    /// Pinch not yet turned into a whole zoom step.
    pinch: f32,
    map_changed: bool,
    pointer_over_panel: bool,
    /// The map before the current paint or erase stroke; `None` when no stroke is under way
    /// or the press began on the panel.
    stroke_start: Option<Map>,
    history: History<Map>,
    typing_in_panel: bool,
    status: String,
    /// The map file, until it has loaded (or failed to).
    loading: Option<Handle<Map>>,
    /// A save under way; its result arrives here.
    saving: Option<Receiver<Saved>>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            map: Map::default(),
            floor_tiles: tiles_in("floor"),
            object_tiles: tiles_in("objects"),
            brush: String::new(),
            brush_size: 1,
            games: Vec::new(),
            game: String::new(),
            hovered: None,
            zoom: 3.0,
            pinch: 0.0,
            map_changed: true,
            pointer_over_panel: false,
            stroke_start: None,
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
    editor.brush = editor.floor_tiles.first().cloned().unwrap_or_default();
    match games_from_ron(GAMES) {
        Ok(games) => editor.games = games,
        Err(error) => editor.status = format!("Could not read games.ron: {error}\n"),
    }
    editor.loading = Some(asset_server.load(MAP_FILE));
    editor.status += &format!("Loading {MAP_FILE}");
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

/// [ and ] shrink and grow the brush.
fn resize_brush(keys: Res<ButtonInput<KeyCode>>, mut editor: ResMut<Editor>) {
    if editor.typing_in_panel {
        return;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        editor.brush_size = (editor.brush_size - 1).max(1);
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        editor.brush_size = (editor.brush_size + 1).min(MAX_BRUSH_SIZE);
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

/// Left click paints the brush on the hovered cells, right click erases them (objects first).
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
    if buttons.any_just_pressed(mouse) && editor.stroke_start.is_none() {
        editor.stroke_start = (!editor.pointer_over_panel).then(|| editor.map.clone());
    }
    let Some(center) = editor.hovered else { return };
    if editor.pointer_over_panel || editor.stroke_start.is_none() || editor.loading.is_some() {
        return;
    }

    // A click over in one frame is pressed and released before this runs: paint on the press too.
    let held = |button| buttons.pressed(button) || buttons.just_pressed(button);
    if held(MouseButton::Left) && !editor.brush.is_empty() {
        let is_object = editor.brush.starts_with("objects/");
        let game = (is_object && editor.brush.contains("cabinet") && !editor.game.is_empty())
            .then(|| editor.game.clone());
        let layer = if is_object {
            &mut editor.map.objects
        } else {
            &mut editor.map.floor
        };
        for cell in brush_anchors(center, &editor.brush, editor.brush_size) {
            let placed = Placed {
                x: cell.x,
                y: cell.y,
                tile: editor.brush.clone(),
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
    if let Some(center) = editor.hovered {
        let size = footprint(&editor.brush);
        for anchor in brush_anchors(center, &editor.brush, editor.brush_size) {
            for dx in 0..size.x {
                for dy in 0..size.y {
                    let cell = anchor + IVec2::new(dx, dy);
                    gizmos.linestrip_2d(diamond(cell.x, cell.y), Color::srgb(1.0, 0.85, 0.2));
                }
            }
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
    editor.floor_tiles = tiles_in("floor");
    editor.object_tiles = tiles_in("objects");
    if editor.brush == tile {
        editor.brush = editor.floor_tiles.first().cloned().unwrap_or_default();
    }
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
                        editor.floor_tiles = tiles_in("floor");
                        editor.object_tiles = tiles_in("objects");
                    }
                    if ui.selectable_label(*mode == Mode::Draw, "Draw").clicked()
                        && *mode != Mode::Draw
                    {
                        *mode = Mode::Draw;
                        studio.enter(&editor.brush);
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
    for (heading, tiles) in [
        ("Floor", &editor.floor_tiles),
        ("Objects", &editor.object_tiles),
    ] {
        ui.heading(heading);
        for tile in tiles {
            let name = tile.split_once('/').map_or(tile.as_str(), |(_, name)| name);
            if ui.selectable_label(editor.brush == *tile, name).clicked() {
                editor.brush = tile.clone();
            }
        }
        ui.add_space(8.0);
    }
    ui.add(egui::Slider::new(&mut editor.brush_size, 1..=MAX_BRUSH_SIZE).text("Brush size"));
    ui.separator();
    ui.label("Cabinet game");
    let selected = editor
        .games
        .iter()
        .find(|game| game.rom == editor.game)
        .map_or("(none)", |game| game.title.as_str());
    egui::ComboBox::from_id_salt("game")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut editor.game, String::new(), "(none)");
            for game in &editor.games {
                ui.selectable_value(&mut editor.game, game.rom.clone(), &game.title);
            }
        });
    ui.small("Paint a cabinet to place or reassign it.");
    ui.separator();
    if let Some(cell) = editor.hovered {
        let object = editor.map.objects.iter().find(|p| p.covers(cell));
        let text = match object {
            Some(Placed {
                tile,
                game: Some(rom),
                ..
            }) => {
                let title = editor.games.iter().find(|game| &game.rom == rom);
                format!(
                    "{tile} [{}]",
                    title.map_or(rom.as_str(), |game| &game.title)
                )
            }
            Some(placed) => placed.tile.clone(),
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
                "Left click: paint\nRight click: erase\n[ / ]: brush size\nScroll, arrows, WASD: pan\n+ / -, pinch: zoom",
            );
}
