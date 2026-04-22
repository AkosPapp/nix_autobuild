extern crate git2;
extern crate serde;
extern crate serde_json;
extern crate serde_nixos;

pub mod api;
pub mod commit;
pub mod package;
pub mod repo;
pub mod semaphore;

use crate::backend::repo::RepoInfoTrait;
use crate::backend::semaphore::Semaphore;
use crate::serialize::VecArcWrapper;
use crate::{AutoBuildOptions, RepoList, repo::RepoInfo};
use actix_web::{App, HttpServer};
use rayon::prelude::*;
use std::{
    env::args,
    sync::{Arc, OnceLock},
    thread,
};

const FRONTEND_PATH: &str = match option_env!("FRONTEND_PATH") {
    Some(path) => path,
    None => "/workspaces/nix_autobuild/dist",
};

static mut BUILD_REPOS: RepoList = RepoList(VecArcWrapper(Vec::new()));
static SETTINGS: OnceLock<Arc<AutoBuildOptions>> = OnceLock::new();

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = args().nth(1).ok_or("No config Path Specified")?;
    let settings = {
        let config_data = std::fs::read_to_string(&config_path)?;
        Arc::new(serde_json::from_str::<AutoBuildOptions>(&config_data)?)
    };

    SETTINGS
        .set(settings.clone())
        .map_err(|_| "SETTINGS already initialized".to_string())?;

    let build_pool_size = if settings.n_build_threads == 0 {
        num_cpus::get() as usize
    } else {
        settings.n_build_threads as usize
    };

    let _build_sem = Arc::new(Semaphore::init(build_pool_size as usize));

    let repo_dir = settings.dir.join("repos");
    let build_dir = settings.dir.join("build");

    if build_dir.exists()  {
        std::fs::remove_dir_all(&build_dir)?;
    }

    let build_repos = RepoList(VecArcWrapper::from(
        settings
            .repos
            .par_iter()
            .map(|repo| {
                let repo_info = RepoInfo::new(
                    repo.clone(),
                    repo_dir.join(&repo.url.replace("/", "_").replace(":", "_")),
                    settings.clone(),
                );
                thread::spawn({
                    let repo_info = repo_info.clone();
                    move || {
                        repo_info.thread_poll();
                    }
                });
                repo_info
            })
            .collect::<Vec<_>>(),
    ));

    // create data directory
    std::fs::create_dir_all(&settings.dir)?;
    std::fs::create_dir_all(&repo_dir)?;

    unsafe {
        BUILD_REPOS = build_repos;
    }

    println!(
        "Starting server on http://{}:{}",
        settings.host, settings.port
    );
    println!("Serving static files from: {}", FRONTEND_PATH);
    HttpServer::new(|| {
        App::new()
            .service(api::repos)
            .service(api::nix_store_files)
            .service(api::store_files)
            .service(api::static_files)
    })
    .bind((settings.host.as_str(), settings.port))?
    .run()
    .await?;

    Ok(())
}
