use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::{commit, package::PackageEnum, repo::RepoInfo, serialize::RwLockWrapper};

#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize))]
#[derive(Debug)]

pub struct CommitInfo {
    pub hash: String,
    pub message: String,
    pub flake_url: String,
    pub status: RwLockWrapper<CommitBuildStatus>,

    pub packages: RwLockWrapper<Vec<PackageEnum>>,

    #[cfg(not(target_arch = "wasm32"))]
    #[serde(skip)]
    pub repo: Arc<RepoInfo>,

    pub unix_secs: i64,
}

unsafe impl Send for CommitInfo {}
unsafe impl Sync for CommitInfo {}


#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize))]
#[derive(Debug)]
pub enum RepoStatus {
    Cloning,
    Opening,
    Idle,
    Pulling,
    Polling,
}


#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize))]
#[derive(Debug)]
pub enum CommitBuildStatus {
    Idle,
    GettingPackages,
}
