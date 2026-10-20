//! Explicit A/B transport; normal chapter calls retain the Base64 contract.
use super::{book, entry, field, number};
use reader_core::{
    cloud_sync::valid_device_id,
    epub::{ContentBlock, EpubBook},
    image_resources::{ImageLocation, ImageReferenceFactory, ImageResources, MAX_IMAGE_BYTES},
};
use serde_json::{json, Value};
use std::{
    ffi::CStr,
    os::raw::c_char,
    path::Path,
    sync::{Arc, LazyLock},
};
static RESOURCES: LazyLock<ImageResources> = LazyLock::new(Default::default);
fn owner(v: &Value) -> Result<String, String> {
    let root = field(v, "root")?;
    let id = field(v, "id")?;
    if !Path::new(root).is_absolute()
        || !valid_device_id(field(v, "device")?)
        || id.is_empty()
        || id.len() > 128
    {
        return Err("Invalid image owner".into());
    }
    Ok(json!([root, id]).to_string())
}
pub(super) fn command(v: &Value) -> Result<Value, String> {
    let owner = owner(v)?;
    match field(v, "action")? {
        "begin" => Ok(json!(RESOURCES.reserve(&owner)?)),
        "prepare" => {
            let root = field(v, "root")?;
            let e = entry(root, field(v, "id")?)?;
            RESOURCES.attach(&owner, field(v, "session")?, book(root, &e)?)?;
            Ok(json!(true))
        }
        "chapter" => RESOURCES.with_session(&owner, field(v, "session")?, |book, factory| {
            chapter(book, number(v, "chapter")?, Some(&factory))
        }),
        "close" => Ok(json!(RESOURCES.close(&owner, field(v, "session")?)?)),
        _ => Err("Invalid image transport action".into()),
    }
}
pub(super) fn chapter(
    parsed: &EpubBook,
    index: usize,
    references: Option<&ImageReferenceFactory>,
) -> Result<Value, String> {
    let source = parsed
        .chapters
        .get(index)
        .ok_or("Reading chapter is out of range")?;
    let loaded = parsed.loaded_chapter(index)?;
    let mut locations = std::collections::HashMap::new();
    if references.is_some() {
        for (i, block) in loaded.chapter.blocks.iter().enumerate() {
            if let ContentBlock::Image { data, .. } = block {
                locations.insert(Arc::as_ptr(data), ImageLocation::Block(i));
            }
        }
        for (i, image) in loaded.images.iter().enumerate() {
            locations.insert(Arc::as_ptr(&image.data), ImageLocation::Supplemental(i));
        }
    }
    let chapter_index = index;
    let mut budget = 0usize;
    let mut blocks = Vec::new();
    for (index, offset, block) in parsed.render_chapter(index)? {
        let mut value = match block {
            ContentBlock::Image { data, alt, .. } => {
                if references.is_none()
                    && data.len().div_ceil(3).saturating_mul(4) > 64 * 1024 * 1024 - budget
                {
                    return Err("EPUB chapter exceeds the 64 MiB limit".into());
                }
                let payload = if let Some(factory) = references {
                    if data.is_empty() || data.len() > MAX_IMAGE_BYTES {
                        return Err("Image exceeds transfer limits".into());
                    }
                    factory.reference(
                        chapter_index,
                        *locations
                            .get(&Arc::as_ptr(&data))
                            .ok_or("Invalid image mapping")?,
                    )?
                } else {
                    reader_core::base64_encode(&data)
                };
                json!({"type":"Image","data":payload,"alt":alt})
            }
            _ => {
                let mut value = serde_json::to_value(block).map_err(|_| "Cannot encode chapter")?;
                // Native presentation is requested separately; default style
                // objects need not be repeated for every canonical paragraph.
                value.as_object_mut().unwrap().remove("native_style");
                value
            }
        };
        value["core_block"] = json!(index);
        value["core_char_offset"] = json!(offset);
        budget = budget.saturating_add(value.to_string().len());
        if budget > 64 * 1024 * 1024 {
            return Err("EPUB chapter exceeds the 64 MiB limit".into());
        }
        blocks.push(value);
    }
    Ok(json!({"title":source.title,"blocks":blocks}))
}
/// Opaque, owned lease on immutable Rust bytes. Swift copies before releasing it.
pub struct ReaderImageBuffer(Arc<Vec<u8>>);
/// # Safety
/// Input must be a readable NUL-terminated UTF-8 JSON string for this call.
/// A non-null result is owned; release it exactly once with image_free.
#[no_mangle]
pub unsafe extern "C" fn reader_ios_image_read(input: *const c_char) -> *mut ReaderImageBuffer {
    let result = std::panic::catch_unwind(|| -> Result<_, String> {
        if input.is_null() {
            return Err("Null image request".into());
        }
        let text = unsafe { CStr::from_ptr(input) }
            .to_str()
            .map_err(|_| "Invalid image request")?;
        if text.len() > 4096 {
            return Err("Image request too large".into());
        }
        let v: Value = serde_json::from_str(text).map_err(|_| "Invalid image request")?;
        RESOURCES.bytes(&owner(&v)?, field(&v, "reference")?)
    });
    match result {
        Ok(Ok(data)) => Box::into_raw(Box::new(ReaderImageBuffer(data))),
        _ => std::ptr::null_mut(),
    }
}
/// # Safety
/// Buffer is null or a live owned handle from image_read; returned bytes are
/// read-only and valid until that handle is freed. Never mutate/reallocate them.
#[no_mangle]
pub unsafe extern "C" fn reader_ios_image_data(buffer: *const ReaderImageBuffer) -> *const u8 {
    unsafe { buffer.as_ref() }.map_or(std::ptr::null(), |buffer| buffer.0.as_ptr())
}
/// # Safety
/// Buffer is null or a live owned handle from image_read.
#[no_mangle]
pub unsafe extern "C" fn reader_ios_image_length(buffer: *const ReaderImageBuffer) -> usize {
    unsafe { buffer.as_ref() }.map_or(0, |buffer| buffer.0.len())
}
/// # Safety
/// Buffer is null or an owned live handle from image_read. Free exactly once;
/// no borrowed byte pointer may be accessed after this call.
#[no_mangle]
pub unsafe extern "C" fn reader_ios_image_free(buffer: *mut ReaderImageBuffer) {
    if !buffer.is_null() {
        drop(unsafe { Box::from_raw(buffer) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    #[test]
    fn ios_resource_headers_and_owned_c_buffers_preserve_wire_and_lifetime() {
        let _test = super::super::TEST_SESSION_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let root = root.path().to_str().unwrap();
        let book = Arc::new(
            EpubBook::open(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../core/tests/fixtures/illustrations.epub"
            ))
            .unwrap(),
        );
        let request = json!({"root":root,"id":"fixture","device":"image-abi-check"});
        let owner = owner(&request).unwrap();
        let token = RESOURCES.open(&owner, book.clone()).unwrap();
        let mut retained: *mut ReaderImageBuffer = std::ptr::null_mut();
        let mut first = String::new();
        let mut count = 0;
        for index in 0..book.chapters.len() {
            let baseline = chapter(&book, index, None).unwrap();
            let mut binary = RESOURCES
                .with_session(&owner, &token, |book, factory| {
                    chapter(book, index, Some(&factory))
                })
                .unwrap();
            for value in binary["blocks"].as_array_mut().unwrap() {
                if value["type"] != "Image" {
                    continue;
                }
                let reference = value["data"].as_str().unwrap().to_owned();
                let mut v = request.clone();
                v["reference"] = json!(reference);
                let input = CString::new(v.to_string()).unwrap();
                let handle = unsafe { reader_ios_image_read(input.as_ptr()) };
                assert!(!handle.is_null());
                let bytes = unsafe {
                    std::slice::from_raw_parts(
                        reader_ios_image_data(handle),
                        reader_ios_image_length(handle),
                    )
                };
                assert_eq!(bytes, &**RESOURCES.bytes(&owner, &reference).unwrap());
                value["data"] = json!(reader_core::base64_encode(bytes));
                count += 1;
                if retained.is_null() {
                    retained = handle;
                    first = reference;
                } else {
                    unsafe { reader_ios_image_free(handle) }
                }
            }
            assert_eq!(baseline, binary, "Chapter {index} metadata changed");
        }
        assert!(count >= 8);
        let mut v = request.clone();
        v["reference"] = json!(first);
        v["id"] = json!("foreign");
        let input = CString::new(v.to_string()).unwrap();
        assert!(unsafe { reader_ios_image_read(input.as_ptr()) }.is_null());
        assert!(RESOURCES.close(&owner, &token).unwrap());
        v["id"] = json!("fixture");
        let input = CString::new(v.to_string()).unwrap();
        assert!(unsafe { reader_ios_image_read(input.as_ptr()) }.is_null());
        // A returned lease survives registry close. No zero-copy mutable Swift Data.
        assert!(unsafe { reader_ios_image_length(retained) } > 0);
        assert!(!unsafe { reader_ios_image_data(retained) }.is_null());
        unsafe {
            reader_ios_image_free(retained);
            reader_ios_image_free(std::ptr::null_mut())
        };
        assert_eq!(unsafe { reader_ios_image_length(std::ptr::null()) }, 0);
        assert!(unsafe { reader_ios_image_data(std::ptr::null()) }.is_null());
        assert!(unsafe { reader_ios_image_read(std::ptr::null()) }.is_null());
        let invalid = CString::new("{}").unwrap();
        assert!(unsafe { reader_ios_image_read(invalid.as_ptr()) }.is_null());
    }
}
