{
  lib,
  stdenv,
  rustPlatform,
  pkg-config,
  patchelf,
  wayland,
  libxkbcommon,
  libx11,
  libxcursor,
  libxrandr,
  libxi,
  libxcb,
}:
let
  appId = "io.github.Go08er.DontGlazeMyNumbersBaby";
  # Loaded at run time rather than linked: the keymap library always, and
  # Xlib/XCB only in X11 sessions.
  runtimeLibs = [
    libxkbcommon
    wayland
    libx11
    libxcursor
    libxrandr
    libxi
    libxcb
  ];
in
rustPlatform.buildRustPackage {
  pname = "dgmnb";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [
    "-p"
    "dgmnb"
  ];
  # The workspace's size-optimised profile (see Cargo.toml).
  buildType = "lean";
  doCheck = false;
  nativeBuildInputs = [
    pkg-config
    patchelf
  ];
  buildInputs = [
    wayland
    libxkbcommon
  ];
  postInstall = ''
    install -Dm644 packaging/${appId}.desktop -t $out/share/applications
    install -Dm644 packaging/${appId}.metainfo.xml -t $out/share/metainfo
    install -Dm644 packaging/icons/${appId}.svg -t $out/share/icons/hicolor/scalable/apps
    install -Dm644 LICENSE THIRD-PARTY-LICENSES.txt apps/dgmnb/assets/fonts/OFL-Inter.txt \
      apps/dgmnb/assets/fonts/OFL-Noto.txt apps/dgmnb/assets/LICENSE-smithay-clipboard.txt \
      -t $out/share/licenses/${appId}
  '';
  postFixup = ''
    patchelf --add-rpath ${lib.makeLibraryPath runtimeLibs} $out/bin/dgmnb
  '';
  meta = {
    description = "Don't Glaze My Numbers, Baby: the lean, software-drawn twin of GMNB";
    homepage = "https://github.com/Go08er/GlazeMyNumbersBaby";
    # MIT code; the embedded Inter and Noto subsets are OFL-1.1; the rest are
    # the crates compiled in (THIRD-PARTY-LICENSES.txt).
    license = with lib.licenses; [
      mit
      ofl
      asl20
      bsd2
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
    mainProgram = "dgmnb";
    platforms = lib.platforms.linux;
  };
}
