//! Reproducible, local corpus audit. It never executes book scripts or sends data.
use reader_core::epub::native::CssPolicy;
use reader_core::epub::{ContentBlock, EpubBook};
use serde_json::json;
use sha2::{Digest, Sha256};
fn main() {
    let mut reports = Vec::new();
    for path in std::env::args().skip(1) {
        let start = std::time::Instant::now();
        let result = (|| -> Result<serde_json::Value, String> {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            let hash = format!("{:x}", Sha256::digest(&bytes));
            let book = EpubBook::open(&path).map_err(|e| e.to_string())?;
            let original = serde_json::to_string(&book.chapters).map_err(|e| e.to_string())?;
            let mut modes = Vec::new();
            for policy in [CssPolicy::None, CssPolicy::Mixed, CssPolicy::Publisher] {
                let mut count = [0usize; 9];
                let mut locations = Vec::new();
                for (chapter, ch) in book.chapters.iter().enumerate() {
                    let view = ch.native_document.present(policy, false, &[]);
                    if !view.errors.is_empty() || view.styles.len() != ch.blocks.len() {
                        return Err(format!("Invalid native presentation: {chapter}"));
                    }
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
                        if locations.len() < 24
                            && (style.table.is_some()
                                || !style.ruby.is_empty()
                                || style.inline.iter().any(|r| r.note.is_some()))
                        {
                            locations.push(json!({"chapter":chapter,"block":block,"table":style.table.is_some(),"ruby":style.ruby.len(),"note":style.inline.iter().any(|r|r.note.is_some())}));
                        }
                    }
                }
                modes.push(json!({"policy":policy.key(),"blocks":count[0],"inline_blocks":count[1],"ruby":count[2],"table_rows":count[3],"list_blocks":count[4],"quotes":count[5],"note_references":count[6],"sized_images":count[7],"pre_blocks":count[8],"locations":locations}));
            }
            if serde_json::to_string(&book.chapters).map_err(|e| e.to_string())? != original {
                return Err("Canonical source mutated".into());
            }
            Ok(
                json!({"path":path,"sha256":hash,"bytes":bytes.len(),"chapters":book.chapters.len(),"stable_source":true,"modes":modes,"seconds":start.elapsed().as_secs_f64()}),
            )
        })();
        eprintln!("Audited {} ({:.2}s)", path, start.elapsed().as_secs_f64());
        reports.push(match result {
            Ok(v) => v,
            Err(e) => json!({"path":path,"error":e}),
        });
    }
    println!("{}", serde_json::to_string_pretty(&reports).unwrap());
}
