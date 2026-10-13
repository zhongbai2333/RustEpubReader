//! Font locations retain the face index: a TTC is a collection, not one font.
use eframe::egui::FontData;

#[derive(Clone, Debug)]
pub(crate) struct SystemFont {
    pub path: String,
    pub index: u32,
}

impl SystemFont {
    pub fn load(&self) -> Option<FontData> {
        let mut data = FontData::from_owned(std::fs::read(&self.path).ok()?);
        data.index = self.index;
        Some(data)
    }
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
            let Ok(bytes) = std::fs::read(&path) else {
                return Vec::new();
            };
            (0..ttf_parser::fonts_in_collection(&bytes).unwrap_or(1))
                .filter_map(|index| {
                    let face = ttf_parser::Face::parse(&bytes, index).ok()?;
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
pub(crate) fn mac_default(bold: bool, serif: bool) -> Option<FontData> {
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

#[cfg(target_os = "macos")]
pub(crate) fn register_defaults(fonts: &mut eframe::egui::FontDefinitions) {
    use eframe::egui::FontFamily;
    for (key, bold, serif) in [
        ("cjk_font", false, false),
        ("cjk_bold", true, false),
        ("serif_font", false, true),
    ] {
        if let Some(data) = mac_default(bold, serif) {
            fonts.font_data.insert(key.to_owned(), data.into());
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
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;
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
                .insert(name.to_owned(), fonts[name].load().unwrap().into());
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
}
