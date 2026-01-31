use std::sync::Arc;

use crate::commit::CommitInfo;
use crate::serialize::{ArcWrapper, RwLockWrapper};
use serde::{Deserialize, Serialize};

#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize, Clone))]
#[derive(Debug)]

pub enum PackageEnum {
    Derivation(ArcWrapper<Package>),

    NixosConfig(ArcWrapper<NixosConfigPackage>),
}

unsafe impl Send for PackageEnum {}
unsafe impl Sync for PackageEnum {}

#[cfg_attr(target_arch = "wasm32", derive(Deserialize, Clone))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize, Clone))]
#[derive(Debug)]
pub enum PackageBuildStatus {
    Idle,
    Building,
    #[cfg(target_arch = "wasm32")]
    UnsupportedArchitecture(String),
    #[cfg(not(target_arch = "wasm32"))]
    UnsupportedArchitecture(&'static str),
    Success(String),
    Failed(String),
}

unsafe impl Send for PackageBuildStatus {}
unsafe impl Sync for PackageBuildStatus {}

#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize))]
#[derive(Debug)]
pub struct Package {
    pub description: String,
    pub name: String,
    pub pkg_type: String,
    pub path: String,

    #[cfg(target_arch = "wasm32")]
    pub arch: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub arch: &'static str,

    pub flake_url: String,
    pub status: RwLockWrapper<PackageBuildStatus>,

    #[cfg(not(target_arch = "wasm32"))]
    #[serde(skip)]
    pub commit: Arc<CommitInfo>,
}
impl Package {
    pub fn get_no_arch_name(&self) -> String {
        self.path.replace(&format!("{}", self.arch), "*")
    }
}

unsafe impl Send for Package {}
unsafe impl Sync for Package {}

#[cfg_attr(target_arch = "wasm32", derive(Deserialize))]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize))]
#[derive(Debug)]
pub struct NixosConfigPackage {
    pub path: String,
    pub pkg_type: String,
    pub flake_url: String,
    pub status: RwLockWrapper<PackageBuildStatus>,
}

unsafe impl Send for NixosConfigPackage {}
unsafe impl Sync for NixosConfigPackage {}
