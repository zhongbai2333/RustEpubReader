//! Local corpus audit. `--lazy` exercises production chapter loading, without scripts/network.
use reader_core::epub::native::CssPolicy;
use reader_core::epub::{ContentBlock, EpubBook};
use serde_json::json;
use sha2::{Digest, Sha256};

fn main() {
    let mut reports = Vec::new();
    let mut failed = false;
    let mut lazy = false;
    let mut paths = Vec::new();
    for argument in std::env::args().skip(1) {
        if argument == "--lazy" {
            lazy = true;
        } else {
            paths.push(argument);
        }
    }
    if paths.is_empty() {
        eprintln!("Usage: native_compatibility [--lazy] EPUB...");
        std::process::exit(2);
    }
    for path in paths {
        let start = std::time::Instant::now();
        let result = (|| -> Result<serde_json::Value, String> {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            let hash = format!("{:x}", Sha256::digest(&bytes));
            let book = if lazy {
                EpubBook::open_lazy(&path)
            } else {
                EpubBook::open(&path)
            }
            .map_err(|e| e.to_string())?;
            let initial_cache = book.chapter_cache_stats();
            if lazy && initial_cache.0 != 0 {
                return Err("Lazy catalogue retained chapter bodies before reading".into());
            }
            let mut counts = [[0usize; 11]; 3];
            let mut locations = [Vec::new(), Vec::new(), Vec::new()];
            let mut canonical_hash = Sha256::new();
            let mut presentation_hashes: [Sha256; 3] = std::array::from_fn(|_| Sha256::new());
            let mut maximum_cached = 0;
            for chapter in 0..book.chapters.len() {
                let loaded = book.loaded_chapter(chapter)?;
                let ch = &loaded.chapter;
                let original = serde_json::to_vec(ch).map_err(|e| e.to_string())?;
                canonical_hash.update((original.len() as u64).to_le_bytes());
                canonical_hash.update(&original);
                maximum_cached = maximum_cached.max(book.chapter_cache_stats().0);
                if lazy && maximum_cached > 8 {
                    return Err("Lazy chapter cache exceeded its count bound".into());
                }
                for (mode, policy) in [CssPolicy::None, CssPolicy::Mixed, CssPolicy::Publisher]
                    .into_iter()
                    .enumerate()
                {
                    let count = &mut counts[mode];
                    let view = ch.native_document.present(policy, false, &[]);
                    if !view.errors.is_empty() || view.styles.len() != ch.blocks.len() {
                        return Err(format!("Invalid native presentation: {chapter}"));
                    }
                    let style_bytes =
                        serde_json::to_vec(&view.styles).map_err(|e| e.to_string())?;
                    presentation_hashes[mode].update((style_bytes.len() as u64).to_le_bytes());
                    presentation_hashes[mode].update(style_bytes);
                    count[10] += loaded.images.len();
                    for (block, style) in view.styles.iter().enumerate() {
                        count[0] += 1;
                        count[1] += !style.inline.is_empty() as usize;
                        count[2] += style.ruby.len();
                        count[3] += style.table.is_some() as usize;
                        count[4] += (style.list_depth > 0) as usize;
                        count[5] += style.quote as usize;
                        count[6] += style.inline.iter().filter(|r| r.note.is_some()).count();
                        count[7] +=
                            (style.image_width.is_some() || style.image_height.is_some()) as usize;
                        count[8] += style.pre as usize;
                        count[9] += matches!(ch.blocks[block], ContentBlock::Image { .. }) as usize;
                        let source = match &ch.blocks[block] {
                            ContentBlock::Paragraph { spans, .. }
                            | ContentBlock::Heading { spans, .. } => {
                                spans.iter().map(|s| s.text.chars().count()).sum()
                            }
                            _ => 0,
                        };
                        if style
                            .inline
                            .iter()
                            .any(|r| r.end > source || r.start > r.end)
                            || style
                                .ruby
                                .iter()
                                .any(|r| r.end > source || r.start >= r.end)
                        {
                            return Err(format!("Source range changed: {chapter}:{block}"));
                        }
                        if locations[mode].len() < 24
                            && (style.table.is_some()
                                || !style.ruby.is_empty()
                                || style.inline.iter().any(|r| r.note.is_some()))
                        {
                            locations[mode].push(json!({"chapter":chapter,"block":block,"table":style.table.is_some(),"ruby":style.ruby.len(),"note":style.inline.iter().any(|r|r.note.is_some())}));
                        }
                    }
                }
                if serde_json::to_vec(ch).map_err(|e| e.to_string())? != original {
                    return Err(format!("Canonical source mutated: {chapter}"));
                }
            }
            let modes: Vec<_> = [CssPolicy::None, CssPolicy::Mixed, CssPolicy::Publisher]
                .into_iter().enumerate().map(|(mode, policy)| {
                    let c = counts[mode];
                    let styles_hash = format!("{:x}", presentation_hashes[mode].clone().finalize());
                    json!({"policy":policy.key(),"styles_sha256":styles_hash,"blocks":c[0],"inline_blocks":c[1],"ruby":c[2],"table_rows":c[3],"list_blocks":c[4],"quotes":c[5],"note_references":c[6],"sized_images":c[7],"pre_blocks":c[8],"canonical_images":c[9],"supplemental_images":c[10],"locations":locations[mode]})
                }).collect();
            Ok(
                json!({"audit_schema":2,"path":path,"sha256":hash,"bytes":bytes.len(),"loading":if lazy {"lazy"} else {"eager"},"chapters":book.chapters.len(),"stable_source":true,"canonical_sha256":format!("{:x}",canonical_hash.finalize()),"maximum_cached_chapters":maximum_cached,"cache_stats":book.chapter_cache_stats(),"modes":modes,"seconds":start.elapsed().as_secs_f64()}),
            )
        })();
        eprintln!("Audited {} ({:.2}s)", path, start.elapsed().as_secs_f64());
        reports.push(match result {
            Ok(v) => v,
            Err(e) => {
                failed = true;
                json!({"path":path,"error":e})
            }
        });
    }
    println!("{}", serde_json::to_string_pretty(&reports).unwrap());
    if failed {
        std::process::exit(1);
    }
}
