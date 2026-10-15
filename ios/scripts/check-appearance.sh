#!/bin/sh
set -eu
IOS_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
IOS_APPEARANCE_OUT=${1:-"$IOS_ROOT/../target/ios-appearance-checks"}
mkdir -p "$IOS_APPEARANCE_OUT"
: "${DEVELOPER_DIR:=/Applications/Xcode.app/Contents/Developer}"
export DEVELOPER_DIR
xcrun swiftc "$IOS_ROOT/Reader/ReadingAppearance.swift" "$IOS_ROOT/Tests/ReadingAppearanceChecks.swift" -o "$IOS_APPEARANCE_OUT/appearance-checks"
"$IOS_APPEARANCE_OUT/appearance-checks"
xcrun swiftc "$IOS_ROOT/Reader/ReadingAppearance.swift" "$IOS_ROOT/Reader/AppearanceLinkage.swift" "$IOS_ROOT/Tests/AppearanceLinkageChecks.swift" -o "$IOS_APPEARANCE_OUT/linkage-checks"
"$IOS_APPEARANCE_OUT/linkage-checks" "$IOS_ROOT/../core/tests/fixtures/appearance-linkage.json"
xcrun swiftc "$IOS_ROOT/Reader/AmbientAppearance.swift" "$IOS_ROOT/Tests/AmbientAppearanceChecks.swift" -o "$IOS_APPEARANCE_OUT/ambient-checks"
"$IOS_APPEARANCE_OUT/ambient-checks"
