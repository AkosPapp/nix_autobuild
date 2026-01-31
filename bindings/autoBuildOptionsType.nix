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

    };
  };
in autoBuildOptionsType
