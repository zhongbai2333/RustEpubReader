#!/bin/sh
set -eu
IOS_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
I18N_CHECK_OUT=${1:-"$IOS_ROOT/../target/ios-i18n-checks"}
mkdir -p "$I18N_CHECK_OUT/Translations.bundle/i18n"
: "${DEVELOPER_DIR:=/Applications/Xcode.app/Contents/Developer}"
export DEVELOPER_DIR
python3 "$IOS_ROOT/../scripts/check-i18n.py"
cp "$IOS_ROOT/../core/src/i18n/en.json" "$IOS_ROOT/../core/src/i18n/zh_cn.json" "$I18N_CHECK_OUT/Translations.bundle/i18n/"
xcrun swiftc "$IOS_ROOT/Reader/ReaderI18n.swift" "$IOS_ROOT/Tests/I18nChecks.swift" -o "$I18N_CHECK_OUT/i18n-checks"
"$I18N_CHECK_OUT/i18n-checks" "$I18N_CHECK_OUT/Translations.bundle"
