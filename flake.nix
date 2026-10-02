{
  description = "GMNB and DGMNB — Windows Calculator ported to Rust, twice: pointlessly beautiful, and lean";

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
          dgmnb = pkgs.callPackage ./nix/dgmnb.nix { };
          flatpak = pkgs.callPackage ./nix/flatpak.nix { inherit gmnb; };
          flatpakGmnb = flatpak.mkApp {
            appId = "io.github.Go08er.GlazeMyNumbersBaby";
            name = "GMNB";
            scriptName = "gmnb-flatpak";
          };
          flatpakDgmnb = flatpak.mkApp {
            appId = "io.github.Go08er.DontGlazeMyNumbersBaby";
            name = "DGMNB";
            scriptName = "dgmnb-flatpak";
          };
        in
        {
          inherit gmnb dgmnb;
          default = gmnb;
          flatpak-source = flatpak.source;
          flatpak-manifest = flatpakGmnb.manifest;
          flatpak-builder-script = flatpakGmnb.script;
          flatpak-dgmnb-manifest = flatpakDgmnb.manifest;
          flatpak-dgmnb-builder-script = flatpakDgmnb.script;
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
          gmnb = {
            type = "app";
            program = "${p.gmnb}/bin/gmnb";
          };
          dgmnb = {
            type = "app";
            program = "${p.dgmnb}/bin/dgmnb";
          };
          flatpak = {
            type = "app";
            program = "${p.flatpak-builder-script}/bin/gmnb-flatpak";
          };
          flatpak-dgmnb = {
            type = "app";
            program = "${p.flatpak-dgmnb-builder-script}/bin/dgmnb-flatpak";
          };
          update-cargo-sources = {
            type = "app";
            program = "${p.update-cargo-sources}/bin/update-cargo-sources";
          };
        }
      );

      overlays.default = final: _prev: {
        gmnb = final.callPackage ./nix/package.nix { };
        dgmnb = final.callPackage ./nix/dgmnb.nix { };
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
            # DGMNB
            wayland
            libxkbcommon
          ];
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          # DGMNB loads the keymap library (and Xlib on X11) at run time.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (
            with pkgs;
            [
              libxkbcommon
              wayland
              libx11
              libxcursor
              libxrandr
              libxi
              libxcb
            ]
          );
          shellHook = ''
            export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}:${pkgs.adwaita-icon-theme}/share:$XDG_DATA_DIRS
          '';
        };
      });
    };
}
