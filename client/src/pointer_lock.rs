//! Locking the mouse pointer to the canvas, for the games played by moving the mouse (the air
//! hockey table, the dartboard): it can't wander off the window, and its moves keep coming. The
//! page does the locking (web/index.html): once a game wants it, a click on the canvas locks
//! the pointer, and Esc lets it go.

use wasm_bindgen::prelude::*;

// Defined in index.html.
#[wasm_bindgen]
extern "C" {
    /// Whether a click on the canvas should lock the pointer (and, turned off, lets it go).
    #[wasm_bindgen(js_name = pointerLockWanted)]
    pub fn wanted(on: bool);
    #[wasm_bindgen(js_name = pointerLocked)]
    pub fn locked() -> bool;
    /// This browser won't lock the pointer.
    #[wasm_bindgen(js_name = pointerLockFailed)]
    pub fn failed() -> bool;
    /// Lets the locked pointer go.
    #[wasm_bindgen(js_name = releasePointer)]
    pub fn release();
}
