# epaint 0.31.1 local patch

Source: the unmodified crates.io epaint 0.31.1 package, checksum
`41fcc0f5a7c613afd2dee5e4b30c3e6acafb8ad6f0edb06068811f708a67c562`.
Upstream: https://github.com/emilk/egui/tree/0.31.1/crates/epaint
Licenses are preserved in LICENSE-MIT and LICENSE-APACHE.

The application retains egui 0.31's public API. This patch replaces the private
ASCII/CJK row-break candidate heuristic in `src/text/text_layout.rs` with ICU4X
2.1.1 line segmentation for the entire paragraph, including across font/style
runs. Normal wrapping uses ICU's default strict UAX #14 behavior. If no legal
break fits, CSS BreakAll opportunities provide an emergency break while retaining
punctuation attachment. Explicit `break_anywhere` uses grapheme boundaries.
Original text, glyph counts, style ranges, hard newlines, and source offsets are
unchanged. All ICU data is compiled locally; rendering makes no network requests.

The selected ICU line algorithm follows Unicode 15.1. It is a versioned rule set,
not a guarantee of every future Unicode version or publisher-specific typography.
ICU's 10,274-case untailored UAX #14 fixture is used for conformance verification.
The raw Unicode 15.1 test file tailors numeric rule LB25 and differs in 34 cases;
this patch does not claim to pass that tailored variant. See ICU's own
`tests/testdata/LineBreakTest.txt` header for that distinction.

Other local edits: normalized Cargo manifest adds ICU and reflects its Rust 1.83
minimum, keeps development tests in an independent workspace, and includes local
license paths. One existing f32 literal is made explicit for current Rust.
`Cargo.toml.orig` preserves the upstream manifest.

From the repository root, run:

```
CARGO_INCREMENTAL=0 cargo test --manifest-path vendor/epaint/Cargo.toml --lib
cargo test -p rust_epub_reader ui::reader_ -- --test-threads=1
```

When updating egui, first check whether upstream now provides Unicode-aware line
breaking, then remove this override only after the punctuation, grapheme, rich
text and pagination regressions pass with the replacement.
