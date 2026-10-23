# Native fonts

All three clients read `resources/fonts/catalog.json`. The catalog, upstream OFL licenses and small specimen images ship with the app. Font binaries stay on demand,
using pinned upstream commits, exact byte counts and SHA-256 digests. The initial
catalog contains twenty regular reading faces: ten for Chinese and ten for
English. Chinese: LXGW WenKai, Source Han Serif SC/TC, Source Han Sans SC,
LXGW WenKai Screen, LXGW WenKai TC, LXGW Marker Gothic, Xiaolai, Yozai
and LXGW ZhenKai Slab GB. English: Literata, Source Serif 4, Lora, Crimson Pro,
Source Sans 3, Atkinson Hyperlegible, Alegreya, EB Garamond, Arvo and Andika.
Static regular faces are preferred; Lora, Alegreya and EB Garamond use their
upstream variable fonts with regular defaults. Variable files whose default face is ExtraLight are excluded. It is an offline catalog: changing its entries requires an
app update; there is no remote manifest or arbitrary download URL UI.

All three clients expose **Same as Chinese font** at the top of the English
font picker. It disables the independent Latin override and uses the selected
Chinese face first, with the normal fallback chain for missing characters.
Existing explicit selections and defaults are preserved. The saved empty Latin
name represents this mode; it is not a missing font and must survive font
registration and restart. Android applies its Latin override to both text painting
and pagination measurement, with font changes invalidating cached pages.

## iOS

Reading settings > Chinese/English Font exposes Import Fonts, Choose a System
Font and Downloadable Fonts. Imports accept TTF/OTF, including multiple files,
with a 64 MiB bound per file. The file picker grants scoped access; validated
files are copied into Application Support/ReaderFonts under content-derived
names and registered with CoreText for this process. Startup registers those
files again. Reimporting identical data does not create another stored copy.
Import hashing/copying and download validation run off the main thread.
Registration runs on the main actor; identical-name collisions also verify the
already loaded file before accepting it. Different binaries sharing a PostScript
name are rejected without leaving a newly created conflicting file behind.

The system entry uses UIFontPickerViewController with individual faces enabled.
Only the selected face is remembered, rather than enumerating user-installed
fonts. Saved external font names are requested from CoreText on launch when
needed. The app declares the Fonts app-usage entitlement; release signing must
include that capability in its provisioning profile. It does not request the
font-installation or font-enumeration entitlement. App-local imports/downloads
use process registration, not system-wide installation.

Both font lists test actual glyph coverage. The English list accepts any font
with the Latin sample glyphs, including bilingual CJK fonts. An incompatible
system selection reports an error and preserves the previous selection.

## Downloads

All three clients enter a separate download view from the font picker, following
the iOS navigation pattern. All clients filter by the edited Chinese/English
slot and apply to that slot. Each entry shows its size, download state, source
and license, then a Use action. All three show the same offline specimen before
download and after installation. Selecting a face uses the existing native reflow and saved-font path;
Classic style retains its existing restrictions on overriding publication fonts.
All clients apply downloaded faces to the Chinese or English slot being edited.

All clients stage files before verifying and installing. A failed or partial
transfer is not discoverable as a font and can be retried independently. Desktop
uses a bounded-time worker, Android an app-scoped IO coroutine, and iOS URLSession
plus background validation. Downloads outlive the settings view while the process
is alive; OS background downloads, resumable transfers and cross-device font-file
sync are not part of this change. Imported/downloaded files are local to the app.

## Official-only entries and local imports

`resources/fonts/external.json` lists Hanyi Senty Song and Hanyi Senty Pine.
Their official product pages explicitly offer free personal, non-commercial
use and require authorization for commercial use:
- https://sentyfont.com/song.htm
- https://sentyfont.com/pine.htm

The application includes links and the checked date, with no download URL,
font bytes, mirrored sample images, or distribution claim for these entries.
Their action opens official samples and acquisition instructions. The official
terms remain authoritative. Direct entries show the OFL notice and full
bundled license, together with project/license URLs. The catalog audit rejects
artifact fields in official-only entries.

Desktop/Android also expose TTF/OTF import within the picker and download view.
Imports copy and validate files in a worker, bound input to 64 MiB, use
content-derived local filenames, and retain metadata for restart discovery.
Duplicate imports reuse the content identity. Invalid files never enter font
discovery. Imported files are not uploaded or synced. Android uses the native
system document picker, then copies while the URI grant is active. Desktop
uses the native file picker. Imported names include a short content ID to
avoid confusing different files with the same source name.

The iOS system font action uses plain text: its old `textformat` SF Symbol can
localize to a word-like glyph, so it no longer appears as a confusing icon.
Chinese filtering accepts either Simplified or Traditional Chinese coverage.

## Verification

`python3 scripts/check-font-catalog.py` checks pinning, safe names, size bounds and
bundled licenses, the exact 10/10 split, specimen hashes/dimensions and their
source-font fingerprints. `--files <directory>` also verifies actual font artifacts.
The main CI runs the metadata and official-only schema checks without downloading fonts.
Font artifact checks remain opt-in to avoid large network downloads in each CI run.

## Offline specimens

`scripts/render-font-previews.swift <font-directory>` renders all twenty exact
catalog faces using CoreText; it rejects changed hashes, unexpected PostScript
names, missing sample glyphs and clipped ink bounds. The shared two-line sample,
68 px font size, baseline and 1200 x 216 canvas are fixed across faces. There is
no per-face scaling or fallback substitution. Preview text is also supplied as
an accessibility description. Traditional Chinese entries use Traditional text.

Only transparent white-glyph PNGs are bundled in `resources/fonts/previews/`,
with a manifest linking each image hash to its source font hash. The twenty
PNGs total 726,232 bytes (about 709 KiB), rather than bundling the roughly
190 MB of actual font downloads. Each native UI tints the mask with its current
foreground color, so the specimens support light and dark appearance. Images
are displayed at their shared aspect ratio, at most 320 logical units wide;
desktop uploads textures only for visible entries, Android uses Coil's asset
cache and iOS uses a bounded UIImage cache. Download state never changes the
specimen size or list position. Reader size, device metrics and layout settings
can make actual reading differ from the fixed sample.
