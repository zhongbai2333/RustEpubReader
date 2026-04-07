pub mod model;
#[cfg(feature = "csc")]
pub mod tokenizer;

use crate::epub::CorrectionInfo;
#[cfg(feature = "csc")]
use crate::epub::CorrectionStatus;

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
    /// ≥ 0.95 — only high-confidence errors.
    Conservative,
    /// ≥ 0.85 — most common homophones.
    #[default]
    Standard,
    /// ≥ 0.70 — aggressive, may have false positives.
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

/// Model loading status.
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
/// Uses ONNX MacBERT-CSC model for character-level error detection.
pub struct CscEngine {
    pub mode: CorrectionMode,
    pub threshold: CscThreshold,
    #[cfg(feature = "csc")]
    session: Option<ort::session::Session>,
    #[cfg(feature = "csc")]
    tokenizer: Option<tokenizer::CscTokenizer>,
}

impl CscEngine {
    pub fn new(mode: CorrectionMode, threshold: CscThreshold) -> Self {
        Self {
            mode,
            threshold,
            #[cfg(feature = "csc")]
            session: None,
            #[cfg(feature = "csc")]
            tokenizer: None,
        }
    }

    /// Check if the engine is loaded and ready for inference.
    pub fn is_ready(&self) -> bool {
        #[cfg(feature = "csc")]
        {
            self.session.is_some() && self.tokenizer.is_some()
        }
        #[cfg(not(feature = "csc"))]
        false
    }

    /// Load ONNX model and tokenizer from data directory.
    #[cfg(feature = "csc")]
    pub fn load(&mut self, data_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
        let model_path = model::model_path(data_dir);
        let vocab_path = model::vocab_path(data_dir);

        if !model_path.exists() {
            return Err("Model file not found. Please download the model first.".into());
        }
        if !vocab_path.exists() {
            return Err("Vocabulary file not found. Please download the model first.".into());
        }

        // Load ONNX session
        let session = ort::session::Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(&model_path)?;

        // Load tokenizer
        let tok = tokenizer::CscTokenizer::from_vocab(&vocab_path)?;

        self.session = Some(session);
        self.tokenizer = Some(tok);
        Ok(())
    }

    /// Check a text string for potential corrections.
    ///
    /// Returns a list of corrections found.
    pub fn check(&mut self, text: &str) -> Vec<CorrectionInfo> {
        if self.mode == CorrectionMode::None {
            return Vec::new();
        }
        #[cfg(feature = "csc")]
        {
            self.check_impl(text)
        }
        #[cfg(not(feature = "csc"))]
        {
            let _ = text;
            Vec::new()
        }
    }

    /// Internal inference implementation.
    #[cfg(feature = "csc")]
    fn check_impl(&mut self, text: &str) -> Vec<CorrectionInfo> {
        let threshold = self.threshold.value();
        let mut all_corrections = Vec::new();

        // Split text into sentences, tracking each sentence's char offset in the full text
        let mut cumulative_offset: usize = 0;
        let mut current = String::new();
        let mut seg_char_start: usize = 0;

        for ch in text.chars() {
            current.push(ch);
            if matches!(ch, '。' | '！' | '？' | '；' | '\n') {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    let leading_ws = current.chars().take_while(|c| c.is_whitespace()).count();
                    if let Ok(corrections) = self.infer_sentence(trimmed, threshold) {
                        for mut c in corrections {
                            c.char_offset += seg_char_start + leading_ws;
                            all_corrections.push(c);
                        }
                    }
                }
                cumulative_offset += current.chars().count();
                seg_char_start = cumulative_offset;
                current.clear();
            }
        }
        // Last segment
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            let leading_ws = current.chars().take_while(|c| c.is_whitespace()).count();
            if let Ok(corrections) = self.infer_sentence(trimmed, threshold) {
                for mut c in corrections {
                    c.char_offset += seg_char_start + leading_ws;
                    all_corrections.push(c);
                }
            }
        }

        all_corrections
    }

    /// Run inference on a single sentence.
    #[cfg(feature = "csc")]
    fn infer_sentence(
        &mut self,
        sentence: &str,
        threshold: f32,
    ) -> Result<Vec<CorrectionInfo>, Box<dyn std::error::Error>> {
        let tok = self.tokenizer.as_ref().ok_or("tokenizer not loaded")?;
        let unk_id = tok.unk_id().unwrap_or(100) as i64;
        let encoded = tok.encode(sentence)?;
        let seq_len = tokenizer::MAX_SEQ_LEN;

        // Build input tensors
        let input_ids_val =
            ort::value::Tensor::from_array(([1usize, seq_len], encoded.input_ids.clone()))?;
        let attention_mask_val =
            ort::value::Tensor::from_array(([1usize, seq_len], encoded.attention_mask.clone()))?;
        let token_type_ids_val =
            ort::value::Tensor::from_array(([1usize, seq_len], encoded.token_type_ids.clone()))?;

        let session = self.session.as_mut().ok_or("session not loaded")?;
        let outputs = session.run(ort::inputs![
            "input_ids" => input_ids_val,
            "attention_mask" => attention_mask_val,
            "token_type_ids" => token_type_ids_val,
        ])?;

        // Output shape: [1, seq_len, vocab_size]
        let (shape, logits_flat) = outputs[0].try_extract_tensor::<f32>()?;
        let vocab_size = shape[2] as usize;

        let chars: Vec<char> = sentence.chars().collect();
        let mut corrections = Vec::new();

        // Minimum probability margin: P(predicted) - P(original) must exceed this
        // to count as a real correction (filters out uncertain predictions)
        let min_margin: f32 = 0.20;

        for (pos, &input_id) in encoded.input_ids.iter().enumerate() {
            // Skip special tokens ([CLS], [SEP], [PAD])
            if encoded.offset_mapping[pos].is_none() || encoded.attention_mask[pos] == 0 {
                continue;
            }

            // Skip [UNK] tokens — OOV chars always get "corrected" by the model
            if input_id == unk_id {
                continue;
            }

            let base = pos * vocab_size;
            let logit_slice = &logits_flat[base..base + vocab_size];

            // Compute argmax predicted token
            let predicted_id = logit_slice
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            if predicted_id as i64 == input_id {
                continue;
            }

            // Compute softmax for both predicted and original tokens
            let max_logit = logit_slice
                .iter()
                .cloned()
                .fold(f32::NEG_INFINITY, f32::max);
            let exp_sum: f32 = logit_slice.iter().map(|&x| (x - max_logit).exp()).sum();
            let p_predicted = (logit_slice[predicted_id] - max_logit).exp() / exp_sum;
            let p_original = (logit_slice[input_id as usize] - max_logit).exp() / exp_sum;

            // Filter 1: predicted confidence must meet threshold
            if p_predicted < threshold {
                continue;
            }

            // Filter 2: margin — if original is also highly probable, skip
            if p_predicted - p_original < min_margin {
                continue;
            }

            // Map token position back to original character
            if let Some((char_start, _char_end)) = encoded.offset_mapping[pos] {
                let original_char: String = if char_start < chars.len() {
                    chars[char_start].to_string()
                } else {
                    tok.id_to_token(input_id as u32).unwrap_or_default()
                };

                // Filter 3: skip non-Chinese characters (punctuation, digits, Latin)
                if let Some(ch) = original_char.chars().next() {
                    if !is_cjk_char(ch) {
                        continue;
                    }
                }

                // Filter 4: skip protected function words / particles
                // These words are almost never misspelled but models frequently mispredict them
                if is_protected_char(&original_char) {
                    continue;
                }

                let corrected_char = tok.id_to_token(predicted_id as u32).unwrap_or_default();

                // Skip if predicted token is [UNK]
                if corrected_char == "[UNK]" {
                    continue;
                }

                // Skip if decoded tokens look the same (tokenization artifacts)
                if original_char == corrected_char {
                    continue;
                }
                // Skip sub-word tokens (##xxx)
                if corrected_char.starts_with("##") {
                    continue;
                }
                // Skip known high-false-positive confusion pairs
                if is_confused_pair(&original_char, &corrected_char) {
                    continue;
                }

                // Filter 4: reject if correction creates a duplicate with neighbor
                // e.g. "脖颈" → "颈颈" — the corrected char matches an adjacent char
                if let Some(corr_ch) = corrected_char.chars().next() {
                    if char_start > 0 && chars[char_start - 1] == corr_ch {
                        continue;
                    }
                    if char_start + 1 < chars.len() && chars[char_start + 1] == corr_ch {
                        continue;
                    }
                }

                corrections.push(CorrectionInfo {
                    original: original_char,
                    corrected: corrected_char,
                    confidence: p_predicted,
                    char_offset: char_start,
                    status: CorrectionStatus::Pending,
                });
            }
        }

        Ok(corrections)
    }
}

/// Known homophone pairs that MacBERT cannot reliably distinguish.
/// If both original and corrected belong to the same group, skip the correction.
#[cfg(feature = "csc")]
fn is_confused_pair(original: &str, corrected: &str) -> bool {
    const GROUPS: &[&[&str]] = &[
        &["他", "她", "它", "牠", "祂"],
        &["的", "得", "地"],
        &["做", "作"],
        &["哪", "那"],
        &["在", "再"],
    ];
    let a = original.trim();
    let b = corrected.trim();
    for group in GROUPS {
        if group.contains(&a) && group.contains(&b) {
            return true;
        }
    }
    false
}

/// Check if a character is CJK (Chinese/Japanese/Korean ideograph).
/// Only CJK characters should be considered for spelling correction.
#[cfg(feature = "csc")]
fn is_cjk_char(ch: char) -> bool {
    let cp = ch as u32;
    matches!(cp,
        0x4E00..=0x9FFF       // CJK Unified Ideographs
        | 0x3400..=0x4DBF     // CJK Extension A
        | 0x20000..=0x2A6DF   // CJK Extension B
        | 0x2A700..=0x2B73F   // CJK Extension C
        | 0x2B740..=0x2B81F   // CJK Extension D
        | 0xF900..=0xFAFF     // CJK Compatibility Ideographs
        | 0x2F800..=0x2FA1F   // CJK Compat Ideographs Supplement
    )
}

/// Characters that are grammatical particles / function words and almost never misspelled.
/// MacBERT frequently mispredicts these because they carry little semantic weight.
#[cfg(feature = "csc")]
fn is_protected_char(s: &str) -> bool {
    const PROTECTED: &[&str] = &[
        // Modal particles (语气助词)
        "吗", "吧", "呢", "啊", "呀", "哇", "哦", "嗯", "喔", "噢", "啦", "嘛", "咯", "喽", "嘞",
        "罢", "咧",
        // Structural particles (结构助词) — already in confused_pair but protect originals too
        "的", "得", "地", // Aspect particles
        "了", "过", "着", // Common function words easily confused by BERT
        "么", "个", "们", "这", "那", "就", "都", "也", "又", "才", "把", "被", "让", "给", "向",
        "往", "从", "到", "为", "而", "且", "或", "与", "及", // Pronouns
        "我", "你", "他", "她", "它", "谁", "啥", // Demonstratives & measure words
        "这", "那", "哪", "几", "多", "些",
    ];
    PROTECTED.contains(&s.trim())
}
