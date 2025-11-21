## nix_autobuild

nix_autobuild polls one or more Nix flake repositories, discovers available packages and NixOS configurations via
`nix flake show --json --all-systems`, and builds matching packages for configured architectures. It will clone repositories
if missing, poll for changes, and run builds in parallel using Rayon.

## Configuration (JSON)

The program reads a JSON configuration file passed as the first CLI argument. Example configuration (see
`example-config.json`):

```json
{
  "repos": [
    {
      "url": "https://git.robo4you.at/akos.papp/DA.git",
      "name": "DA",
      "poll_interval_sec": 60
    },
    {
      "url": "https://github.com/AkosPapp/nix.git",
      "name": "AkosPapp/nix",
      "poll_interval_sec": 60
    }
  ],
  "dir": "data",
  "supported_architectures": [
    "x86_64-linux"
  ]
}
```

Fields:
- `repos`: array of repositories to poll. Each repo has:
  - `url`: git clone URL
  - `name`: local directory name under `dir` where the repo will be cloned
  - `poll_interval_sec`: how often (in seconds) to pull for changes
- `dir`: base directory used to store cloned repositories (created if missing)
- `supported_architectures`: list of architecture names to build for (must be one of the supported constants in the code, e.g. `x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, `aarch64-darwin`)

The program validates that each `supported_architectures` value is one of the supported set and will panic at startup if an unknown architecture is provided.

## Usage

Run the program with the path to your config JSON file as the first argument:

```bash
cargo run -- /path/to/config.json
```

Or build and run the release binary:

```bash
cargo build --release
./target/release/nix_autobuild /path/to/config.json
```

Notes:
- The program will clone each configured repository into `<cwd>/<dir>/<repo.name>` if the clone does not already exist.
- It calls `nix flake show --json --all-systems <repo_path>` to discover packages and NixOS configurations, recursively
  parsing the JSON structure.
- Packages whose path encodes a supported architecture will be built with `nix build --no-link --print-out-paths <flake>#<pkg>`.
- Builds are executed in parallel across discovered packages using Rayon. The program filters packages by the
  `supported_architectures` list before building.

## Example

Use the bundled `example-config.json` as a starting point. Update `repos`, `dir`, or `supported_architectures` as needed.

## Development

Requirements:
- Rust (stable) and Cargo
- Nix (for `nix build` and `nix flake show` commands)

Build:

```bash
cargo build
```

Run (example):

```bash
cargo run -- ./example-config.json
```

## License

This project is open source.
