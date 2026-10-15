# Native reading activity

The iOS, Android and desktop dashboards share `reader_core::reading_activity`.
They expose today, the last 7 days, 7/30-day trends, reading/listening totals,
active days, the current streak, and time by book. Reader entry defaults to the
current book; library entry defaults to all books. Statistics live under the
reader's More actions menu, alongside search and correction.

## Counting and storage

Clients observe monotonic active intervals, exclude suspension gaps longer than
2 seconds, and pause foreground reading after 5 minutes without interaction.
Backgrounding, opening reading tools, and loading a book pause reading. Playing
audio contributes only to listening. Foreground restoration and actual reader
interaction resume activity; automatic TTS following does not refresh idle time.
Clients checkpoint at 15 active seconds and flush on lifecycle/state transitions.
Fractional seconds remain in the client accumulator until a whole second exists.

`reading-activity.json` is a versioned local store. Book identity is the original
EPUB SHA-256, not a title or a local path. Each device owns per-book, per-date
reading and listening counters. A random client session and cumulative active
seconds make local checkpoint retries idempotent and ignore older checkpoints.
Writes replace a temporary file atomically. Invalid or unsupported existing
stores produce an error and retain the original file.

Dates are Gregorian local dates when recorded. Short heartbeat intervals are
split at local midnight. Historical buckets are not reinterpreted after a
timezone change. Old desktop `BookConfig.reading_stats` totals are retained in total time; their UTC/idle counting rules do not enter new trends.
The dashboard has no separate legacy-record section.
The original book configuration and its old daily sessions remain intact.

## Synchronization

`DeviceSnapshot.activity` is an optional extension to the existing GitHub
snapshot. Own-device CAS retries merge each daily reading/listening counter by
maximum; different devices contribute separate components. Repeated imports
cannot add the same component twice. Existing account, repository and sync
controls apply. Local checkpointing does not trigger a network request.

Encrypted LAN `SyncProgress` and `ProgressResponse` carry optional activity
snapshots. Old messages remain readable. Forwarding preserves every source
identity instead of attributing imported activity to the forwarding device.

iOS nearby transfer uses an isolated staging directory. Its activity identity
(`CoreClient.device`) is independent of the pairing/keychain identity. Staging
retains logical source components. Applying a current transfer imports remote
components while preserving activity recorded by the live reader during transfer.

Simultaneous use of multiple devices contributes separate time. No clock-based
last-writer overwrite, progress percentages, inferred word counts, or reset
operation is part of this statistics model.

## Verification

Regression coverage includes duplicate/out-of-order checkpoints, local midnight
and timezone splitting, legacy preservation, corrupt-file retention, device
merge/forwarding, old protocol messages, an actual encrypted localhost LAN
exchange, and iOS staging while live recording continues. Android's monotonic
clock tests cover idle pause/resume, exclusive playback, and suspension gaps.
Visual checks use generated EPUB fixtures. A real-account, three-device GitHub
round trip and physical-device background playback are separate acceptance checks.
