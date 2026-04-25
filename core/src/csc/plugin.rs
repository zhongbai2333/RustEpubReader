//! Dynamic-library client for the CSC inference plugin.
//!
//! The actual ML inference (ONNX Runtime + tokenizer) is built and distributed
//! separately from the [RustEpubReader-Model](https://github.com/zhongbai2333/RustEpubReader-Model)
//! repository. This module loads the platform-specific dynamic library via
//! `libloading` and exposes a safe Rust wrapper around its C ABI.
//!
//! ABI version is checked at load time; mismatch returns an error.
#![cfg(feature = "csc")]

use crate::epub::{CorrectionInfo, CorrectionStatus};
use libloading::{Library, Symbol};
use serde::Deserialize;
use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};

/// Bumped when the C ABI surface changes incompatibly.
pub const PLUGIN_ABI_VERSION: u32 = 1;

/// Opaque handle owned by the plugin.
#[repr(C)]
pub struct CscEngineFfi {
    _opaque: [u8; 0],
    _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}

type FnAbiVersion = unsafe extern "C" fn() -> u32;
#[allow(dead_code)]
type FnEngineVersion = unsafe extern "C" fn() -> *const c_char;
type FnEngineNew = unsafe extern "C" fn() -> *mut CscEngineFfi;
type FnEngineFree = unsafe extern "C" fn(*mut CscEngineFfi);
type FnEngineLoad = unsafe extern "C" fn(
    *mut CscEngineFfi,
    *const c_char, // model path
    *const c_char, // vocab path
    *const c_char, // ep hint
) -> i32;
type FnEngineLastError = unsafe extern "C" fn(*const CscEngineFfi) -> *const c_char;
type FnEngineEp = unsafe extern "C" fn(*const CscEngineFfi) -> *const c_char;
type FnEngineCheck =
    unsafe extern "C" fn(*mut CscEngineFfi, *const c_char, f32) -> *mut c_char;
type FnStringFree = unsafe extern "C" fn(*mut c_char);

struct VTable {
    engine_new: FnEngineNew,
    engine_free: FnEngineFree,
    engine_load: FnEngineLoad,
    engine_last_error: FnEngineLastError,
    engine_ep: FnEngineEp,
    engine_check: FnEngineCheck,
    string_free: FnStringFree,
}

pub struct PluginHandle {
    // Library MUST outlive engine; declared first because Rust drops fields in
    // declaration order, and Drop unloads symbols when Library is freed.
    _lib: Library,
    engine: *mut CscEngineFfi,
    fns: VTable,
    ep: String,
}

// SAFETY: the underlying C engine is single-threaded; we only call into it from
// `&mut self` methods, and Library is Send/Sync per libloading docs.
unsafe impl Send for PluginHandle {}

#[derive(Debug)]
pub enum PluginError {
    NotFound(PathBuf),
    DlOpen(String),
    AbiMismatch { expected: u32, found: u32 },
    MissingSymbol(String),
    EngineCreate,
    EngineLoad(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::NotFound(p) => write!(f, "CSC plugin library not found: {}", p.display()),
            PluginError::DlOpen(e) => write!(f, "failed to load CSC plugin: {e}"),
            PluginError::AbiMismatch { expected, found } => {
                write!(
                    f,
                    "CSC plugin ABI v{found} is incompatible with this app (expects v{expected})"
                )
            }
            PluginError::MissingSymbol(s) => write!(f, "CSC plugin missing symbol `{s}`"),
            PluginError::EngineCreate => write!(f, "csc_engine_new returned null"),
            PluginError::EngineLoad(e) => write!(f, "csc_engine_load failed: {e}"),
        }
    }
}

impl std::error::Error for PluginError {}

/// Build the platform-specific subdirectory name used under
/// `<data_dir>/csc-plugin/v1/<platform>/`.
pub fn current_platform_dirname(prefer_directml: bool) -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        if prefer_directml {
            "windows-x86_64-directml"
        } else {
            "windows-x86_64-cpu"
        }
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "windows-aarch64-cpu"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x86_64"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "linux-aarch64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x86_64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-aarch64"
    } else {
        "unknown"
    }
}

/// File name of the dynamic library shipped by the plugin (without
/// onnxruntime / DirectML which sit alongside).
pub fn plugin_library_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "csc_plugin.dll"
    } else if cfg!(target_os = "macos") {
        "libcsc_plugin.dylib"
    } else {
        "libcsc_plugin.so"
    }
}

impl PluginHandle {
    pub fn open(
        plugin_dir: &Path,
        model_path: &Path,
        vocab_path: &Path,
        ep_hint: &str,
    ) -> Result<Self, PluginError> {
        let lib_path = plugin_dir.join(plugin_library_filename());
        if !lib_path.exists() {
            return Err(PluginError::NotFound(lib_path));
        }

        // Make co-located onnxruntime / DirectML libs discoverable.
        prepare_search_path(plugin_dir);

        let lib = unsafe { Library::new(&lib_path) }
            .map_err(|e| PluginError::DlOpen(format!("{}: {}", lib_path.display(), e)))?;

        // ABI check first — fail fast on incompatible plugins.
        let abi: Symbol<FnAbiVersion> = unsafe { lib.get(b"csc_plugin_abi_version\0") }
            .map_err(|_| PluginError::MissingSymbol("csc_plugin_abi_version".into()))?;
        let found = unsafe { abi() };
        if found != PLUGIN_ABI_VERSION {
            return Err(PluginError::AbiMismatch {
                expected: PLUGIN_ABI_VERSION,
                found,
            });
        }

        macro_rules! sym {
            ($name:literal) => {{
                let s: Symbol<_> = unsafe { lib.get(concat!($name, "\0").as_bytes()) }
                    .map_err(|_| PluginError::MissingSymbol($name.into()))?;
                *s
            }};
        }

        let fns = VTable {
            engine_new: sym!("csc_engine_new"),
            engine_free: sym!("csc_engine_free"),
            engine_load: sym!("csc_engine_load"),
            engine_last_error: sym!("csc_engine_last_error"),
            engine_ep: sym!("csc_engine_execution_provider"),
            engine_check: sym!("csc_engine_check"),
            string_free: sym!("csc_string_free"),
        };

        // Construct engine.
        let engine = unsafe { (fns.engine_new)() };
        if engine.is_null() {
            return Err(PluginError::EngineCreate);
        }

        // Load model + vocab.
        let model_c = path_to_cstring(model_path);
        let vocab_c = path_to_cstring(vocab_path);
        let ep_c = CString::new(ep_hint.replace('\0', "")).unwrap_or_default();
        let rc = unsafe {
            (fns.engine_load)(engine, model_c.as_ptr(), vocab_c.as_ptr(), ep_c.as_ptr())
        };
        if rc != 0 {
            let msg = read_last_error(&fns, engine).unwrap_or_else(|| format!("rc={rc}"));
            unsafe { (fns.engine_free)(engine) };
            return Err(PluginError::EngineLoad(msg));
        }

        let ep = unsafe { (fns.engine_ep)(engine) };
        let ep = c_str_to_string(ep).unwrap_or_else(|| "unknown".to_string());

        Ok(Self {
            _lib: lib,
            engine,
            fns,
            ep,
        })
    }

    pub fn execution_provider(&self) -> &str {
        &self.ep
    }

    pub fn check(&mut self, text: &str, threshold: f32) -> Vec<CorrectionInfo> {
        let cleaned = if text.contains('\0') {
            text.replace('\0', "")
        } else {
            text.to_string()
        };
        let Ok(text_c) = CString::new(cleaned) else {
            return Vec::new();
        };
        let resp_ptr =
            unsafe { (self.fns.engine_check)(self.engine, text_c.as_ptr(), threshold) };
        if resp_ptr.is_null() {
            return Vec::new();
        }
        let resp_json = unsafe { CStr::from_ptr(resp_ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.fns.string_free)(resp_ptr) };

        let entries: Vec<WireCorrection> =
            serde_json::from_str(&resp_json).unwrap_or_default();
        entries
            .into_iter()
            .map(|e| CorrectionInfo {
                original: e.original,
                corrected: e.corrected,
                confidence: e.confidence,
                char_offset: e.char_offset,
                status: CorrectionStatus::Pending,
            })
            .collect()
    }
}

impl Drop for PluginHandle {
    fn drop(&mut self) {
        if !self.engine.is_null() {
            unsafe { (self.fns.engine_free)(self.engine) };
            self.engine = std::ptr::null_mut();
        }
    }
}

#[derive(Deserialize)]
struct WireCorrection {
    original: String,
    corrected: String,
    confidence: f32,
    #[serde(default)]
    char_offset: usize,
}

fn path_to_cstring(p: &Path) -> CString {
    let s = p.to_string_lossy().replace('\0', "");
    CString::new(s).unwrap_or_default()
}

fn c_str_to_string(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

fn read_last_error(fns: &VTable, engine: *mut CscEngineFfi) -> Option<String> {
    let p = unsafe { (fns.engine_last_error)(engine) };
    c_str_to_string(p)
}

/// Make co-located dependent libraries (onnxruntime, DirectML, …) discoverable
/// to the dynamic loader. Called before `dlopen`/`LoadLibrary`.
fn prepare_search_path(plugin_dir: &Path) {
    // Tell `ort`'s `load-dynamic` mode where to find libonnxruntime.
    let ort_lib = if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };
    let full = plugin_dir.join(ort_lib);
    if full.exists() {
        // Process-global env mutation; safe because plugin loading is
        // single-shot during init before any other thread starts using `ort`.
        std::env::set_var("ORT_DYLIB_PATH", &full);
    }

    #[cfg(target_os = "windows")]
    {
        // Prepend plugin_dir to PATH so DirectML.dll / onnxruntime.dll resolve.
        let prev = std::env::var("PATH").unwrap_or_default();
        let new_path = if prev.is_empty() {
            plugin_dir.display().to_string()
        } else {
            format!("{};{}", plugin_dir.display(), prev)
        };
        std::env::set_var("PATH", new_path);
    }
}
