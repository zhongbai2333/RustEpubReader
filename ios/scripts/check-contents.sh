#!/bin/sh
set -eu
IOS_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
IOS_CONTENTS_OUT=${1:-"$IOS_ROOT/../target/ios-contents-checks"}
mkdir -p "$IOS_CONTENTS_OUT"
mkdir -p "$IOS_CONTENTS_OUT/i18n"
cp "$IOS_ROOT/../core/src/i18n/en.json" "$IOS_ROOT/../core/src/i18n/zh_cn.json" "$IOS_CONTENTS_OUT/i18n/"
: "${DEVELOPER_DIR:=/Applications/Xcode.app/Contents/Developer}"
export DEVELOPER_DIR
xcrun swiftc "$IOS_ROOT/Reader/ReaderI18n.swift" "$IOS_ROOT/Reader/ReaderContentsModel.swift" "$IOS_ROOT/Tests/ContentsChecks.swift" -o "$IOS_CONTENTS_OUT/contents-checks"
"$IOS_CONTENTS_OUT/contents-checks" -reader.language zh_cn
