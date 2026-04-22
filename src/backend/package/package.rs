use crate::ARCHITECTURES;
use crate::backend::SETTINGS;
use crate::backend::commit::CommitInfoTrait;
use crate::backend::package::PackageBase;
use crate::serialize::RwLockWrapper;
use crate::{
    commit::CommitInfo,
    package::{Package, PackageBuildStatus},
};
use serde_json::{Map, Value};
use std::{sync::Arc, thread};

impl PackageBase for Package {
    fn from_map(
        map: &Map<String, Value>,
        path: String,
        commit: &Arc<CommitInfo>,
    ) -> Option<Arc<Self>> {
        // extract architecture between the first and second dot in the path
        Some(Arc::new(Package {
            description: map.get("description")?.as_str()?.to_string(),
            name: map.get("name")?.as_str()?.to_string(),
            pkg_type: map.get("type")?.as_str()?.to_string(),
            arch: {
                || {
                    let s = &path[path.find('.')? + 1..];
                    //println!("Extracting architecture from path segment: {}", s);
                    ARCHITECTURES.into_iter().find(|&a| s.starts_with(a))
                }
            }()
            .unwrap_or("unknown"),
            flake_url: format!("{}#{}", commit.flake_url, path),
            path,
            commit: commit.clone(),
            status: RwLockWrapper::new(PackageBuildStatus::Idle),
        }))
    }

    fn build(self: Arc<Self>) {
        thread::spawn(move || {
            // skip packages not matching supported architectures
            *self.status.0.write().unwrap() = PackageBuildStatus::Building;
            let mut arch_supported = false;
            for arch in self.commit.repo.settings.supported_architectures.iter() {
                if self.arch == arch {
                    arch_supported = true;
                    break;
                }
            }

            if !arch_supported {
                println!("SKIP\t{} unsupported arch: {}", self.flake_url, self.arch);
                *self.status.0.write().unwrap() =
                    PackageBuildStatus::UnsupportedArchitecture(self.arch);
                return;
            }

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
