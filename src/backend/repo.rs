use crate::backend::commit::CommitInfoTrait;
use crate::serialize::RwLockWrapper;
use crate::{AutoBuildOptions, Repo, repo::RepoInfo};
use crate::{
    commit::{CommitInfo, RepoStatus},
    serialize::RwLockHashMapArc,
};
use git2::{Commit, Repository};
use std::sync::RwLock;
use std::{collections::HashMap, path::PathBuf, sync::Arc, thread};

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
        new_commits: &mut HashMap<String, Arc<CommitInfo>>,
    );

    fn get_or_create_commit<'repo>(
        self: &Arc<Self>,
        commit: &Commit<'repo>,
        new_commits: &mut HashMap<String, Arc<CommitInfo>>,
    );

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
                    let mut new_commits: HashMap<String, Arc<CommitInfo>> = HashMap::new();
                    // Add the current commit first
                    // Then add parent commits up to build_depth - 1
                    self.parse_commit_parents(
                        &commit,
                        self.repo.build_depth.saturating_sub(1),
                        &mut new_commits,
                    );

                    *self
                        .branch_commit_hashes
                        .get(&branch_name)
                        .unwrap()
                        .0
                        .write()
                        .unwrap() = new_commits.values().map(|c| c.hash.clone()).collect();

                    *self.commits.inner().write().unwrap() = new_commits;
                });

            // sleep for poll interval
            while !self.pull(&repo)? {
                *self.status.0.write().unwrap() = RepoStatus::Idle;
                thread::sleep(std::time::Duration::from_secs(self.repo.poll_interval_sec));
            }
        }
    }

    fn parse_commit_parents<'repo>(
        self: &Arc<RepoInfo>,
        commit: &Commit<'repo>,
        depth: u8,
        mut new_commits: &mut HashMap<String, Arc<CommitInfo>>,
    ) {
        if depth == 0 {
            return;
        }
        self.get_or_create_commit(&commit, &mut new_commits);
        for commit in commit.parents() {
            self.parse_commit_parents(&commit, depth - 1, new_commits);
        }
    }

    fn get_or_create_commit<'repo>(
        self: &Arc<RepoInfo>,
        commit: &Commit<'repo>,
        new_commits: &mut HashMap<String, Arc<CommitInfo>>,
    ) {
        if new_commits.contains_key(&commit.id().to_string()) {
            return;
        }
        let commit_info = self
            .commits
            .inner()
            .read()
            .unwrap()
            .get(&commit.id().to_string())
            .cloned()
            .unwrap_or_else(|| {
                let commit_info = CommitInfo::new(self.clone(), &commit);
                commit_info.clone().build();
                commit_info
            });
        new_commits.insert(commit.id().to_string(), commit_info);
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
