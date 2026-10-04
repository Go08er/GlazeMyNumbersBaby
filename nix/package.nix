{
  lib,
  stdenv,
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
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
  src = lib.cleanSource ../.;
  x86 = stdenv.hostPlatform.isx86_64;
  # Installed as bin/gmnb: built for any x86-64, it tells an older CPU why
  # GMNB (built for x86-64-v3 below) can't run there, else runs it.
  launcher = rustPlatform.buildRustPackage {
    pname = "gmnb-launcher";
    inherit version src;
    cargoLock.lockFile = ../Cargo.lock;
    cargoBuildFlags = [
      "-p"
      "gmnb-launcher"
    ];
    doCheck = false;
  };
in
rustPlatform.buildRustPackage {
  pname = "gmnb";
  inherit version src;
  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [ "-p" "gmnb" ];
  doCheck = false;
  # On x86_64, for x86-64-v3 (CPUs with AVX2).
  env = lib.optionalAttrs x86 {
    RUSTFLAGS = "-C target-cpu=x86-64-v3";
  };
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
  # The launcher execs the real binary with its environment, so only it
  # needs wrapping.
  dontWrapGApps = true;
  postInstall = ''
    install -Dm755 $out/bin/gmnb -t $out/libexec/gmnb
    install -Dm755 ${launcher}/bin/gmnb-launcher $out/bin/gmnb
    install -Dm644 packaging/${appId}.desktop -t $out/share/applications
    install -Dm644 packaging/${appId}.metainfo.xml -t $out/share/metainfo
    install -Dm644 packaging/icons/${appId}.svg -t $out/share/icons/hicolor/scalable/apps
    install -Dm644 LICENSE THIRD-PARTY-LICENSES.txt apps/gmnb/assets/fonts/OFL-Outfit.txt \
      -t $out/share/licenses/${appId}
  '';
  preFixup = ''
    wrapGApp $out/bin/gmnb
  '';
  meta = {
    description = "GlazeMyNumbers,Baby: Windows Calculator ported to Rust, made pointlessly beautiful";
    homepage = "https://github.com/Go08er/GlazeMyNumbersBaby";
    # MIT code; the embedded Outfit typeface is OFL-1.1; the rest are the
    # crates compiled in (THIRD-PARTY-LICENSES.txt).
    license = with lib.licenses; [
      mit
      ofl
      asl20
      bsd3
      isc
      unicode-30
      zlib
      # webpki-roots' data; not in lib.licenses.
      {
        spdxId = "CDLA-Permissive-2.0";
        fullName = "Community Data License Agreement Permissive 2.0";
        url = "https://cdla.dev/permissive-2-0/";
        free = true;
      }
    ];
    mainProgram = "gmnb";
    platforms = lib.platforms.linux;
  };
}
