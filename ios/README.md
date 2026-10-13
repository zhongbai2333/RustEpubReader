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

The workflow runs core/bridge tests and the host layout/OAuth checks, builds an
ad-hoc signed arm64 simulator app, and archives an **unsigned arm64 Release** for
physical iPhone/iPad. Normal CI, `run-build` PR builds and release validation reuse
this workflow; it also supports manual dispatch. Logs and build provenance are
uploaded. Release validation artifacts stay in Actions rather than masquerading
as installable GitHub Release assets.

No signing secrets are required for these checks. This is build validation, not
XCTest/UI gesture coverage or distribution: there is no XCTest target, exported
IPA, provisioning-profile installation, TestFlight or App Store upload. Device
installation/export requires configuring the owner's signing team separately.
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
  repositories are rejected. Annotations/settings/stats are not in this schema.
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

## Native reader and remaining scope

Library and sync remain peer destinations (iPhone tabs / iPad sidebar). Reading is
fullscreen. Menus, forms and compact TOC rows are native. Defaults remain PingFang
SC; Songti is offered when resolved. Paper/light/dark themes and curl/scroll modes
are available. Equal top/bottom safe insets and minimum 24pt horizontal text inset
are independent of tool visibility; hiding controls does not repaginate.

The public `UIPageViewController.pageCurl` data source supplies physical leaves,
including adjacent chapters. Wide iPad (at least 1000pt, body size at most24pt)
uses `.mid` spine/double-sided content; smaller windows use one leaf. An odd final
spread gets one blank sentinel. Reduce Motion chooses public scroll transition.
No private `_UIPageCurl` access or custom curl shader is used.

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
TXT conversion UI and TTS are still outside this change. Existing tables retain
the core's text-row representation. Custom online dictionaries/translation and
correction-contribution UI are not yet ported; the system dictionary is available.

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
