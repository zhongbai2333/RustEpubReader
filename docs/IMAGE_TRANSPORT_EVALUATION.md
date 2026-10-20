# Native image transport assessment

The normal Android/iOS reader still uses its existing Base64 chapter contract.
The resource transport below is an explicitly requested evaluation path. Desktop
already receives image bytes directly and does not need an artificial bridge.
Neither the shipping Rust profile nor the normal renderer is automatically changed
by a good serialization result.

## Resource ownership

`reader_core::image_resources::ImageResources` holds one current publication
session, without an unbounded ID-to-bytes map. Reserve on the reading event,
prepare off Main, and attach only that reservation. A replaced/closed reservation
cannot attach or close its replacement. Immutable book/cache work takes place
outside the registry mutex, with cancellation checked before returning results.

`img1:<nonce>:<chapter>:<kind>:<index>` is an ephemeral opaque locator. Callers
must not construct paths from it or persist it as a reading position. The owner
is the Android book path or the iOS root/book-ID tuple. Fetch validates its
namespace, generation, chapter and actual image target. Bytes are nonempty and
at most 32 MiB. Resource headers are limited to 64 MiB; existing iOS legacy
chapter admission is retained. The legacy Android serializer did not have that
wire budget and is not claimed to have acquired it in this assessment.

Close invalidates new requests. Already returned Java `ByteArray`, Swift `Data`
or Rust `Arc` values retain their ordinary ownership. It does not revoke a
bitmap, free every shared book cache, or cancel unrelated normal-reader work.
Production adoption would also require normal close/archive/open-generation
integration and native layout/bitmap-cache changes.

Android exposes `RustBridge.imageTransport(path, request)` and
`RustBridge.imageBytes(path, reference)`. iOS uses the `image_transport` JSON
operation and `reader_ios_image_read/data/length/free`. Swift copies the borrowed
read-only Rust bytes into owned `Data` before freeing the opaque handle. Mutable
zero-copy `Data` over a shared immutable Rust allocation is deliberately avoided.

The control actions are `begin`, `prepare`, `chapter` and `close`; subsequent
requests supply the returned `session`, with `chapter` also supplying its index.
The requested resource header preserves the platform's existing wire shape,
using an opaque ID in each image's `data` field. Normal `getChapter`/`chapter`
continues to return Base64. Never pass the evaluation header to a normal Base64
image decoder. iOS flattened media keeps canonical block/scalar offsets;
Android keeps canonical blocks and separate supplemental media. Regression tests
resolve IDs and compare the entire legacy JSON, including order, text and alt.

## Reproducing the native assessment

Build both profiles from an unchanged tree with the opt-in harnesses:

- Android: `:app:assembleRelease`, `-PREADER_PERFORMANCE_CHECKS`,
  `-PREADER_DIAGNOSTIC_APPLICATION_ID=local.reader.performance`. Set
  `READER_RUST_PROFILE=release` or `release-fast`; all three native ABIs are built.
- iOS: Release Simulator `ReaderPreview`,
  `PRODUCT_BUNDLE_IDENTIFIER=local.reader.performance`,
  `SWIFT_ACTIVE_COMPILATION_CONDITIONS=READER_PERFORMANCE_CHECK`, and
  `READER_RUST_PROFILE=release` or `release-fast` as an Xcode build setting.
  Preserve the project's Rust build phase. A Simulator app is not a device IPA.
- Wrap each command with `scripts/check-native-performance.py build` to record
  source and artifact fingerprints. Include the selected environment in the
  recorded command, for example `--command env READER_RUST_PROFILE=release-fast
  android/gradlew ...`. Copy each completed app/APK into an ignored immutable
  directory before the next build replaces shared output files.

The image collector takes a JSON build-matrix manifest:

```json
{
  "passed": true,
  "source": {"sha256": "...", "files": {}},
  "builds": [
    {
      "platform": "android",
      "profile": "release",
      "output": "/absolute/build-record-directory",
      "artifact": {"path": "/absolute/pinned.apk", "sha256": "...", "bytes": 1}
    }
  ]
}
```

Supply all four platform/profile combinations for the default full assessment.
`--platform android` or `--platform ios` can repeat only an affected client;
the manifest must include the requested client's presets (`release` alone for
`--memory`). `source` must be the exact complete
source object from all four successful `build.json` records; the abbreviated
object above only illustrates its shape. Each artifact hash must match its build
record. `output` contains that record. The collector verifies these independently;
setting `passed` alone cannot make a stale/mismatched artifact valid. Android
also requires packaged `assets/reader-native.json`, whose selected profile and
library fingerprints must match the actual APK SO entries. A BuildConfig label
alone is not sufficient.

Generate the 12-image fixture with `scripts/performance-fixtures.py` and supply
its `illustrated.epub`. The second input is the official [illustrated Alice
EPUB](https://www.gutenberg.org/ebooks/28885), retained with its license and not
bundled in this repository. The evaluated input SHA-256 is
`69c757719102f175303e7e8b8e508f1466e9aa78ddd9afb05609d00150699b3d`;
chapter 7 has six images in this parser. Different editions are different corpora.

```sh
python3 scripts/check-image-bridge.py \
  --builds /absolute/build-matrix.json \
  --generated /absolute/fixtures/illustrated.epub \
  --alice /absolute/pg28885.epub \
  --serial emulator-5556 --pinned-build \
  --output /absolute/new/image-samples
```

`ANDROID_HOME` must identify the SDK. The collector refuses physical Android
serials and non-diagnostic packages. It owns a new compatible iPad Simulator and
removes only that device. `--pinned-build` attributes results to the recorded
immutable artifact and logs workspace changes separately. At least three fresh
processes run per platform/profile. Seven alternating samples per selection
(first/all images) retain every raw result. Fingerprints, complete case coverage,
independent processes, cross-platform/profile image hashes and finite timings are
checked. The ten collector regressions are wired into normal and performance CI; local
runs are verified, and hosted execution remains a separate gate.

Schema 2 uses a blank native host without startup library/network work, explicitly
waits for its window to settle, and records first chapter and decoder preparation
separately. It measures cached chapter bridge/parse, selected image transfer,
native bounds and bounded bitmap decoding. Both first/all selections have the
same decoded-size limits. Iteration zero remains in the all-sample summary and
is separately excluded from the steady summary; this is not best-of selection.
Schema 1 was a preliminary method and is not accepted by the current collector.

For isolated retained-chapter memory, add `--memory` and use a new output directory.
This runs only the size profile, one transport per fresh process, with the generated
12-image chapter. It holds the actual native chapter DTO through five observations.
Android explicitly requests probe-only GC and records PSS/heap use; iOS records
physical footprint/RSS with an extended DTO lifetime. Baseline/steady values and
ranges are retained. This excludes bitmap-cache and whole-reader residency and
is not a per-allocation peak measurement. Ordinary reading never requests that GC.

## Interpretation

The core still reads/normalizes referenced chapter images together in
`loaded_chapter`. Reduced JSON and selected binary reads are transport benefits,
not proof of per-image ZIP loading. A prototype must account for dimensions,
preparation, cache keys and invalidation before changing native pagination.

Host allocation counters, process snapshots, actual bitmap decode and UI frame
intervals are different measurements. Keep them separate. Compare within a fixed
platform/device/fixture/profile; do not rank Android versus iOS simulator numbers.
Initial boot/framework activity and host memory pressure can cause substantial
noise. Preserve rejected/earlier runs rather than silently replacing them.

Build sizes here are native diagnostic APKs/Simulator apps. Warm serial builds
cannot establish clean CI build-time savings, and probe binaries are not final
product packages. A profile decision needs representative reading workloads in
addition to this bridge assessment. Current default remains `release`.

## Cached Android library publication

The locally installed cargo-ndk4.1.2 output-copy freshness check compares only
modification times. Switching from a newer performance-profile library back to
an older cached size-profile library can therefore retain the wrong output.
An independent APK-vs-Cargo SHA audit reproduced this; that invalidates the
affected preliminary profile labels, even when the native runtime tests pass.

The Gradle integration now builds with cargo-ndk without its mtime-based `-o`
copy, then uses `scripts/android-native-artifacts.py publish` to publish the
selected Cargo files by content. It honors Cargo's actual target directory,
preflights all three ABIs, uses owned atomic temporary copies and writes stable
provenance without local machine paths. Generated JNI/assets live in separate profile directories under `build/` and
are connected to the variants through the public
[generated-source API](https://developer.android.com/reference/tools/gradle-api/9.3/com/android/build/api/variant/SourceDirectories).
Their task outputs and dependencies are explicit; old source-tree JNI caches are
retained and excluded from packaging. Cargo retains responsibility
for Rust incremental dependency checks. Existing cached builds are not deleted.

Cargo already strips the JNI library, so AGP preserves its exact bytes. Both
assemble tasks independently hash actual APK entries against selected Cargo
outputs and embedded provenance; a stale or mismatched package fails the build.
The publication regressions cover a destination with a newer timestamp, cached
switches in both directions, unchanged output reuse, missing ABI, changed source,
wrong/missing provenance, duplicate ZIP entries and incomplete/traversing output
listings. Real cached-build switching is a separate runtime acceptance record.
