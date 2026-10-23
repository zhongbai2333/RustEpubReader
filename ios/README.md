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

`.github/workflows/ios-build.yml` runs on the Apple Silicon `macos-26` runner
with Xcode 26.4.1 selected explicitly. Rustup installs both iOS target standard
libraries; the build phase preserves that toolchain instead of preferring
Homebrew Rust. The shared scheme and pinned `Package.resolved` must be committed.

The workflow runs three build/check jobs independently: core/bridge tests plus
seven host logic-check groups, one ad-hoc signed simulator application build,
and the unsigned arm64 Release archive/IPA. The final job requires all three
jobs and verifies package integrity and source provenance before uploading the
usual artifact bundle. Hosted jobs do not boot simulators, build UI-test runners,
or execute UI cases.

Normal CI, `run-build` PR builds and release builds reuse this workflow; it also
supports manual dispatch. Logs and build provenance are uploaded. GitHub Releases
attach the unsigned IPA and include signing instructions in the release notes.
Checksum receipts, build information, simulator applications and device archives
remain in Actions.

Native UI automation runs locally. The `ReaderUITests` target and collector
retain the TXT/service/contribution cases, five selection modes with three
repetitions each, and three system Reduce Motion repetitions. Files checks
remain a separate local suite. Missing, failed or incomplete local results are
not counted as passes. See [local mobile checks](../docs/MOBILE_RUNTIME_CI.md).

No signing secrets are required for these checks or unsigned IPA packaging.
The IPA requires signing the app and all embedded frameworks with the user's
own identity and matching provisioning profile before installation; see
[unsigned IPA installation](UNSIGNED_IPA.md). The workflow checks package
structure and integrity, not successful installation on a physical device. The isolated
`ReaderUITests` XCTest target exercises simulator UI interactions; publisher
signing/provisioning, TestFlight and App Store upload remain unconfigured. See
[local mobile checks](../docs/MOBILE_RUNTIME_CI.md) for invocation and evidence.
See [build/CI notes](../docs/BUILD_AND_CI.md) and the
[Windows handoff checklist](../docs/WINDOWS_TESTING.md).

## EPUB / TXT import and reading position

- The native file importer and document associations accept EPUB and TXT. EPUB
  scoped access remains open until Rust validates and copies the book. TXT is copied
  with scoped access into private staging before background analysis/conversion; the
  source file is not changed. The picker supports multiple EPUBs, imported
  sequentially; one failed file does not stop the remaining files. TXT files enter
  individual conversion sheets.
- TXT conversion uses the shared mixed-heading detector, optional unnumbered-heading
  detection and custom regex rules. The native sheet supports per-heading exclusion,
  body excerpts and adding a heading from source-line search. It previews the first
  500 chapters, displays up to 80 source-search matches, and converts the full text.
  Input is bounded at 64 MiB; absent selected headings use explicitly labelled
  length-based parts. Cancelling a boundary keeps its text in the body. Ambiguous
  unnumbered titles still need user review. Normal dismissal releases staged files;
  the first import after a process restart clears stale UUID directories in the
  private TXT staging namespace. See the [capability audit](../docs/PLATFORM_PARITY.md).
- Owned UUID EPUBs, per-book configs, index and sync journal live under
  `Library/Application Support/ReaderLibrary`. Documents is an optional Files
  transfer area, not the managed library. Re-import uses SHA-256 identity and keeps
  existing position. Local compressed-file limit is 1 GiB; each rendered chapter
  payload remains bounded at 64 MiB. Open returns a body-free catalogue; decoded
  chapter bodies use a bounded cache. Native stylesheet/script structures are
  created only on demand.
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
- Reuses `github_sync`/`cloud_sync` and the private marked `EpubReaderInfo`.
  Reading progress is reconciled before background EPUB transfers, up to 1 GiB
  per book, with bounded batches and size/hash checks. Files above 20 MiB use
  8 MiB private draft-release assets and account-scoped resumable chunk caches;
  v1 blobs stay readable. Large-file metadata is optional so older clients can
  still exchange progress. Files are staged on disk and promoted only after
  complete hash verification, with per-device journals and SHA compare-and-set
  retries. Public or unmarked unrelated
  repositories are rejected. Daily reading/listening activity is included in the
  device snapshot; see [reading activity](../docs/READING_ACTIVITY.md). Bookmarks,
  highlights and notes synchronize through separate causal annotation objects and
  indices, applied after progress and transferred before EPUB files. Concurrent
  note edits are preserved and explicit deletions propagate. Settings remain
  local; see [annotation sync](../docs/ANNOTATION_SYNC.md).
- Failed books have a persistent account/device retry queue and do not prevent
  other books from completing. A manual retry targets queued files after refreshing
  progress; automatic retries back off and still respect global rate limits.
- Blocking Rust network work runs on a worker. Downloads are staged and parsed
  before publishing; apply merges against the latest local journal under serialized
  storage access. Invalid remote text coordinates cannot overwrite local positions.
  Accepted newer progress automatically moves the open reader with a native
  transition and a bottom-left return button. Long-press the button for the
  previous chapter/page. A newer local turn cancels an obsolete pending jump.
- Foreground sync runs on enable, foreground entry, manual request and every five
  minutes while active. Background, disable, cancellation and account changes
  invalidate Swift intent and native epoch. Stale prepared results cannot apply.
  Already transmitted remote requests can complete; cancelling does not roll them
  back. Rate-limit replies set a cooldown. Tokens and paths are never cloud records.
- PIN transfer uses the shared encrypted pairing/transport protocol with an iOS
  foreground lifecycle. The native local-network purpose is declared. Bonjour advertises and discovers `_epubreader._tcp` while foregrounded;
  manual private/local IPv4 addresses remain available. First pairing needs a
  four-digit PIN; saved peers have identity-bound reconnect and throttled
  foreground automatic reconnect. Discovery is only an address hint, not
  authentication. Listener startup, active sockets and
  staged apply are invalidated on cancellation/background. Peer keys use Keychain;
  decrypted EPUBs are bounded, hashed and parsed before managed-library insertion.
  Accepted newer progress for the open book uses the same automatic jump and
  return history as GitHub sync.

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
The reading entry uses UIKit/CoreText for continuous scrolling and pagination.
Paged reading offers system paper curl, sliding, cover transitions and page changes
without animation. Legacy mode preferences migrate into distinct reading-mode
and page-animation settings. Switching modes and rotating restore the
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
gesture completion, backgrounding and reader exit. The publication catalogue remains available while chapter bodies load on demand.
Paged reading reuses a nearby chapter layout window and trims it after completed
turns; it no longer typesets the entire book when it opens. Large individual chapters
still require chapter-level layout. See [pagination and bounded chapter loading](../docs/EPUB_PAGINATION_AND_PERFORMANCE.md#chapter-loading).

Paged reading can show two leaves in landscape when each body column is at
least `max(280pt, 12 * fontSize)` after margins and symmetric horizontal safe areas.
Narrow windows or larger type use one leaf. Curl uses the public `.mid` spine and
double-sided content; sliding moves a two-leaf container. An odd final spread has
one blank leaf. Reduce Motion uses the public sliding transition and an inline
settings explanation. No private page-curl API or custom curl shader is used.
Native/Mixed/Classic styles use the shared native CSS subset. Book-local scripts
use the bounded QuickJS adapter when enabled; the adapter exposes only the documented native interfaces, without network or filesystem access.
See [native typography](../docs/EPUB_NATIVE_TYPOGRAPHY.md) and
[media and interactivity](../docs/EPUB_MEDIA_AND_INTERACTIVITY.md).

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
and note details support editing existing text, inspecting source revisions and
saving a revised note that resolves the concurrent versions shown to the user. Bookmarks, highlights and notes can also
be exported as Markdown through the system share/save interface.

The iOS-specific ONNX Runtime engine is statically linked through the official
Swift package (1.24.2). Existing MacBERT int8 weights and vocabulary are **not**
app resources: explicit first-use download verifies pinned size/SHA256, is
cancellable, and stores backup-excluded files under the managed library. Users can
load cached files offline or remove them. Inference checks the selected chapter,
then presents suggestions for explicit accept/ignore; suggestions are not a
promise of correction quality. No executable plugins are downloaded.

Book opening fades the cover into a paper-colored surface and expands it to the
reader bounds while parsing/layout work proceeds,
with a static Reduce Motion fallback and no artificial minimum delay. Three
clients have stable-ID bookshelf selection, batch original export and confirmed
archive actions. Filtered selections remain selected. Local-only archiving hides
the shelf entry while retaining its EPUB and annotations. With GitHub sync enabled,
lifecycle records propagate between updated clients; archived originals are only
unlinked after a verified remote copy exists. The archive list has restore and
separately confirmed permanent-delete actions. See [archive and permanent deletion](../docs/GITHUB_SYNC.md#archive-and-permanent-deletion).
About includes app identity, source, relevant licenses and optional daily
release notice. iOS checks release metadata but never installs an app update.

Native rendering supports illustrations, ruby, basic text tables, note references
and book-local links. It preserves canonical text positions while applying the
supported style subset. Fixed-layout fidelity, vertical text and arbitrary
browser CSS/JavaScript remain outside that subset. The system dictionary and
TXT conversion/correction sheet are available. Custom online dictionary/translation results now use native panels, with OpenAI
compatible and JSON service settings. Keys use Keychain; no built-in cloud service
is supplied. Accepted MacBERT corrections can be reviewed, removed from the
selection and explicitly submitted as public samples. System speech and remaining platform-specific behavior are recorded in the [capability audit](../docs/PLATFORM_PARITY.md).

The settings include portable library backup export, verification and merge
restore through the native Files interface. Backups contain book data and
reading records, but exclude account credentials, pairing keys, downloaded
fonts/models and global UI settings. They are unencrypted ZIP packages; see
[backup format and restore behavior](../docs/BACKUPS.md).

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

## Checks

Run the Swift host checks under `ios/scripts/` and shared Rust/bridge tests listed
in [build and CI](../docs/BUILD_AND_CI.md). Native UI test selection and diagnostic
installation requirements are documented in [local mobile checks](../docs/MOBILE_RUNTIME_CI.md).
Use isolated application IDs and data for scripted checks. Test device permissions,
external file providers, Keychain, camera input and cross-device transfers on
physical devices separately from simulator checks.

## References

- [Public page curl](https://developer.apple.com/documentation/uikit/uipageviewcontroller/transitionstyle-swift.enum/pagecurl)
- [GitHub device authorization](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow)
- [OAuth scope semantics](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/scopes-for-oauth-apps)
- [Cargo build-std](https://doc.rust-lang.org/cargo/reference/unstable.html#build-std)

- [System dictionary](https://developer.apple.com/documentation/uikit/uireferencelibraryviewcontroller)
- [ONNX Runtime Apple integration](https://onnxruntime.ai/docs/get-started/with-obj-c.html)
- [Local network privacy](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy)
