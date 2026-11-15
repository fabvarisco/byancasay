{
  description = "byancasay - cowsay, but it's Byanca, a cyberpunk neko for your terminal";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    in
    {
      packages = forAllSystems (pkgs: rec {
        byancasay = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let base = baseNameOf path; in
              !(builtins.elem base [ "target" "result" "tools" "docs" ".github" ]);
          };
          cargoLock.lockFile = ./Cargo.lock;
          meta = {
            description = cargoToml.package.description;
            license = pkgs.lib.licenses.mit;
            mainProgram = "byancasay";
            platforms = pkgs.lib.platforms.unix;
          };
        };
        default = byancasay;
      });

      apps = forAllSystems (pkgs: rec {
        byancasay = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.byancasay}/bin/byancasay";
          meta.description = "Run byancasay";
        };
        default = byancasay;
      });

      overlays.default = final: _prev: {
        byancasay = self.packages.${final.stdenv.hostPlatform.system}.byancasay;
      };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            (python3.withPackages (ps: [ ps.pillow ps.numpy ]))
          ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixpkgs-fmt);
    };
}
