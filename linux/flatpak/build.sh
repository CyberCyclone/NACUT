#!/usr/bin/env bash
# Builds and installs the NACUT Flatpak for the current user.
# Needs: flatpak, flatpak-builder, python3 with aiohttp and tomlkit (for flatpak-cargo-generator).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y --noninteractive flathub org.gnome.Platform//51 org.gnome.Sdk//51 org.freedesktop.Sdk.Extension.rust-stable//26.08

# Vendored crate sources for an offline build, from the committed linux/Cargo.lock (never regenerated
# here, so the build uses exactly the reviewed dependency versions).
# The generator is pinned to a commit of flatpak-builder-tools rather than its moving master branch.
GENERATOR_COMMIT=9d86d336a39b9af38b86f114b6ce5820fe260f91
if [ ! -f "flatpak-cargo-generator-$GENERATOR_COMMIT.py" ]; then
  curl -fsSL -o "flatpak-cargo-generator-$GENERATOR_COMMIT.py" \
    "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$GENERATOR_COMMIT/cargo/flatpak-cargo-generator.py"
fi
(cd ../ && cargo metadata --locked --format-version 1 >/dev/null) || { echo "linux/Cargo.lock is out of date; update and review it first" >&2; exit 1; }
python3 "flatpak-cargo-generator-$GENERATOR_COMMIT.py" ../Cargo.lock -o cargo-sources.json

flatpak-builder --user --install --force-clean build-dir io.github.cybercyclone.nacut.yml
echo "Installed. Run: flatpak run io.github.cybercyclone.nacut"
