# Local catalog and annotation persistence

The library catalog (`library.json`) and per-book JSON configurations use the
shared `reader_core::storage` snapshot writer. A checked read distinguishes an
absent file from invalid data and IO failures. A failed library reload must retain
the previous in-memory shelf and report the failure rather than publish an empty
catalog.

Writes serialize and validate the target schema before creating a unique sibling
temporary file. Bytes are flushed before the temporary file replaces the target;
Unix builds also sync the parent directory. Temporary files are created with
owner-only permissions on Unix. A previous valid snapshot is kept at the full
filename plus `.bak`. A first save also creates a recovery copy.

If the primary JSON cannot be decoded, a schema-valid backup may restore it.
The original bytes are first retained in a unique `*.json.corrupt.*` file. If
either preservation or restoration fails, the operation reports an error. If
both copies are invalid, neither is replaced by default data. Permission or IO
errors are not evidence of JSON corruption. An absent primary is treated as
absent even when a backup remains, so deleting a record cannot resurrect it.

Recovery may return the previous completed edit. These are local snapshots, not
a complete portable backup, a transaction spanning multiple files, a guarantee
against device failure, or a replacement for cloud conflict handling. Backups and
preserved damaged configurations can contain private annotations. They stay in
the app's local data directory; the progress sync format is unchanged. Versioned annotation sync is described
in `ANNOTATION_SYNC.md`; it is separate from local recovery snapshots.

Library load/save and mutation APIs return errors. iOS passes them through its
existing result envelope. Android converts checked native storage errors into
Java IO exceptions, which the library/reader handlers report without constructing
a successful placeholder entry. Desktop preserves the old shelf on failed
reloads and uses its existing error UI. A failed catalog save does not unlink a
book during local removal. Ordinary book/config saves use the snapshot writer;
binary EPUB restoration continues to use the separate atomic byte writer.

Tests cover invalid JSON/schema, healthy backups, unrecoverable originals,
backup-write failure, primary absence, original-byte preservation, library
recovery, removal failure, and the iOS envelope. Android's isolated
`local.reader.columns` instrumentation additionally verifies actual JNI error
propagation and backup recovery. Local tests do not establish physical-device,
Windows/Linux runtime, or hosted CI acceptance.

Legacy catalog migration uses the same streaming atomic-copy writer as import.
If a legacy entry's proposed managed UUID is occupied by different file bytes,
the migration allocates a new identity instead of overwriting the orphan. Equal
content may retain its existing identity. A failed copy is returned to the caller
before publishing the changed catalog; it is not merely logged as a successful
migration. Regression tests preserve source bytes, an occupied target, and the
original catalog when atomic publication fails. This is not a multi-file migration
transaction or the portable user-backup feature.


The main CI workflow includes a native Windows core-test job. Desktop packaging
cannot by itself validate replacing an existing file, corruption recovery,
backup rollback or annotation tombstones on Windows. The job runs the core
library's complete enabled-feature regression suite in temporary test libraries
and retains its log on failure. Workflow configuration/lint is local evidence;
Windows runtime acceptance requires the corresponding hosted run after an
authorized push. No live book library or account is used by these regressions.
