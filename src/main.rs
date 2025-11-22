use rayon::prelude::*;
use serde_derive::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{env::args, fmt::Debug, thread};

const ARCHITECTURES: [&str; 4] = [
    "aarch64-linux",
    "x86_64-linux",
    "x86_64-darwin",
    "aarch64-darwin",
];

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Repo {
    url: String,
    name: String,
    poll_interval_sec: u64,
}

trait PackageBase: Debug + Send + Sync {
    fn from_map(map: &Map<String, Value>, path: String) -> Option<Box<Self>>
    where
        Self: Sized;
    fn get_path(&self) -> &str;
    fn build(
        &self,
        full_path: &str,
        settings: &Settings,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self._build(full_path, settings)
    }
    fn _build(
        &self,
        full_path: &str,
        _settings: &Settings,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pkg_path = format!("{}#{}", full_path, self.get_path());

        println!("BUILD\t{}", pkg_path);
        let output = std::process::Command::new("nix")
            .arg("build")
            .arg("--no-link")
            .arg("--print-out-paths")
            .arg(&pkg_path)
            .output()?;

        if output.status.code().unwrap_or(-1) != 0 {
            let build_error = String::from_utf8_lossy(&output.stderr);
            println!("ERROR\t{} -> {}", pkg_path, build_error);
            return Err("Failed to build package".into());
        }

        let build_output = String::from_utf8_lossy(&output.stdout);
        let build_output = build_output.trim();
        println!("RESULT\t{} -> {}", pkg_path, build_output);
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Settings {
    repos: Vec<Repo>,
    dir: String,
    supported_architectures: Vec<String>,
    thread_count: usize,
}

#[derive(Debug)]
struct Package {
    _description: String,
    _name: String,
    _pkg_type: String,
    path: String,
    arch: &'static str,
}

impl PackageBase for Package {
    fn from_map(map: &Map<String, Value>, path: String) -> Option<Box<Self>> {
        // extract architecture between the first and second dot in the path
        Some(Box::new(Package {
            _description: map.get("description")?.as_str()?.to_string(),
            _name: map.get("name")?.as_str()?.to_string(),
            _pkg_type: map.get("type")?.as_str()?.to_string(),
            arch: {
                || {
                    let s = &path[path.find('.')? + 1..];
                    //println!("Extracting architecture from path segment: {}", s);
                    ARCHITECTURES.into_iter().find(|&a| s.starts_with(a))
                }
            }()
            .unwrap_or("unknown"),
            path,
        }))
    }

    fn get_path(&self) -> &str {
        &self.path
    }

    fn build(
        &self,
        full_path: &str,
        settings: &Settings,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // skip packages not matching supported architectures
        let mut arch_supported = false;
        for arch in &settings.supported_architectures {
            if self.arch == arch {
                arch_supported = true;
                break;
            }
        }

        let pkg_path = format!("{}#{}", full_path, self.get_path());

        if !arch_supported {
            println!("SKIP\t{} unsupported arch: {}", pkg_path, self.arch);
            return Ok(());
        }

        self._build(full_path, settings)
    }
}

#[derive(Debug)]
struct NixosConfigPackage {
    path: String,
    _pkg_type: String,
}

impl PackageBase for NixosConfigPackage {
    fn from_map(map: &Map<String, Value>, path: String) -> Option<Box<Self>> {
        // extract architecture between the first and second dot in the path
        if !path.starts_with("nixosConfigurations") {
            return None;
        }

        let pkg_type = map.get("type")?.as_str()?;
        if pkg_type != "nixos-configuration" {
            return None;
        }

        Some(Box::new(NixosConfigPackage {
            path: format!("{}.config.system.build.toplevel", path),
            _pkg_type: pkg_type.to_string(),
        }))
    }

    fn get_path(&self) -> &str {
        &self.path
    }
}

impl Repo {
    pub fn thread_poll(&self, full_path: String, settings: &Settings) {
        loop {
            if let Err(e) = self.thread_loop(full_path.clone(), settings) {
                println!("ERROR in repo {}: {}", self.name, e);
            }
            if let Err(e) = self.delete_repo(&full_path) {
                println!("ERROR deleting {}: {}", self.name, e);
            }
        }
    }

    pub fn thread_loop(
        &self,
        full_path: String,
        settings: &Settings,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // clone repo if not exists
        if !std::path::Path::new(&full_path).exists() {
            self.clone_repo(&full_path)?;
        }

        loop {
            println!("POLL\t{}", self.name);
            self.get_pkgs_list(&full_path)?.par_iter().for_each(|pkg| {
                // skip packages not matching supported architectures
                if let Err(e) = pkg.build(&full_path, settings) {
                    println!("ERROR building package {}: {}", pkg.get_path(), e);
                }
            });

            //exit(1);

            // sleep for poll interval

            while !self.pull_repo(&full_path)? {
                thread::sleep(std::time::Duration::from_secs(self.poll_interval_sec));
            }
        }
    }

    // returns true if pull repo got updated
    fn pull_repo(&self, full_path: &str) -> Result<bool, Box<dyn std::error::Error>> {
        println!("PULL\t{} at {}", full_path, self.url);
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(full_path)
            .arg("pull")
            .output()?;
        if output.status.code().unwrap_or(-1) != 0 {
            let pull_error = String::from_utf8_lossy(&output.stderr);
            println!("ERROR pulling {} -> {}", self.name, pull_error);
            return Err("Failed to pull repository".into());
        }
        let pull_output = String::from_utf8_lossy(&output.stdout);
        let pull_output = pull_output.trim();
        println!("PULL\t{} -> '{}'", self.name, pull_output);
        Ok(pull_output != "Already up to date.")
    }

    fn clone_repo(&self, full_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("Cloning repository: {} to {}", self.url, full_path);
        let output = std::process::Command::new("git")
            .arg("clone")
            .arg(&self.url)
            .arg(full_path)
            .output()?;
        println!("Cloned repository: {}", self.name);

        if output.status.code().unwrap_or(-1) != 0 {
            let clone_error = String::from_utf8_lossy(&output.stderr);
            println!("ERROR cloning {} -> {}", self.name, clone_error);
            return Err("Failed to clone repository".into());
        }

        Ok(())
    }

    fn _parse_pkgs_value(
        map: &Map<String, Value>,
        path: String,
        pkgs: &mut Vec<Box<dyn PackageBase>>,
    ) {
        if let Some(pkg) = Package::from_map(map, path.clone()) {
            //println!(
            //    "Found package: {} at path: {:#?}",
            //    if path.is_empty() { "<root>" } else { &path },
            //    pkg
            //);
            pkgs.push(pkg);
        } else if let Some(nixos_cfg) = NixosConfigPackage::from_map(map, path.clone()) {
            //println!(
            //    "Found NixOS configuration: {} at path: {:#?}",
            //    if path.is_empty() { "<root>" } else { &path },
            //    nixos_cfg
            //);
            pkgs.push(nixos_cfg);
        } else {
            for key in map.keys() {
                if let Some(new_map) = map[key].as_object() {
                    let mut new_path = path.clone();
                    if !new_path.is_empty() {
                        new_path.push('.');
                    }
                    new_path.push_str(key);

                    Self::_parse_pkgs_value(new_map, new_path, pkgs);
                }
            }
        }
    }

    fn get_pkgs_list(
        &self,
        full_path: &str,
    ) -> Result<Vec<Box<dyn PackageBase>>, Box<dyn std::error::Error>> {
        let output = std::process::Command::new("nix")
            .arg("flake")
            .arg("show")
            .arg("--json")
            .arg(full_path)
            .arg("--all-systems")
            .output()?;
        println!("LIST\t{} {}", full_path, self.name); // TODO: add error handling

        if output.status.code().unwrap_or(-1) != 0 {
            let list_error = String::from_utf8_lossy(&output.stderr);
            println!("ERROR listing {} -> {}", self.name, list_error);
            return Err("Failed to list packages in flake".into());
        }

        let pkgs_json = String::from_utf8(output.stdout)?;
        //println!("{}", pkgs_json);

        let pkgs_value: Value = serde_json::from_str(&pkgs_json)?;
        //println!("{:#?}", pkgs_value);

        let Some(pkgs_object) = pkgs_value.as_object() else {
            return Err("No packages found in flake".into());
        };

        let mut pkgs_vec: Vec<Box<dyn PackageBase>> = Vec::new();
        Self::_parse_pkgs_value(pkgs_object, String::new(), &mut pkgs_vec);

        Ok(pkgs_vec)
    }

    fn delete_repo(&self, full_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        println!("DELETE\t{} at {}", self.name, full_path);
        let output = std::process::Command::new("rm")
            .arg("-rf")
            .arg(full_path)
            .output()?;
        if output.status.code().unwrap_or(-1) != 0 {
            let delete_error = String::from_utf8_lossy(&output.stderr);
            println!("ERROR deleting {} -> {}", self.name, delete_error);
            return Err("Failed to delete repository".into());
        }
        println!("DELETED\t{} at {}", self.name, full_path);
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = args().nth(1).ok_or("No config Path Specified")?;
    let settings = {
        let config_data = std::fs::read_to_string(&config_path)?;
        serde_json::from_str::<Settings>(&config_data)?
    };

    rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build_global()
        .expect("global pool already initialized");

    let base_path = std::path::Path::new(&settings.dir);

    for supported_arch in &settings.supported_architectures {
        let mut found = false;
        for arch in ARCHITECTURES {
            if supported_arch == arch {
                found = true;
                break;
            }
        }
        if !found {
            eprintln!("Unsupported architecture in settings: {}", supported_arch);
            return Err("Unsupported architecture in settings".into());
        }
    }

    // create data directory if not exists
    std::fs::create_dir_all(&settings.dir)?;

    thread::scope({
        |s| {
            for repo in &settings.repos {
                let full_path_buf = base_path.join(&repo.name);
                let full_path = full_path_buf.to_string_lossy().into_owned();
                s.spawn(|| repo.thread_poll(full_path, &settings));
            }
        }
    });
    Ok(())
}
