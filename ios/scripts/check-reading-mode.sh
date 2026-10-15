#!/bin/sh
set -eu
IOS_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
IOS_MODE_OUT=${1:-"$IOS_ROOT/../target/ios-reading-mode-checks"}
mkdir -p "$IOS_MODE_OUT"
: "${DEVELOPER_DIR:=/Applications/Xcode.app/Contents/Developer}"
export DEVELOPER_DIR
xcrun swiftc "$IOS_ROOT/Reader/ReadingMode.swift" "$IOS_ROOT/Tests/ReadingModeChecks.swift" -o "$IOS_MODE_OUT/reading-mode-checks"
"$IOS_MODE_OUT/reading-mode-checks"
