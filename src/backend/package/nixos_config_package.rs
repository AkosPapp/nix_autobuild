use crate::backend::SETTINGS;
use crate::backend::commit::CommitInfoTrait;
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
            commit: commit.clone(),
        }))
    }

    fn build(self: Arc<Self>) {
        thread::spawn(move || {
            *self.status.0.write().unwrap() = PackageBuildStatus::Building;

            let settings = SETTINGS.get().unwrap();

            let link_path = if settings.enable_gcroots {
                // <build_dir>/build/<repo_name>/<commit_hash>/<package_path>
                let link_path = self
                    .commit
                    .get_build_dir()
                    .join(self.path.replace("/", "_"));
                Some(link_path)
            } else {
                None
            };

            match Self::build_static(self.flake_url.as_str(), &self.status, link_path) {
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
