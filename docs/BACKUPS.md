# Portable library backups

The three native clients use the shared `reader_core::backup` protocol. The
settings entry prepares a file, or verifies a selected file and asks the user to
merge it. Picking, saving and sharing remain native. Book bytes are streamed
through files, never Base64/JSON over the bridge.

## Format and boundaries

Version 1 is an **unencrypted** ZIP (`.readerbackup`):

- `manifest.json`: format identifier, schema version, creation time, typed
  publication configurations, active/archive states and reading-activity source
  components. Publication paths are empty. Causal annotation identifiers and
  deletion revisions are preserved. An optional `classification` field carries
  local categories and assignments; it is omitted when empty.
- `books/<sha256>.epub`: one original for each included content hash. A cloud-only
  archived record may have no locally available original; that absence is explicit.

Only these typed records are exported. Account credentials, pairing/private keys,
installation counters, heartbeat session identities, downloaded fonts/models and
global UI preferences are excluded. Book text, notes and reading history are
private user content and remain readable in the package.

Admission bounds: 5,000 publications/lifecycle records, 64 MiB manifest, 1 GiB per
original, 64 GiB total original bytes. Annotation and activity limits also apply.
These are rejection limits, not a promise that a phone has enough disk space or
background execution time for a package at the limit. Staging requires additional
space; ENOSPC before publication leaves the live library intact.

Unknown format versions, repeated publication IDs/hashes, unexpected ZIP entries,
duplicate names, unsafe paths/types, invalid annotation state, inconsistent
materialized annotations, size/checksum mismatches and unreadable EPUBs are rejected.
Inspection uses bounded streaming plus native EPUB admission. Restore reopens and
revalidates the actual selected bytes. The UI supplies the verified manifest's
SHA-256; changed package content requires a new verification/confirmation.

## Merge rules

Originals are matched against actual local SHA-256, including archived entries.
New publications receive local UUIDs; a foreign UUID never authorizes overwriting
a local original. Repeated import does not add duplicate publications or annotation
revisions. Reading position follows its explicit edit revision, independently of
recent reopening. Older packages without a revision fall back to their historical
`last_opened` clock as legacy data. Recent reading is merged separately by maximum;
other book configuration follows `updated_at`. These clocks cannot correct an
incorrectly set device clock. Annotation merging uses causal revisions instead of
wall time.

Categories merge by case-insensitive name. Foreign category and book UUIDs are
mapped to local identities, and existing local assignments take precedence.
Empty categories are retained. Older packages without classification metadata
leave local categories intact. Packages containing classification metadata require
a client with category support; older clients reject the unknown manifest field.

Lifecycle events retain their original revision and event identity. Newer local
archive/delete events win over an old package. Incoming newer archive/delete
events may hide an existing shelf entry; restoring is not a forced resurrection.
Existing local originals are not purged by restore. Activity components merge by
per-source maxima; restored legacy timers are not reimported for each new device.

## Publication and interruption

Cooperating library/config/archive/activity access uses the same reentrant root
lease and process file lock. Restore stages all validated output, captures inverse
files, and durably publishes a prepared journal before modifying live files.
The whitelist contains only the catalog, lifecycle/activity stores and managed
UUID EPUB/config files. Originals remain in journal storage throughout rollback.

After all files are published, a committed journal is written. A newly acquired
outer lease first recovers any journal: prepared transactions roll back; committed
transactions retain the new state. The journal directory is renamed before cleanup
so interruption during cleanup cannot destroy originals needed for rollback.
Malformed recovery data blocks access instead of exposing partial state. Abandoned
UUID staging directories under the private root are reclaimed after recovery.
Unix files/directories are synced; abrupt-process tests do not prove every file
system's power-loss guarantees, especially on Windows/network storage.

Native UI prevents a reading session or cloud task from starting during backup
work. Restoration invalidates prepared cloud state and reloads the shelf after
success. Incoming LAN publication validation/catalog mutation holds the same root
lease. This does not make every unrelated app file a multi-file transaction or
merge arbitrary stale in-memory `Library` objects.

## Regression coverage

Core tests cover round-trip, identity remapping, duplicate import, legacy
statistics, newer positions/deletion, invalid packages, path boundaries,
publication failure and abrupt child-process exit followed by normal library
access. Native file-provider tests should cover saving, selecting, verifying,
merging and rejecting malformed packages in isolated libraries. Test external
providers and large libraries separately on physical devices.
