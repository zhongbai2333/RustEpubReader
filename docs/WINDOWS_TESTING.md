# Windows final testing

## Prepare the source

Commit and transfer the complete source change, including currently new iOS,
bridge, sync and UI files. Record the source SHA tested on Windows. A copy of only
tracked modified files is incomplete. Do not copy macOS `target/`, development
signing files or device-local runtime settings into the Windows checkout.

For a source build, install stable Rust with the MSVC toolchain and Visual Studio
C++ build tools / Windows SDK. See the
[official Rust Windows installation prerequisites](https://www.rust-lang.org/tools/install).
Use a Visual Studio Developer PowerShell so `rc.exe` and the native compiler are
available. CI already has Windows x64 and arm64 build jobs; use the package matching
the machine architecture if testing an Actions artifact instead.

```powershell
git status --short
git rev-parse HEAD
rustc --version
Get-Command rc.exe
cargo build --locked --profile release-fast -p rust_epub_reader
```

Desktop data lives in the process working directory. Start from a dedicated
writable test folder, so a double-click shortcut's working directory cannot make
the book library appear to disappear. For example, from the repository root:

```powershell
$readerExe = (Resolve-Path .\target\release-fast\rust_epub_reader.exe).Path
$readerTestDir = Join-Path $env:LOCALAPPDATA 'RustEpubReader\FinalTest'
New-Item -ItemType Directory -Force $readerTestDir | Out-Null
Push-Location $readerTestDir
try { & $readerExe } finally { Pop-Location }
```

Sign into GitHub on Windows separately; tokens are protected by Windows credential
storage and are not synchronized from the macOS Keychain. LAN pairing also needs
its own Windows device identity. Use owned fixture books for tests involving
deletion, annotations or synchronization.

## Acceptance checklist

| Area | What to verify |
| --- | --- |
| Startup | First start and restart; no missing DLLs; credentials remain available after restarting |
| Library | EPUB import, invalid file rejection, cover loading, search/sort, opening animation, multi-select/export/local delete |
| Fonts/layout | Chinese fonts available on Windows; no Japanese fallback for Chinese; scale 100%/150%/200%, window resizing, light/dark theme |
| Font library | Open the separate download view; verify Chinese/English filtering and offline specimens before downloading; download and apply a font; import valid TTF/OTF, reject invalid files and retry failed downloads |
| English override | Select Follow Chinese Font, verify Latin glyphs use the selected Chinese font, reopen settings and restart; select an explicit English face and verify the override survives restart |
| Reading | Scroll and curl, single/double page, partial mouse drag, reversal/cancellation, rapid repeated turns, forward/backward across chapter boundaries |
| Reader UI | Hidden toolbar recovery, animated panels, compact TOC, no spine shadow jumps, correct page overlap and top/bottom gradients |
| Local progress | Reopen the same book, restart the app, change font size/mode; restore text position rather than another device's page number |
| Reading tools | Search, highlights/notes/bookmarks, dictionary/TTS where configured, optional correction plugin/model download/load |
| GitHub login | Purpose/scope text, cancellation/denial, renewed permissions, sign-out; synchronization requires explicit enabling |
| Staged sync | Reading progress arrives before a new EPUB finishes; library indicator and status text reflect the phase; current open book offers a jump without moving silently |
| File failures | Disconnect during transfer; other completed files remain; reconnect and retry failed files; no duplicate book import or lost local position |
| Lifecycle | Disable sync/sign out during a transfer; no stale account result applies; reading remains responsive |
| Contribution choice | Default does not count sync commits toward the contribution graph; the user's toggle persists independently of enabling sync |
| LAN sharing | Private-network firewall permission, PIN pairing, real EPUB/progress transfer with another device; reject invalid PIN and corrupt file |

Cloud limits: 20 MiB per book; highlights/notes/settings are currently local.
Local deletion does not delete the repository's book file. Mobile background file
work runs while the app is foregrounded; it is not an unlimited background service.
Keep these boundaries in mind when comparing all three clients.

Record failures with source SHA, Windows version/architecture, DPI, exact steps,
expected/actual behavior, and a screenshot or debug log. `--debug` enables desktop
diagnostics; redact account/private book information before sharing logs. A
successful Windows build alone does not complete this acceptance checklist.
