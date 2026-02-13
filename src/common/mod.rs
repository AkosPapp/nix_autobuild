pub mod commit;
pub mod macros;
pub mod package;
pub mod repo;
pub mod serialize;

// Re-export dependencies needed by macros
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub use paste;
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub use regex;

use serde::{Deserialize, Serialize};
use serde_nixos::NixosType;
use std::{path::PathBuf, sync::Arc};

// Import macro exported at crate root
use crate::{generate_nixos_module, repo::RepoInfo, serialize::VecArcWrapper};

#[derive(Deserialize, Serialize, Clone, Debug, NixosType)]
pub struct Repo {
    #[nixos(description = "Repository URL", example = "\"github.com/org/repo\"")]
    pub url: String,

    #[nixos(
        description = "Polling interval in seconds to check for updates",
        default = "300"
    )]
    pub poll_interval_sec: u64,

    #[nixos(
        description = "Branches to monitor. If empty or not set, all branches are monitored.",
        default = "[]",
        example = "[\"main\" \"dev\"]"
    )]
    pub branches: Vec<String>,

    #[nixos(
        description = "How many commints to build from the tip of each branch",
        default = "1"
    )]
    pub build_depth: u8,
}

#[cfg(not(target_arch = "wasm32"))]
generate_nixos_module!(AutoBuildOptions);

#[derive(Deserialize, Serialize, Clone, Debug, NixosType)]

pub struct AutoBuildOptions {
    #[nixos(description = "List of repositories to monitor", default = "[]")]
    pub repos: Vec<Repo>,

    #[nixos(
        description = "Directory used to checkout repositories",
        default = "\"/var/lib/nix_autobuild\""
    )]
    pub dir: PathBuf,

    #[nixos(
        description = "List of supported Nix build architectures (e.g. x86_64-linux)",
        default = "[]",
        example = "[\"x86_64-linux\" \"aarch64-linux\"]"
    )]
    pub supported_architectures: Vec<String>,

    #[nixos(
        description = "Host address for the server to bind to",
        default = "\"127.0.0.1\""
    )]
    pub host: String,

    #[nixos(
        description = "Port for the server to bind to",
        default = "8080"
    )]
    pub port: u16,

    #[nixos(
        description = "Number of threads to use for building. If 0, uses the number of CPU cores.",
        default = "0"
    )]
    pub n_build_threads: usize,
}

pub const ARCHITECTURES: [&str; 24] = [
    "aarch64-darwin",
    "aarch64-linux",
    "armv5tel-linux",
    "armv6l-linux",
    "armv7a-linux",
    "armv7l-linux",
    "i686-linux",
    "loongarch64-linux",
    "m68k-linux",
    "microblazeel-linux",
    "microblaze-linux",
    "mips64el-linux",
    "mips64-linux",
    "mipsel-linux",
    "mips-linux",
    "powerpc64le-linux",
    "powerpc64-linux",
    "powerpc-linux",
    "riscv32-linux",
    "riscv64-linux",
    "s390-linux",
    "s390x-linux",
    "x86_64-darwin",
    "x86_64-linux",
];

#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize))]
#[derive(Debug)]
pub struct RepoList(pub VecArcWrapper<RepoInfo>);
