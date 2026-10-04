%global app_id io.github.Go08er.GlazeMyNumbersBaby
%global dapp_id io.github.Go08er.DontGlazeMyNumbersBaby
# The cargo profiles strip release builds (once %build drops the flags of
# Fedora's that would override them), so there is no debuginfo to package.
%global debug_package %{nil}

Name:           gmnb
Version:        0.2.0
Release:        1%{?dist}
Summary:        GlazeMyNumbers,Baby: a pointlessly beautiful calculator
# The Outfit typeface embedded in the binary is OFL-1.1; the rest are the
# crates compiled in (THIRD-PARTY-LICENSES.txt).
License:        MIT AND OFL-1.1 AND Apache-2.0 AND BSD-3-Clause AND CDLA-Permissive-2.0 AND ISC AND Unicode-3.0 AND Zlib
URL:            https://github.com/Go08er/GlazeMyNumbersBaby
Source0:        %{url}/archive/v%{version}/GlazeMyNumbersBaby-%{version}.tar.gz

BuildRequires:  cargo >= 1.92
BuildRequires:  rust >= 1.92
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4) >= 4.18
BuildRequires:  pkgconfig(libadwaita-1) >= 1.7
BuildRequires:  pkgconfig(pango) >= 1.56
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib
Requires:       hicolor-icon-theme

%description
GMNB is a Rust port of the open-source Windows Calculator with a GTK 4
interface: Standard, Scientific, Programmer, Graphing, Date calculation and
13 unit converters including live currency rates, with history and memory.
The original arbitrary-precision engine was ported function-for-function.
On x86_64 it needs a CPU with AVX2 (x86-64-v3): Intel Core from Haswell
(2013) or AMD from Excavator (2015) on, though not every Pentium, Celeron
or Atom. The dgmnb package runs on any 64-bit PC.

Not affiliated with or endorsed by Microsoft.

%package -n dgmnb
Summary:        Don't Glaze My Numbers, Baby: the lean twin of GMNB
# The Inter and Noto font subsets embedded in the binary are OFL-1.1; the
# rest are the crates compiled in (THIRD-PARTY-LICENSES.txt).
License:        MIT AND OFL-1.1 AND Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND CDLA-Permissive-2.0 AND ISC AND Unicode-3.0 AND Zlib
Requires:       hicolor-icon-theme
# Loaded at run time, so not picked up automatically.
Requires:       libxkbcommon
Recommends:     libX11 libXcursor libXrandr libXi

%description -n dgmnb
DGMNB is the same Rust port of the open-source Windows Calculator as GMNB,
with every mode and feature, drawn in software with a plain interface. It
needs no GPU, uses around 13 MB of memory and stays idle while unused.

Not affiliated with or endorsed by Microsoft.

%prep
%autosetup -n GlazeMyNumbersBaby-%{version}

%build
# %%set_build_flags exports Fedora's RUSTFLAGS, whose optimisation, debuginfo,
# codegen-unit and strip settings would override the cargo profiles (DGMNB's
# is optimised for size; release builds are stripped). Keep the rest.
flags= c=
for f in ${RUSTFLAGS:-}; do
  # "-C x=y" is "-Cx=y" split in two.
  if [ "$f" = -C ]; then c=-C; continue; fi
  case "$c$f" in
    -Copt-level=*|-Cdebuginfo=*|-Ccodegen-units=*|-Cstrip=*) ;;
    *) flags="$flags $c$f" ;;
  esac
  c=
done
export RUSTFLAGS="$flags"
# On x86_64 GMNB is built for x86-64-v3 (CPUs with AVX2), behind a launcher
# built for any x86-64 that tells other CPUs so; DGMNB runs anywhere (its
# CORE-MATH takes an x86-64-v3 build of the C where the CPU has it).
cargo build --release --locked -p gmnb-launcher
%ifarch x86_64
RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=x86-64-v3" \
  cargo build --release --locked -p gmnb --target-dir target/v3
%else
cargo build --release --locked -p gmnb --target-dir target/v3
%endif
cargo build --profile lean --locked -p dgmnb

%install
install -Dm755 target/release/gmnb-launcher %{buildroot}%{_bindir}/gmnb
install -Dm755 target/v3/release/gmnb %{buildroot}%{_libexecdir}/gmnb/gmnb
install -Dm644 packaging/%{app_id}.desktop %{buildroot}%{_datadir}/applications/%{app_id}.desktop
install -Dm644 packaging/%{app_id}.metainfo.xml %{buildroot}%{_metainfodir}/%{app_id}.metainfo.xml
install -Dm644 packaging/icons/%{app_id}.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg
install -Dm755 target/lean/dgmnb %{buildroot}%{_bindir}/dgmnb
install -Dm644 packaging/%{dapp_id}.desktop %{buildroot}%{_datadir}/applications/%{dapp_id}.desktop
install -Dm644 packaging/%{dapp_id}.metainfo.xml %{buildroot}%{_metainfodir}/%{dapp_id}.metainfo.xml
install -Dm644 packaging/icons/%{dapp_id}.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{dapp_id}.svg

%check
for id in %{app_id} %{dapp_id}; do
  desktop-file-validate %{buildroot}%{_datadir}/applications/$id.desktop
  appstream-util validate-relax --nonet %{buildroot}%{_metainfodir}/$id.metainfo.xml
done

%files
%license LICENSE THIRD-PARTY-LICENSES.txt apps/gmnb/assets/fonts/OFL-Outfit.txt
%doc README.md
%{_bindir}/gmnb
%{_libexecdir}/gmnb/
%{_datadir}/applications/%{app_id}.desktop
%{_metainfodir}/%{app_id}.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg

%files -n dgmnb
%license LICENSE THIRD-PARTY-LICENSES.txt apps/dgmnb/assets/fonts/OFL-Inter.txt apps/dgmnb/assets/fonts/OFL-Noto.txt apps/dgmnb/assets/LICENSE-smithay-clipboard.txt
%doc README.md
%{_bindir}/dgmnb
%{_datadir}/applications/%{dapp_id}.desktop
%{_metainfodir}/%{dapp_id}.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/%{dapp_id}.svg

%changelog
* Sun Oct 04 2026 Go08er <Go08er@users.noreply.github.com> - 0.2.0-1
- Add the dgmnb subpackage, the lean twin
* Fri Oct 02 2026 Go08er <Go08er@users.noreply.github.com> - 0.1.0-1
- Initial release
