#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
# Preserve the selected rustup toolchain in CI; Homebrew is a local fallback.
export PATH="$PATH:/opt/homebrew/bin"
if [ -n "${READER_RUST_BIN:-}" ]; then
    export PATH="$READER_RUST_BIN:$PATH"
fi
case "${PLATFORM_NAME:-iphonesimulator}" in
    iphoneos) TARGET=aarch64-apple-ios ;;
    iphonesimulator) case "${ARCHS:-arm64}" in
        x86_64) TARGET=x86_64-apple-ios ;;
        *) TARGET=aarch64-apple-ios-sim ;;
    esac ;;
    *) exit 1 ;;
esac
# Rust's simulator triple is not a valid Clang deployment triple. QuickJS
# bindgen passes TARGET verbatim, so provide the public Clang equivalent last.
if [ "$TARGET" = aarch64-apple-ios-sim ]; then
    export BINDGEN_EXTRA_CLANG_ARGS="${BINDGEN_EXTRA_CLANG_ARGS:-} --target=arm64-apple-ios${IPHONEOS_DEPLOYMENT_TARGET:-17.0}-simulator"
fi
CARGO_PROFILE_ARGS=(--profile dev)
CARGO_PROFILE_DIR=debug
if [ "${CONFIGURATION:-Debug}" = Release ]; then
    CARGO_PROFILE_ARGS=(--release)
    CARGO_PROFILE_DIR=release
fi
# Homebrew Rust includes matching std source, but only host prebuilt std. Build
# matching iOS std locally; no global toolchain replacement. rustup users with
# the target installed do not need this fallback.
if [ -d "$(rustc --print sysroot)/lib/rustlib/$TARGET/lib" ]; then
    cargo build --locked "${CARGO_PROFILE_ARGS[@]}" -p reader_ios --target "$TARGET"
else
    RUSTC_BOOTSTRAP=1 cargo build --locked "${CARGO_PROFILE_ARGS[@]}" -Z build-std=std,panic_abort -p reader_ios --target "$TARGET"
fi
mkdir -p "$BUILT_PRODUCTS_DIR/RustCore"
RUST_TARGET_DIR=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cp "$RUST_TARGET_DIR/$TARGET/$CARGO_PROFILE_DIR/libreader_ios.a" "$BUILT_PRODUCTS_DIR/RustCore/libreader_ios.a"
