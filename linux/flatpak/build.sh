#!/usr/bin/env bash
# Builds and installs the NACUT Flatpak for the current user.
# Needs: flatpak, flatpak-builder, python3 with aiohttp and tomlkit (for flatpak-cargo-generator).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y --noninteractive flathub org.gnome.Platform//51 org.gnome.Sdk//51 org.freedesktop.Sdk.Extension.rust-stable//26.08

# Vendored crate sources for an offline build, generated from linux/Cargo.lock.
if [ ! -f flatpak-cargo-generator.py ]; then
  curl -fsSLO https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
fi
(cd ../ && cargo generate-lockfile --offline 2>/dev/null || cargo generate-lockfile)
python3 flatpak-cargo-generator.py ../Cargo.lock -o cargo-sources.json

flatpak-builder --user --install --force-clean build-dir io.github.cybercyclone.nacut.yml
echo "Installed. Run: flatpak run io.github.cybercyclone.nacut"
