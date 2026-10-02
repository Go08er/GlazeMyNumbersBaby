# nix develop -f tools/bench/shell.nix   (from the repository root)
let
  flake = builtins.getFlake (toString ../..);
  pkgs = flake.inputs.nixpkgs.legacyPackages.${builtins.currentSystem};
  runtime = with pkgs; [ wayland libxkbcommon fontconfig freetype libdecor ];
in
pkgs.mkShell {
  packages = with pkgs; [
    cargo rustc pkg-config cmake gtk4 wayland wayland-protocols wayland-scanner
    libxkbcommon fontconfig freetype pango cairo libdecor dbus libGL weston bc
    libx11 libxext libxinerama libxcursor libxrender libxfixes libxft
  ];
  # winit and friends dlopen these.
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtime;
}
