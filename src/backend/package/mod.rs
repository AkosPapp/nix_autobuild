pub mod nixos_config_package;
pub mod package;

use crate::backend::SETTINGS;
use crate::backend::semaphore::Semaphore;
use crate::serialize::RwLockWrapper;
use crate::{
    commit::CommitInfo,
    package::{PackageBuildStatus, PackageEnum},
};
use serde_json::{Map, Value};
use std::sync::Arc;

pub trait PackageEnumTrait {
    fn build(&self);
}

impl PackageEnumTrait for PackageEnum {
    fn build(&self) {
        match self {
            PackageEnum::Derivation(pkg) => {
                pkg.inner().clone().build();
            }
            PackageEnum::NixosConfig(pkg) => {
                pkg.inner().clone().build();
            }
        }
    }
}

pub trait PackageBase: Send + Sync {
    fn from_map(
        map: &Map<String, Value>,
        path: String,
        commit: &Arc<CommitInfo>,
    ) -> Option<Arc<Self>>
    where
        Self: Sized;

    fn build(self: Arc<Self>);

    fn build_static(
        flake_pkg_url: &str,
        status: &RwLockWrapper<PackageBuildStatus>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        status
            .0
            .write()
            .unwrap()
            .clone_from(&PackageBuildStatus::WaitingForBuild);
        Semaphore::get_sem().execute(|| {
            status
                .0
                .write()
                .unwrap()
                .clone_from(&PackageBuildStatus::Building);
            println!("BUILD\t{}", flake_pkg_url);
            let mut cmd = std::process::Command::new("nix");
            cmd.arg("build").arg("--no-link").arg("--print-out-paths");

            if let Some(settings) = SETTINGS.get() {
                if !settings.builders.is_empty() {
                    cmd.arg("--builders").arg(settings.builders.join(","));
                }
            }

            let output = cmd.arg(&flake_pkg_url).output()?;

            if output.status.code().unwrap_or(-1) != 0 {
                let build_error = String::from_utf8_lossy(&output.stderr);
                println!("ERROR\t{} -> {}", flake_pkg_url, build_error);
                return Err(build_error.into());
            }

            let build_output = String::from_utf8_lossy(&output.stdout);
            let build_output = build_output.trim();
            println!("RESULT\t{} -> {}", flake_pkg_url, build_output);
            Ok(build_output.to_string())
        })
    }
}
