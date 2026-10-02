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
cargo build --release --locked -p gmnb
cargo build --profile lean --locked -p dgmnb

%install
install -Dm755 target/release/gmnb %{buildroot}%{_bindir}/gmnb
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
%{_datadir}/applications/%{app_id}.desktop
%{_metainfodir}/%{app_id}.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg

%files -n dgmnb
%license LICENSE apps/dgmnb/assets/fonts/OFL-Inter.txt apps/dgmnb/assets/fonts/OFL-Noto.txt
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
