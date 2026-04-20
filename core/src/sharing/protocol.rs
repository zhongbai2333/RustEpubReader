//! Defines the network protocol and message formats for P2P sharing.
use serde::{Deserialize, Serialize};

/// All messages are framed as: [4 bytes big-endian length][JSON payload]
/// For book transfers: after the BookData message, raw epub bytes follow.
const MAX_MESSAGE_SIZE: usize = 10_000_000; // 10 MB

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Message {
    /// Client introduces itself; includes pairing_uuid if previously paired
    Hello {
        device_id: String,
        device_name: String,
        #[serde(default)]
        pairing_uuid: Option<String>,
    },

    // ── Pairing (first-time) ──
    /// Server says pairing is needed; includes ECDH public key for encrypted PIN exchange
    PairNeeded { ecdh_public_key: String },
    /// Client responds with its ECDH public key
    PairKeyExchange { ecdh_public_key: String },
    /// Client sends target device's PIN + own RSA public key (encrypted with ECDH-derived key)
    PairRequest { pin: String, public_key_pem: String },
    /// Server accepts: returns generated UUID + own RSA public key
    PairAccepted {
        pairing_uuid: String,
        public_key_pem: String,
        device_name: String,
    },
    /// Server rejects (wrong PIN)
    PairRejected,

    // ── Re-authentication (already paired) ──
    /// Server sends a challenge nonce + pairing UUID for verification
    Challenge { nonce: String, pairing_uuid: String },
    /// Client responds with signature of nonce
    ChallengeResponse { signature: String },
    /// Server confirms authentication.
    ///
    /// `public_key_pem` is optional for backward compatibility:
    /// - New servers should provide current public key so clients can self-heal
    ///   stale paired-key records.
    /// - Old servers may omit it.
    Authenticated {
        #[serde(default)]
        public_key_pem: Option<String>,
    },

    // ── Session key exchange (after pair or auth) ──
    /// Client sends RSA-encrypted AES-256 session key (base64)
    SessionKey { encrypted_key: String },
    /// Server acknowledges session key
    SessionKeyAck,

    // ── Data commands (sent encrypted after session established) ──
    /// Request book list from peer
    ListBooks,
    /// Response with book list
    BookList { books: Vec<SharedBookInfo> },

    /// Send a book (followed by encrypted raw bytes)
    SendBook {
        title: String,
        hash: String,
        size: u64,
    },
    /// Acknowledge book receipt
    BookReceived { hash: String },

    /// Request a book by hash
    RequestBook { hash: String },
    /// Book data header (followed by encrypted raw bytes)
    BookData {
        title: String,
        hash: String,
        size: u64,
    },
    /// Book not found
    BookNotFound { hash: String },

    /// Sync reading progress
    SyncProgress { entries: Vec<ProgressEntry> },
    /// Response with merged progress
    ProgressResponse { entries: Vec<ProgressEntry> },

    /// Client signals it is done; server should close gracefully
    Goodbye,

    /// Error
    Error { message: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SharedBookInfo {
    pub title: String,
    pub hash: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProgressEntry {
    pub book_hash: String,
    pub title: String,
    pub chapter: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter_title: Option<String>,
    pub timestamp: u64,
}

/// Write a framed message to a writer
pub fn write_message(writer: &mut impl std::io::Write, msg: &Message) -> Result<(), String> {
    let json = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
    let len = (json.len() as u32).to_be_bytes();
    writer.write_all(&len).map_err(|e| e.to_string())?;
    writer.write_all(&json).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Read a framed message from a reader
pub fn read_message(reader: &mut impl std::io::Read) -> Result<Message, String> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err("Message too large".into());
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

/// Write raw bytes (for book transfer)
pub fn write_raw(writer: &mut impl std::io::Write, data: &[u8]) -> Result<(), String> {
    writer.write_all(data).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Maximum allowed size for raw book transfers (500 MB).
const MAX_RAW_SIZE: u64 = 500_000_000;

/// Read exactly `size` raw bytes.
/// Returns an error if `size` exceeds `MAX_RAW_SIZE` to prevent memory exhaustion attacks.
pub fn read_raw(reader: &mut impl std::io::Read, size: u64) -> Result<Vec<u8>, String> {
    if size > MAX_RAW_SIZE {
        return Err(format!(
            "Raw data size {} exceeds maximum allowed {} bytes",
            size, MAX_RAW_SIZE
        ));
    }
    let mut buf = vec![0u8; size as usize];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}
