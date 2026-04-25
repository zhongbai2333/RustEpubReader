//! Paths, URLs and download lists for CSC model files and the dynamic plugin.
//!
//! The model files (~99 MB) and the platform-specific plugin (~10–25 MB) are
//! both fetched from `https://dl.zhongbai233.com` on first use.
use std::path::PathBuf;

const MODEL_FILENAME: &str = "csc-macbert-int8.onnx";
const VOCAB_FILENAME: &str = "csc-vocab.txt";
const MANIFEST_FILENAME: &str = "csc-manifest.json";

const MODEL_BASE_URL: &str = "https://dl.zhongbai233.com/models";
const PLUGIN_BASE_URL: &str = "https://dl.zhongbai233.com/plugins/v1";

/// SHA256 hash of the model file. Empty = skip verification (model not yet hosted).
const MODEL_SHA256: &str = "";

// ── Model paths ────────────────────────────────────────────────────────────

pub fn model_dir(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("models")
}

pub fn model_path(data_dir: &str) -> PathBuf {
    model_dir(data_dir).join(MODEL_FILENAME)
}

pub fn vocab_path(data_dir: &str) -> PathBuf {
    model_dir(data_dir).join(VOCAB_FILENAME)
}

pub fn manifest_path(data_dir: &str) -> PathBuf {
    model_dir(data_dir).join(MANIFEST_FILENAME)
}

pub fn is_model_available(data_dir: &str) -> bool {
    model_path(data_dir).exists() && vocab_path(data_dir).exists()
}

// ── Plugin paths ───────────────────────────────────────────────────────────

/// `<data_dir>/csc-plugin/v1/`
pub fn plugin_root_dir(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("csc-plugin").join("v1")
}

/// `<data_dir>/csc-plugin/v1/<platform>/`
pub fn plugin_dir(data_dir: &str, platform: &str) -> PathBuf {
    plugin_root_dir(data_dir).join(platform)
}

/// Build a runtime-detected list of plugin directories the loader should try,
/// in priority order. With `prefer_gpu`, GPU-accelerated variants come first.
#[cfg(feature = "csc")]
pub fn plugin_candidate_dirs(data_dir: &str, prefer_gpu: bool) -> Vec<PathBuf> {
    let primary = super::plugin::current_platform_dirname(prefer_gpu);
    let fallback = super::plugin::current_platform_dirname(false);
    let mut out = vec![plugin_dir(data_dir, primary)];
    if primary != fallback {
        out.push(plugin_dir(data_dir, fallback));
    }
    out
}

// ── Download URLs ──────────────────────────────────────────────────────────

pub fn model_url() -> String {
    format!("{MODEL_BASE_URL}/{MODEL_FILENAME}")
}

pub fn vocab_url() -> String {
    format!("{MODEL_BASE_URL}/{VOCAB_FILENAME}")
}

pub fn manifest_url() -> String {
    format!("{MODEL_BASE_URL}/{MANIFEST_FILENAME}")
}

/// Files needed for the model itself (ONNX + vocab + manifest).
pub fn required_model_files() -> Vec<(String, &'static str)> {
    vec![
        (model_url(), MODEL_FILENAME),
        (vocab_url(), VOCAB_FILENAME),
        (manifest_url(), MANIFEST_FILENAME),
    ]
}

/// Plugin file list for a given platform directory name. The first element of
/// the tuple is the URL, the second is the destination filename relative to
/// `<data_dir>/csc-plugin/v1/<platform>/`.
pub fn required_plugin_files_for(platform: &str) -> Vec<(String, String)> {
    let lib_filename = if platform.starts_with("windows-") {
        "csc_plugin.dll"
    } else if platform.starts_with("macos-") {
        "libcsc_plugin.dylib"
    } else if platform.starts_with("linux-") {
        "libcsc_plugin.so"
    } else {
        // android-* bundles only the ORT runtime; Kotlin engine lives in-APK.
        ""
    };

    let onnx_runtime: &[&str] = if platform.starts_with("windows-") {
        &["onnxruntime.dll"]
    } else if platform.starts_with("macos-") {
        &["libonnxruntime.dylib"]
    } else if platform.starts_with("linux-") {
        &["libonnxruntime.so"]
    } else if platform.starts_with("android-") {
        &["libonnxruntime.so", "libonnxruntime4j_jni.so"]
    } else {
        &[]
    };

    let directml_extra: &[&str] = if platform.ends_with("-directml") {
        &["DirectML.dll"]
    } else {
        &[]
    };

    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |name: &str| {
        if name.is_empty() {
            return;
        }
        out.push((
            format!("{PLUGIN_BASE_URL}/{platform}/{name}"),
            name.to_string(),
        ));
    };
    push(lib_filename);
    for f in onnx_runtime {
        push(f);
    }
    for f in directml_extra {
        push(f);
    }
    out
}

/// Convenience wrapper for the running platform.
#[cfg(feature = "csc")]
pub fn required_plugin_files(prefer_gpu: bool) -> Vec<(String, String)> {
    let platform = super::plugin::current_platform_dirname(prefer_gpu);
    required_plugin_files_for(platform)
}

/// Aggregate files for a full CSC download (model + matching plugin variant).
/// Each entry is `(url, dest_path_under_data_dir)`.
#[cfg(feature = "csc")]
pub fn required_files(data_dir: &str, prefer_gpu: bool) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = required_model_files()
        .into_iter()
        .map(|(url, name)| (url, model_dir(data_dir).join(name)))
        .collect();
    let platform = super::plugin::current_platform_dirname(prefer_gpu);
    let plugin_dir = plugin_dir(data_dir, platform);
    for (url, name) in required_plugin_files(prefer_gpu) {
        out.push((url, plugin_dir.join(name)));
    }
    out
}

// ── Integrity ──────────────────────────────────────────────────────────────

/// Verify model integrity via SHA256 hash.
/// Returns true if hash is not configured (empty) or matches.
#[allow(clippy::const_is_empty)]
pub fn verify_model(data_dir: &str) -> bool {
    if MODEL_SHA256.is_empty() {
        return true;
    }
    let path = model_path(data_dir);
    let Ok(data) = std::fs::read(path) else {
        return false;
    };
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(&data));
    hash == MODEL_SHA256
}
