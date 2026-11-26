#!/bin/bash
# Builds the Rust core (core/nacut-ffi) for macOS (Apple Silicon + Intel), generates the Swift
# bindings, and packages NacutCore/nacutFFI.xcframework for the NacutCore Swift package.
#
#   apple/scripts/build-core.sh [debug|release]   (default: release)
set -euo pipefail

PROFILE="${1:-release}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
CORE="$HERE/../core"
PKG="$HERE/NacutCore"
OUT="$CORE/target/apple"

# Rust from rustup (Homebrew keg) or ~/.cargo, even when Xcode runs this with a minimal PATH.
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:/usr/local/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET=26.0

FLAGS=()
[ "$PROFILE" = "release" ] && FLAGS+=(--release)

TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
for t in "${TARGETS[@]}"; do
  rustup target add "$t" >/dev/null 2>&1 || true
  cargo build --locked --manifest-path "$CORE/Cargo.toml" -p nacut-ffi --lib --target "$t" ${FLAGS[@]+"${FLAGS[@]}"}
done

rm -rf "$OUT" && mkdir -p "$OUT/include"
lipo -create \
  "$CORE/target/aarch64-apple-darwin/$PROFILE/libnacut_ffi.a" \
  "$CORE/target/x86_64-apple-darwin/$PROFILE/libnacut_ffi.a" \
  -output "$OUT/libnacut_ffi.a"

# Bindings from the host build's dylib (library mode reads the UniFFI metadata and runs
# `cargo metadata`, so it must run inside the workspace).
HOST_DYLIB="$CORE/target/aarch64-apple-darwin/$PROFILE/libnacut_ffi.dylib"
(cd "$CORE" && cargo run -q --locked -p nacut-ffi --bin uniffi-bindgen -- \
  generate --library "$HOST_DYLIB" --language swift --out-dir "$OUT/gen" 2>&1 | grep -v swiftformat || true)
test -f "$OUT/gen/nacut.swift" || { echo "Swift binding generation failed" >&2; exit 1; }

cp "$OUT/gen/nacutFFI.h" "$OUT/include/"
cat > "$OUT/include/module.modulemap" <<'MAP'
module nacutFFI {
    header "nacutFFI.h"
    export *
}
MAP

rm -rf "$PKG/nacutFFI.xcframework"
xcodebuild -create-xcframework -library "$OUT/libnacut_ffi.a" -headers "$OUT/include" \
  -output "$PKG/nacutFFI.xcframework" >/dev/null
cp "$OUT/gen/nacut.swift" "$PKG/Sources/NacutCore/nacut.swift"
echo "NacutCore ready: $(du -sh "$PKG/nacutFFI.xcframework" | cut -f1) xcframework, $(wc -l < "$PKG/Sources/NacutCore/nacut.swift") lines of Swift bindings"
