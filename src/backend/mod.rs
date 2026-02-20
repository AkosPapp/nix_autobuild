extern crate git2;
extern crate serde;
extern crate serde_json;
extern crate serde_nixos;
use crate::serialize::RwLockWrapper;
use crate::{ARCHITECTURES, AutoBuildOptions, Repo, RepoList, repo::RepoInfo};
use crate::{
    commit::{CommitBuildStatus, CommitInfo, RepoStatus},
    package::{NixosConfigPackage, Package, PackageBuildStatus, PackageEnum},
    serialize::{RwLockHashMapArc, VecArcWrapper},
};
use actix_web::{App, HttpResponse, HttpServer, Responder, get};
use git2::{Commit, Repository};
use rayon::prelude::*;
use serde_json::{Map, Value};
use std::mem::MaybeUninit;
use std::os::linux::raw::stat;
use std::sync::{Condvar, Mutex, RwLock};
use std::{collections::HashMap, env::args, path::PathBuf, sync::Arc, thread};

const FRONTEND_PATH: &str = match option_env!("FRONTEND_PATH") {
    Some(path) => path,
    None => "/workspaces/nix_autobuild/result/dist",
};

static mut SEM: MaybeUninit<Semaphore> = MaybeUninit::uninit();
/// A simple semaphore implementation using Mutex and Condvar
pub struct Semaphore {
    count: Mutex<usize>,
    condvar: Condvar,
}

impl Semaphore {
    #[allow(static_mut_refs)]
    pub fn init(count: usize) {
        unsafe {
            SEM.write(Semaphore {
                count: Mutex::new(count),
                condvar: Condvar::new(),
            });
        }
    }

    #[allow(static_mut_refs)]
    pub fn get_sem() -> &'static Self {
        unsafe { SEM.assume_init_ref() }
    }

    fn acquire(&self) {
        let mut count = self.count.lock().unwrap();
        while *count == 0 {
            count = self.condvar.wait(count).unwrap();
        }
        *count -= 1;
    }

    fn release(&self) {
        let mut count = self.count.lock().unwrap();
        *count += 1;
        self.condvar.notify_one();
    }

    pub fn execute<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.acquire();
        let result = f();
        self.release();
        result
    }
}

pub trait RepoInfoTrait {
    fn new(repo: Repo, checkout_path: PathBuf, settings: Arc<AutoBuildOptions>) -> Arc<RepoInfo>;

    fn clone_repo(&self) -> Result<git2::Repository, git2::Error>;

    fn clone_or_open(&self) -> Result<git2::Repository, git2::Error>;
    fn pull(&self, repository: &Repository) -> Result<bool, git2::Error>;

    fn thread_poll(self: Arc<Self>);

    fn parse_commit_parents<'repo>(
        self: &Arc<Self>,
        commit: &Commit<'repo>,
        depth: u8,
        commits: &mut Vec<Arc<CommitInfo>>,
    );

    fn get_or_create_commit<'repo>(self: &Arc<Self>, commit: &Commit<'repo>) -> Arc<CommitInfo>;

    fn thread_loop(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error>>;

    fn delete_repo(&self) -> Result<(), Box<dyn std::error::Error>>;
}

impl RepoInfoTrait for RepoInfo {
    fn new(repo: Repo, checkout_path: PathBuf, settings: Arc<AutoBuildOptions>) -> Arc<RepoInfo> {
        let mut branch_commit_hashes = HashMap::new();
        for branch in &repo.branches {
            branch_commit_hashes.insert(branch.clone(), RwLockWrapper::new(Vec::new()));
        }
        let credentials = if let Some(credentials_file) = &repo.credentials_file {
            match std::fs::read_to_string(credentials_file) {
                Ok(creds) => Some(creds.trim().to_string()),
                Err(e) => {
                    println!("ERROR reading credentials file {}: {}", credentials_file, e);
                    None
                }
            }
        } else {
            None
        };
        Arc::new(RepoInfo {
            flake_url: format!("git+https://{}", repo.url),
            repo,
            checkout_path,
            branch_commit_hashes: branch_commit_hashes,
            commits: RwLockHashMapArc::new(RwLock::new(HashMap::new())),
            status: RwLockWrapper::new(RepoStatus::Idle),
            credentials,
            settings,
        })
    }

    fn clone_repo(&self) -> Result<git2::Repository, git2::Error> {
        *self.status.0.write().unwrap() = RepoStatus::Cloning;
        println!("CLONE\t{}", format!("https://{}", &self.repo.url));

        let clone_url = if let Some(credentials) = &self.credentials {
            format!("https://{}@{}", credentials, &self.repo.url)
        } else {
            format!("https://{}", &self.repo.url)
        };
        let res = Repository::clone(clone_url.as_str(), &self.checkout_path);

        *self.status.0.write().unwrap() = RepoStatus::Idle;

        match &res {
            Ok(_) => println!("CLONE DONE\t{}", self.checkout_path.display()),
            Err(e) => println!("CLONE ERROR\t{}: {}", self.checkout_path.display(), e),
        };

        res
    }

    fn clone_or_open(&self) -> Result<git2::Repository, git2::Error> {
        *self.status.0.write().unwrap() = RepoStatus::Opening;
        println!("OPEN\t{}", self.checkout_path.display());
        let res = match Repository::open(&self.checkout_path) {
            Ok(repo) => Ok(repo),
            Err(_) => self.clone_repo(),
        };
        *self.status.0.write().unwrap() = RepoStatus::Idle;
        match &res {
            Ok(_) => println!("OPENED\t{}", self.checkout_path.display()),
            Err(e) => println!("OPEN ERROR\t{}: {}", self.checkout_path.display(), e),
        };
        res
    }

    fn pull(&self, repository: &Repository) -> Result<bool, git2::Error> {
        *self.status.0.write().unwrap() = RepoStatus::Pulling;
        println!("PULL\t{}", self.checkout_path.display());
        let mut remote = repository.find_remote("origin")?;
        let mut fetch_options = git2::FetchOptions::new();
        let before_refs = repository
            .references()?
            .filter_map(|r| r.ok())
            .filter_map(|r| r.target().map(|t| (r.name().unwrap_or("").to_string(), t)))
            .collect::<std::collections::HashMap<_, _>>();

        remote
            .fetch(&self.repo.branches, Some(&mut fetch_options), None)
            .map_err(|err| {
                eprintln!("PULL ERROR\t{}: {}", self.checkout_path.display(), err);
                err
            })?;

        let after_refs = repository
            .references()
            .map_err(|err| {
                eprintln!("PULL ERROR\t{}: {}", self.checkout_path.display(), err);
                err
            })?
            .filter_map(|r| r.ok())
            .filter_map(|r| r.target().map(|t| (r.name().unwrap_or("").to_string(), t)))
            .collect::<std::collections::HashMap<_, _>>();

        let has_changes = before_refs != after_refs;
        *self.status.0.write().unwrap() = RepoStatus::Idle;

        match has_changes {
            true => println!("PULL DONE\t{}", self.checkout_path.display()),
            false => println!("PULL NO CHANGES\t{}", self.checkout_path.display()),
        }
        Ok(has_changes)
    }

    fn thread_poll(self: Arc<RepoInfo>) {
        loop {
            if let Err(e) = self.clone().thread_loop() {
                println!("ERROR in repo {}: {}", self.checkout_path.display(), e);
            }
            if let Err(e) = self.delete_repo() {
                println!("ERROR deleting {}: {}", self.checkout_path.display(), e);
            }
        }
    }

    fn parse_commit_parents<'repo>(
        self: &Arc<RepoInfo>,
        commit: &Commit<'repo>,
        depth: u8,
        commits: &mut Vec<Arc<CommitInfo>>,
    ) {
        if depth == 0 {
            return;
        }
        for commit in commit.parents() {
            commits.push(self.get_or_create_commit(&commit));
        }
        for commit in commit.parents() {
            self.parse_commit_parents(&commit, depth - 1, commits);
        }
    }

    fn get_or_create_commit<'repo>(
        self: &Arc<RepoInfo>,
        commit: &Commit<'repo>,
    ) -> Arc<CommitInfo> {
        let mut commits = self.commits.inner().write().unwrap();
        if let Some(commit_info) = commits.get(&commit.id().to_string()) {
            return commit_info.clone();
        }
        let commit = CommitInfo::new(self.clone(), &commit);
        commits.insert(commit.hash.clone(), commit.clone());
        drop(commits);
        commit.clone().build();
        commit
    }

    fn thread_loop(self: Arc<RepoInfo>) -> Result<(), Box<dyn std::error::Error>> {
        // clone repo if not exists
        let repo = self.clone_or_open().map_err(|err| {
            eprintln!(
                "ERROR cloning or opening repo {}: {}",
                self.checkout_path.display(),
                err
            );
            err
        })?;

        loop {
            println!("POLL\t{}", self.checkout_path.display());
            *self.status.0.write().unwrap() = RepoStatus::Polling;

            repo.branches(Some(git2::BranchType::Remote))
                .map_err(|err| {
                    eprintln!(
                        "ERROR listing branches for repo {}: {}",
                        self.checkout_path.display(),
                        err
                    );
                    err
                })?
                .for_each(|branch_result| {
                    let Ok((branch, _)) = branch_result else {
                        return;
                    };
                    let Ok(Some(branch_name)) = branch.name() else {
                        return;
                    };
                    let branch_name = branch_name.replace("origin/", "");

                    if !self.repo.branches.contains(&branch_name) {
                        return;
                    }

                    if let Ok(name) = branch.name() {
                        if let Some(name_str) = name {
                            println!("Found branch: {}", name_str);
                        }
                    }

                    let commit = branch.get().peel_to_commit().expect("no commit on branch");
                    let mut commits: Vec<Arc<CommitInfo>> = Vec::new();
                    // Add the current commit first
                    commits.push(self.get_or_create_commit(&commit));
                    // Then add parent commits up to build_depth - 1
                    self.parse_commit_parents(
                        &commit,
                        self.repo.build_depth.saturating_sub(1),
                        &mut commits,
                    );

                    *self
                        .branch_commit_hashes
                        .get(&branch_name)
                        .unwrap()
                        .0
                        .write()
                        .unwrap() = commits.iter().map(|c| c.hash.clone()).collect();
                });

            // sleep for poll interval
            while !self.pull(&repo)? {
                *self.status.0.write().unwrap() = RepoStatus::Idle;
                thread::sleep(std::time::Duration::from_secs(self.repo.poll_interval_sec));
            }
        }
    }

    fn delete_repo(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("DELETE\t{}", self.checkout_path.display());
        let output = std::process::Command::new("rm")
            .arg("-rf")
            .arg(&self.checkout_path)
            .output()?;
        if output.status.code().unwrap_or(-1) != 0 {
            let delete_error = String::from_utf8_lossy(&output.stderr);
            println!(
                "ERROR deleting {} -> {}",
                self.checkout_path.display(),
                delete_error
            );
            return Err("Failed to delete repository".into());
        }
        println!("DELETED\t{}", self.checkout_path.display());
        Ok(())
    }
}

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
            let output = std::process::Command::new("nix")
                .arg("build")
                .arg("--no-link")
                .arg("--print-out-paths")
                .arg(&flake_pkg_url)
                .output()?;

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

static mut BUILD_REPOS: RepoList = RepoList(VecArcWrapper(Vec::new()));

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = args().nth(1).ok_or("No config Path Specified")?;
    let settings = {
        let config_data = std::fs::read_to_string(&config_path)?;
        Arc::new(serde_json::from_str::<AutoBuildOptions>(&config_data)?)
    };

    let build_pool_size = if settings.n_build_threads == 0 {
        num_cpus::get() as usize
    } else {
        settings.n_build_threads as usize
    };

    let build_sem = Arc::new(Semaphore::init(build_pool_size as usize));

    let repo_dir = settings.dir.join("repos");

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
            .service(repos)
            .service(nix_store_files)
            .service(store_files)
            .service(static_files)
    })
    .bind((settings.host.as_str(), settings.port))?
    .run()
    .await?;

    Ok(())
}

#[allow(static_mut_refs)]
#[get("/repos")]
async fn repos() -> impl Responder {
    println!("INFO\tRequested repo info");
    let json = serde_json::to_string_pretty(unsafe { &BUILD_REPOS }).unwrap();
    // Append Access-Control-Allow-Origin for browsers during development.
    HttpResponse::Ok().body(json)
}

async fn server_nix_file(path: String) -> actix_web::Result<HttpResponse> {
    println!("INFO\tRequested nix file: {}", path);

    let metadata = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(_) => return Err(actix_web::error::ErrorNotFound("404 Not Found")),
    };

    if metadata.is_file() {
        match std::fs::read(&path) {
            Ok(contents) => Ok(HttpResponse::Ok().body(contents)),
            Err(_) => Err(actix_web::error::ErrorNotFound("404 Not Found")),
        }
    } else if metadata.is_dir() {
        match std::fs::read_dir(&path) {
            Ok(entries) => {
                let mut listing = String::from("<html><body><h1>Directory listing</h1><ul>");
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let suffix = if is_dir { "/" } else { "" };
                        listing.push_str(&format!(
                            "<li><a href=\"{}/{}{}\">{}{}</a></li>",
                            path, name, suffix, name, suffix
                        ));
                    }
                }
                listing.push_str("</ul></body></html>");
                Ok(HttpResponse::Ok().content_type("text/html").body(listing))
            }
            Err(_) => Err(actix_web::error::ErrorInternalServerError(
                "Failed to read directory",
            )),
        }
    } else {
        Err(actix_web::error::ErrorNotFound("404 Not Found"))
    }
}

#[get("/nix/store{path:.*}")]
// serve file if available or list the nix store directory
async fn nix_store_files(path: actix_web::web::Path<String>) -> actix_web::Result<HttpResponse> {
    let full_path = format!("/nix/store{}", path.into_inner());
    server_nix_file(full_path).await
}

#[get("/store{path:.*}")]
// serve file if available or list the nix store directory
async fn store_files(path: actix_web::web::Path<String>) -> actix_web::Result<HttpResponse> {
    let full_path = format!("/nix/store{}", path.into_inner());
    server_nix_file(full_path).await
}

#[get("/{path:.*}")]
async fn static_files(
    path: actix_web::web::Path<String>,
) -> actix_web::Result<actix_files::NamedFile> {
    let file_path = if path.is_empty() {
        "index.html".to_string()
    } else {
        path.into_inner()
    };
    println!("INFO\tRequested static file: {}", file_path);
    // TODO: Sanitize file_path to prevent directory traversal attacks

    let full_path = format!("{}/{}", FRONTEND_PATH, file_path);
    println!("INFO\tFull static file path: {}", full_path);
    match actix_files::NamedFile::open_async(full_path).await {
        Ok(named_file) => Ok(named_file.use_last_modified(true)),
        Err(_) => Err(actix_web::error::ErrorNotFound("404 Not Found")),
    }
}
