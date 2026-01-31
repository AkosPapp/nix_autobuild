
#[cfg(target_arch = "wasm32")]
pub mod frontend;

#[cfg(not(target_arch = "wasm32"))]
pub mod backend;

use std::{collections::HashMap, path::PathBuf, sync::Arc, thread};

pub mod common;
pub use common::*;
