# iOS reader

Native SwiftUI/UIKit shell connected to the same `reader_core` used by desktop
and Android. The old synthetic library and prototype-only position storage have
been replaced by owned EPUB files and the core's library/configuration/journal.
The development bundle remains `com.zhongbai233.epub.reader.ios.preview` so this
work does not replace another installed reader.

## Build and run

Requires full Xcode 26 or newer, a matching simulator runtime, Cargo/Rust and either installed
Apple iOS target standard libraries or matching `rust-src`. Current project
architectures are **arm64 iPhone/iPad and Apple Silicon simulator**; an Intel
simulator / XCFramework distribution is not packaged. Minimum deployment is iOS17.
The Xcode build phase compiles `ios-bridge` and links the Rust static library.
With this machine's Homebrew Rust (matching sources, host-only prebuilt std),
`build-rust.sh` uses local `build-std` via `RUSTC_BOOTSTRAP=1`. It does not replace
the global toolchain. A rustup toolchain with the target installed uses normal
Cargo instead. Debug uses dev profile; Release uses release profile.

```sh
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -project ios/RustEpubReader.xcodeproj -scheme ReaderPreview \
  -configuration Debug -sdk iphonesimulator \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath target/ios-preview build

cargo test -p reader_ios
cargo test -p reader_core --features github-sync
ios/scripts/check-layout.sh
ios/scripts/check-oauth.sh
ios/scripts/check-appearance.sh
ios/scripts/check-reading-mode.sh
ios/scripts/check-speech.sh
ios/scripts/check-contents.sh
ios/scripts/check-i18n.sh
```

Open the project, choose ReaderPreview and an iPhone/iPad simulator, then Run.
Simulator builds use Xcode's local ad-hoc signing and own application/Keychain
access-group entitlements; disabling simulator signing prevents Keychain access.
Physical installation requires the user's Apple signing team. No signing secrets,
App Store upload or TestFlight setup are included.

## Continuous integration

`.github/workflows/ios-build.yml` runs on the Apple Silicon `macos-15` runner
with Xcode 26.3 selected explicitly. Rustup installs both iOS target standard
libraries; the build phase preserves that toolchain instead of preferring
Homebrew Rust. The shared scheme and pinned `Package.resolved` must be committed.

The workflow runs core/bridge tests and all seven host check groups (layout,
OAuth, appearance, reading mode, speech, contents and localization), builds an
ad-hoc signed arm64 simulator app, and archives an **unsigned arm64 Release** for
physical iPhone/iPad, then packages an **unsigned IPA** with a SHA-256 receipt.
Normal CI, `run-build` PR builds and release builds reuse this workflow; it also
supports manual dispatch. Logs and build provenance are uploaded. GitHub Releases
attach the unsigned IPA, checksum, signing instructions and build information;
simulator applications and device archives remain in Actions.

No signing secrets are required for these checks or unsigned IPA packaging.
The IPA requires signing the app and all embedded frameworks with the user's
own identity and matching provisioning profile before installation; see
[unsigned IPA installation](UNSIGNED_IPA.md). The workflow checks package
structure and integrity, not successful installation. There is no XCTest target,
publisher signing/provisioning, TestFlight or App Store upload.
See [build/CI notes](../docs/BUILD_AND_CI.md) and the
[Windows handoff checklist](../docs/WINDOWS_TESTING.md).

## EPUB and reading position

- Native file importer accepts EPUB from Files, including security-scoped URLs.
  The scope remains open until Rust validates and copies the book. File associations
  also support opening an EPUB from another application. An invalid EPUB is rejected.
- Owned UUID EPUBs, per-book configs, index and sync journal live under
  `Library/Application Support/ReaderLibrary`. Documents is an optional Files
  transfer area, not the managed library. Re-import uses SHA-256 identity and keeps
  existing position. Local compressed-file limit is 100 MiB; rendered chapter
  payload limit is 64 MiB. The distinct cloud book limit is 20 MiB.
- The owned JSON C ABI in `ios-bridge` has an explicit C header and matching free
  function. Swift frees each Rust result exactly once. Errors return an envelope;
  debug unwinds are caught before FFI. Release inherits the workspace's panic-abort
  policy. Only one current parsed book is cached; no JNI dependency. Native Keychain also protects the local pairing identity.
- Rust owns EPUB chapters/TOC, heading/paragraph spans, images and core positions.
  UIKit/CoreText lays out the same attributed text used for drawing, preserving
  basic bold/italic styles and fitting image blocks to a physical leaf.
- `EpubBook::render_blocks` adds paragraph/container images for rendering while
  retaining canonical shared block numbers. Supplemental images attach to the next
  original block's start or the last original block's end. They do not add shared
  characters or change desktop/Android stored positions. Inline paragraph images
  appear before their paragraph; arbitrary CSS inline layout is not reproduced.
- Shared positions are `{chapter, block, char_offset}` using Rust Unicode scalars.
  `BlockCoordinate` explicitly maps scalars to/from CoreText UTF-16, including emoji,
  combining marks, styles and synthetic layout newlines. Device page numbers never
  enter shared storage. Reflow resolves the stored text position into new pages.
- Completed native turns commit the leftmost real visible leaf. Cancelled turns
  do not save. TOC jumps are explicit position commits. Saves run in order off the
  main thread and request short native background time to finish local writes.

## GitHub account and sync

- Uses the existing public OAuth client ID and GitHub device authorization flow.
  No client secret is embedded. Native URLSession requests exact `repo` scope,
  handles pending/slow-down/expiry/denial, then verifies the GitHub username and
  `X-OAuth-Scopes` before accepting credentials. The fixed GitHub authorization URL
  opens in SFSafariViewController; user credentials are entered on GitHub.
- OAuth credential data is stored in native Keychain with
  `WhenUnlockedThisDeviceOnly`; never in UserDefaults, library files or the journal.
  Login cancellation invalidates its generation and cannot save a late result.
- Login defaults to sync disabled. The separate enable toggle explains the upload
  destination. `repo` access is broader than one repository; purpose-first UI explains the fixed sync destination and offers the full scope disclosure before and during authorization.
  Sign-out removes this app's local Keychain credential and stops sync; it does not
  revoke the OAuth App's grant on GitHub or delete a remote repository.
- Reuses `github_sync`/`cloud_sync`: private marked `EpubReaderInfo`, EPUB files and
  reading progress first, then background EPUB transfers, 20 MiB/book, bounded
  upload/download batches, size/hash checks,
  per-device journal and SHA compare-and-set retries. Public or unmarked unrelated
  repositories are rejected. Annotations and settings are not in this schema. Daily reading/listening activity is included in the device snapshot; see [reading activity](../docs/READING_ACTIVITY.md).
- Failed books have a persistent account/device retry queue and do not prevent
  other books from completing. A manual retry targets queued files after refreshing
  progress; automatic retries back off and still respect global rate limits.
- Blocking Rust network work runs on a worker. Downloads are staged and parsed
  before publishing; apply merges against the latest local journal under serialized
  storage access. Invalid remote text coordinates cannot overwrite local positions.
  Open-book progress is offered as an explicit jump; reading is not moved silently.
- Foreground sync runs on enable, foreground entry, manual request and every five
  minutes while active. Background, disable, cancellation and account changes
  invalidate Swift intent and native epoch. Stale prepared results cannot apply.
  Already transmitted remote requests can complete; cancelling does not roll them
  back. Rate-limit replies set a cooldown. Tokens and paths are never cloud records.
- PIN transfer uses the shared encrypted pairing/transport protocol with an iOS
  foreground lifecycle. The native local-network purpose is declared. Enter a
  private/local device address and four-digit PIN explicitly; iOS does not yet
  broadcast or discover peers automatically. Listener startup, active sockets and
  staged apply are invalidated on cancellation/background. Peer keys use Keychain;
  decrypted EPUBs are bounded, hashed and parsed before managed-library insertion.
  Incoming progress for the open book is offered, never applied silently.

## Private sensor API and distribution

The current ambient-light implementation dynamically calls private IOHID APIs
on physical devices in both Debug and Release. See
[appearance behavior](../docs/READING_APPEARANCE.md). A successful unsigned archive
only confirms compilation: this implementation is not declared App Store ready,
and sensor access may change across OS versions. [App Review section 2.5.1](https://developer.apple.com/app-store/review/guidelines/#software-requirements) requires public APIs. This review preserves the
existing sensor behavior; it does not substitute public APIs or validate store
acceptance.

## Native reader and remaining scope

Library and sync remain peer destinations (iPhone tabs / iPad sidebar). Reading is
fullscreen. Menus, forms and compact TOC rows are native. Chinese and English
fonts have independent preferences (defaults: PingFang SC and Helvetica). Font
choices are filtered by actual glyph coverage; Songti is offered when resolved.
Legacy single-font preferences migrate once without overwriting later choices.
The reading entry now uses UIKit/CoreText for all three modes: vertical continuous
scrolling, paper curl, and horizontal sliding. The old native/sliding preferences
migrate once into `preview.readingMode`. Switching modes and rotating restore the
chapter/block/Unicode-scalar anchor; visibility of controls does not repaginate.

Continuous scrolling virtualizes paper surfaces in a native scroll view, shortens chapter-ending
rows to their measured text height, and continues into the next chapter without
loading another reader. Heights are exact after reflow, and visible text-line
coordinates drive progress and restoration. Scroll mode fills a window by viewport
height, with neighboring chapters and a small warm cache. Several short chapters
can share one screen; the next chapter stays reachable even when it is shorter
than the viewport. Overlapping paper views keep their coordinates as the window
moves, without reloading a table or resetting the pan offset. The resident window
is also extended when either
visible edge approaches its loaded boundary, even if the chapter at the top has
not changed. This covers a long chapter followed by a short bridge and another
long chapter while a single drag remains held. Measured line maps
are reused while scrolling. A common-mode display link and immediate backing
draws for new paper rows keep rendering active while a finger remains down;
the link stops after dragging/deceleration or detaching the view. Progress writes are coalesced and flushed at
gesture completion, backgrounding and reader exit. The parsed publication remains
in memory; exceptionally large individual chapters still need chapter-level layout.
The two paginated modes currently typeset the entire book.

Both paginated modes show two leaves in landscape when each body column is at
least `max(280pt, 12 * fontSize)` after margins and symmetric horizontal safe areas.
Narrow windows or larger type use one leaf. Curl uses the public `.mid` spine and
double-sided content; sliding moves a two-leaf container. An odd final spread has
one blank leaf. Reduce Motion uses the public sliding transition and an inline
settings explanation. No private page-curl API or custom curl shader is used.
This change does not implement the planned native CSS/JavaScript emulation.

`ios/scripts/check-reading-mode.sh` verifies preference migration and spread
thresholds. `ios/scripts/check-layout.sh` includes continuous row coverage and
within-page position restoration. In Debug, `--verify-reading-modes` reads
Documents/reading-modes.epub (fixture: Tests/fixtures/native-reading-modes.epub),
exercises the actual UIKit controller, and writes
Library/Caches/reading-modes-verification.json with screenshots. The diagnostic
also exercises 32/320/3200-chapter publications, reports initial layout counts and
timings, checks the five-chapter cache for the long-chapter fixture, forward/reverse
pixel continuity, coalesced progress, far jumps and background flushing. A separate
3200-short-chapter fixture checks 240 continuous offset steps with drawing active,
display callbacks in UITrackingRunLoopMode, unchanged pan coordinates and native
animated scrolling in both directions. Simulator timings do not
establish physical-device frame rates.

The mixed long/short fixture checks that the following long chapter is loaded
and has drawn backing contents while the top stays in the preceding chapter,
before any drag-end callback. It also checks uninterrupted offset continuity.

Long-press and held drag select text on the current leaf. The system dictionary,
copy, four highlight colors, notes and bookmarks use native controls. Annotations
and accepted single-character corrections persist in the shared per-book schema;
original EPUBs remain intact. Selection does not extend across a page boundary,
and the notes list currently supports jump/delete rather than editing an old note.

The iOS-specific ONNX Runtime engine is statically linked through the official
Swift package (1.24.2). Existing MacBERT int8 weights and vocabulary are **not**
app resources: explicit first-use download verifies pinned size/SHA256, is
cancellable, and stores backup-excluded files under the managed library. Users can
load cached files offline or remove them. Inference checks the selected chapter,
then presents suggestions for explicit accept/ignore; suggestions are not a
promise of correction quality. No executable plugins are downloaded.

Book opening presents an unfolding cover while real parsing/layout work proceeds,
with a static Reduce Motion fallback and no artificial minimum delay. Three
clients have stable-ID bookshelf selection, batch export and confirmed local
delete. Filtered selections remain selected. iOS local deletion markers prevent
an in-flight download from immediately restoring a removed book; remote copies
remain. About includes app identity, source, relevant licenses and optional daily
release notice. iOS checks release metadata but never installs an app update.

This integration is not complete EPUB feature parity: arbitrary CSS/fixed-layout,
image-only chapters previously excluded by the canonical parser, interactive links,
TXT conversion UI is still outside this change. Existing tables retain
the core's text-row representation. Custom online dictionaries/translation and
correction-contribution UI are not yet ported; the system dictionary is available.

## Read aloud

The reading menu opens a native read-aloud sheet. Edge online voices are the
initial default (`zh-CN-XiaoxiaoNeural`), using the same service as desktop and
Android; system voices are an explicit alternative. Retired service presets are
omitted. Online mode sends the current and prefetched next text chunk to that
service. It does not silently switch voice sources after a failure.

Playback starts at the current logical text position, skips media-only blocks,
and continues across chapters. Pause/resume, previous/next readable block, rate,
application volume and follow controls share one state store. Source, voice,
rate and volume persist locally. Manual page turns, native scrolling or navigation
detach following; the recovery control returns to the speech position. Native
online highlighting uses returned word boundaries; system speech and publisher
layout highlight bounded text units/paragraphs. These highlights are transient.
The publisher view adds an overlay without changing publication layout.

This version supports foreground playback. Leaving the book stops the session;
inactive scenes, audio interruptions and disconnected headphones pause it.
Background/lock-screen playback and physical-device audio acceptance remain
outside the verified scope. See [third-party notices](ThirdParty/README.md)
for the protocol checksum adapter's license and source.

`ios/scripts/check-speech.sh` checks Unicode chunking, scalar coordinates,
inline-image fragments, protocol frames, word offsets and checksum vectors.
A Debug app launched with `--verify-speech` also writes
`Library/Caches/speech-verification.json`: state transitions use generated WAV
with the real player, while online/system synthesis checks use actual returned
audio. The diagnostic uses an isolated preferences suite, not reader settings.

## Verification

Current parity/UI builds, own fixture EPUBs, tests, native screenshots and
fingerprints are in `target/reader-parity-management`. Earlier core integration
and design evidence remain in `target/ios-core-integration` and
`target/ios-design-review`; they do not validate later source changes. Host layout checks verify full contiguous Unicode text,
block/scalar mapping, supplemental media mapping, reflow, cancelled commits and
spread sentinels. URLProtocol OAuth checks use synthetic credentials and intercept
all network requests; Rust GitHub transport tests use a local simulated API.
Neither substitutes for live account authorization or actual cross-device acceptance.

The simulator check uses own synthetic **real EPUB files**, not a bundled mock
library. Native MacBERT load/inference and saved highlight redisplay were observed.
A real encrypted PIN transfer from the iOS listener to an isolated Rust client
validated the received EPUB; this is narrower than three-device acceptance. Physical-device gesture latency, minimum iOS17 runtime and App Store
acceptance remain separate from simulator compilation and observed navigation.

## References

- [Public page curl](https://developer.apple.com/documentation/uikit/uipageviewcontroller/transitionstyle-swift.enum/pagecurl)
- [GitHub device authorization](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow)
- [OAuth scope semantics](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps)
- [Cargo build-std](https://doc.rust-lang.org/cargo/reference/unstable.html#build-std)

- [System dictionary](https://developer.apple.com/documentation/uikit/uireferencelibraryviewcontroller)
- [ONNX Runtime Apple integration](https://onnxruntime.ai/docs/get-started/with-obj-c.html)
- [Local network privacy](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy)
