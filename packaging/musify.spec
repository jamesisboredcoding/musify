# Binary RPM around a binary built by `cargo build --release`; see scripts/package.sh.
Name:           musify
Version:        %{version}
Release:        1%{?dist}
Summary:        A music player for Linux with YouTube Music
License:        MIT
URL:            https://github.com/jamesisboredcoding/musify
Requires:       alsa-lib
Recommends:     yt-dlp
Recommends:     nodejs

%description
Musify plays music from a folder on your computer and your YouTube Music library,
with its own window design, synced lyrics and an optional Discord status.
YouTube songs need yt-dlp (and Node.js for YouTube's challenges).

%install
cp %{_sourcedir}/LICENSE %{_builddir}/ 2>/dev/null || true
install -Dm755 %{_sourcedir}/musify %{buildroot}%{_bindir}/musify
install -Dm644 %{_sourcedir}/musify.desktop %{buildroot}%{_datadir}/applications/musify.desktop
install -Dm644 %{_sourcedir}/musify.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/musify.svg
install -Dm644 %{_sourcedir}/musify-256.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/musify.png

%files
%license LICENSE
%{_bindir}/musify
%{_datadir}/applications/musify.desktop
%{_datadir}/icons/hicolor/scalable/apps/musify.svg
%{_datadir}/icons/hicolor/256x256/apps/musify.png
