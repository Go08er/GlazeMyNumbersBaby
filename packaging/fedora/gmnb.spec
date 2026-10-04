%global app_id io.github.Go08er.GlazeMyNumbersBaby
%global dapp_id io.github.Go08er.DontGlazeMyNumbersBaby
# Release builds are stripped by their cargo profiles.
%global debug_package %{nil}

Name:           gmnb
Version:        0.2.0
Release:        1%{?dist}
Summary:        GlazeMyNumbers,Baby: a pointlessly beautiful calculator
# The Outfit typeface embedded in the binary is OFL-1.1.
License:        MIT AND OFL-1.1
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
On x86_64 it needs a CPU from 2013 or newer (Intel Haswell, AMD Excavator,
Ryzen, or later); the dgmnb package runs on any 64-bit PC.

Not affiliated with or endorsed by Microsoft.

%package -n dgmnb
Summary:        Don't Glaze My Numbers, Baby: the lean twin of GMNB
# The Inter and Noto font subsets embedded in the binary are OFL-1.1.
License:        MIT AND OFL-1.1
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
# On x86_64 GMNB is built for x86-64-v3 (2013+ CPUs), behind a launcher built
# for any x86-64 that tells older CPUs so; DGMNB runs anywhere. TARGET_CPU
# pins the CPU the core-math crate's C is compiled for (its build script
# otherwise uses the build machine's own, -march=native).
cargo build --release --locked -p gmnb-launcher
%ifarch x86_64
RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=x86-64-v3" TARGET_CPU=x86-64-v3 \
  cargo build --release --locked -p gmnb --target-dir target/v3
TARGET_CPU=x86-64 cargo build --profile lean --locked -p dgmnb
%else
cargo build --release --locked -p gmnb --target-dir target/v3
cargo build --profile lean --locked -p dgmnb
%endif

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
%license LICENSE apps/gmnb/assets/fonts/OFL-Outfit.txt
%doc README.md
%{_bindir}/gmnb
%{_libexecdir}/gmnb/
%{_datadir}/applications/%{app_id}.desktop
%{_metainfodir}/%{app_id}.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg

%files -n dgmnb
%license LICENSE apps/dgmnb/assets/fonts/OFL-Inter.txt apps/dgmnb/assets/fonts/OFL-Noto.txt apps/dgmnb/assets/LICENSE-smithay-clipboard.txt
%doc README.md
%{_bindir}/dgmnb
%{_datadir}/applications/%{dapp_id}.desktop
%{_metainfodir}/%{dapp_id}.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/%{dapp_id}.svg

%changelog
* Fri Oct 02 2026 Go08er <Go08er@users.noreply.github.com> - 0.2.0-1
- Add the dgmnb subpackage, the lean twin
* Fri Oct 02 2026 Go08er <Go08er@users.noreply.github.com> - 0.1.0-1
- Initial release
