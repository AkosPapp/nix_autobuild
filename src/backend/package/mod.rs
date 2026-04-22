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
use std::path::PathBuf;
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
        link_path: Option<PathBuf>,
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
            cmd.arg("build").arg("--print-out-paths");

            if let Some(link_path) = link_path {
                if let Some(parent) = link_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                cmd.arg("--out-link").arg(link_path);
            } else {
                cmd.arg("--no-link");
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

            rayon::spawn({
                let build_output = build_output.to_string();
                move || {
                    let settings = SETTINGS.get().unwrap();
                    settings.copy_to.iter().for_each(|dest| {
                        println!("COPY\t{} -> {}", build_output, dest);
                        let mut cmd = std::process::Command::new("nix");
                        cmd.arg("copy")
                            .arg("--no-check-sigs")
                            .arg("--to")
                            .arg(dest)
                            .arg(&build_output);

                        if settings.copy_to_substitute_on_destination {
                            cmd.arg("--substitute-on-destination");
                        }

                        let output = cmd.output().expect("Failed to execute nix copy command");
                        if output.status.code().unwrap_or(-1) != 0 {
                            let copy_error = String::from_utf8_lossy(&output.stderr);
                            println!("COPY ERROR\t{} -> {}: {}", build_output, dest, copy_error);
                        } else {
                            println!("COPY DONE\t{} -> {}", build_output, dest);
                        }
                    });
                }
            });

            Ok(build_output.to_string())
        })
    }
}
