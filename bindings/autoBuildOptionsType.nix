{lib, ...}: let
  types = lib.types;
in let
  repoType = {
    options = {
      url = lib.mkOption {
        type = types.str;
        description = "Repository URL";
        example = "github.com/org/repo";
      };

      poll_interval_sec = lib.mkOption {
        type = types.int;
        description = "Polling interval in seconds to check for updates";
        default = 300;
      };

      branches = lib.mkOption {
        type = types.listOf types.str;
        description = "Branches to monitor. If empty or not set, all branches are monitored.";
        default = [];
        example = ["main" "dev"];
      };

      build_depth = lib.mkOption {
        type = types.int;
        description = "How many commints to build from the tip of each branch";
        default = 1;
      };

      credentials_file = lib.mkOption {
        type = types.nullOr types.str;
        description = "Optional path to a credentials file. When set, the file must contain a single line with credentials in the format `username:password` (no quotes). If omitted or empty, no credentials are used.";
        default = "";
        example = "/path/to/credentials";
      };

    };
  };
  autoBuildOptionsType = {
    options = {
    repos = lib.mkOption {
      type = types.listOf (types.submodule repoType);
      description = "List of repositories to monitor";
      default = [];
    };

    dir = lib.mkOption {
      type = types.path;
      description = "Directory used to checkout repositories";
      default = "/var/lib/nix_autobuild";
    };

    supported_architectures = lib.mkOption {
      type = types.listOf types.str;
      description = "List of supported Nix build architectures (e.g. x86_64-linux)";
      default = [];
      example = ["x86_64-linux" "aarch64-linux"];
    };

    host = lib.mkOption {
      type = types.str;
      description = "Host address for the server to bind to";
      default = "127.0.0.1";
    };

    port = lib.mkOption {
      type = types.int;
      description = "Port for the server to bind to";
      default = 8080;
    };

    n_build_threads = lib.mkOption {
      type = types.int;
      description = "Number of threads to use for building. If 0, uses the number of CPU cores.";
      default = 0;
    };

    builders = lib.mkOption {
      type = types.listOf types.str;
      description = "List of builders to use for building. If empty, uses the default Nix builders configured on the system.";
      default = [];
      example = ["ssh-ng://builder@host1"];
    };

    copy_to = lib.mkOption {
      type = types.listOf types.str;
      description = "List of hosts to copy built packages to. If empty, no files are copied.";
      default = [];
      example = ["ssh-ng://user@host1"];
    };

    copy_to_substitute_on_destination = lib.mkOption {
      type = types.bool;
      description = "Whether to substitute on destination when copying built packages";
      default = true;
    };

    enable_gcroots = lib.mkOption {
      type = types.bool;
      description = "Whether to enable gcroots for the built packages";
      default = true;
    };

    };
  };
in autoBuildOptionsType
