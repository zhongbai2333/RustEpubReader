# EPUB media and interactivity

## Rendering behavior

The shared parser retains PNG, JPEG and WebP images, percent-encoded EPUB image
paths, Base64 and percent-encoded image data URIs, inline SVG, standalone SVG,
and SVG wrappers referencing raster images declared in the EPUB manifest.
SVG is rasterized to bounded PNG output before reaching native clients. Its
resolver reads manifest resources from the EPUB; it does not fetch remote images
or read files on the host device.

Illustrations embedded in paragraphs, headings, lists and tables are supplemental
render blocks. Existing chapter/block coordinates and text offsets remain stable
for saved progress, highlights and synchronization. Inline pictures render as
separate image blocks rather than inline text runs. Image-only spine pages are
attached to neighboring text chapters; wholly illustrated books retain pageable
image chapters.

| Client | Behavior |
| --- | --- |
| Desktop | Proportional images in continuous scrolling and paginated single/dual columns; long images fit the page. Curl snapshots retain image textures through cache eviction. |
| Android | Supplemental images in scrolling and paginated content; page height constrains long images. Decoding runs off the UI thread with downsampling. |
| iOS | ImageIO thumbnails, CoreText image attachments and proportional native page painting; the leading image reserves room for its chapter title. |

Full fixed-layout EPUB/CSS fidelity, animated images and arbitrary SVG features
are not established by these checks. Failed images retain a bounded text fallback.

## Native presentation styles and publication scripts

All clients use their native text/image renderer for all presentation styles.
There is no WebView dependency or browser rendering path. Mixed is the default.
Scrolling, sliding and the platform's native curl remain independent of style.

| Policy key / UI name | Native behavior |
| --- | --- |
| none / 原生 | Reader typography; retain semantic emphasis, illustrations and interactive visibility. |
| mixed / 混合 | Respect supported alignment, emphasis and indentation; reader font, font size, line height and paragraph spacing take priority. |
| publisher / 经典 | Also use supported publication font size, family, line height, paragraph margins and text color. Unsupported properties fall back to native typography. |

Stylesheets, bounded local imports and inline declarations are resolved by the
shared Rust core. Selectors, inheritance and important declarations contribute to
native block styles. This is a CSS subset: flex/grid, fixed-layout fidelity,
positioned boxes, embedded web fonts, pseudo-elements and arbitrary CSS animations
are not implemented. Existing semantic HTML formatting and native illustration
support remain available in every style. All clients have separate Chinese and Latin reader-font selections; an empty
Latin selection follows the Chinese font. Android uses the same mixed-font spans
for rendering and pagination measurement. Classic family resolution falls back to installed fonts. See the
[platform capability audit](PLATFORM_PARITY.md) for client differences.

Publication scripts have an independent switch, initially enabled. QuickJS runs
book-local classic scripts and inline event handlers with a small native document
adapter. Supported interfaces include ID/simple selectors, textContent/innerText,
hidden, classList, style properties, attributes, dataset, ready/load and click
listeners. Native controls dispatch bounded events, and changed styles trigger
native repagination. Input controls, arbitrary DOM creation, module imports,
Canvas, browser layout measurement, asynchronous timers and persistent web
storage are not implemented. Unsupported scripts report errors in the presentation
response and fall back to static native content; disabling scripts does not run
startup code or replay events.

The interpreter exposes no Rust bridge, files, sockets, credentials, GitHub or
library mutation APIs. Each evaluation has a 16 MiB heap, 256 KiB stack and 100 ms
execution deadline. Sources/imports, node count and event history are bounded.
Interaction history is per open book/chapter, replayed deterministically for at
most 128 events, and is not persisted or synchronized. Random/time-dependent
scripts are consequently outside the verified compatibility subset.

Canonical chapter/block indices and source text remain unchanged. Hidden content
is omitted from drawing rather than removed from the canonical book. Dynamic
text is a display override; native selection/annotation support for that generated
text is limited and does not create new shared block coordinates.

## Native body links

iOS retains link targets in regular text and table-of-contents lists. Book-local
links resolve relative to the clicked chapter, including percent-encoded fragments,
container IDs, inline Unicode scalar offsets and legacy named anchors. They use
normal reader navigation in scrolling, sliding and curl modes without requiring
publication scripts. Semantic note references retain the native note sheet.
Only explicit HTTP, HTTPS, mail and telephone schemes use the platform URL handler;
unresolved local links and other schemes do not open host files.

## Verification

`core/tests/epub_native.rs` verifies style priority, local imports, inline important
rules, click-to-reveal and script disabling against a real EPUB, along with
execution/memory limits and rejected resource paths. Existing image tests retain
native scalar and block coordinates. Android's device runner accepts `-e check
native` to test actual JNI output and native pagination. These checks establish
the supported subset, not full browser compatibility or store acceptance.

## Native typography increment

See [EPUB_NATIVE_TYPOGRAPHY.md](EPUB_NATIVE_TYPOGRAPHY.md) for the new shared units,
inline styles, native ruby, text tables, note references, corpus checks, and their
remaining limitations. This supersedes the older typography subset description
above; JavaScript capabilities remain unchanged.
