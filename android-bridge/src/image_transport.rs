//! Explicitly requested A/B transport; the normal reader continues using Base64.
use super::get_book;
use jni::{
    objects::{JClass, JString},
    sys::{jbyteArray, jstring},
    JNIEnv,
};
use reader_core::{
    base64_encode,
    epub::EpubBook,
    image_resources::{ImageLocation, ImageReferenceFactory, ImageResources, MAX_IMAGE_BYTES},
};
use serde_json::{json, Value};
use std::sync::LazyLock;
static RESOURCES: LazyLock<ImageResources> = LazyLock::new(Default::default);
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key].as_str().ok_or_else(|| format!("Missing {key}"))
}
fn payload(
    factory: Option<&ImageReferenceFactory>,
    chapter: usize,
    location: ImageLocation,
    data: &[u8],
) -> Result<String, String> {
    match factory {
        None => Ok(base64_encode(data)),
        Some(factory) => {
            if data.is_empty() || data.len() > MAX_IMAGE_BYTES {
                return Err("Image exceeds transfer limits".into());
            }
            factory.reference(chapter, location)
        }
    }
}
pub(super) fn chapter(
    book: &EpubBook,
    idx: usize,
    references: Option<&ImageReferenceFactory>,
) -> Result<Value, String> {
    if idx >= book.chapters.len() {
        return Err("Reading chapter is out of range".into());
    }
    let loaded = book.loaded_chapter(idx)?;
    let chapter = &loaded.chapter;
    let blocks: Vec<Value> = chapter
        .blocks
        .iter()
        .enumerate()
        .map(|(block_index, block)| {
            let value = match block {
                reader_core::epub::ContentBlock::Paragraph {
                    spans, anchor_id, ..
                } => {
                    serde_json::json!({
                        "type": "paragraph",
                        "spans": spans.iter().map(|s| serde_json::json!({
                            "text": s.text,
                            "style": format!("{:?}", s.style),
                            "linkUrl": s.link_url,
                        })).collect::<Vec<_>>(),
                        "anchor_id": anchor_id,
                    })
                }
                reader_core::epub::ContentBlock::Heading {
                    level,
                    spans,
                    anchor_id,
                    ..
                } => {
                    serde_json::json!({
                        "type": "heading",
                        "level": level,
                        "spans": spans.iter().map(|s| serde_json::json!({
                            "text": s.text,
                            "style": format!("{:?}", s.style),
                            "linkUrl": s.link_url,
                        })).collect::<Vec<_>>(),
                        "anchor_id": anchor_id,
                    })
                }
                reader_core::epub::ContentBlock::Separator => {
                    serde_json::json!({ "type": "separator" })
                }
                reader_core::epub::ContentBlock::BlankLine => {
                    serde_json::json!({ "type": "blankLine" })
                }
                reader_core::epub::ContentBlock::Image { data, alt, .. } => {
                    serde_json::json!({
                        "type": "image",
                        "data": payload(references, idx, ImageLocation::Block(block_index), data)?,
                        "alt": alt,
                    })
                }
            };
            Ok(value)
        })
        .collect::<Result<_, String>>()?;

    let json = serde_json::json!({
        "title": chapter.title,
        "sourceHref": chapter.source_href,
        "blocks": blocks,
        "images": loaded.images.iter().enumerate().map(|(n,image)| Ok(serde_json::json!({
            "block": image.block, "after": image.after, "data": payload(references,idx,ImageLocation::Supplemental(n),&image.data)?, "alt": image.alt
        }))).collect::<Result<Vec<_>,String>>()?,
    });

    if references.is_some() && json.to_string().len() > 64 * 1024 * 1024 {
        return Err("EPUB chapter exceeds the 64 MiB limit".into());
    }
    Ok(json)
}
fn command(path: &str, v: &Value) -> Result<Value, String> {
    match field(v, "action")? {
        "begin" => Ok(json!(RESOURCES.reserve(path)?)),
        "prepare" => {
            RESOURCES.attach(path, field(v, "session")?, get_book(path)?)?;
            Ok(json!(true))
        }
        "chapter" => RESOURCES.with_session(path, field(v, "session")?, |book, factory| {
            let idx = v["chapter"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("Invalid chapter")?;
            chapter(book, idx, Some(&factory))
        }),
        "close" => Ok(json!(RESOURCES.close(path, field(v, "session")?)?)),
        _ => Err("Invalid image transport action".into()),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_imageTransport(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    request: JString,
) -> jstring {
    let path: String =
        checked_storage!(env, env.get_string(&path).map_err(|e| e.to_string())).into();
    let input: String =
        checked_storage!(env, env.get_string(&request).map_err(|e| e.to_string())).into();
    if input.len() > 4096 {
        let _ = env.throw_new("java/io/IOException", "Image request too large");
        return std::ptr::null_mut();
    }
    let value: Value =
        checked_storage!(env, serde_json::from_str(&input).map_err(|e| e.to_string()));
    let result = checked_storage!(env, command(&path, &value));
    jni_string_or_null!(env, result.to_string())
}
#[unsafe(no_mangle)]
pub extern "C" fn Java_com_zhongbai233_epub_reader_RustBridge_imageBytes(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    reference: JString,
) -> jbyteArray {
    let path: String =
        checked_storage!(env, env.get_string(&path).map_err(|e| e.to_string())).into();
    let reference: String =
        checked_storage!(env, env.get_string(&reference).map_err(|e| e.to_string())).into();
    let data = checked_storage!(env, RESOURCES.bytes(&path, &reference));
    checked_storage!(
        env,
        env.byte_array_from_slice(&data).map_err(|e| e.to_string())
    )
    .into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn android_resource_header_is_exact_legacy_wire_after_resolving_images() {
        let book = Arc::new(
            EpubBook::open(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../core/tests/fixtures/illustrations.epub"
            ))
            .unwrap(),
        );
        let store = ImageResources::default();
        let owner = "isolated/android/book";
        let token = store.open(owner, book.clone()).unwrap();
        let mut images = 0;
        for index in 0..book.chapters.len() {
            let baseline = chapter(&book, index, None).unwrap();
            let mut binary = store
                .with_session(owner, &token, |book, factory| {
                    chapter(book, index, Some(&factory))
                })
                .unwrap();
            for key in ["blocks", "images"] {
                for value in binary[key].as_array_mut().unwrap() {
                    if key == "images" || value["type"] == "image" {
                        let data = store.bytes(owner, value["data"].as_str().unwrap()).unwrap();
                        value["data"] = json!(base64_encode(&data));
                        images += 1;
                    }
                }
            }
            assert_eq!(baseline, binary, "Chapter {index} metadata changed");
        }
        assert!(images >= 8);
        assert!(chapter(&book, usize::MAX, None).is_err());
        assert!(store.close(owner, &token).unwrap());
    }
    use std::sync::Arc;
}
