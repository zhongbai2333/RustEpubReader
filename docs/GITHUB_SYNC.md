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
- `books/<sha256>.epub`: immutable EPUB bytes (20 MiB maximum per file).
- `devices/<device-id>.json`: device-specific book inventory, reading positions and daily reading/listening activity.

EPUB files, reading progress and per-device daily reading/listening activity are
synchronized. Bookmarks, highlights and notes remain local, along with settings.
Activity merges preserve device ownership and avoid double-counting; see
[reading activity](READING_ACTIVITY.md). Positions currently use the
shared parser's chapter/block/character coordinates; EPUB CFI is not implemented.
Progress suggestions select the latest timestamp, so intentional rereading can
move backwards. Each device's record remains separate; device clocks should be
correct. The current open book never jumps automatically: use the settings
button to apply another device's suggested position. Books not currently open
receive newer positions automatically. Uploads use SHA-based compare-and-set;
conflicts are reread and retried without force updates.

All three clients sync and apply reading progress before starting EPUB transfers.
The file phase uses validated device manifests as an index to avoid checking every
already uploaded book individually. Automatic sync runs while the desktop app is
open or a mobile app is in the foreground, normally every five minutes. Mobile clients cancel further requests when
backgrounded, disabled, signed out, or when the account changes. Network
failures retain local records and retry; missing permissions require renewed
authorization. GitHub rate-limit delays are respected. Background downloads are
bounded to 20 attempts / 100 MiB per pass; uploads also have a 20-attempt limit.
Additional batches resume after 15 seconds. A failed book does not stop other
books: its account/device-specific retry record is saved locally, with exponential
backoff from 30 seconds up to one hour. **Retry failed books** retries only the
queued files, after refreshing progress, and still respects GitHub rate limits.
Completed uploads are retained in the manifest; download failures are cleared
only after EPUB validation and a successful library save.
Downloads are verified by size and SHA-256 before import. Unreadable/oversized
local books are skipped and counted. Disabling sync or signing out cancels future
requests without deleting local books or the repository; an already sent request
may complete. Local deletions do not delete cloud files; a previously advertised
book removed on this device is not automatically restored on the same device.

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
