#!/bin/sh
set -eu
IOS_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
IOS_CHECK_OUT=${1:-"$IOS_ROOT/../target/ios-oauth-checks"}
mkdir -p "$IOS_CHECK_OUT"
: "${DEVELOPER_DIR:=/Applications/Xcode.app/Contents/Developer}"
export DEVELOPER_DIR
IOS_MAC_SDK=$(xcrun --sdk macosx --show-sdk-path)
IOS_HOST_ARCH=$(uname -m)
xcrun swiftc -target "$IOS_HOST_ARCH-apple-macosx14.0" -sdk "$IOS_MAC_SDK" \
 "$IOS_ROOT/Reader/GitHubOAuth.swift" "$IOS_ROOT/Tests/OAuthChecks.swift" -o "$IOS_CHECK_OUT/oauth-checks"
"$IOS_CHECK_OUT/oauth-checks"
