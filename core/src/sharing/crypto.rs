//! RSA keypair management, signing/verification, and AES-GCM session encryption.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::rngs::OsRng;
use rsa::pkcs8::{
    DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding,
};
use rsa::sha2::{Digest, Sha256};
use rsa::{Oaep, Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};

const RSA_BITS: usize = 2048;

/// Generate a new RSA-2048 keypair. Returns (private_key_pem, public_key_pem).
pub fn generate_rsa_keypair() -> Result<(String, String), String> {
    let private_key =
        RsaPrivateKey::new(&mut OsRng, RSA_BITS).map_err(|e| format!("RSA keygen: {e}"))?;
    let public_key = RsaPublicKey::from(&private_key);
    let priv_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| format!("PEM encode: {e}"))?
        .to_string();
    let pub_pem = public_key
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| format!("PEM encode: {e}"))?;
    Ok((priv_pem, pub_pem))
}

/// Sign data with an RSA private key (PKCS1v15 + SHA-256).
pub fn rsa_sign(private_key_pem: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let key =
        RsaPrivateKey::from_pkcs8_pem(private_key_pem).map_err(|e| format!("Bad key: {e}"))?;
    let digest = Sha256::digest(data);
    key.sign(Pkcs1v15Sign::new::<Sha256>(), &digest)
        .map_err(|e| format!("Sign: {e}"))
}

/// Verify an RSA signature (PKCS1v15 + SHA-256).
pub fn rsa_verify(public_key_pem: &str, data: &[u8], signature: &[u8]) -> bool {
    let Ok(key) = RsaPublicKey::from_public_key_pem(public_key_pem) else {
        return false;
    };
    let digest = Sha256::digest(data);
    key.verify(Pkcs1v15Sign::new::<Sha256>(), &digest, signature)
        .is_ok()
}

/// Encrypt data with an RSA public key (OAEP + SHA-256). For key exchange only.
pub fn rsa_encrypt(public_key_pem: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let key =
        RsaPublicKey::from_public_key_pem(public_key_pem).map_err(|e| format!("Bad key: {e}"))?;
    key.encrypt(&mut OsRng, Oaep::new::<Sha256>(), plaintext)
        .map_err(|e| format!("Encrypt: {e}"))
}

/// Decrypt data with an RSA private key (OAEP + SHA-256). For key exchange only.
pub fn rsa_decrypt(private_key_pem: &str, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    let key =
        RsaPrivateKey::from_pkcs8_pem(private_key_pem).map_err(|e| format!("Bad key: {e}"))?;
    key.decrypt(Oaep::new::<Sha256>(), ciphertext)
        .map_err(|e| format!("Decrypt: {e}"))
}

/// Generate a random AES-256 key (32 bytes).
pub fn generate_aes_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    use rand::RngCore;
    OsRng.fill_bytes(&mut key);
    key
}

/// Generate a random nonce (32 bytes) for challenge-response.
pub fn generate_nonce() -> Vec<u8> {
    let mut buf = [0u8; 32];
    use rand::RngCore;
    OsRng.fill_bytes(&mut buf);
    buf.to_vec()
}

/// AES-256-GCM encrypt. Nonce derived from counter (12 bytes).
pub fn aes_encrypt(key: &[u8; 32], counter: u64, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("AES init: {e}"))?;
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..12].copy_from_slice(&counter.to_be_bytes());
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("AES encrypt: {e}"))
}

/// AES-256-GCM decrypt. Nonce derived from counter (12 bytes).
pub fn aes_decrypt(key: &[u8; 32], counter: u64, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("AES init: {e}"))?;
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[4..12].copy_from_slice(&counter.to_be_bytes());
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("AES decrypt: {e}"))
}

/// Write a length-framed, AES-encrypted message.
pub fn write_encrypted(
    writer: &mut impl std::io::Write,
    key: &[u8; 32],
    counter: &mut u64,
    plaintext: &[u8],
) -> Result<(), String> {
    let ct = aes_encrypt(key, *counter, plaintext)?;
    *counter += 1;
    let len = (ct.len() as u32).to_be_bytes();
    writer.write_all(&len).map_err(|e| e.to_string())?;
    writer.write_all(&ct).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Read a length-framed, AES-encrypted message.
/// Maximum allowed ciphertext size is ~1 MB to prevent memory exhaustion from malicious peers.
pub fn read_encrypted(
    reader: &mut impl std::io::Read,
    key: &[u8; 32],
    counter: &mut u64,
) -> Result<Vec<u8>, String> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
    let len = u32::from_be_bytes(len_buf) as usize;
    // Encrypted messages (JSON commands) should never exceed 1 MB.
    // Raw data transfers use write_encrypted_raw which sends in 64KB chunks.
    if len > 1_048_576 {
        return Err(format!(
            "Encrypted message too large: {} bytes (max 1 MB)",
            len
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
    let pt = aes_decrypt(key, *counter, &buf)?;
    *counter += 1;
    Ok(pt)
}

/// Write a JSON message encrypted with AES session key.
pub fn write_encrypted_message(
    writer: &mut impl std::io::Write,
    msg: &super::protocol::Message,
    key: &[u8; 32],
    counter: &mut u64,
) -> Result<(), String> {
    let json = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
    write_encrypted(writer, key, counter, &json)
}

/// Read a JSON message decrypted with AES session key.
pub fn read_encrypted_message(
    reader: &mut impl std::io::Read,
    key: &[u8; 32],
    counter: &mut u64,
) -> Result<super::protocol::Message, String> {
    let pt = read_encrypted(reader, key, counter)?;
    serde_json::from_slice(&pt).map_err(|e| e.to_string())
}

/// Write raw data encrypted in chunks.
pub fn write_encrypted_raw(
    writer: &mut impl std::io::Write,
    key: &[u8; 32],
    counter: &mut u64,
    data: &[u8],
) -> Result<(), String> {
    // Send total size first (unencrypted framing)
    let total = (data.len() as u64).to_be_bytes();
    writer.write_all(&total).map_err(|e| e.to_string())?;
    // Send encrypted in 64KB chunks
    const CHUNK: usize = 65536;
    for chunk in data.chunks(CHUNK) {
        write_encrypted(writer, key, counter, chunk)?;
    }
    Ok(())
}

/// Read raw data decrypted in chunks.
pub fn read_encrypted_raw(
    reader: &mut impl std::io::Read,
    key: &[u8; 32],
    counter: &mut u64,
) -> Result<Vec<u8>, String> {
    let mut total_buf = [0u8; 8];
    reader
        .read_exact(&mut total_buf)
        .map_err(|e| e.to_string())?;
    let total = u64::from_be_bytes(total_buf) as usize;
    if total > 500_000_000 {
        return Err("Raw data too large".into());
    }
    let mut result = Vec::with_capacity(total);
    while result.len() < total {
        let pt = read_encrypted(reader, key, counter)?;
        result.extend_from_slice(&pt);
    }
    result.truncate(total);
    Ok(result)
}

// ── ECDH (X25519) for protecting PIN during pairing ──

/// Generate an X25519 ephemeral keypair. Returns (secret_bytes, public_bytes) both 32 bytes.
pub fn generate_ecdh_keypair() -> ([u8; 32], [u8; 32]) {
    let secret = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let public = x25519_dalek::PublicKey::from(&secret);
    (secret.to_bytes(), public.to_bytes())
}

/// Perform X25519 ECDH and derive a 32-byte AES key via HKDF-SHA256.
pub fn ecdh_derive_key(our_secret: &[u8; 32], their_public: &[u8; 32]) -> Result<[u8; 32], String> {
    use hkdf::Hkdf;
    use x25519_dalek::{PublicKey, StaticSecret};

    let secret = StaticSecret::from(*our_secret);
    let public = PublicKey::from(*their_public);
    let shared_secret = secret.diffie_hellman(&public);

    let hk = Hkdf::<Sha256>::new(Some(b"epub-reader-pairing"), shared_secret.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"aes-256-gcm-key", &mut key)
        .map_err(|e| format!("HKDF expand: {e}"))?;
    Ok(key)
}
