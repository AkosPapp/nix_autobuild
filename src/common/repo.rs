#[cfg(target_arch = "wasm32")]
use serde::Deserialize;

#[cfg(not(target_arch = "wasm32"))]
use crate::AutoBuildOptions;
#[cfg(not(target_arch = "wasm32"))]
use serde::Serialize;
// #[cfg(not(target_arch = "wasm32"))]
// std::sync::Arc;

use crate::Repo;
use crate::serialize::RwLockHashMapArc;
use crate::{
    common::commit::{CommitInfo, RepoStatus},
    serialize::RwLockWrapper,
};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

unsafe impl Send for RepoStatus {}
unsafe impl Sync for RepoStatus {}

#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize))]
#[derive(Debug)]

pub struct RepoInfo {
    pub flake_url: String,
    pub repo: Repo,
    pub checkout_path: PathBuf,
    pub branch_commit_hashes: HashMap<String, RwLockWrapper<Vec<String>>>,

    pub commits: RwLockHashMapArc<CommitInfo>,

    pub status: RwLockWrapper<RepoStatus>,

    #[cfg(not(target_arch = "wasm32"))]
    #[serde(skip)]
    pub settings: Arc<AutoBuildOptions>,

    #[cfg(not(target_arch = "wasm32"))]
    #[serde(skip)]
    pub credentials: Option<String>,
}
