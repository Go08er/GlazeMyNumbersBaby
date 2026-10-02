%global app_id io.github.Go08er.GlazeMyNumbersBaby
# Release builds are stripped by the cargo profile.
%global debug_package %{nil}

Name:           gmnb
Version:        0.1.0
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
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib
Requires:       hicolor-icon-theme

%description
GMNB is a Rust port of the open-source Windows Calculator with a GTK 4
interface: Standard, Scientific, Programmer, Graphing, Date calculation and
13 unit converters including live currency rates, with history and memory.
The original arbitrary-precision engine was ported function-for-function.

Not affiliated with or endorsed by Microsoft.

%prep
%autosetup -n GlazeMyNumbersBaby-%{version}

%build
cargo build --release --locked -p gmnb

%install
install -Dm755 target/release/gmnb %{buildroot}%{_bindir}/gmnb
install -Dm644 packaging/%{app_id}.desktop %{buildroot}%{_datadir}/applications/%{app_id}.desktop
install -Dm644 packaging/%{app_id}.metainfo.xml %{buildroot}%{_metainfodir}/%{app_id}.metainfo.xml
install -Dm644 packaging/icons/%{app_id}.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/%{app_id}.desktop
appstream-util validate-relax --nonet %{buildroot}%{_metainfodir}/%{app_id}.metainfo.xml

%files
%license LICENSE apps/gmnb/assets/fonts/OFL-Outfit.txt
%doc README.md
%{_bindir}/gmnb
%{_datadir}/applications/%{app_id}.desktop
%{_metainfodir}/%{app_id}.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg

%changelog
* Fri Oct 02 2026 Go08er <Go08er@users.noreply.github.com> - 0.1.0-1
- Initial release
