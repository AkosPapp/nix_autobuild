use crate::backend::package::PackageBase;
use crate::serialize::RwLockWrapper;
use crate::{
    commit::CommitInfo,
    package::{NixosConfigPackage, PackageBuildStatus},
};
use serde_json::{Map, Value};
use std::{sync::Arc, thread};

impl PackageBase for NixosConfigPackage {
    fn from_map(
        map: &Map<String, Value>,
        path: String,
        commit: &Arc<CommitInfo>,
    ) -> Option<Arc<Self>> {
        // extract architecture between the first and second dot in the path
        if !path.starts_with("nixosConfigurations") {
            return None;
        }

        let pkg_type = map.get("type")?.as_str()?;
        if pkg_type != "nixos-configuration" {
            return None;
        }

        let path = format!("{}.config.system.build.toplevel", path);
        Some(Arc::new(NixosConfigPackage {
            pkg_type: pkg_type.to_string(),
            flake_url: format!("{}#{}", commit.flake_url, path),
            path,
            status: RwLockWrapper::new(PackageBuildStatus::Idle),
        }))
    }

    fn build(self: Arc<Self>) {
        thread::spawn(move || {
            *self.status.0.write().unwrap() = PackageBuildStatus::Building;

            match Self::build_static(self.flake_url.as_str(), &self.status) {
                Ok(path) => {
                    *self.status.0.write().unwrap() = PackageBuildStatus::Success(path);
                }
                Err(e) => {
                    *self.status.0.write().unwrap() = PackageBuildStatus::Failed(e.to_string());
                }
            };
        });
    }
}
