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
    fn build(&self, full_path: &str, settings: &Settings) {
        self._build(full_path, settings)
    }
    fn _build(&self, full_path: &str, _settings: &Settings) {
        let pkg_path = format!("{}#{}", full_path, self.get_path());

        println!("BUILD\t{}", pkg_path);
        let output = std::process::Command::new("nix")
            .arg("build")
            .arg("--no-link")
            .arg("--print-out-paths")
            .arg(&pkg_path)
            .output()
            .expect("failed to execute process");

        if output.status.code().unwrap_or(-1) != 0 {
            let build_error = String::from_utf8_lossy(&output.stderr);
            println!("ERROR\t{} -> {}", pkg_path, build_error);
            return;
        }

        let build_output = String::from_utf8_lossy(&output.stdout);
        let build_output = build_output.trim();
        println!("RESULT\t{} -> {}", pkg_path, build_output);
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Settings {
    repos: Vec<Repo>,
    dir: String,
    supported_architectures: Vec<String>,
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
                    for a in ARCHITECTURES {
                        if s.starts_with(a) {
                            return Some(a);
                        }
                    }
                    return None;
                }
            }()
            .unwrap_or("unknown"),
            path,
        }))
    }

    fn get_path(&self) -> &str {
        &self.path
    }

    fn build(&self, full_path: &str, settings: &Settings) {
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
            return;
        }

        self._build(full_path, settings);
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
        // clone repo if not exists
        if !std::path::Path::new(&full_path).exists() {
            self.clone_repo(&full_path);
        }

        loop {
            println!("POLL\t{}", self.name);
            self.get_pkgs_list(&full_path).par_iter().for_each(|pkg| {
                // skip packages not matching supported architectures
                pkg.build(&full_path, &settings);
            });

            //exit(1);

            // sleep for poll interval

            while !self.pull_repo(&full_path).unwrap() {
                thread::sleep(std::time::Duration::from_secs(self.poll_interval_sec));
            }
        }
    }

    // returns true if pull repo got updated
    fn pull_repo(&self, full_path: &str) -> Option<bool> {
        println!("PULL\t{} at {}", full_path, self.url);
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&full_path)
            .arg("pull")
            .output()
            .expect("failed to execute process");
        if output.status.code().unwrap_or(-1) != 0 {
            let pull_error = String::from_utf8_lossy(&output.stderr);
            println!("ERROR pulling {} -> {}", self.name, pull_error);
            return None;
        }
        let pull_output = String::from_utf8_lossy(&output.stdout);
        let pull_output = pull_output.trim();
        println!("PULL\t{} -> '{}'", self.name, pull_output);
        return Some(pull_output != "Already up to date.");
    }

    fn clone_repo(&self, full_path: &str) {
        println!("Cloning repository: {} to {}", self.url, full_path);
        let _output = std::process::Command::new("git")
            .arg("clone")
            .arg(&self.url)
            .arg(&full_path)
            .output()
            .expect("failed to execute process");
        println!("Cloned repository: {}", self.name);
    }

    fn _parse_pkgs_value(
        &self,
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
                        new_path.push_str(".");
                    }
                    new_path.push_str(key);

                    self._parse_pkgs_value(new_map, new_path, pkgs);
                }
            }
        }
    }

    fn get_pkgs_list(&self, full_path: &str) -> Vec<Box<dyn PackageBase>> {
        let _output = std::process::Command::new("nix")
            .arg("flake")
            .arg("show")
            .arg("--json")
            .arg(&full_path)
            .arg("--all-systems")
            .output()
            .expect("failed to execute process");
        println!("LIST\t{} {}", full_path, self.name);

        let pkgs_json = String::from_utf8_lossy(&_output.stdout);
        //println!("{}", str);

        let pkgs_value: Value = serde_json::from_str(&pkgs_json).unwrap();
        //println!("{:#?}", pkgs);

        let mut pkgs_vec: Vec<Box<dyn PackageBase>> = Vec::new();
        self._parse_pkgs_value(
            &pkgs_value.as_object().unwrap(),
            String::new(),
            &mut pkgs_vec,
        );

        pkgs_vec
    }
}

fn main() {
    let config_path = args().nth(1).expect("No config Path Specified");
    let settings = {
        let config_data =
            std::fs::read_to_string(&config_path).expect("Unable to read config file");
        serde_json::from_str::<Settings>(&config_data).expect("Unable to parse config file")
    };

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
            panic!("Unsupported architecture in settings: {}", supported_arch);
        }
    }

    // create data directory if not exists
    std::fs::create_dir_all(&settings.dir).unwrap();

    thread::scope({
        |s| {
            for repo in &settings.repos {
                let full_path_buf = std::env::current_dir()
                    .expect("failed to get current directory")
                    .join(base_path)
                    .join(&repo.name);
                let full_path = full_path_buf.to_string_lossy().into_owned();
                s.spawn(|| repo.thread_poll(full_path, &settings));
            }
        }
    });
}
