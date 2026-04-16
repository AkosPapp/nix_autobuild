#[cfg(target_arch = "wasm32")]
pub mod frontend;

#[cfg(not(target_arch = "wasm32"))]
pub mod backend;

pub mod common;
pub use common::*;
