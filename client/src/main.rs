mod cabinets;
mod chat;
mod emulator;
mod help;
mod hockey;
mod pixels;
mod player;
mod pool;
mod room;
mod seats;
mod settings;
mod shuffleboard;
#[cfg(feature = "test-hooks")]
mod testing;
mod touch;

use bevy::asset::AssetId;
use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
use cabinets::{Cabinets, CabinetsPlugin};
use chat::ChatPlugin;
use emulator::EmulatorPlugin;
use help::HelpPlugin;
use hockey::{HockeyPlugin, HockeyTables};
use player::{PlayerPlugin, Walkable, spawn_player};
use pool::{PoolPlugin, PoolTables};
use room::RoomPlugin;
use settings::SettingsPlugin;
use shuffleboard::{ShuffleboardPlugin, ShuffleboardTables};
use touch::TouchPlugin;
use world::{Map, MapPlugin, map_sprite};

/// All text is in Fira Mono cut down to Latin-1, so names and chat can have accents and ñ
/// (Bevy's own font is ASCII only). SIL Open Font License: fonts/OFL.txt.
const FONT: &[u8] = include_bytes!("../fonts/FiraMono-Latin1.ttf");

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Arcade Bar".into(),
                    // Render into <canvas id="bevy"> in web/index.html, sized to its parent.
                    canvas: Some("#bevy".into()),
                    fit_canvas_to_parent: true,
                    // Let browser shortcuts through (dev tools, reload).
                    prevent_default_event_handling: false,
                    ..default()
                }),
                ..default()
            })
            // Pixel art: sample textures without smoothing.
            .set(ImagePlugin::default_nearest())
            // The server only has the PNGs; don't request a .meta file for each.
            .set(AssetPlugin {
                meta_check: AssetMetaCheck::Never,
                ..default()
            }),
        MapPlugin,
        PlayerPlugin,
        CabinetsPlugin,
        ChatPlugin,
        EmulatorPlugin,
        HelpPlugin,
        PoolPlugin,
        HockeyPlugin,
        ShuffleboardPlugin,
        RoomPlugin,
        SettingsPlugin,
        TouchPlugin,
    ))
    .init_state::<Mode>()
    .insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
    .add_systems(Startup, setup)
    .add_systems(Update, build_bar.run_if(in_state(Mode::Loading)));
    // Text without a font of its own uses the default handle's.
    app.world_mut()
        .resource_mut::<Assets<Font>>()
        .insert(AssetId::default(), Font::from_bytes(FONT.to_vec()))
        .expect("the default font handle takes a font");
    #[cfg(feature = "test-hooks")]
    app.add_plugins(testing::TestingPlugin);
    app.run();
}

/// Waiting for the bar's map, walking around the bar, playing a cabinet's game, or at one of
/// the tables: pool, air hockey or shuffleboard.
#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Mode {
    #[default]
    Loading,
    Walking,
    Playing,
    Pool,
    Hockey,
    Shuffleboard,
}

/// The bar's map, made with the editor, while it loads.
#[derive(Resource)]
struct LoadingMap(Handle<Map>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2d);
    // The Worker serves the map last saved from the web editor, or the one built with the site.
    commands.insert_resource(LoadingMap(asset_server.load("maps/bar.ron")));
}

/// Once the map is here, draws the bar and puts the player in it. A map that fails to load is
/// logged by the asset server, and the bar stays empty.
fn build_bar(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    loading: Res<LoadingMap>,
    maps: Res<Assets<Map>>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let Some(map) = maps.get(&loading.0) else {
        return;
    };
    for placed in &map.floor {
        commands.spawn(map_sprite(&asset_server, placed, false));
    }
    for placed in &map.objects {
        commands.spawn(map_sprite(&asset_server, placed, true));
    }
    let walkable = Walkable::from_map(map);
    spawn_player(&mut commands, &asset_server, &walkable);
    commands.insert_resource(walkable);
    commands.insert_resource(Cabinets::from_map(map));
    commands.insert_resource(PoolTables::from_map(map));
    commands.insert_resource(HockeyTables::from_map(map));
    commands.insert_resource(ShuffleboardTables::from_map(map));
    #[cfg(feature = "test-hooks")]
    commands.insert_resource(testing::Objects(map.objects.clone()));
    commands.remove_resource::<LoadingMap>();
    mode.set(Mode::Walking);
}
