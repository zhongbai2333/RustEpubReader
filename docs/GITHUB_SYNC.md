# GitHub sync

Use **Multi-platform sync** in the library on all three clients, then choose
local network sharing or GitHub sign-in / account management. The desktop reader
toolbar uses the same entry. **Manage GitHub account** is also available in reader
settings and the correction-data contribution interface.
The unified account interface requests `repo` access to create and synchronize the
private `EpubReaderInfo` repository. Sign-in alone does not enable synchronization
or submit correction samples. **Sign in and enable sync** starts sync only after
authorization succeeds; cancelling or failing authorization does not enable it.
Existing narrow-scope tokens require renewed device authorization, not deletion.
GitHub's `repo` scope also covers other private repositories accessible to the
account; this app's sync transport writes only its marked sync repository. Legacy
Android contribution-only authorization can use `public_repo` and needs renewed
permission for private sync.

Desktop uses macOS Keychain, Windows credential storage or Linux Secret Service.
iOS uses native Keychain. Android encrypts the token with an AES-GCM key in Android
Keystore, migrates legacy plaintext tokens after successful secure storage, and
excludes credentials and device identity from backups/transfers.
Tokens are never included in sync files or settings JSON.

The app creates or reuses the private `EpubReaderInfo` repository. A repository
with that name must have the reader schema marker; an unrelated repository or a
public repository is rejected without uploading books. If initial creation was
interrupted before the marker was written, rename that incomplete repository
before retrying. The client never deletes or changes repository visibility.

- `epub-reader.json`: application/schema marker.
- `books/<sha256>.epub`: legacy immutable EPUB bytes, up to 20 MiB.
- `books-v2/<sha256>.json`: ordered chunk manifest for files above 20 MiB,
  up to 1 GiB. Each part records its asset ID, byte count and SHA-256.
- Private draft releases tagged `reader-file-v2-<sha256>`: 8 MiB binary assets
  for each large file, with a shorter final part. They remain unpublished so
  repository release immutability cannot prevent repair or permanent deletion.
- `devices/<device-id>.json`: device-specific book inventory, reading positions and daily reading/listening activity.

The existing private repository and authorization are reused; users configure no
additional storage. Device snapshots retain schema version 1 and add an optional
`large_books` inventory. Older clients can exchange progress and small books while
ignoring this field; large files require an updated client on each device.

EPUB files, reading progress and per-device daily reading/listening activity are
synchronized. Bookmarks, highlights and notes remain local, along with settings.
Activity merges preserve device ownership and avoid double-counting; see
[reading activity](READING_ACTIVITY.md). Positions currently use the
shared parser's chapter/block/character coordinates; EPUB CFI is not implemented.
Progress uses the newest actual reading edit, including rereading an earlier
chapter. New records carry millisecond edit revisions and their original device
identity; equal-time legacy records use a deterministic device tie-break. Applying
another device's record preserves that revision. Opening, closing and restoring a
book do not create a newer edit. All clients persist the winning locator and
update an open reader automatically. A local edit made while a network request is
in flight is compared again before applying the response. Device clocks should be
correct; the original order of conflicting legacy edits within the same second
cannot be reconstructed. After upgrade, actual edits supersede legacy records
without inheriting artificially inflated legacy seconds; unread imports are
bootstrap state and cannot reset an existing reading position. Uploads use SHA-based compare-and-set; conflicts are
reread and retried without force updates.

All three clients sync and apply reading progress before starting EPUB transfers.
The file phase uses validated device manifests as an index to avoid checking every
already uploaded book individually. Automatic sync runs while the desktop app is
open or a mobile app is in the foreground, normally every five minutes when idle. Changed reading positions are scheduled
within about five seconds in the foreground, subject to network/backoff and an
already-running sync. Mobile clients cancel further requests when
backgrounded, disabled, signed out, or when the account changes. Network
failures retain local records and retry; missing permissions require renewed
authorization. GitHub rate-limit delays are respected. Background downloads are
bounded to 20 attempts and 100 MiB of legacy downloads per pass. Chunk downloads
have a separate 100 MiB budget, shared with large-file archive verification.
Uploads also have a 20-attempt limit, with a separate 100 MiB budget for newly
uploaded chunks. These are separate budgets, not a total network-traffic cap.
Additional batches resume after 15 seconds. A failed book does not stop other
books: its account/device-specific retry record is saved locally, with exponential
backoff from 30 seconds up to one hour. **Retry failed books** retries only the
queued files, after refreshing progress, and still respects GitHub rate limits.
Completed uploads are retained in the manifest; download failures are cleared
only after EPUB validation and a successful library save.
Large transfers retain verified parts across batches, failures and app restarts.
Uploads reuse verified remote assets and publish a manifest only after all parts
exist. Downloads use account-scoped disk caches and a streamed assembly; both
each part and the assembled file must pass size/SHA-256 checks before library
import. Only the failing or missing parts transfer again. Platform staging and
archive restoration copy from files rather than allocating an entire large book.
Signed GitHub asset redirects are restricted to GitHub's asset hosts and never
receive the repository token. Completed cache files are removed after successful
import, or after archive verification has finished.
Downloads are verified by size and SHA-256 before import. Unreadable/oversized
local books are skipped and counted. Disabling sync or signing out cancels future
requests without deleting local books or the repository; an already sent request
may complete. File availability is checked against the current local library,
rather than the historical upload/download inventory, so a missing local book can
be downloaded again. Legacy local deletion markers no longer suppress cloud
imports.

## Archive and permanent deletion

The shelf's single-book and batch actions archive books. Local-only archiving
hides entries while retaining their managed EPUBs, UUIDs, progress and annotation
configs. The archive list supports restoration and a separately confirmed
permanent deletion. Archive records rebase owned paths after an iOS sandbox move.

Snapshots include a per-hash lifecycle record (`active`, `archived`, `deleted`),
with a revision and event identifier. Newer lifecycle events converge across
devices. On sync, archived books leave every shelf; a retained local EPUB is
uploaded if needed. Before unlinking an archived local file, the file phase reads
the actual remote EPUB and verifies its size and SHA-256. A manifest alone is not
sufficient. Missing/corrupt cloud copies leave the local file intact and enter the
per-book retry queue. Oversized local-only archives remain on disk. Restoration
uses the original local UUID/config when available and downloads missing EPUBs on
the next sync.

Permanent deletion publishes a durable tombstone before deleting only the
hash-named EPUB, or that large file's storage drafts/assets and chunk manifest,
from the private sync repository. Other updated clients suppress
stale uploads/downloads and remove local copies on their next sync. Failed cloud
deletions are retried separately. Deleting a file does not rewrite Git history;
old commits may retain the bytes. The deletion record remains to prevent stale
inventories from resurrecting the book. All devices must run a version that
understands lifecycle records and progress revisions for these rules to hold.

## Status and verification

The shelf and sync settings show the same current stage, processed book count
and per-stage progress. Unknown work uses an indeterminate indicator. Counts
represent processed files (including attempts), not elapsed-time estimates;
large-file transfers additionally report verified bytes for the current book.
Individual failures remain visible and retryable. iOS device authorization shows
an inline copied confirmation and browser-opening feedback.

`reader_core` exposes the portable schema and the optional `github-sync` API
transport. Desktop, Android and iOS use this transport and repository format.
Mobile clients stage
downloads and apply them to the live library only after account/session checks.
Android resume uses the saved library position, allowing intentional rereading to
move backwards despite older preferences.
Private repositories are
not end-to-end encrypted storage, and Git history retains previous file versions.

Validation: local simulated GitHub API tests cover permission checks, private
repository creation, public/unrelated repository refusal, upload/download hashes,
progress conflicts, cancellation, progress-first ordering, per-book failure
isolation, persistent retries and rate-limit stopping. Real-account OAuth, remote creation and
multi-device acceptance require an authorized live run.
Chunk tests also cover interrupted uploads/downloads, damaged chunks and starter
assets, untrusted redirects, whole-file integrity, disk-backed import, permanent
deletion and a 129 MiB file progressing across multiple bounded passes.
