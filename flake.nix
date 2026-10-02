{
  description = "GMNB (GlazeMyNumbers,Baby) — Windows Calculator, ported to Rust and made pointlessly beautiful";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          gmnb = pkgs.callPackage ./nix/package.nix { };
          flatpak = pkgs.callPackage ./nix/flatpak.nix { inherit gmnb; };
        in
        {
          inherit gmnb;
          default = gmnb;
          flatpak-source = flatpak.source;
          flatpak-manifest = flatpak.manifest;
          flatpak-builder-script = flatpak.script;
          update-cargo-sources = pkgs.writeShellApplication {
            name = "update-cargo-sources";
            runtimeInputs = [ pkgs.flatpak-builder-tools ];
            text = ''
              flatpak-cargo-generator Cargo.lock -o packaging/flatpak/cargo-sources.json
              echo "updated packaging/flatpak/cargo-sources.json"
            '';
          };
        }
      );

      apps = forAllSystems (
        pkgs:
        let
          p = self.packages.${pkgs.stdenv.hostPlatform.system};
        in
        {
          default = {
            type = "app";
            program = "${p.gmnb}/bin/gmnb";
          };
          flatpak = {
            type = "app";
            program = "${p.flatpak-builder-script}/bin/gmnb-flatpak";
          };
          update-cargo-sources = {
            type = "app";
            program = "${p.update-cargo-sources}/bin/update-cargo-sources";
          };
        }
      );

      overlays.default = final: _prev: {
        gmnb = final.callPackage ./nix/package.nix { };
      };

      nixosModules.default = import ./nix/module.nix self;

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            pkg-config
            gtk4
            libadwaita
            glib
            adwaita-icon-theme
            flatpak-builder-tools
          ];
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          shellHook = ''
            export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}:${pkgs.adwaita-icon-theme}/share:$XDG_DATA_DIRS
          '';
        };
      });
    };
}
