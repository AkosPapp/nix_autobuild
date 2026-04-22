use crate::backend::SETTINGS;
use crate::backend::package::{PackageBase, PackageEnumTrait};
use crate::backend::semaphore::Semaphore;
use crate::repo::RepoInfo;
use crate::serialize::RwLockWrapper;
use crate::{
    commit::{CommitBuildStatus, CommitInfo},
    package::{NixosConfigPackage, Package, PackageEnum},
};
use git2::Commit;
use rayon::prelude::*;
use serde_json::{Map, Value};
use std::{sync::Arc, thread};
pub trait CommitInfoTrait {
    fn new(repo: Arc<RepoInfo>, commit: &Commit) -> Arc<CommitInfo>;

    fn build(self: Arc<Self>);

    fn get_pkgs_list(
        self: &Arc<Self>,
        flake_url: &str,
    ) -> Result<Vec<PackageEnum>, Box<dyn std::error::Error>>;

    fn _parse_pkgs_value(
        map: &Map<String, Value>,
        path: String,
        commit: &Arc<CommitInfo>,
        pkgs: &mut Vec<PackageEnum>,
    );
    fn get_build_dir(&self) -> std::path::PathBuf;
}

impl CommitInfoTrait for CommitInfo {
    fn new(repo: Arc<RepoInfo>, commit: &Commit) -> Arc<CommitInfo> {
        let hash = commit.id().to_string();
        Arc::new(CommitInfo {
            message: commit
                .message()
                .unwrap_or("<no message>")
                .trim()
                .to_string(),
            flake_url: format!("git+https://{}?rev={}", &repo.repo.url, &hash),
            hash,
            packages: RwLockWrapper::new(Vec::new()),
            repo: repo.clone(),
            status: RwLockWrapper::new(CommitBuildStatus::Idle),
            unix_secs: commit.time().seconds(),
        })
    }

    fn build(self: Arc<Self>) {
        thread::spawn(move || {
            *self.status.0.write().unwrap() = CommitBuildStatus::GettingPackages;
            let Ok(pkgs) = self.get_pkgs_list(&self.flake_url) else {
                return;
            };
            {
                let mut pkgs_writer = self.packages.0.write().unwrap();
                pkgs.iter().for_each(|pkg| {
                    pkgs_writer.push(pkg.clone());
                });
            }
            pkgs.par_iter().for_each(|pkg| {
                pkg.build();
            });
            *self.status.0.write().unwrap() = CommitBuildStatus::Idle;
        });
    }

    fn get_pkgs_list(
        self: &Arc<Self>,
        flake_url: &str,
    ) -> Result<Vec<PackageEnum>, Box<dyn std::error::Error>> {
        Semaphore::get_sem().execute(|| {
            *self.status.0.write().unwrap() = CommitBuildStatus::GettingPackages;
            let output = std::process::Command::new("nix")
                .arg("flake")
                .arg("show")
                .arg("--json")
                .arg("--all-systems")
                .arg(flake_url)
                .output()?;
            println!("LIST\t{}", flake_url); // TODO: add error handling

            if output.status.code().unwrap_or(-1) != 0 {
                let list_error = String::from_utf8_lossy(&output.stderr);
                println!("ERROR listing {} -> {}", flake_url, list_error);
                return Err("Failed to list packages in flake".into());
            }

            let pkgs_json = String::from_utf8(output.stdout)?;
            //println!("{}", pkgs_json);

            let pkgs_value: Value = serde_json::from_str(&pkgs_json)?;
            //println!("{:#?}", pkgs_value);

            let Some(pkgs_object) = pkgs_value.as_object() else {
                return Err("No packages found in flake".into());
            };

            let mut pkgs_vec: Vec<PackageEnum> = Vec::new();
            Self::_parse_pkgs_value(pkgs_object, String::new(), &self, &mut pkgs_vec);
            *self.status.0.write().unwrap() = CommitBuildStatus::Idle;
            Ok(pkgs_vec)
        })
    }

    fn _parse_pkgs_value(
        map: &Map<String, Value>,
        path: String,
        commit: &Arc<CommitInfo>,
        pkgs: &mut Vec<PackageEnum>,
    ) {
        if let Some(pkg) = Package::from_map(map, path.clone(), commit) {
            //println!(
            //    "Found package: {} at path: {:#?}",
            //    if path.is_empty() { "<root>" } else { &path },
            //    pkg
            //);
            pkgs.push(PackageEnum::Derivation(pkg.into()));
        } else if let Some(nixos_cfg) = NixosConfigPackage::from_map(map, path.clone(), commit) {
            //println!(
            //    "Found NixOS configuration: {} at path: {:#?}",
            //    if path.is_empty() { "<root>" } else { &path },
            //    nixos_cfg
            //);
            pkgs.push(PackageEnum::NixosConfig(nixos_cfg.into()));
        } else {
            for key in map.keys() {
                if let Some(new_map) = map[key].as_object() {
                    let mut new_path = path.clone();
                    if !new_path.is_empty() {
                        new_path.push('.');
                    }
                    new_path.push_str(key);

                    Self::_parse_pkgs_value(new_map, new_path, commit, pkgs);
                }
            }
        }
    }

    fn get_build_dir(&self) -> std::path::PathBuf {
        let settings = SETTINGS.get().unwrap();
        settings
            .dir
            .join("build")
            .join(
                &self
                    .repo
                    .flake_url
                    .strip_prefix("git+https://")
                    .unwrap()
                    .replace("/", "_")
                    .replace(":", "_"),
            )
            .join(&self.hash)
    }
}

impl Drop for CommitInfo {
    fn drop(&mut self) {
        println!("Dropping commit {}", self.hash);

        let build_dir = self.get_build_dir();
        if build_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&build_dir) {
                println!("Failed to remove build directory {:?}: {}", build_dir, e);
            } else {
                println!("Removed build directory {:?}", build_dir);
            }
        }
    }
}
