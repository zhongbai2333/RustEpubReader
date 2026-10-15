# Book sharing

Desktop, Android, and iOS use `reader_core::book_share` for the same package,
QR-ticket, transfer, and import protocol. Sharing is separate from private
whole-library synchronization. A recipient needs neither a GitHub account nor
an application-operated server.

## User flow

The book menu and batch selection open the application's share page. Choose
local network or GitHub sharing. The separate **Export** button opens the
platform's file saving or sharing flow; exported EPUB names use book titles.

The library's **Receive books** entry accepts a QR image or pasted share link.
Android and iOS also offer camera scanning. A successfully received package is
verified and staged before its book list is displayed. Import requires explicit
confirmation. Identical EPUB content is skipped, preserving existing identity,
reading position, annotations, and configuration. Imported books receive fresh
local identities and retain the sender's display title.

## Local network

The sender opens an ephemeral local TCP listener for one verified recipient.
The QR ticket carries the private address, random session, temporary four-digit
PIN, expiry, and a random package key. The existing PIN system's AES-GCM and
HKDF primitives are reused, with independent keys for handshake directions and
receipts. This does not register a permanent paired device or exchange progress.

An unopened share expires after ten minutes. Closing the sharing page, moving
the mobile app to the background, or minimizing the desktop window cancels it.
A visible desktop window may retain its QR while another window has keyboard
focus. Verification is acknowledged before the sender closes the listener;
interrupted downloads can be restarted while the session remains available.
This version does not promise transfer resumption at a byte offset.

## GitHub transport

Each sender uses their own dedicated **public** `EpubReaderShares` repository.
The existing device login flow supplies the publisher's authorization. A marker
identifies the repository. An existing private or unrelated repository is
rejected without changing its visibility. Private `EpubReaderInfo` sync storage
is never used for publication.

Each share has a `reader-share-<random UUID>` Release containing a randomly named
`.rershare` attachment. AES-256-GCM encrypts the manifest (including titles and
content hashes) and all EPUB bytes. The key exists only in the active page and
the QR/link, and is absent from GitHub and local history. Users must save the
link before closing the page if they want to share it later. Anyone with that
link can decrypt the package.

Recipients download public Release attachments with a separate anonymous HTTP
client. Publisher authorization is never supplied to that client. Redirects
are restricted to HTTPS GitHub download hosts. New links use direct Release
URLs to avoid anonymous REST metadata requests; older asset-ID links are also
understood. No proxy, application server, or receiving GitHub login is involved.

Local history stores repository, release ID, random share ID, time, count, and
publication status; no keys, tokens, or book titles. Deleting a share validates
its owner and tag, then deletes its Release and tag. Upload failures attempt the
same cleanup. If cleanup fails, history retains a deletable unfinished entry.
Deletion prevents later downloads from GitHub, but cannot recall downloaded
copies or provide guaranteed expiry/download-count limits.

## Bounds and verification

At most 100 books, 128 MiB per EPUB, and 512 MiB total are accepted. Bounded
manifest and frame parsing, authenticated encryption, file hashes, EPUB ZIP
expansion checks, and readable-chapter validation run before import. Download
prefix authentication rejects bad keys and incompatible declared sizes early.
Cancellation during import rolls back newly created files before catalog
commit. Temporary data uses private directories/files and is excluded from
mobile backup. Abandoned cleanup only removes marked directories belonging to
dead transfer processes, preserving live and unrelated data.

`core/src/book_share/tests.rs` and the GitHub HTTP mock tests cover protocol,
selection, cancellation, corruption, duplicate preservation, public/private
repository separation, anonymous reception, and cleanup failures.
`core/examples/book_share_probe.rs` supplies synthetic native interoperability
fixtures. Android's `book-share` instrumentation check and iOS Debug's
`--verify-book-share` check exercise their real native bridges and QR image
decoders without real books or GitHub credentials. Physical camera scans and
live authenticated GitHub publication require separate device/account checks.
