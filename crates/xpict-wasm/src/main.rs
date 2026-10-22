//! Emscripten executable wrapper so rustc/em++ emit a main module (JS + wasm).
//! An empty `main` would DCE the C ABI; mention the exports so they stay linked.

fn main() {
    let _keep = (
        xpict_wasm::xpict_render as *const (),
        xpict_wasm::xpict_scene_to_svg as *const (),
        xpict_wasm::xpict_string_free as *const (),
    );
    std::hint::black_box(_keep);
}
