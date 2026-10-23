# Native EPUB typography

Desktop (egui), Android (Compose), and iOS (CoreText) use the shared Rust
presentation metadata. There is no embedded browser. Default style remains
Mixed; the Native / Mixed / Classic styles and independent script switch are
unchanged. This work does not expand the JavaScript API.

## Supported in this increment

- Descendant font inheritance, explicit `inherit` / `unset`, selector specificity,
  important declarations, local stylesheet imports, and bounded CSS variables.
- `px`, `pt`, `em`, `rem`, percentages, and additive `calc()` lengths. Relative
  font sizes use their actual parent; inherited absolute/percentage line heights
  retain the parent's computed length rather than scaling again in a child.
- Inline bold/italic, underline/strike, smaller text, subscript/superscript, and
  native ruby. Small caps use platform font features where available; desktop
  approximates ASCII small caps. Embedded publisher font loading is not included.
- Semantic lists, nested list rows, blockquote insets, preformatted line breaks,
  and supported block margin/padding lengths. Mixed keeps reader-controlled body
  font size and line spacing; Classic resolves authored font sizes and spacing.
- Images with width/height and maximum dimensions, preserving aspect ratio and
  respecting the native page's available area. Indefinite scroll height does not
  turn fixed dimensions into NaN or collapse percentage-only height images.
- Basic text tables with headers, wrapped columns, alignment, and colspan.
- Semantic note references (`epub:type="noteref"`, `doc-noteref`, and recognized
  legacy classes) open a native note view. Book-local cross-chapter and UTF-8
  fragment targets are supported. Closing does not navigate away from the page.

The source chapter/block order and original text spans remain canonical. Inline
ranges use Unicode scalar positions, converted once to UTF-16 for Android and
CoreText. Hidden ruby annotation/fallback characters remain in the source.
Desktop now uses egui's native CJK soft wrapping rather than inserting synthetic
newlines; font styles do not change stored progress or highlight coordinates.

## Boundaries

This is a native CSS subset, not a browser compatibility claim. Flex/grid,
absolute positioning, floats, fixed-layout reproduction, vertical writing,
automatic hyphenation, and embedded font-face loading are outside this increment.

Tables are basic text grids. Rowspan, nested tables, table cell images, and inline
interactions inside cells are not implemented. Desktop and Android table cells
and expanded nested list rows do not currently expose character-level selection.
iOS table rows are native raster attachments with accessible source text and
bounded row anchors; character-level cell selection is unavailable. Oversized
table rows on Android/iOS fall back to complete native text rather than clipping
a single row attachment. Desktop pagination now measures native line/image bounds and continues long
paragraphs/table rows across leaves with canonical source anchors. Android also
uses native font measurements and splits oversized headings; oversized mobile
tables retain the existing complete-text fallback. See
[Pagination and image loading](EPUB_PAGINATION_AND_PERFORMANCE.md).

Multiple supplemental images in one canonical paragraph currently share that
paragraph's first illustration sizing policy. Ordinary invalid resources retain
the existing bounded fallback; complex publisher layouts can differ substantially
from their intended design.

## Reproducible checks

The synthetic fixture `core/tests/fixtures/native-typography.epub` covers emoji
scalar positions, font inheritance, decorations, units, nested lists, quotes,
colspan, two illustrations, preformatted text, ruby, and a cross-chapter note.
It is a regression fixture, not evidence of general publishing compatibility.

Shared tests live in `core/tests/epub_typography.rs`. The existing native-script
and illustration suites also run. Desktop tests exercise native glyph layout,
column limits, images, pagination, and turn gestures. Android's device runner
accepts `-e check typography` and `-e check corpus`; iOS Debug's `--verify-native`
uses the real Rust bridge, CoreText pagination, native attachment drawing, and
line overlap checks. These are distinct from UI inspection and physical-device
acceptance.

`cargo run -p reader_core --example native_compatibility -- <epub paths...>` audits
all source chapters in three styles with scripts disabled, records resource SHA-256,
checks presentation ranges and canonical text stability, and reports structural
features. It does not prove that every source block is visibly correct.

## Corpus

The corpus includes two published literary ebooks and three IDPF feature samples:

| EPUB | Source | Purpose |
| --- | --- | --- |
| Pride and Prejudice | [Project Gutenberg #1342](https://www.gutenberg.org/ebooks/1342) | Long prose, images, table rows |
| Alice's Adventures in Wonderland | [Standard Ebooks](https://standardebooks.org/ebooks/lewis-carroll/alices-adventures-in-wonderland/john-tenniel) | Publisher styles, lists, illustrations |
| Accessible EPUB 3 | [IDPF EPUB 3 samples](https://idpf.github.io/epub3-samples/30/samples.html) | Technical content and semantic structures |
| Hefty Water | [IDPF EPUB 3 samples](https://idpf.github.io/epub3-samples/30/samples.html) | Small ruby feature sample |
| Kusamakura | [IDPF EPUB 3 samples](https://idpf.github.io/epub3-samples/30/samples.html) | Extensive Japanese ruby; vertical writing remains unsupported |

Keep original publication/license notices inside downloaded EPUBs and record
download URLs, sizes and fingerprints in a local corpus manifest. Run parser
audits and native drawing checks separately; a parser pass is not visual acceptance.
