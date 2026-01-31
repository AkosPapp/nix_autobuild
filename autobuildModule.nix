self: {
  lib,
  config,
  pkgs,
  ...
}: {
  options.services.nix_autobuild = {
    enable = lib.mkEnableOption "Nix Autobuild service";

    settings = lib.mkOption {
      type = lib.types.submodule import ./bindings/autoBuildOptionsType.nix;
      description = "Configuration options for the Nix Autobuild service";
      default = {};
    };
  };

  config = lib.mkIf config.services.nix_autobuild.enable (
    let
      configFile = builtins.toFile "config.json" (builtins.toJSON config.services.nix_autobuild.settings);
      nix_autobuild = self.packages.${pkgs.system}.backend;
    in {
      systemd.services = {
        "nix_autobuild" = {
          description = "A simple build tool for Nix projects.";
          after = ["network.target"];
          wantedBy = ["multi-user.target"];
          path = [pkgs.nix pkgs.git];
          serviceConfig = {
            ExecStart = "${nix_autobuild}/bin/nix_autobuild ${configFile}";
            User = "root";
            Restart = "always";
          };
        };
      };
    }
  );
}
