//! UniFFI bindgen CLI wrapper.
//!
//! UniFFI 0.28 exposes the `uniffi-bindgen` CLI as `uniffi::uniffi_bindgen_main()`
//! behind the `cli` feature. This binary just forwards to it so we can build a
//! version-matched bindgen locally instead of relying on a crates.io binary
//! (which does not exist for 0.28).

fn main() {
    uniffi::uniffi_bindgen_main();
}
