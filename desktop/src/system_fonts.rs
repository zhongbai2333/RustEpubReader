//! Font locations retain the face index: a TTC is a collection, not one font.
use eframe::egui::FontData;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, OnceLock, Weak};

#[derive(Clone, Debug)]
pub(crate) struct SystemFont {
    pub path: String,
    pub index: u32,
}

impl SystemFont {
    pub fn load(&self) -> Option<Arc<FontData>> {
        type FontCache = std::collections::HashMap<(String, u32), Weak<FontData>>;
        static CACHE: OnceLock<Mutex<FontCache>> = OnceLock::new();
        let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
        cache.retain(|_, font| font.strong_count() > 0);
        let key = (self.path.clone(), self.index);
        if let Some(font) = cache.get(&key).and_then(Weak::upgrade) {
            return Some(font);
        }
        // epaint's rasterizer uses outlines only. Do not load every face in a TTC,
        // or hundreds of MB of bitmap strikes it cannot render (Apple Color Emoji).
        // All outline, metric, naming and shaping tables of this face are retained.
        let bytes = read_face(&self.path, self.index, false).ok()?;
        ttf_parser::Face::parse(&bytes, 0).ok()?;
        let font = Arc::new(FontData::from_owned(bytes));
        cache.insert(key, Arc::downgrade(&font));
        Some(font)
    }
}

pub(crate) fn load_path(path: &str) -> Option<Arc<FontData>> {
    SystemFont {
        path: path.into(),
        index: 0,
    }
    .load()
}

fn read_face(path: &str, index: u32, metadata_only: bool) -> std::io::Result<Vec<u8>> {
    use std::io::{Error, ErrorKind};
    let invalid = || Error::new(ErrorKind::InvalidData, "invalid font table directory");
    let mut file = std::fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut header = [0u8; 12];
    file.read_exact(&mut header)?;
    if &header[..4] == b"ttcf" {
        let count = u32::from_be_bytes(header[8..12].try_into().unwrap());
        if index >= count {
            return Err(invalid());
        }
        file.seek(SeekFrom::Start(12 + u64::from(index) * 4))?;
        let mut offset = [0; 4];
        file.read_exact(&mut offset)?;
        file.seek(SeekFrom::Start(u64::from(u32::from_be_bytes(offset))))?;
        file.read_exact(&mut header)?;
    } else if index != 0 {
        return Err(invalid());
    }
    if !matches!(&header[..4], b"\0\x01\0\0" | b"OTTO" | b"true") {
        return Err(invalid());
    }
    let count = u16::from_be_bytes(header[4..6].try_into().unwrap()) as usize;
    if count == 0 || count > 1024 {
        return Err(invalid());
    }
    let mut records = vec![[0u8; 16]; count];
    for record in &mut records {
        file.read_exact(record)?;
    }
    records.retain(|r| {
        if metadata_only {
            matches!(&r[..4], b"head" | b"hhea" | b"maxp" | b"name")
        } else {
            !matches!(
                &r[..4],
                b"sbix" | b"CBDT" | b"CBLC" | b"EBDT" | b"EBLC" | b"EBSC" | b"SVG " | b"DSIG"
            )
        }
    });
    if records.is_empty() {
        return Err(invalid());
    }
    records.sort_unstable_by_key(|r| [r[0], r[1], r[2], r[3]]);
    let total = records
        .iter()
        .try_fold(12 + records.len() * 16, |sum, r| {
            let offset = u32::from_be_bytes(r[8..12].try_into().unwrap()) as u64;
            let length = u32::from_be_bytes(r[12..16].try_into().unwrap()) as usize;
            if offset + length as u64 > file_len {
                return None;
            }
            sum.checked_add((length + 3) & !3)
        })
        .filter(|size| *size <= 256 * 1024 * 1024)
        .ok_or_else(invalid)?;
    let mut output = Vec::with_capacity(total);
    let count = records.len() as u16;
    let power = 1u16 << (15 - count.leading_zeros());
    header[4..6].copy_from_slice(&count.to_be_bytes());
    header[6..8].copy_from_slice(&(power * 16).to_be_bytes());
    header[8..10].copy_from_slice(&(power.ilog2() as u16).to_be_bytes());
    header[10..12].copy_from_slice(&(count * 16 - power * 16).to_be_bytes());
    output.extend_from_slice(&header);
    output.resize(12 + records.len() * 16, 0);
    let mut head_adjustment = None;
    for (i, mut record) in records.into_iter().enumerate() {
        let offset = u32::from_be_bytes(record[8..12].try_into().unwrap());
        let length = u32::from_be_bytes(record[12..16].try_into().unwrap()) as usize;
        let start = output.len();
        output.resize(start + length, 0);
        file.seek(SeekFrom::Start(u64::from(offset)))?;
        file.read_exact(&mut output[start..])?;
        if &record[..4] == b"head" && length >= 12 {
            output[start + 8..start + 12].fill(0);
            head_adjustment = Some(start + 8);
        }
        output.resize((output.len() + 3) & !3, 0);
        record[4..8].copy_from_slice(&sfnt_checksum(&output[start..]).to_be_bytes());
        record[8..12].copy_from_slice(&(start as u32).to_be_bytes());
        output[12 + i * 16..28 + i * 16].copy_from_slice(&record);
    }
    if let Some(offset) = head_adjustment {
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(sfnt_checksum(&output));
        output[offset..offset + 4].copy_from_slice(&adjustment.to_be_bytes());
    }
    Ok(output)
}

fn sfnt_checksum(bytes: &[u8]) -> u32 {
    bytes.as_chunks::<4>().0.iter().fold(0u32, |sum, word| {
        sum.wrapping_add(u32::from_be_bytes(*word))
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn discover() -> std::collections::HashMap<String, SystemFont> {
    static FONTS: std::sync::OnceLock<std::collections::HashMap<String, SystemFont>> =
        std::sync::OnceLock::new();
    FONTS.get_or_init(discover_uncached).clone()
}

#[cfg(target_os = "macos")]
fn discover_uncached() -> std::collections::HashMap<String, SystemFont> {
    use std::collections::HashMap;
    let mut result = HashMap::new();
    let mut files: HashMap<String, Vec<(String, u32)>> = HashMap::new();
    let collection = core_text::font_collection::create_for_all_families();
    let Some(descriptors) = collection.get_descriptors() else {
        return result;
    };
    for descriptor in descriptors.iter() {
        let Some(path) = descriptor.font_path() else {
            continue;
        };
        let path = path.to_string_lossy().into_owned();
        let faces = files.entry(path.clone()).or_insert_with(|| {
            let Ok(mut file) = std::fs::File::open(&path) else {
                return Vec::new();
            };
            let mut header = [0; 12];
            if file.read_exact(&mut header).is_err() {
                return Vec::new();
            }
            let count = if &header[..4] == b"ttcf" {
                u32::from_be_bytes(header[8..].try_into().unwrap()).min(4096)
            } else {
                1
            };
            (0..count)
                .filter_map(|index| {
                    let bytes = read_face(&path, index, true).ok()?;
                    let face = ttf_parser::Face::parse(&bytes, 0).ok()?;
                    let name = face.names().into_iter().find_map(|name| {
                        (name.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
                            .then(|| name.to_string())
                            .flatten()
                    })?;
                    Some((name, index))
                })
                .collect()
        });
        let postscript = descriptor.font_name();
        let Some((_, index)) = faces.iter().find(|(name, _)| name == &postscript) else {
            continue;
        };
        let font = SystemFont {
            path,
            index: *index,
        };
        let family = descriptor.family_name();
        let style = descriptor.style_name();
        let friendly = match family.as_str() {
            "PingFang SC" => "苹方-简",
            "PingFang TC" => "苹方-繁",
            "PingFang HK" => "苹方-港",
            "Songti SC" => "宋体-简",
            "Songti TC" => "宋体-繁",
            "Kaiti SC" => "楷体-简",
            "Kaiti TC" => "楷体-繁",
            "Heiti SC" => "黑体-简",
            "Heiti TC" => "黑体-繁",
            "Hiragino Sans GB" => "冬青黑体-简",
            _ => &family,
        };
        result.insert(format!("{friendly} {style}"), font.clone());
        // Stable family aliases always select the regular face, never collection index zero.
        if matches!(style.as_str(), "Regular" | "Roman") || postscript.ends_with("-Regular") {
            result.insert(friendly.to_owned(), font.clone());
            if family == "PingFang SC" {
                result.insert("苹方".to_owned(), font);
            }
        }
    }
    result
}

#[cfg(target_os = "macos")]
pub(crate) fn mac_default(bold: bool, serif: bool) -> Option<Arc<FontData>> {
    static FONTS: std::sync::OnceLock<std::collections::HashMap<String, SystemFont>> =
        std::sync::OnceLock::new();
    let fonts = FONTS.get_or_init(discover);
    let name = if serif {
        "宋体-简"
    } else if bold {
        "苹方-简 Semibold"
    } else {
        "苹方-简"
    };
    fonts.get(name).or_else(|| fonts.get("苹方-简"))?.load()
}

/// Append a bounded pair of installed outline fonts for missing script glyphs.
/// Existing user/EPUB/CJK choices retain priority; no OS font is redistributed.
pub(crate) fn register_fallbacks(fonts: &mut eframe::egui::FontDefinitions) {
    use eframe::egui::FontFamily;
    #[cfg(target_os = "macos")]
    let candidates: [&[&str]; 2] = [
        &["/System/Library/Fonts/Supplemental/Arial.ttf"],
        &["/System/Library/Fonts/Supplemental/Arial Bold.ttf"],
    ];
    #[cfg(target_os = "windows")]
    let candidates: [&[&str]; 2] = [
        &[
            r"C:\Windows\Fonts\arial.ttf",
            r"C:\Windows\Fonts\segoeui.ttf",
        ],
        &[
            r"C:\Windows\Fonts\arialbd.ttf",
            r"C:\Windows\Fonts\segoeuib.ttf",
        ],
    ];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let candidates: [&[&str]; 2] = [
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        ],
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
        ],
    ];
    let keys = [
        "reader_system_fallback_regular",
        "reader_system_fallback_bold",
    ];
    let mut available = [false; 2];
    for (index, paths) in candidates.iter().enumerate() {
        if let Some(data) = paths.iter().find_map(|path| load_path(path)) {
            fonts.font_data.insert(keys[index].into(), data);
            available[index] = true;
        }
    }
    for (family, chain) in &mut fonts.families {
        let preferred =
            usize::from(matches!(family,FontFamily::Name(name) if name.as_ref()=="Bold"));
        chain.retain(|name| !keys.contains(&name.as_str()));
        // Named publication families can replace the default proportional chain.
        // Keep the bundled outline emoji faces reachable there as well; system
        // color fonts may contain bitmap-only glyphs our rasterizer cannot draw.
        for name in ["NotoEmoji-Regular", "emoji-icon-font"] {
            if fonts.font_data.contains_key(name) && !chain.iter().any(|item| item == name) {
                chain.push(name.into());
            }
        }
        for index in [preferred, 1 - preferred] {
            if available[index] && !chain.iter().any(|name| name == keys[index]) {
                chain.push(keys[index].into());
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn register_defaults(fonts: &mut eframe::egui::FontDefinitions) {
    use eframe::egui::FontFamily;
    for (key, bold, serif) in [
        ("cjk_font", false, false),
        ("cjk_bold", true, false),
        ("serif_font", false, true),
    ] {
        if let Some(data) = mac_default(bold, serif) {
            fonts.font_data.insert(key.to_owned(), data);
        }
    }
    if fonts.font_data.contains_key("cjk_font") {
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let chain = fonts.families.entry(family).or_default();
            if !chain.iter().any(|name| name == "cjk_font") {
                chain.push("cjk_font".to_owned());
            }
        }
        fonts.families.insert(
            FontFamily::Name("Bold".into()),
            vec!["cjk_bold".into(), "cjk_font".into()],
        );
        fonts.families.insert(
            FontFamily::Name("Serif".into()),
            vec!["serif_font".into(), "cjk_font".into()],
        );
    }
    register_fallbacks(fonts);
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;
    #[test]
    fn native_default_fallbacks_cover_the_real_corpus_hebrew_and_greek() {
        use eframe::egui::{FontDefinitions, FontFamily};
        let mut definitions = FontDefinitions::default();
        register_defaults(&mut definitions);
        let before = definitions.families.clone();
        register_fallbacks(&mut definitions);
        assert_eq!(before, definitions.families);
        for family in [
            FontFamily::Proportional,
            FontFamily::Name("Bold".into()),
            FontFamily::Name("Serif".into()),
        ] {
            for ch in "חוϰητος😀".chars() {
                assert!(
                    definitions.families[&family]
                        .iter()
                        .filter_map(|name| definitions.font_data.get(name))
                        .any(|data| ttf_parser::Face::parse(&data.font, data.index)
                            .ok()
                            .and_then(|face| face.glyph_index(ch))
                            .is_some_and(|glyph| glyph.0 != 0)),
                    "Missing corpus glyph {ch:?} in {family:?}"
                );
            }
        }
    }
    #[test]
    fn installed_chinese_faces_load_with_correct_collection_index() {
        let fonts = discover();
        for (name, expected) in [
            ("苹方-简", "PingFangSC-Regular"),
            ("宋体-简", "STSongti-SC-Regular"),
        ] {
            let font = fonts.get(name).unwrap_or_else(|| panic!("missing {name}"));
            let data = font.load().expect("read installed font");
            let face =
                ttf_parser::Face::parse(&data.font, data.index).expect("parse selected face");
            assert!(
                face.names()
                    .into_iter()
                    .any(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                        && n.to_string().as_deref() == Some(expected)),
                "wrong face for {name}: {:?}",
                face.names()
                    .into_iter()
                    .filter(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
                    .filter_map(|n| n.to_string())
                    .collect::<Vec<_>>()
            );
            for ch in ['中', '文', '骨', '门'] {
                assert!(face.glyph_index(ch).is_some());
            }
        }
        let ctx = eframe::egui::Context::default();
        let mut definitions = eframe::egui::FontDefinitions::default();
        register_defaults(&mut definitions);
        for name in ["苹方-简", "宋体-简"] {
            definitions
                .font_data
                .insert(name.to_owned(), fonts[name].load().unwrap());
            definitions.families.insert(
                eframe::egui::FontFamily::Name(name.into()),
                vec![name.to_owned()],
            );
        }
        ctx.set_fonts(definitions);
        let _ = ctx.run(Default::default(), |ctx| {
            for name in ["苹方-简", "宋体-简"] {
                let galley = ctx.fonts(|fonts| {
                    fonts.layout_no_wrap(
                        "中文阅读骨门".into(),
                        eframe::egui::FontId::new(
                            18.0,
                            eframe::egui::FontFamily::Name(name.into()),
                        ),
                        eframe::egui::Color32::BLACK,
                    )
                });
                assert!(galley.size().x > 0.0);
            }
        });
        assert!(mac_default(false, false).is_some());
        assert!(mac_default(true, false).is_some());
    }

    #[test]
    fn compact_faces_preserve_outline_tables_and_share_live_buffers() {
        let fonts = discover();
        for name in ["苹方-简", "苹方-简 Semibold", "宋体-简"] {
            let selected = &fonts[name];
            let original = std::fs::read(&selected.path).unwrap();
            let original = ttf_parser::RawFace::parse(&original, selected.index).unwrap();
            let data = selected.load().unwrap();
            assert!(Arc::ptr_eq(&data, &selected.load().unwrap()));
            let compact = ttf_parser::RawFace::parse(&data.font, 0).unwrap();
            for record in original.table_records {
                if record.tag == ttf_parser::Tag::from_bytes(b"head") {
                    continue; // checksumAdjustment changes after collection extraction.
                }
                if let Some(table) = compact.table(record.tag) {
                    assert_eq!(
                        original.table(record.tag),
                        Some(table),
                        "{name}: {:?}",
                        record.tag
                    );
                } else {
                    assert!(matches!(
                        &record.tag.to_bytes(),
                        b"sbix"
                            | b"CBDT"
                            | b"CBLC"
                            | b"EBDT"
                            | b"EBLC"
                            | b"EBSC"
                            | b"SVG "
                            | b"DSIG"
                    ));
                }
            }
            assert_eq!(sfnt_checksum(&data.font), 0xB1B0_AFBA);
            let temporary = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(temporary.path(), &data.font).unwrap();
            let isolated = load_path(temporary.path().to_str().unwrap()).unwrap();
            let weak = Arc::downgrade(&isolated);
            drop(isolated);
            assert!(
                weak.upgrade().is_none(),
                "font cache must not pin unused faces"
            );
        }
        let emoji = load_path("/System/Library/Fonts/Apple Color Emoji.ttc").unwrap();
        assert!(
            emoji.font.len() < 2 * 1024 * 1024,
            "outline renderer must not retain unused bitmap strikes"
        );
    }
}
