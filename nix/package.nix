{
  lib,
  rustPlatform,
  pkg-config,
  wrapGAppsHook4,
  gtk4,
  libadwaita,
  glib,
  adwaita-icon-theme,
  gsettings-desktop-schemas,
}:
let
  appId = "io.github.Go08er.GlazeMyNumbersBaby";
in
rustPlatform.buildRustPackage {
  pname = "gmnb";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [ "-p" "gmnb" ];
  doCheck = false;
  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook4
  ];
  buildInputs = [
    gtk4
    libadwaita
    glib
    # wrapGAppsHook4 exports these on XDG_DATA_DIRS for the wrapped binary.
    adwaita-icon-theme
    gsettings-desktop-schemas
  ];
  postInstall = ''
    install -Dm644 packaging/${appId}.desktop -t $out/share/applications
    install -Dm644 packaging/${appId}.metainfo.xml -t $out/share/metainfo
    install -Dm644 packaging/icons/${appId}.svg -t $out/share/icons/hicolor/scalable/apps
    install -Dm644 LICENSE apps/gmnb/assets/fonts/OFL-Outfit.txt -t $out/share/licenses/${appId}
  '';
  meta = {
    description = "GlazeMyNumbers,Baby: Windows Calculator ported to Rust, made pointlessly beautiful";
    homepage = "https://github.com/Go08er/GlazeMyNumbersBaby";
    # MIT code; the embedded Outfit typeface is OFL-1.1.
    license = with lib.licenses; [
      mit
      ofl
    ];
    mainProgram = "gmnb";
    platforms = lib.platforms.linux;
  };
}
