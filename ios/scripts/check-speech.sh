#!/bin/sh
set -eu
IOS_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
IOS_SPEECH_OUT=${1:-"$IOS_ROOT/../target/ios-speech-checks"}
mkdir -p "$IOS_SPEECH_OUT"
: "${DEVELOPER_DIR:=/Applications/Xcode.app/Contents/Developer}"
export DEVELOPER_DIR
xcrun swiftc -parse-as-library -swift-version 5 \
  "$IOS_ROOT/Reader/ReaderI18n.swift" "$IOS_ROOT/Reader/ReadingLayout.swift" "$IOS_ROOT/Reader/SpeechText.swift" \
  "$IOS_ROOT/Reader/EdgeSpeechToken.swift" "$IOS_ROOT/Reader/EdgeSpeech.swift" \
  "$IOS_ROOT/Tests/SpeechChecks.swift" -o "$IOS_SPEECH_OUT/speech-checks"
"$IOS_SPEECH_OUT/speech-checks" "$IOS_ROOT/Tests/fixtures/speech-token-vectors.json"
