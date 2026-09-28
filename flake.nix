{
  description = "Temporion — CPU (Tctl), NVIDIA GPU and NVMe temperatures in the GNOME top bar";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default-linux";
  };

  outputs =
    { self, nixpkgs, systems, ... }:
    let
      uuid = "temporion@masterzack69";
      eachSystem = nixpkgs.lib.genAttrs (import systems);
      pkgsFor = system: import nixpkgs { inherit system; };
    in
    {
      packages = eachSystem (
        system:
        let
          pkgs = pkgsFor system;

          # The Rust daemon: all sensor logic, zero external crates.
          temporiond = pkgs.rustPlatform.buildRustPackage {
            pname = "temporiond";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            doCheck = false;
            meta = {
              description = "Zero-dependency daemon streaming CPU/GPU/NVMe temperatures";
              homepage = "https://github.com/MasterZack69/temporion";
              license = pkgs.lib.licenses.agpl3Only;
              platforms = pkgs.lib.platforms.linux;
              mainProgram = "temporiond";
            };
          };

          # The GNOME extension: a thin GJS reader with the daemon's absolute
          # store path baked in, installed to the standard extensions dir.
          temporion = pkgs.stdenvNoCC.mkDerivation {
            pname = "temporion-gnome-extension";
            version = "0.1.0";
            src = ./.;
            dontConfigure = true;
            dontBuild = true;
            installPhase = ''
              runHook preInstall
              dir="$out/share/gnome-shell/extensions/${uuid}"
              mkdir -p "$dir"
              cp -r extension/. "$dir"/
              substituteInPlace "$dir/extension.js" \
                --replace-fail '@TEMPORIOND@' '${temporiond}/bin/temporiond'
              runHook postInstall
            '';
            passthru = {
              extensionUuid = uuid;
              inherit temporiond;
            };
            meta = {
              description = "Temporion GNOME Shell extension (frontend for temporiond)";
              homepage = "https://github.com/MasterZack69/temporion";
              license = pkgs.lib.licenses.agpl3Only;
              platforms = pkgs.lib.platforms.linux;
            };
          };
        in
        {
          inherit temporiond temporion;
          default = temporion;
        }
      );

      # `nix run` runs the daemon standalone (handy for verifying sensors).
      apps = eachSystem (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.temporiond}/bin/temporiond";
        };
      });

      checks = eachSystem (system: {
        inherit (self.packages.${system}) temporiond temporion;
      });

      formatter = eachSystem (system: (pkgsFor system).nixpkgs-fmt);

      devShells = eachSystem (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              gjs
              nodejs
            ];
          };
        }
      );
    };
}
