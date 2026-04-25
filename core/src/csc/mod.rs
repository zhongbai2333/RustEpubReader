//! Chinese Spelling Correction (CSC) public API.
//!
//! The actual ML inference engine ships as a separately-distributed dynamic
//! library (see the `RustEpubReader-Model` repository). This module exposes a
//! lightweight client that loads the plugin via `libloading` only when CSC is
//! enabled by the user.
pub mod model;
#[cfg(feature = "csc")]
pub mod plugin;

use crate::epub::CorrectionInfo;

/// Correction mode for the reading experience.
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum CorrectionMode {
    /// No correction — show original text as-is.
    #[default]
    None,
    /// Read-only annotations — detect and mark errors but don't allow editing.
    ReadOnly,
    /// Read-write — detect, mark, and allow the user to accept/reject/ignore.
    ReadWrite,
}

/// Confidence threshold for spelling correction.
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum CscThreshold {
    /// ≥ 0.97 — only high-confidence errors.
    Conservative,
    /// ≥ 0.90 — most common homophones.
    #[default]
    Standard,
    /// ≥ 0.80 — aggressive, may have false positives.
    Aggressive,
}

impl CscThreshold {
    pub fn value(&self) -> f32 {
        match self {
            Self::Conservative => 0.97,
            Self::Standard => 0.90,
            Self::Aggressive => 0.80,
        }
    }
}

/// Model + plugin loading status.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum ModelStatus {
    #[default]
    NotDownloaded,
    Downloading {
        progress: f32,
    },
    Downloaded,
    Loading,
    Ready,
    Error(String),
}

/// Chinese Spelling Correction engine.
///
/// On desktop this is a thin wrapper around a dynamically-loaded plugin
/// (`csc_plugin.{dll,dylib,so}`) that contains the ONNX Runtime + tokenizer.
/// When the plugin or model is missing, all calls are no-ops.
pub struct CscEngine {
    pub mode: CorrectionMode,
    pub threshold: CscThreshold,
    #[cfg(feature = "csc")]
    plugin: Option<plugin::PluginHandle>,
}

impl CscEngine {
    pub fn new(mode: CorrectionMode, threshold: CscThreshold) -> Self {
        Self {
            mode,
            threshold,
            #[cfg(feature = "csc")]
            plugin: None,
        }
    }

    /// `true` when the dynamic plugin is loaded and ready for inference.
    pub fn is_ready(&self) -> bool {
        #[cfg(feature = "csc")]
        {
            self.plugin.is_some()
        }
        #[cfg(not(feature = "csc"))]
        {
            false
        }
    }

    /// Human-readable name of the active execution provider (e.g. "CPU",
    /// "DirectML").
    pub fn execution_provider(&self) -> String {
        #[cfg(feature = "csc")]
        {
            self.plugin
                .as_ref()
                .map(|p| p.execution_provider().to_string())
                .unwrap_or_else(|| "none".to_string())
        }
        #[cfg(not(feature = "csc"))]
        {
            "none".to_string()
        }
    }

    /// Locate the platform-specific plugin under `<data_dir>/csc-plugin/v1/...`,
    /// load it, then load model + vocab from `<data_dir>/models/`.
    #[cfg(feature = "csc")]
    pub fn load(&mut self, data_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.load_with(data_dir, true)
    }

    /// Like [`load`] but lets the caller choose whether to prefer GPU
    /// acceleration (DirectML on Windows). Falls back to CPU automatically.
    #[cfg(feature = "csc")]
    pub fn load_with(
        &mut self,
        data_dir: &str,
        prefer_gpu: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let model_path = model::model_path(data_dir);
        let vocab_path = model::vocab_path(data_dir);

        if !model_path.exists() {
            return Err("model file not found; please download CSC model".into());
        }
        if !vocab_path.exists() {
            return Err("vocab file not found; please download CSC model".into());
        }

        let candidates = model::plugin_candidate_dirs(data_dir, prefer_gpu);
        let ep_hints: &[&str] = if prefer_gpu {
            &["directml", "cpu"]
        } else {
            &["cpu"]
        };

        let mut last_err: Option<plugin::PluginError> = None;
        for plugin_dir in &candidates {
            for &ep in ep_hints {
                match plugin::PluginHandle::open(plugin_dir, &model_path, &vocab_path, ep) {
                    Ok(h) => {
                        self.plugin = Some(h);
                        return Ok(());
                    }
                    Err(e) => last_err = Some(e),
                }
            }
        }
        Err(last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "no plugin candidate available".to_string())
            .into())
    }

    /// Stub used when `csc` feature is disabled.
    #[cfg(not(feature = "csc"))]
    pub fn load(&mut self, _data_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
        Err("CSC support is disabled at compile time".into())
    }

    /// Run correction on a text block. Returns an empty list when the engine
    /// is not loaded or `mode == None`.
    pub fn check(&mut self, text: &str) -> Vec<CorrectionInfo> {
        if self.mode == CorrectionMode::None {
            return Vec::new();
        }
        #[cfg(feature = "csc")]
        {
            let threshold = self.threshold.value();
            match self.plugin.as_mut() {
                Some(p) => p.check(text, threshold),
                None => Vec::new(),
            }
        }
        #[cfg(not(feature = "csc"))]
        {
            let _ = text;
            Vec::new()
        }
    }
}
