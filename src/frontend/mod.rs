use std::collections::BTreeMap;

use crate::{
    RepoList,
    commit::CommitInfo,
    package::{self, PackageBuildStatus, PackageEnum},
    repo::{self, RepoInfo},
};
use gloo_timers::callback::Interval;
use serde::de;
use serde_json;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;
use yew::prelude::*;

// Fetch the repo list via Fetch API and return deserialized RepoList
async fn fetch_repos() -> Result<RepoList, String> {
    let window = web_sys::window().ok_or_else(|| "no window available".to_string())?;
    let location = window.location();
    let protocol = location.protocol().map_err(|_| "no protocol".to_string())?;
    let host = location.host().map_err(|_| "no host".to_string())?;
    let url = format!("{}//{}/repos", protocol, host);
    let resp_value = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(|e| format!("fetch failed: {e:?}"))?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "failed to cast response".to_string())?;

    let text_promise = resp
        .text()
        .map_err(|e| format!("response.text() failed: {e:?}"))?;
    let text_js = JsFuture::from(text_promise)
        .await
        .map_err(|e| format!("awaiting text failed: {e:?}"))?;

    let text = text_js
        .as_string()
        .ok_or_else(|| "response not text".to_string())?;

    serde_json::from_str(&text).map_err(|e| format!("failed to parse json: {e}"))
}

fn repos(repos: &RepoList, props: &Props) -> Html {
    let all_packages: Vec<Package> = repos
        .0
        .0
        .iter()
        .flat_map(|repo| {
            // Collect packages from each repo
            repo.commits.0.iter().flat_map(|(_hash, commit)| {
                commit
                    .packages
                    .0
                    .iter()
                    .map(|pkg| Package { repo, commit, pkg })
            })
        })
        .collect();

    // group repo -> package_name -> branch -> commit -> arch -> package info
    let mut grouped: BTreeMap<
        String, // repo url
        (
            &RepoInfo,
            BTreeMap<
                String, // package name
                BTreeMap<
                    String, // branch name
                    BTreeMap<
                        String, // commit hash
                        BTreeMap<
                            String, // arch
                            &Package,
                        >,
                    >,
                >,
            >,
        ),
    > = BTreeMap::new();

    // Initialize all repos in the map (even if they have no packages)
    for repo in repos.0.0.iter() {
        grouped.insert(repo.repo.url.clone(), (repo, BTreeMap::new()));
    }

    for package in &all_packages {
        let repo_url = package.repo.repo.url.clone();
        let arch = match package.pkg {
            PackageEnum::Derivation(arc_wrapper) => arc_wrapper.0.arch.clone(),
            PackageEnum::NixosConfig(_arc_wrapper) => "NONE".to_string(),
        };
        let package_name = match package.pkg {
            PackageEnum::Derivation(arc_wrapper) => arc_wrapper.0.get_no_arch_name(),
            PackageEnum::NixosConfig(arc_wrapper) => arc_wrapper.0.path.clone(),
        };

        // Find which branches contain this commit
        for (branch_name, commit_hashes) in &package.repo.branch_commit_hashes {
            if commit_hashes.0.contains(&package.commit.hash) {
                // Entry already exists from initialization above
                if let Some(entry) = grouped.get_mut(&repo_url) {
                    entry
                        .1
                        .entry(package_name.clone())
                        .or_default()
                        .entry(branch_name.clone())
                        .or_default()
                        .entry(package.commit.hash.clone())
                        .or_default()
                        .insert(arch.clone(), package);
                }
            }
        }
    }

    html! {
        <div class="stack">
            { for grouped.iter().map(|(repo_name, repo)| {
                repo_html(repo_name, repo, props)
            }) }
        </div>
    }
}

// group repo -> package_name -> branch -> commit -> arch -> package info
fn repo_html(
    repo_name: &str,
    repo_data: &(
        &RepoInfo,
        BTreeMap<String, BTreeMap<String, BTreeMap<String, BTreeMap<String, &Package<'_>>>>>,
    ),
    props: &Props,
) -> Html {
    let status_text = format!("{:?}", repo_data.0.status.0);
    let status_class = match status_text.as_str() {
        s if s.contains("Success") => "status-success",
        s if s.contains("Failed") || s.contains("Failure") => "status-failed",
        s if s.contains("Building") || s.contains("Running") => "status-building",
        s if s.contains("Pending") || s.contains("Queued") || s.contains("WaitingForBuild") => {
            "status-pending"
        }
        _ => "status-unknown",
    };
    let is_open = props.repo_name.as_deref() == Some(repo_name);
    let link_url = if is_open {
        Props::default().get_url().unwrap_or_default()
    } else {
        props
            .with_repo_name(repo_name.to_string())
            .get_url()
            .unwrap_or_default()
    };

    html! {
        <section class="card">
            <a href={link_url}>
                <div class="repo-header">
                    <h2>{ repo_name }</h2>
                    <span class={classes!("status-indicator", status_class)}>{ status_text }</span>
                </div>
                <p class="meta">{ &repo_data.0.flake_url }</p>
            </a>
            if is_open {
                { for repo_data.1.iter().map(|(package_name, branches)| {
                    package_name_html(package_name, branches, props)
                }) }
            }
        </section>
    }
}

fn package_name_html(
    package_name: &String,
    branches: &BTreeMap<String, BTreeMap<String, BTreeMap<String, &Package<'_>>>>,
    props: &Props,
) -> Html {
    let is_open = props.package_name.as_deref() == Some(package_name);
    let link_url = if is_open {
        props.clear_from_package().get_url().unwrap_or_default()
    } else {
        props
            .with_package(package_name.clone())
            .get_url()
            .unwrap_or_default()
    };

    html! {
        <div class="card">
            <a href={link_url}>
                <h3>{ package_name }</h3>
            </a>
            if is_open {
                { for branches.iter().map(|(branch_name, commits)| {
                    branch_html(branch_name, commits, props)
                }) }
            }
        </div>
    }
}

fn branch_html(
    branch_name: &String,
    commits: &BTreeMap<String, BTreeMap<String, &Package<'_>>>,
    props: &Props,
) -> Html {
    let is_open = props.branch.as_deref() == Some(branch_name);
    let link_url = if is_open {
        props.clear_from_branch().get_url().unwrap_or_default()
    } else {
        props
            .with_branch(branch_name.clone())
            .get_url()
            .unwrap_or_default()
    };

    // Sort commits by timestamp (newest first)
    let mut sorted_commits: Vec<_> = commits.iter().collect();
    sorted_commits.sort_by(|(_, archs_a), (_, archs_b)| {
        let timestamp_a = archs_a
            .values()
            .next()
            .map(|p| p.commit.unix_secs)
            .unwrap_or(0);
        let timestamp_b = archs_b
            .values()
            .next()
            .map(|p| p.commit.unix_secs)
            .unwrap_or(0);
        timestamp_b.cmp(&timestamp_a) // Reverse order for newest first
    });

    html! {
        <div class="card">
            <a href={link_url}>
                <h4>{ branch_name }</h4>
            </a>
            if is_open {
                <ul>
                    { for sorted_commits.iter().map(|(commit_hash, archs)| {
                        commit_html(commit_hash, archs, props)
                    }) }
                </ul>
            }
        </div>
    }
}

fn commit_html(
    commit_hash: &String,
    archs: &BTreeMap<String, &Package<'_>>,
    props: &Props,
) -> Html {
    let short_hash = &commit_hash[..7.min(commit_hash.len())];
    let commit_message = archs
        .values()
        .next()
        .map(|p| p.commit.message.as_str())
        .unwrap_or("no commit message");
    let is_open = props.commit_hash.as_deref() == Some(commit_hash);
    let link_url = if is_open {
        props.clear_from_commit().get_url().unwrap_or_default()
    } else {
        props
            .with_commit(commit_hash.clone())
            .get_url()
            .unwrap_or_default()
    };

    html! {
        <li class="card">
            <a href={link_url}>
                { format!("{} - {}", short_hash, commit_message) }
            </a>
            if is_open {
                <div>
                    { for archs.iter().map(|(arch, package)| {
                        arch_html(arch, package, props)
                    }) }
                </div>
            }
        </li>
    }
}

fn arch_html(arch: &String, package: &Package<'_>, props: &Props) -> Html {
    let (_name, pkg_type, status_text, result) = match package.pkg {
        PackageEnum::Derivation(arc_wrapper) => (
            arc_wrapper.0.name.clone(),
            arc_wrapper.0.pkg_type.clone(),
            format!("{:?}", arc_wrapper.0.status.0),
            match &arc_wrapper.0.status.0 {
                PackageBuildStatus::Success(path) => Some(path.clone()),
                _ => None,
            },
        ),
        PackageEnum::NixosConfig(arc_wrapper) => (
            arc_wrapper.0.pkg_type.clone(),
            "NixOS Config".to_string(),
            format!("{:?}", arc_wrapper.0.status.0),
            match (&arc_wrapper.0.status.0) {
                PackageBuildStatus::Success(path) => Some(path.clone()),
                _ => None,
            },
        ),
    };

    let status_class = match status_text.as_str() {
        s if s.contains("Success") => "status-success",
        s if s.contains("Failed") || s.contains("Failure") => "status-failed",
        s if s.contains("Building") || s.contains("Running") => "status-building",
        s if s.contains("Pending") || s.contains("Queued") || s.contains("WaitingForBuild") => {
            "status-pending"
        }
        _ => "status-unknown",
    };

    let is_selected = props.arch.as_deref() == Some(arch);
    let link_url = if is_selected {
        props.clear_arch().get_url().unwrap_or_default()
    } else {
        props.with_arch(arch.clone()).get_url().unwrap_or_default()
    };

    html! {
        <div class="card">
            <a href={link_url}>
                <div class="pkg-header">
                    <p>{ format!("{} ({})", arch, pkg_type) }</p>
                    <span class={classes!("status-indicator", status_class)}>{ status_text }</span>
                </div>
                if let Some(result_path) = result {
                    <p class="meta">
                        <a href={result_path.clone()} class="result-link">{ "→ Build Result" }</a>
                    </p>
                }
            </a>
        </div>
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct Props {
    pub repo_name: Option<String>,
    pub package_name: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
    pub arch: Option<String>,
}

impl Props {
    pub fn from_url() -> Self {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return Self::default(),
        };

        let location = window.location();
        let search = match location.search() {
            Ok(s) => s,
            Err(_) => return Self::default(),
        };

        // Parse URLSearchParams
        let url_params = match web_sys::UrlSearchParams::new_with_str(&search) {
            Ok(params) => params,
            Err(_) => return Self::default(),
        };

        Self {
            repo_name: url_params.get("repo"),
            package_name: url_params.get("package"),
            branch: url_params.get("branch"),
            commit_hash: url_params.get("commit"),
            arch: url_params.get("arch"),
        }
    }

    pub fn get_url(&self) -> Option<String> {
        let mut params = vec![];

        let window = match web_sys::window() {
            Some(w) => w,
            None => return None,
        };

        let location = window.location();

        if let Some(repo) = &self.repo_name {
            params.push(format!("repo={}", repo));
        }
        if let Some(package) = &self.package_name {
            params.push(format!("package={}", package));
        }
        if let Some(branch) = &self.branch {
            params.push(format!("branch={}", branch));
        }
        if let Some(commit) = &self.commit_hash {
            params.push(format!("commit={}", commit));
        }
        if let Some(arch) = &self.arch {
            params.push(format!("arch={}", arch));
        }

        Some(format!(
            "{}//{}{}?{}",
            location.protocol().ok()?,
            location.host().ok()?,
            location.pathname().ok()?,
            params.join("&")
        ))
    }

    pub fn with_repo_name(&self, repo_name: String) -> Self {
        Self {
            repo_name: Some(repo_name),
            package_name: None,
            branch: None,
            commit_hash: None,
            arch: None,
        }
    }

    pub fn with_package(&self, package_name: String) -> Self {
        Self {
            repo_name: self.repo_name.clone(),
            package_name: Some(package_name),
            branch: None,
            commit_hash: None,
            arch: None,
        }
    }

    pub fn with_branch(&self, branch: String) -> Self {
        Self {
            repo_name: self.repo_name.clone(),
            package_name: self.package_name.clone(),
            branch: Some(branch),
            commit_hash: None,
            arch: None,
        }
    }

    pub fn with_commit(&self, commit_hash: String) -> Self {
        Self {
            repo_name: self.repo_name.clone(),
            package_name: self.package_name.clone(),
            branch: self.branch.clone(),
            commit_hash: Some(commit_hash),
            arch: None,
        }
    }

    pub fn with_arch(&self, arch: String) -> Self {
        Self {
            repo_name: self.repo_name.clone(),
            package_name: self.package_name.clone(),
            branch: self.branch.clone(),
            commit_hash: self.commit_hash.clone(),
            arch: Some(arch),
        }
    }

    pub fn clear_from_package(&self) -> Self {
        Self {
            repo_name: self.repo_name.clone(),
            package_name: None,
            branch: None,
            commit_hash: None,
            arch: None,
        }
    }

    pub fn clear_from_branch(&self) -> Self {
        Self {
            repo_name: self.repo_name.clone(),
            package_name: self.package_name.clone(),
            branch: None,
            commit_hash: None,
            arch: None,
        }
    }

    pub fn clear_from_commit(&self) -> Self {
        Self {
            repo_name: self.repo_name.clone(),
            package_name: self.package_name.clone(),
            branch: self.branch.clone(),
            commit_hash: None,
            arch: None,
        }
    }

    pub fn clear_arch(&self) -> Self {
        Self {
            repo_name: self.repo_name.clone(),
            package_name: self.package_name.clone(),
            branch: self.branch.clone(),
            commit_hash: self.commit_hash.clone(),
            arch: None,
        }
    }
}

impl Default for Props {
    fn default() -> Self {
        Self {
            repo_name: None,
            package_name: None,
            branch: None,
            commit_hash: None,
            arch: None,
        }
    }
}

#[derive(Debug)]
pub struct Package<'a> {
    repo: &'a RepoInfo,
    commit: &'a CommitInfo,
    pkg: &'a PackageEnum,
}

#[derive(Properties, PartialEq)]
struct TableRowProps {
    repo_url: String,
    package_path: String,
    branch: String,
    commit_message: String,
    status_class: String,
    repo_debug: String,
    commit_debug: String,
    pkg_debug: String,
}

#[function_component]
fn TableRow(props: &TableRowProps) -> Html {
    let expanded = use_state(|| false);
    let toggle = {
        let expanded = expanded.clone();
        Callback::from(move |_| {
            expanded.set(!*expanded);
        })
    };

    html! {
        <>
            <tr onclick={toggle} class="table-row-hover" style="cursor: pointer; border-bottom: 1px solid rgba(255, 255, 255, 0.08);">
                <td style="padding: 12px; color: var(--text);">{ &props.repo_url }</td>
                <td style="padding: 12px; font-family: monospace; font-size: 0.9em; color: var(--text);">{ &props.package_path }</td>
                <td style="padding: 12px; color: var(--text);">{ &props.branch }</td>
                <td style="padding: 12px; color: var(--muted);">{ &props.commit_message }</td>
                <td style="padding: 12px; text-align: center;">
                    <span style={format!("display: inline-block; width: 12px; height: 12px; border-radius: 50%; {}",
                        match props.status_class.as_str() {
                            "status-success" => "background-color: #4caf50;",
                            "status-failed" => "background-color: #f44336;",
                            "status-building" => "background-color: #ff9800;",
                            "status-pending" => "background-color: #2196f3;",
                            _ => "background-color: #9e9e9e;",
                        }
                    )} title={props.status_class.clone()}></span>
                </td>
            </tr>
            if *expanded {
                <tr>
                    <td colspan="5" style="background: var(--card-strong); padding: 10px; border-bottom: 1px solid rgba(255, 255, 255, 0.08);">
                        <details open={true}>
                            <summary><strong style="color: var(--text);">{ "Repository Debug Info" }</strong></summary>
                            <pre style="overflow-x: auto; white-space: pre-wrap; color: var(--muted); background: var(--card); padding: 8px; border-radius: 4px; margin-top: 8px;">{ &props.repo_debug }</pre>
                        </details>
                        <details open={true}>
                            <summary><strong style="color: var(--text);">{ "Commit Debug Info" }</strong></summary>
                            <pre style="overflow-x: auto; white-space: pre-wrap; color: var(--muted); background: var(--card); padding: 8px; border-radius: 4px; margin-top: 8px;">{ &props.commit_debug }</pre>
                        </details>
                        <details open={true}>
                            <summary><strong style="color: var(--text);">{ "Package Debug Info" }</strong></summary>
                            <pre style="overflow-x: auto; white-space: pre-wrap; color: var(--muted); background: var(--card); padding: 8px; border-radius: 4px; margin-top: 8px;">{ &props.pkg_debug }</pre>
                        </details>
                    </td>
                </tr>
            }
        </>
    }
}

fn format_repo_debug(repo: &RepoInfo) -> String {
    format!(
        "RepoInfo {{\n  flake_url: {:?},\n  repo: {:#?},\n  checkout_path: {:?},\n  branch_commit_hashes: {:#?},\n  commits: <{} commits (excluded from display)>,\n  status: {:?},\n}}",
        repo.flake_url,
        repo.repo,
        repo.checkout_path,
        repo.branch_commit_hashes,
        repo.commits.0.len(),
        repo.status.0
    )
}

fn format_commit_debug(commit: &CommitInfo) -> String {
    format!(
        "CommitInfo {{\n  message: {:?},\n  flake_url: {:?},\n  hash: {:?},\n  packages: <{} packages (excluded from display)>,\n  unix_secs: {},\n  status: {:?},\n}}",
        commit.message,
        commit.flake_url,
        commit.hash,
        commit.packages.0.len(),
        commit.unix_secs,
        commit.status.0
    )
}

fn repos_table(repos: &RepoList) -> Html {
    let mut package_list: Vec<(&RepoInfo, &CommitInfo, &PackageEnum)> = repos
        .0
        .0
        .iter()
        .flat_map(|repo| {
            repo.commits.0.iter().flat_map(move |(_hash, commit)| {
                commit.packages.0.iter().map(move |pkg| (repo, commit, pkg))
            })
        })
        .collect();

    // Sort by: repo name, package name, branch, commit time (desc), arch
    package_list.sort_by(|(repo_a, commit_a, pkg_a), (repo_b, commit_b, pkg_b)| {
        let repo_name_a = &repo_a.repo.url;
        let repo_name_b = &repo_b.repo.url;

        let pkg_name_a = match pkg_a {
            PackageEnum::Derivation(arc_wrapper) => arc_wrapper.0.get_no_arch_name(),
            PackageEnum::NixosConfig(arc_wrapper) => arc_wrapper.0.path.clone(),
        };
        let pkg_name_b = match pkg_b {
            PackageEnum::Derivation(arc_wrapper) => arc_wrapper.0.get_no_arch_name(),
            PackageEnum::NixosConfig(arc_wrapper) => arc_wrapper.0.path.clone(),
        };

        let branch_a = repo_a
            .branch_commit_hashes
            .iter()
            .find_map(|(branch, hashes)| {
                if hashes.0.contains(&commit_a.hash) {
                    Some(branch.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "-".to_string());
        let branch_b = repo_b
            .branch_commit_hashes
            .iter()
            .find_map(|(branch, hashes)| {
                if hashes.0.contains(&commit_b.hash) {
                    Some(branch.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "-".to_string());

        let arch_a = match pkg_a {
            PackageEnum::Derivation(arc_wrapper) => arc_wrapper.0.arch.clone(),
            PackageEnum::NixosConfig(_arc_wrapper) => "N/A".to_string(),
        };
        let arch_b = match pkg_b {
            PackageEnum::Derivation(arc_wrapper) => arc_wrapper.0.arch.clone(),
            PackageEnum::NixosConfig(_arc_wrapper) => "N/A".to_string(),
        };

        repo_name_a
            .cmp(repo_name_b)
            .then_with(|| pkg_name_a.cmp(&pkg_name_b))
            .then_with(|| branch_a.cmp(&branch_b))
            .then_with(|| commit_b.unix_secs.cmp(&commit_a.unix_secs)) // Descending (newest first)
            .then_with(|| arch_a.cmp(&arch_b))
    });

    html! {
        <table style="width: 100%; border-collapse: collapse; background: var(--card); box-shadow: var(--shadow); border-radius: var(--radius); overflow: hidden;">
            <thead>
                <tr style="background: var(--card-strong); border-bottom: 2px solid rgba(255, 255, 255, 0.08);">
                    <th style="padding: 12px; text-align: left; font-weight: 600; color: var(--text);">{ "Repository" }</th>
                    <th style="padding: 12px; text-align: left; font-weight: 600; color: var(--text);">{ "Package Path" }</th>
                    <th style="padding: 12px; text-align: left; font-weight: 600; color: var(--text);">{ "Branch" }</th>
                    <th style="padding: 12px; text-align: left; font-weight: 600; color: var(--text);">{ "Commit" }</th>
                    <th style="padding: 12px; text-align: center; font-weight: 600; color: var(--text);">{ "Status" }</th>
                </tr>
            </thead>
            <tbody>
                { for package_list.iter().map(|(repo, commit, pkg)| {
                    let package_path = match pkg {
                        PackageEnum::Derivation(arc_wrapper) => arc_wrapper.0.path.clone(),
                        PackageEnum::NixosConfig(arc_wrapper) => arc_wrapper.0.path.clone(),
                    };
                    let branch = repo.branch_commit_hashes.iter()
                        .find_map(|(branch, hashes)| {
                            if hashes.0.contains(&commit.hash) {
                                Some(branch.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| "-".to_string());

                    let commit_first_line = commit.message.lines().next().unwrap_or("");
                    let commit_display = if commit_first_line.len() > 10 {
                        format!("{}...", &commit_first_line[..10])
                    } else {
                        commit_first_line.to_string()
                    };

                    let status_text = match pkg {
                        PackageEnum::Derivation(arc_wrapper) => format!("{:?}", arc_wrapper.0.status.0),
                        PackageEnum::NixosConfig(arc_wrapper) => format!("{:?}", arc_wrapper.0.status.0),
                    };

                    let status_class = match status_text.as_str() {
                        s if s.contains("Success") => "status-success",
                        s if s.contains("Failed") || s.contains("Failure") => "status-failed",
                        s if s.contains("Building") || s.contains("Running") => "status-building",
                        s if s.contains("Pending") || s.contains("Queued") || s.contains("WaitingForBuild") => "status-pending",
                        _ => "status-unknown",
                    };

                    html! {
                        <TableRow
                            repo_url={repo.repo.url.clone()}
                            package_path={package_path}
                            branch={branch}
                            commit_message={commit_display}
                            status_class={status_class.to_string()}
                            repo_debug={format_repo_debug(repo)}
                            commit_debug={format_commit_debug(commit)}
                            pkg_debug={format!("{:#?}", pkg)}
                        />
                    }
                }) }
            </tbody>
        </table>
    }
}

#[function_component]
fn App() -> Html {
    let data = use_state(|| None::<Result<RepoList, String>>);
    let props = Props::from_url();

    {
        let data = data.clone();
        // Fetch immediately, then refresh every second
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local({
                let data = data.clone();
                async move {
                    let res = fetch_repos().await;
                    data.set(Some(res));
                }
            });

            let interval = Interval::new(1000, move || {
                let data = data.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let res = fetch_repos().await;
                    data.set(Some(res));
                });
            });

            move || drop(interval)
        });
    }

    let body = match &*data {
        Some(Ok(list)) => repos(&list, &props),
        Some(Err(err)) => html! { <p class="meta error">{ format!("Error: {}", err) }</p> },
        None => html! { <p class="meta">{ "Loading data..." }</p> },
    };

    let table = match &*data {
        Some(Ok(list)) => repos_table(&list),
        _ => html! { <p class="meta">{ "No table to display" }</p> },
    };

    html! {
        <div class="app-bg">
            <main class="page">
                <header class="page-header">
                    <p class="kicker">{ "Nix Autobuild" }</p>
                    <h1>{ "Repository Overview" }</h1>
                    <p class="meta">{ "Auto-refreshing every second" }</p>
                </header>
                { body }
                { table }
                { format!("{:?}", props) }
            </main>
        </div>
    }
}

//fn main() {
//    yew::Renderer::<App>::new().render();
//}

pub fn main() {
    yew::Renderer::<App>::new().render();
}
