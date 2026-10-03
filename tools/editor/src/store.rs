//! Where the palette comes from and where saves go: the repo's files on the desktop, the
//! editor Worker on the web. The map itself loads through the asset server on both.

use crossbeam_channel::Sender;
use world::Map;

use crate::{ASSETS, MAP_FILE};

/// How a save went, as the status line shows it.
pub type Saved = Result<String, String>;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{save, tiles_in};
#[cfg(target_arch = "wasm32")]
pub use web::{save, tiles_in};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fs;

    use super::*;

    /// Tile names ("floor/wood") for the PNGs in assets/tiles/<folder>.
    pub fn tiles_in(folder: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(format!("{ASSETS}/tiles/{folder}"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_stem()?.to_string_lossy().into_owned();
                (path.extension()? == "png").then(|| format!("{folder}/{name}"))
            })
            .collect();
        names.sort();
        names
    }

    /// Writes assets/maps/bar.ron.
    pub fn save(map: &Map, done: Sender<Saved>) {
        let result = fs::create_dir_all(format!("{ASSETS}/maps"))
            .and_then(|()| fs::write(format!("{ASSETS}/{MAP_FILE}"), map.to_ron()));
        let _ = done.send(
            result
                .map(|()| format!("Saved {MAP_FILE}"))
                .map_err(|error| format!("Could not save: {error}")),
        );
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;

    // The tiles there were when this was built (tools/editor/build.rs).
    include!(concat!(env!("OUT_DIR"), "/tiles.rs"));

    /// Tile names ("floor/wood") in assets/tiles/<folder>.
    pub fn tiles_in(folder: &str) -> Vec<String> {
        TILES
            .iter()
            .filter(|tile| tile.starts_with(&format!("{folder}/")))
            .map(|tile| tile.to_string())
            .collect()
    }

    /// Sends the map to the Worker, which keeps it for the bar (server/src/lib.rs, `save_map`).
    pub fn save(map: &Map, done: Sender<Saved>) {
        let url = format!("{ASSETS}/{MAP_FILE}");
        let request = ehttp::Request::put(url, map.to_ron().into_bytes());
        ehttp::fetch(request, move |response| {
            let result = match response {
                // A Cloudflare Access session that ran out ends up on its login page instead.
                Ok(response) if response.ok && response.url.ends_with(MAP_FILE) => {
                    Ok("Saved; the bar shows it from its next load".into())
                }
                Ok(response) => Err(format!(
                    "Could not save: {} {}",
                    response.status,
                    response.text().unwrap_or(&response.status_text)
                )),
                Err(error) => Err(format!("Could not save: {error}")),
            };
            let _ = done.send(result);
        });
    }
}
