//! Tokenizer integration for the Chinese Spelling Correction model.
use std::path::Path;

/// Maximum sequence length for MacBERT input.
pub const MAX_SEQ_LEN: usize = 128;

/// Tokenized output ready for ONNX inference.
pub struct TokenizedInput {
    pub input_ids: Vec<i64>,
    pub attention_mask: Vec<i64>,
    pub token_type_ids: Vec<i64>,
    /// Mapping from token index → (char_start, char_end) in original text.
    /// Only valid for non-special tokens (excludes [CLS], [SEP], [PAD]).
    pub offset_mapping: Vec<Option<(usize, usize)>>,
}

/// WordPiece tokenizer backed by HuggingFace tokenizers crate.
pub struct CscTokenizer {
    inner: tokenizers::Tokenizer,
}

impl CscTokenizer {
    /// Load tokenizer from a vocab.txt file (BERT WordPiece format).
    pub fn from_vocab(vocab_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        use tokenizers::models::wordpiece::WordPiece;

        let wp = WordPiece::from_file(&vocab_path.to_string_lossy())
            .unk_token("[UNK]".into())
            .build()
            .map_err(|e| format!("Failed to load WordPiece vocab: {}", e))?;

        let mut tokenizer = tokenizers::Tokenizer::new(wp);

        // BERT-style pre-tokenization + normalization
        tokenizer.with_normalizer(Some(tokenizers::normalizers::BertNormalizer::default()));

        // For Chinese CSC, each character is its own token — add Chinese char splitter
        tokenizer.with_pre_tokenizer(Some(tokenizers::pre_tokenizers::sequence::Sequence::new(
            vec![tokenizers::pre_tokenizers::bert::BertPreTokenizer.into()],
        )));

        Ok(Self { inner: tokenizer })
    }

    /// Tokenize a single sentence. Adds [CLS] and [SEP], pads to MAX_SEQ_LEN.
    pub fn encode(&self, text: &str) -> Result<TokenizedInput, Box<dyn std::error::Error>> {
        let encoding = self
            .inner
            .encode(text, true)
            .map_err(|e| format!("Tokenization error: {}", e))?;

        let mut input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let mut attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| m as i64)
            .collect();
        let mut token_type_ids: Vec<i64> =
            encoding.get_type_ids().iter().map(|&t| t as i64).collect();

        // Build offset mapping
        let raw_offsets = encoding.get_offsets();
        let special_mask = encoding.get_special_tokens_mask();

        // HuggingFace tokenizers return byte offsets — convert to char offsets
        // Build byte→char mapping for the input text
        let byte_to_char: Vec<usize> = {
            let char_count = text.chars().count();
            let mut map = vec![char_count; text.len() + 1];
            for (ci, (bi, _)) in text.char_indices().enumerate() {
                map[bi] = ci;
            }
            map[text.len()] = char_count;
            map
        };

        let mut offset_mapping: Vec<Option<(usize, usize)>> = raw_offsets
            .iter()
            .zip(special_mask.iter())
            .map(|(&(s, e), &is_special)| {
                if is_special != 0 {
                    None
                } else {
                    let cs = byte_to_char.get(s).copied().unwrap_or(0);
                    let ce = byte_to_char.get(e).copied().unwrap_or(text.chars().count());
                    Some((cs, ce))
                }
            })
            .collect();

        // Truncate to MAX_SEQ_LEN
        if input_ids.len() > MAX_SEQ_LEN {
            input_ids.truncate(MAX_SEQ_LEN);
            attention_mask.truncate(MAX_SEQ_LEN);
            token_type_ids.truncate(MAX_SEQ_LEN);
            offset_mapping.truncate(MAX_SEQ_LEN);
            // Ensure last token is [SEP]
            if let Some(sep_id) = self.inner.token_to_id("[SEP]") {
                if let Some(last) = input_ids.last_mut() {
                    *last = sep_id as i64;
                }
                if let Some(last) = offset_mapping.last_mut() {
                    *last = None;
                }
            }
        }

        // Pad to MAX_SEQ_LEN
        let pad_id = self.inner.token_to_id("[PAD]").unwrap_or(0) as i64;
        while input_ids.len() < MAX_SEQ_LEN {
            input_ids.push(pad_id);
            attention_mask.push(0);
            token_type_ids.push(0);
            offset_mapping.push(None);
        }

        Ok(TokenizedInput {
            input_ids,
            attention_mask,
            token_type_ids,
            offset_mapping,
        })
    }

    /// Decode a single token ID back to its string representation.
    pub fn id_to_token(&self, id: u32) -> Option<String> {
        self.inner.id_to_token(id)
    }

    /// Get the token ID for [UNK].
    pub fn unk_id(&self) -> Option<u32> {
        self.inner.token_to_id("[UNK]")
    }

    /// Get the vocabulary size.
    pub fn vocab_size(&self) -> usize {
        self.inner.get_vocab_size(true)
    }
}
