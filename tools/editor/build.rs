//! The palette for the web build, which can't list assets/tiles/ itself: the tile names, as
//! `tiles_in` would list them, in `$OUT_DIR/tiles.rs` (tools/editor/src/store.rs).

use std::path::Path;
use std::{env, fs};

fn main() {
    let tiles = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../assets/tiles");
    let mut names = Vec::new();
    for folder in ["floor", "objects"] {
        println!("cargo:rerun-if-changed={}", tiles.join(folder).display());
        let mut in_folder: Vec<String> = fs::read_dir(tiles.join(folder))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_stem()?.to_string_lossy().into_owned();
                (path.extension()? == "png").then(|| format!("{folder}/{name}"))
            })
            .collect();
        in_folder.sort();
        names.extend(in_folder);
    }
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("tiles.rs");
    fs::write(out, format!("const TILES: &[&str] = &{names:?};\n")).unwrap();
}
