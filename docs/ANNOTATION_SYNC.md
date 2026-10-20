# Annotation protocol and offline Markdown export

Implementation status: the shared core and full-workspace regression passed at
source `8ac53ac25b273b5bc3f93c35398338db0b9862db119d4ae58f20adb8bbd4aa26`
(293 tests, zero failures; five explicitly ignored entries). Native iPad and
Android diagnostic bridge checks also passed; Android's first new probe used the
wrong case for its paragraph tag and that failed record is retained. Subsequent
desktop layout changes have a focused bilingual/large-text regression. Runtime
reports bind to each actual build, not to an assumed common final source tree.
Later local checkpoints include actual mobile annotation panels and two visual
review rounds (`verification-stage05-local.md`), plus scalar-coordinate and
native renderer checks. Real-account cloud sync, physical devices and hosted CI
remain separate, unfinished acceptance gates. Native selection gestures are a
separate check from bridge mutation and coordinate conversion.

## Commit and merge boundary

Each managed book configuration contains its visible bookmark/highlight/note
vectors and a versioned `annotation_state`. One atomic snapshot publishes both.
A failed save cannot publish a revision for an unsaved edit. A nonserialized
loaded snapshot tracks which fields a caller actually changed; a stale reader
save merges those fields into the latest configuration under the shared writer
lock. A thread-bound reentrant root lease also covers cooperating catalog, archive
and configuration calls; its persistent OS file lock protects cross-process
counters. Portable restore now uses that lease with its own recoverable publication
journal; see `BACKUPS.md`. The lease alone does not merge stale whole-catalog
application objects. Independent notes and newer reading progress are not replaced by an old
whole-book in-memory snapshot. Removing an optional field is also a change.

Bookmarks use chapter/block identity within the publication's SHA-256 scope.
New highlights use random UUIDs on all clients; old bounded alphanumeric IDs are
accepted. Notes follow highlight identity. Reusing a timestamp/position/count as
a new ID is forbidden, since an old deletion could suppress the new annotation.

Each entry is a multi-value register of causal maxima. Vector clocks determine
causality; the recorded Unix edit time is only for display. Concurrent independent
entries union. Concurrent note text is preserved in stable order, separated by
`---`; an intentional edit of the observed combined text supersedes those
versions. Concurrent deletion wins; a later explicit re-add must observe the
recorded deletion. Missing vectors from another device are not deletions.

The installation actor's global counter is reserved durably before publishing a
configuration. A failed save may leave a gap. Recovering an older book snapshot
cannot reuse a published counter. Recovering a damaged counter file rotates the
actor rather than trusting a potentially previously used recovered counter.
Legacy vectors are seeded deterministically without claiming a fresh edit time.

An editor carries the note content it actually saw. If another edit arrives while
the draft is open, saving creates a separate causal branch and preserves both
texts, even for two drafts on one installation. A stale draft against a deleted
note returns an error, leaves the persisted deletion intact, and remains editable
in the UI. The UI must not close or erase a failed draft.

The first protocol bounds a book to 10,000 entry identities, 32 concurrent values
per entry, 128 causal actors per entry, and 8 MiB serialized state. New note edits
are limited to 32 KiB; legacy note values up to 1 MiB can be preserved. Exceeding a
bound reports an error, without silently discarding edits/tombstones. Tombstone
compaction needs acknowledgement/retention design; absence is never sufficient.

## Transport and ordering

Progress snapshot schema 1 is unchanged. Older clients cannot erase annotations
by rewriting their own progress snapshot. They do not gain annotation sync.

The platform applies progress first. The subsequent worker transfers annotation
metadata before book files. Annotation objects are content-addressed and verified
at `annotation-books-v1/<device>/<book hash>/<object hash>.json`. A bounded schema-1
index at `annotation-devices-v1/<device>.json` is published with GitHub Contents
CAS only after referenced objects exist. Retrying publication is idempotent.
Concurrent changes to the same installation index are retained for a retry rather
than force-overwritten. Object data contains no paths, tokens or pairing keys.

Account-scoped local caches retain received metadata for a book whose file has
not yet downloaded. Network workers stage these caches; only the guarded platform
apply step merges them into live configurations. Cancelled/signed-out generations
cannot apply old worker results. The apply also covers newly downloaded/restored
books, preserves reading progress and refreshes annotations in the open reader.
A bulk-transfer failure can still deliver staged metadata via a partial result;
its terminal error remains visible and drives retry/reauthorization. Cancellation
remains an error, never a partial-success application.

Limits include 128 device indices, 5,000 annotated book hashes, a 64 MiB download
batch/cache, and per-object validation before merge. Annotation failures are
reported separately and do not roll back progress already applied. Authentication
and rate-limit errors still apply to subsequent network work. Cloud publication
uses the existing contribution preference and private-repository validation.

Current immutable objects are retained. Removing a current annotation/tombstone
or a book file is not erasure of repository history. Automatic tombstone/object
pruning and retention guarantees have not been implemented or validated.

## Export and on-demand details

Markdown export is independent of cloud configuration. It reads only referenced
chapters and contains the book title, available author, chapter headings,
bookmarks, highlighted text and visible merged notes. Offsets count Unicode scalar
values, not UTF-8 bytes or UTF-16 units. Invalid ranges return an error rather than
silently omitting text. Book text and notes are escaped as text; export does not
embed remote images/scripts. Installation identifiers and secrets are omitted.
The output is bounded to 16 MiB and uses a sanitized display-title `.md` filename.

Desktop prepares the document off the UI thread, opens the native Save dialog,
and atomically writes the selected file. Mobile clients prepare a temporary file
and hand it to the system share/save UI. Duplicate preparation is disabled; a
share handoff is not reported as proof that a receiving app saved the document.

Source and edit times are available through an explicit details action. Legacy
source/time remains unknown. A short installation identifier is not presented as
a guessed hardware model. Zero/out-of-range timestamps are not rendered as 1970.
Concurrent texts are selectable; ordinary note editing resolves observed versions.

## Required validation

Targeted tests cover causal ordering with skewed clocks, idempotence, offline
union/deletion, concurrent notes, stale configuration/editor saves, failed commit,
archive/restore, counter recovery, invalid remote objects and immutable object/index
ordering. New Android/iOS diagnostic checks exercise actual JNI/C ABI tools,
conflicting drafts, details, export and deletion under isolated installations.
Their source presence is not a passing runtime result. Local bridge/native runs,
three-client annotation screenshots and visual review are recorded in the stage05
checkpoint. Hosted CI, real accounts and physical-device acceptance remain open.
