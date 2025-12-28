{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
  };

  outputs =
    { self, nixpkgs }:
    {

      packages =
        nixpkgs.lib.genAttrs (nixpkgs.lib.remove "x86_64-freebsd" nixpkgs.lib.systems.flakeExposed)
          (system: {
            goober-counting = nixpkgs.legacyPackages.${system}.rustPackages_1_89.rustPlatform.buildRustPackage {
              pname = "goober-counting";
              version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
              src = ./.;
              cargoLock = {
                lockFile = ./Cargo.lock;
                allowBuiltinFetchGit = true;
              };
              meta.mainProgram = "goober-counting";
            };

            default = self.packages.${system}.goober-counting;
          });

      nixosModules = {
        goober-counting =
          {
            lib,
            config,
            pkgs,
            ...
          }:
          {
            options = {
              services.goober-counting = {
                enable = lib.mkEnableOption "goober-counting";
                token = lib.mkOption { type = lib.types.str; };
              };
            };
            config = lib.mkIf config.services.goober-counting.enable {
              systemd.services.goober-counting = {
                wantedBy = [ "multi-user.target" ];
                after = [ "network.target" ];
                environment.GOOBER_COUNTING_DISCORD_TOKEN = config.services.goober-counting.token;
                serviceConfig = {
                  ExecStart = lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.goober-counting;
                  Restart = "always";
                };
              };
            };
          };

        default = self.nixosModules.goober-counting;
      };

    };
}
