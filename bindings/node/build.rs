//! NAPI-RS build setup for the genATP Node.js native addon.
//!
//! Generates the `binding.gyp`-like NAPI configuration so `@napi-rs/cli`
//! can produce correct `.node` binaries for each platform.

extern crate napi_build;

fn main() {
    napi_build::setup();
}
