# Native reader contracts

The reader remains native on desktop, Android and iOS. This document describes
coordinates at persistence and bridge boundaries; it does not introduce another
cloud schema or imply browser-level CSS/JavaScript compatibility.

## Coordinates

- `chapter` indexes the canonical EPUB spine chapter list, not a displayed TOC
  label, chapter-number string or artificial page title.
- `block` indexes the original canonical chapter blocks. Inserted ruby, list
  markers, table fragments, images and generated headings must not renumber it.
- Persisted `char_offset`, highlight offsets and exported excerpts count Unicode
  scalar values. A scalar is neither a UTF-8 byte nor a user-perceived grapheme.
  Combining marks and ZWJ components each occupy their own scalar position.
- A highlight is a half-open range. Page/scroll drawing intersects it with the
  current source fragment and the correct chapter; it need not fit wholly inside
  one page to be visible.
- Android Compose layout/fragments and iOS attributed strings use UTF-16 offsets
  internally. Conversion occurs at the boundary. An offset inside a surrogate
  pair rounds down to the preceding scalar boundary. Bounds are clamped; an empty
  intersection paints nothing.

`tests/fixtures/reader-coordinates.json` is the shared contract fixture, including
non-BMP emoji/CJK, combining marks, ZWJ sequences and empty input. Kotlin tests
exercise the production `ReaderTextCoordinates` conversions and fragment
intersections. iOS runtime checks use the production `BlockCoordinate` and
`LaidOutChapter` mapping; Rust tests exercise the real Markdown excerpt path.
Android draw projections are remembered for their source/fragment instead of
rebuilding whole-paragraph conversions in the draw loop.

Historical Android highlights could have stored UTF-16 values without marking
their unit. Their platform/unit cannot be reliably inferred from a UUID, timestamp
or the text alone. No bulk migration or guessed rewrite is performed. New Android
selections use the common scalar contract, and stored contract-compliant marks
render consistently. This does not claim recovery of ambiguous historical marks.

## Time and causality

`last_opened` records recent reading, including reopening the same page. Position
edits have an independent `BookConfig.position_modified_ms` and optional original
`position_version` (author, clock and legacy precision). Applying a remote edit
preserves that revision; reflow, reopen, receipt, reload and close do not mint one.
Portable restores keep the position revision and merge recent reading separately.
A typed modern reading edit supersedes an inflated historical legacy timestamp;
equal modern clocks use the original device identity as the deterministic tie.
These wall clocks cannot repair an incorrectly set modern device clock.

GitHub and encrypted LAN preserve `ProgressVersion`. LAN message `versions` maps
are additive/default-empty: old peers can read the unchanged progress entries and
new peers can read their historical messages. A historical LAN record cannot
recover its original author or subsecond precision; it is marked `legacy` with a
stable relay label. Equal legacy locators resolve conservatively by coordinates,
so a backwards reread within one legacy second cannot be inferred. Modern clients
retain exact author/milliseconds through relays; a receipt never becomes a new edit.

The shared background `sharing::apply_received_progress` indexes cached hashes,
validates canonical chapter/block/scalar bounds, rechecks current membership and
position revision at publication, and returns committed notices. It does not save
a stale copy of a platform journal. Desktop notices navigate without hashing EPUBs
or rewriting positions in the frame loop; Android/iOS likewise commit before
notifying the open reader. Local viewport changes keep the existing navigation
cancellation/generation guards. Network snapshots merge into live peer progress
instead of replacing edits made during the session. Pairing removal causality and
global settings/journal storage are separate existing boundaries, not claimed as
fixed by this extraction.

Annotations use causal revisions/tombstones. Display timestamps or device clock
skew do not pick a winning concurrent note. `concurrent_notes` in native view
payloads is additive, read-only presentation metadata computed from revision
state. It is never inferred from a separator or other user-authored text.

## Shared defaults and platform choices

The style keys are `none`, `mixed` and `publisher`; `mixed` is the fresh-install
default. Scripts are a separate setting enabled by default. The single-column
switch defaults to false, allowing each native layout to select columns for the
viewport. Every style retains the available native page-turn modes. Platform
font names, size units and responsive thresholds are platform choices, not
interchangeable persisted scalar coordinates.

## Ownership boundaries

- Core owns EPUB admission/canonical blocks, safe book/config persistence,
  annotation reconciliation, backup transactions and bounded chapter caches.
- Desktop `app/settings.rs` owns settings serialization and the snapshot adapter;
  the frame loop invokes it. Extraction preserves the existing settings behavior
  and does not claim that global settings now share the safe book-store policy.
- Android `BookOpeningSession` owns the opening job, native cancellation ticket
  and late-result generation. ReaderViewModel owns UI state and routes the
  session callbacks. A finished old job cannot clear a newer ticket.
- iOS `LibraryBackup.swift` owns the native backup screen; `CoreStore` remains
  the serialized bridge boundary. The extraction does not add another library
  or account instance.
- Existing native navigation modules own layout-aware jumps/return history, and
  bounded shelf-cover workers/caches own cover work. Native view state remains
  on each platform; no WebView is introduced.

Each extraction or contract change has its own source-bound checks. An earlier
full-workspace test or simulator screenshot does not verify a later source tree.

## Offline native corpus checks

`core/examples/native_compatibility.rs` audits either eager parsing or the actual
`loaded_chapter` API (`--lazy`). Every chapter is presented under `none`, `mixed`
and `publisher`, checking scalar bounds and unchanged canonical content. Audit
schema2 also hashes the complete ordered style payload per policy, so matching
summary counts cannot hide a changed alignment, font, spacing or inline style. The lazy
path starts without resident bodies and checks the eight-chapter cache bound.
A failed book produces a nonzero exit; an empty invocation is not a passing audit.

Build/source/fixture identity and eager/lazy parity are checked together:

```sh
python3 scripts/check-native-performance.py build \
  --app target/debug/examples/native_compatibility \
  --output /absolute/new-build-report-directory \
  --command cargo build --locked -p reader_core --example native_compatibility
python3 scripts/check-epub-compatibility.py \
  --binary target/debug/examples/native_compatibility \
  --build-record /absolute/new-build-report-directory/build.json \
  --output /absolute/new-corpus-report-directory \
  /absolute/path/to/book.epub
```

Both report directories must be new. The collector retains each native process's
JSON/log, exact commands, input/binary/source fingerprints and comparison result.
CI runs the two committed typography/script fixtures; separately acquired real
books stay outside repository distribution. Parser/presentation parity does not
prove a platform's rendered pixels, spoken accessibility or script compatibility.

On iOS, paper surfaces expose forward/reverse accessibility actions and distinct
note/link/publication actions. Note references are not repeated as ordinary link
actions. Scroll accessibility respects the system Reduce Motion preference. The
native controller checks exercise actual callback dispatch and exact position
return in all four paged modes; this is separate from physical VoiceOver speech.

Actual platform corpus checks accept an owned `cases.json` (Android cache/corpus)
or `corpus-cases.json` (iOS Documents) list of named chapters, with optional minimum
image/table/note counts. Missing declared fixtures or an empty manifest fails;
real books are not silently skipped. Complete text coverage is checked for every
page; first/middle/last and semantic pages are actually painted. The two committed
fixtures run in mobile CI; separately acquired real-corpus reports retain their
own source/artifact/input fingerprints. A report of representative drawing is not
an assertion that every page's pixels or physical screen-reader audio was tested.

Android paragraph splitting rechecks the actual short fragment after choosing a
line end from the original shaped paragraph. CJK line-breaking context can change
when a fragment becomes an independent text control. A candidate that reshapes
past the remaining page height is shortened at a native line/scalar boundary;
canonical text and fragment source offsets remain intact. The full remaining
chapter is not repeatedly shaped during this correction.

## Native-window coverage and large text

Corpus cases may declare `font_size` (8–96, default 18). Reports expose
`base_font_size`; publication CSS can still choose an absolute size. This is
separate from system Dynamic Type/font scaling. Mobile CI uses a 32-point
typography case, including ruby, alongside the ordinary script fixture.
CoreText ruby paragraphs keep requested line height as a minimum and allow
natural annotation ascent instead of enforcing a fixed maximum that overlaps
adjacent lines.

Desktop `performance-checks` builds expose `--verify-native-corpus`. The caller
must use a new owned working directory with `native-corpus.fixture.json`:
`{"cases":[{"name":"fixture","chapter":0,"font_size":32}],"files":{"fixture":{"path":"/absolute/fixture.epub","sha256":"..."}}}`.
Optional `min_images`, `min_tables`, and `min_notes` belong to each case. The
ordinary native reader renders three policies at 600/1200 window widths; output
is written to a new `native-corpus-results` directory. It checks source ranges,
native glyph availability, actual image decoding, notes, injected keyboard
forward/return, representative screenshot acknowledgements, and opening Escape
cancellation before reopening. The harness is absent from default builds.

Named desktop font families retain bounded installed script fallbacks and the
already bundled outline emoji fallbacks after the chosen font. System color
font tables alone are not proof that the outline renderer can draw a glyph.
No system fonts are redistributed. Hebrew/Greek and emoji checks do not imply
complete bidirectional shaping or browser-level layout.
