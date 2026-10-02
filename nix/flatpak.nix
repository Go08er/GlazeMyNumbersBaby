# Flatpak packaging, driven from Nix.
#
# packaging/flatpak/<app-id>.json are the canonical (Flathub-style)
# manifests: they build from the git tag with crates from cargo-sources.json.
# Here Nix reuses them verbatim and only swaps the source for an offline
# tarball of the current tree with every crate vendored (hash-pinned through
# Cargo.lock), shared by both twins. The compile itself must happen inside
# the runtime's SDK sandbox so the binary links against the runtime, which
# is why the last step is an impure `nix run .#flatpak` script rather than a
# derivation.
{
  lib,
  runCommand,
  writeText,
  writeShellApplication,
  flatpak-builder,
  appstream,
  git,
  gmnb,
}:
let
  # Self-contained offline source tree: repo + dereferenced vendor/ dir.
  source = runCommand "gmnb-flatpak-source" { } ''
    cp -r ${gmnb.src} $out
    chmod -R u+w $out
    cp -rL ${gmnb.cargoDeps} $out/vendor
    chmod -R u+w $out/vendor
    rm -rf $out/vendor/.cargo $out/vendor/Cargo.lock
    mkdir -p $out/.cargo
    cat > $out/.cargo/config.toml <<'EOF'
    [source.crates-io]
    replace-with = "vendored-sources"

    [source.vendored-sources]
    directory = "vendor"
    EOF
  '';

  # flatpak-builder keeps file modes from `dir` sources, and store paths are
  # read-only, so hand it a tarball with sane modes instead.
  sourceTarball = runCommand "gmnb-flatpak-source.tar" { } ''
    mkdir stage
    cp -r ${source} stage/gmnb
    tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@1 \
      --mode=u+w -cf $out -C stage gmnb
  '';

  # One app: its manifest (with the offline source) and build script.
  mkApp =
    {
      appId,
      # Bundle file name and the name used in messages, e.g. "GMNB".
      name,
      # Script / app name, e.g. "gmnb-flatpak".
      scriptName,
    }:
    let
      upstream = lib.importJSON ../packaging/flatpak/${appId}.json;
      manifest = writeText "${appId}.json" (
        builtins.toJSON (
          upstream
          // {
            modules = map (
              m:
              m
              // {
                sources = [
                  {
                    type = "archive";
                    path = "${sourceTarball}";
                  }
                ];
              }
            ) upstream.modules;
          }
        )
      );

      script = writeShellApplication {
        name = scriptName;
        runtimeInputs = [
          git
          flatpak-builder
          appstream # flatpak-builder runs `appstreamcli compose` on the host
        ];
        text = ''
          usage() {
            cat <<'EOF'
          Build ${name} as a Flatpak bundle.

          usage: nix run .#${scriptName} -- [--install] [--keep-cache] [--out DIR]

            --install      also install the bundle into your user Flatpak installation
            --keep-cache   keep flatpak-builder's cache (.flatpak-builder/) afterwards
            --out DIR      where to write ${name}.flatpak (default: ./dist)

          Run from the repository root. Needs `flatpak` on PATH and network access
          the first time (to install the runtime's SDK + rust-stable from Flathub).
          EOF
          }

          install=0 keep=0 out="$PWD/dist"
          while [ $# -gt 0 ]; do
            case "$1" in
              --install) install=1 ;;
              --keep-cache) keep=1 ;;
              --out) out="$2"; shift ;;
              -h|--help) usage; exit 0 ;;
              *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
            esac
            shift
          done

          if ! command -v flatpak >/dev/null; then
            echo "error: flatpak is not installed (needed to build and run flatpaks)" >&2
            exit 1
          fi

          work="$PWD"
          if [ ! -f "$work/flake.nix" ]; then
            echo "error: run this from the repository root" >&2
            exit 2
          fi
          # Flakes only see files git knows about; untracked files would be
          # silently missing from the bundle.
          if git -C "$work" rev-parse --git-dir >/dev/null 2>&1; then
            untracked=$(git -C "$work" ls-files --others --exclude-standard)
            if [ -n "$untracked" ]; then
              echo "error: these files are untracked, so Nix cannot see them and they would be missing from the build:" >&2
              while IFS= read -r f; do echo "  $f" >&2; done <<< "$untracked"
              echo "run 'git add -A' (staging is enough) and try again." >&2
              exit 1
            fi
          fi

          state="$work/.flatpak-builder"
          builddir="$work/flatpak-build"
          repo="$work/flatpak-repo"
          mkdir -p "$out"

          flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo

          echo ">> manifest: ${manifest}"
          flatpak-builder \
            --user \
            --install-deps-from=flathub \
            --force-clean \
            --state-dir="$state" \
            --repo="$repo" \
            "$builddir" ${manifest}

          flatpak build-bundle \
            --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
            "$repo" "$out/${name}.flatpak" ${appId}

          rm -rf "$builddir"
          if [ "$keep" -eq 0 ]; then rm -rf "$state"; fi

          echo ">> bundle: $out/${name}.flatpak ($(du -h "$out/${name}.flatpak" | cut -f1))"
          if [ "$install" -eq 1 ]; then
            flatpak install --user -y --noninteractive --reinstall "$out/${name}.flatpak"
            echo ">> installed; run with: flatpak run ${appId}"
          fi
        '';
      };
    in
    {
      inherit manifest script;
    };
in
{
  inherit
    source
    sourceTarball
    mkApp
    ;
}
