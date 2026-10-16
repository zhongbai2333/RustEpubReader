//! Contains image resolution, extraction, and parsing logic for EPUB files.
use std::collections::HashMap;
use std::path::Path;

use base64::Engine;
use scraper::{Html, Selector};

pub(super) fn image_src(node: &scraper::node::Element) -> &str {
    node.attr("src")
        .or_else(|| node.attr("data-src"))
        // Foreign SVG attributes carry an XML namespace: Element::attr only
        // searches the null namespace, whereas attrs() exposes local names.
        .or_else(|| {
            node.attrs()
                .find_map(|(name, value)| matches!(name, "href" | "xlink:href").then_some(value))
        })
        .unwrap_or("")
}

pub(super) fn decode_url_path(value: &str) -> String {
    String::from_utf8_lossy(&decode_percent(value)).into_owned()
}
fn decode_percent(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push((a * 16 + b) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Produce a bounded raster usable by every native renderer. SVG may embed
/// data URLs, but must never resolve paths from the host filesystem or network.
pub(super) fn native_image_bytes(data: Vec<u8>) -> Vec<u8> {
    native_image_bytes_resolving(data, |_| None)
}
fn native_image_bytes_resolving(
    data: Vec<u8>,
    resolve: impl Fn(&str) -> Option<std::sync::Arc<Vec<u8>>> + Send + Sync,
) -> Vec<u8> {
    let prefix = String::from_utf8_lossy(&data[..data.len().min(1024)]);
    if !prefix.contains("<svg") {
        return data;
    }
    let render = || -> Option<Vec<u8>> {
        static FONTS: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
            std::sync::OnceLock::new();
        let fonts = FONTS.get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            std::sync::Arc::new(db)
        });
        let options = resvg::usvg::Options {
            fontdb: fonts.clone(),
            image_href_resolver: resvg::usvg::ImageHrefResolver {
                resolve_string: Box::new(|href, options| {
                    let data = resolve(href)?;
                    (resvg::usvg::ImageHrefResolver::default_data_resolver())(
                        "text/plain",
                        data,
                        options,
                    )
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let tree = resvg::usvg::Tree::from_data(&data, &options).ok()?;
        let size = tree.size();
        let scale = (2048. / size.width()).min(2048. / size.height()).min(1.);
        let mut pixmap = resvg::tiny_skia::Pixmap::new(
            (size.width() * scale).ceil().max(1.) as u32,
            (size.height() * scale).ceil().max(1.) as u32,
        )?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmap.encode_png().ok()
    };
    render().unwrap_or(data)
}

fn prepare_epub_image(
    data: Vec<u8>,
    path: &str,
    manifest: &[String],
    read: &mut impl FnMut(&str) -> Option<Vec<u8>>,
) -> Vec<u8> {
    let mut resources = HashMap::new();
    if String::from_utf8_lossy(&data[..data.len().min(1024)]).contains("<svg") {
        let document = Html::parse_document(&String::from_utf8_lossy(&data));
        let images = Selector::parse("image").expect("valid selector");
        for image in document.select(&images) {
            let href = image_src(image.value()).trim();
            // Only manifest resources can be loaded. No SVG path can reach the host
            // filesystem or make a network request, including file:/absolute URLs.
            if href.contains(":") {
                continue;
            }
            let directory = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
            let Some((_, resolved)) = clean_and_resolve_src(href, directory) else {
                continue;
            };
            let Some(resource) = manifest.iter().find(|candidate| {
                normalize_resource_path(candidate).trim_start_matches('/')
                    == resolved.trim_start_matches('/')
            }) else {
                continue;
            };
            if let Some(bytes) = read(resource) {
                resources.insert(href.to_string(), std::sync::Arc::new(bytes));
            }
        }
    }
    native_image_bytes_resolving(data, |href| resources.get(href.trim()).cloned())
}

fn normalize_resource_path(path: &str) -> String {
    let clean = decode_url_path(
        path.split('#')
            .next()
            .unwrap_or(path)
            .split('?')
            .next()
            .unwrap_or(path)
            .replace('\\', "/")
            .as_str(),
    );
    let absolute = clean.starts_with('/');
    let mut parts = Vec::new();
    for part in clean.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

/// Strip fragment/query from a src path, resolve it relative to chapter_dir,
/// and return `(clean, resolved)`.  Returns `None` when the cleaned path is empty.
pub(super) fn clean_and_resolve_src(src: &str, chapter_dir: &Path) -> Option<(String, String)> {
    let clean = src
        .split('#')
        .next()
        .unwrap_or(src)
        .split('?')
        .next()
        .unwrap_or(src)
        .trim();
    if clean.is_empty() {
        return None;
    }
    let resolved = if clean.starts_with('/') {
        clean.to_string()
    } else {
        chapter_dir.join(clean).to_string_lossy().to_string()
    };
    Some((
        normalize_resource_path(clean),
        normalize_resource_path(&resolved),
    ))
}

/// Look up image data by resolved path, falling back to filename match.
pub(super) fn lookup_image_by_path<'a>(
    clean: &str,
    resolved: &str,
    image_resources: &'a HashMap<String, Vec<u8>>,
) -> Option<&'a Vec<u8>> {
    let resolved = normalize_resource_path(resolved);
    if let Some(data) = image_resources.get(&resolved) {
        return Some(data);
    }
    let alt_resolved = if resolved.starts_with('/') {
        resolved.trim_start_matches('/').to_string()
    } else {
        format!("/{resolved}")
    };
    if let Some(data) = image_resources.get(&alt_resolved) {
        return Some(data);
    }
    let file_name = Path::new(&normalize_resource_path(clean))
        .file_name()
        .map(|n| n.to_string_lossy().to_string())?;
    image_resources.iter().find_map(|(k, v)| {
        let kn = Path::new(k)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if kn == file_name {
            Some(v)
        } else {
            None
        }
    })
}

/// Load only the images referenced by `<img>`/`<image>` tags in this chapter's HTML.
/// This avoids eagerly loading all images in the EPUB into memory.
pub(super) fn load_referenced_images(
    document: &Html,
    chapter_path: &str,
    image_resource_paths: &[String],
    image_resources: &mut HashMap<String, Vec<u8>>,
    mut read_resource_bytes: impl FnMut(&str) -> Option<Vec<u8>>,
) {
    let img_sel = Selector::parse("img, image").expect("valid selector");
    let chapter_dir = Path::new(chapter_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));

    for elem in document.select(&img_sel) {
        let src = image_src(elem.value());
        let (clean, resolved) = match clean_and_resolve_src(src, chapter_dir) {
            Some(pair) => pair,
            None => continue,
        };
        if src.trim().starts_with("data:") || image_resources.contains_key(&resolved) {
            continue;
        }
        // Try direct path match
        if let Some(epub_path) = image_resource_paths.iter().find(|candidate| {
            let candidate_norm = normalize_resource_path(candidate);
            candidate_norm == resolved
                || candidate_norm.trim_start_matches('/') == resolved.trim_start_matches('/')
        }) {
            if let Some(data) = read_resource_bytes(epub_path) {
                let data = prepare_epub_image(
                    data,
                    epub_path,
                    image_resource_paths,
                    &mut read_resource_bytes,
                );
                image_resources.insert(normalize_resource_path(epub_path), data.clone());
                image_resources.insert(resolved.clone(), data);
                continue;
            }
        }
        // Fallback: match by filename
        let file_name = Path::new(&clean)
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        if let Some(file_name) = file_name {
            if let Some(epub_path) = image_resource_paths.iter().find(|candidate| {
                Path::new(candidate.as_str())
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .as_deref()
                    == Some(&file_name)
            }) {
                if let Some(data) = read_resource_bytes(epub_path) {
                    let data = prepare_epub_image(
                        data,
                        epub_path,
                        image_resource_paths,
                        &mut read_resource_bytes,
                    );
                    image_resources.insert(normalize_resource_path(epub_path), data.clone());
                    image_resources.insert(resolved.clone(), data);
                }
            }
        }
    }
}

pub(super) fn resolve_image_data(
    raw_src: &str,
    chapter_path: &str,
    image_resources: &HashMap<String, Vec<u8>>,
) -> Option<std::sync::Arc<Vec<u8>>> {
    let src = raw_src.trim();
    if src.is_empty() {
        return None;
    }

    if src.starts_with("data:") {
        return decode_data_uri(src)
            .map(native_image_bytes)
            .map(std::sync::Arc::new);
    }

    let lowered = src.to_ascii_lowercase();
    if lowered.starts_with("http://") || lowered.starts_with("https://") {
        return None;
    }

    let chapter_dir = Path::new(chapter_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let (clean, resolved) = clean_and_resolve_src(src, chapter_dir)?;
    lookup_image_by_path(&clean, &resolved, image_resources)
        .cloned()
        .map(std::sync::Arc::new)
}

fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
    let comma = uri.find(',')?;
    if comma + 1 >= uri.len() {
        return None;
    }
    let (meta, payload) = uri.split_at(comma);
    let payload = &payload[1..];

    if meta.to_ascii_lowercase().contains(";base64") {
        base64::engine::general_purpose::STANDARD
            .decode(payload)
            .ok()
    } else {
        Some(decode_percent(payload))
    }
}

#[cfg(test)]
mod svg_tests {
    use super::*;
    #[test]
    fn svg_cannot_load_host_files_but_can_load_epub_manifest_resources() {
        let directory = tempfile::tempdir().unwrap();
        let mut source = resvg::tiny_skia::Pixmap::new(8, 8).unwrap();
        source.fill(resvg::tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let bytes = source.encode_png().unwrap();
        let file = directory.path().join("outside.png");
        std::fs::write(&file, &bytes).unwrap();
        let svg = format!("<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='8' height='8'><image xlink:href='{}' width='8' height='8'/></svg>",file.display());
        let png = native_image_bytes(svg.into_bytes());
        let decoded = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert!(decoded.pixels().iter().all(|pixel| pixel.alpha() == 0));
        let local = b"<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='8' height='8'><image xlink:href='inside.png' width='8' height='8'/></svg>";
        let manifest = vec!["/OPS/inside.png".into()];
        let mut read = |path: &str| (path == "/OPS/inside.png").then(|| bytes.clone());
        let png = prepare_epub_image(local.to_vec(), "/OPS/vector.svg", &manifest, &mut read);
        let decoded = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert!(decoded
            .pixels()
            .iter()
            .all(|pixel| pixel.red() == 255 && pixel.alpha() == 255));
    }
}
