//! Peer connection and management for local sharing.
use serde::{Deserialize, Serialize};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use super::crypto;
use super::protocol::*;
use crate::{base64_decode, base64_encode, now_secs};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_name: String,
    pub pairing_uuid: String,
    pub remote_public_key_pem: String,
    pub paired_at: u64,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct PeerStore {
    pub device_id: String,
    pub device_name: String,
    #[serde(default)]
    pub private_key_pem: String,
    #[serde(default)]
    pub public_key_pem: String,
    pub paired: Vec<PairedDevice>,
    pub progress: Vec<ProgressEntry>,
}

impl PeerStore {
    pub fn load(data_dir: &str) -> Self {
        let path = PathBuf::from(data_dir).join("peers.json");
        let mut store = if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self {
                device_id: Uuid::new_v4().to_string(),
                device_name: hostname(),
                ..Default::default()
            }
        };
        if store.device_id.is_empty() {
            store.device_id = Uuid::new_v4().to_string();
        }
        if store.device_name.is_empty() {
            store.device_name = hostname();
        }

        // Try loading private key from OS keychain
        #[cfg(feature = "keychain")]
        {
            if store.private_key_pem.is_empty() {
                if let Some(pem) = super::keystore::load_private_key(&store.device_id) {
                    store.private_key_pem = pem;
                }
            } else {
                // Migrate: move plaintext key from JSON to keychain
                let _ =
                    super::keystore::store_private_key(&store.device_id, &store.private_key_pem);
                store.private_key_pem.clear();
                store.save(data_dir); // re-save without private key
                                      // Reload from keychain
                if let Some(pem) = super::keystore::load_private_key(&store.device_id) {
                    store.private_key_pem = pem;
                }
            }
        }

        // Generate RSA keypair on first use
        if store.private_key_pem.is_empty() || store.public_key_pem.is_empty() {
            if let Ok((priv_pem, pub_pem)) = crypto::generate_rsa_keypair() {
                store.public_key_pem = pub_pem;
                store.private_key_pem = priv_pem;
                #[cfg(feature = "keychain")]
                {
                    let _ = super::keystore::store_private_key(
                        &store.device_id,
                        &store.private_key_pem,
                    );
                }
                store.save(data_dir);
            }
        }
        store
    }

    pub fn save(&self, data_dir: &str) {
        let dir = PathBuf::from(data_dir);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("peers.json");

        #[cfg(feature = "keychain")]
        {
            // Strip private key before writing JSON
            let mut copy = self.clone();
            copy.private_key_pem.clear();
            if let Ok(json) = serde_json::to_string_pretty(&copy) {
                let _ = std::fs::write(&path, json);
            }
        }
        #[cfg(not(feature = "keychain"))]
        {
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(&path, json);
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perm) = std::fs::metadata(&path).map(|m| m.permissions()) {
                perm.set_mode(0o600);
                let _ = std::fs::set_permissions(&path, perm);
            }
        }
    }

    pub fn is_paired(&self, device_id: &str) -> bool {
        self.paired.iter().any(|p| p.device_id == device_id)
    }

    pub fn find_paired(&self, device_id: &str) -> Option<&PairedDevice> {
        self.paired.iter().find(|p| p.device_id == device_id)
    }

    pub fn find_paired_by_uuid(&self, uuid: &str) -> Option<&PairedDevice> {
        self.paired.iter().find(|p| p.pairing_uuid == uuid)
    }

    pub fn add_paired(
        &mut self,
        device_id: String,
        device_name: String,
        pairing_uuid: String,
        remote_public_key_pem: String,
    ) {
        // Update existing or insert new
        if let Some(p) = self.paired.iter_mut().find(|p| p.device_id == device_id) {
            p.device_name = device_name;
            p.pairing_uuid = pairing_uuid;
            p.remote_public_key_pem = remote_public_key_pem;
            p.paired_at = now_secs();
        } else {
            self.paired.push(PairedDevice {
                device_id,
                device_name,
                pairing_uuid,
                remote_public_key_pem,
                paired_at: now_secs(),
            });
        }
    }

    pub fn remove_paired(&mut self, device_id: &str) -> bool {
        let before = self.paired.len();
        self.paired.retain(|p| p.device_id != device_id);
        self.paired.len() < before
    }

    pub fn merge_progress(&mut self, remote: &[ProgressEntry]) -> Vec<ProgressEntry> {
        let mut changed = Vec::new();
        for r in remote {
            if let Some(local) = self
                .progress
                .iter_mut()
                .find(|p| p.book_hash == r.book_hash)
            {
                let should_replace = r.timestamp > local.timestamp
                    || (r.timestamp == local.timestamp
                        && (r.chapter != local.chapter
                            || (local.chapter_title.is_none() && r.chapter_title.is_some())));

                if should_replace {
                    *local = r.clone();
                    changed.push(r.clone());
                }
            } else {
                self.progress.push(r.clone());
                changed.push(r.clone());
            }
        }
        changed
    }
}

/// Start a sharing server. Returns the local address string (ip:port).
pub fn start_server(
    bind_addr: &str,
    _data_dir: &str,
    _books_dir: &str,
    _pin: &str,
    _store: Arc<Mutex<PeerStore>>,
) -> Result<(TcpListener, String), String> {
    let listener = TcpListener::bind(bind_addr).map_err(|e| e.to_string())?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .to_string();
    Ok((listener, local_addr))
}

/// Handle a single client connection on the server side.
///
/// Protocol:
///   1. Client → Hello { device_id, device_name, pairing_uuid }
///      2a. If not paired → PairNeeded → PairRequest(pin, pubkey) → PairAccepted/PairRejected
///      2b. If paired → Challenge(nonce, uuid) → ChallengeResponse(sig) → Authenticated
///   3. SessionKey exchange (client encrypts AES key with server's RSA pubkey)
///   4. All subsequent messages are AES-encrypted
pub fn handle_client(
    stream: &mut TcpStream,
    data_dir: &str,
    books_dir: &str,
    pin: &str,
    store: Arc<Mutex<PeerStore>>,
    extra_book_paths: &[String],
) -> Result<(), String> {
    let peer_addr = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or("?".into());
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();
    dbg_log!("SERVER: new client connection from {}", peer_addr);
    dbg_log!("SERVER: expecting PIN (len={})", pin.len());

    let msg = read_message(stream)?;
    dbg_log!(
        "SERVER: received message: {:?}",
        std::mem::discriminant(&msg)
    );
    let (client_id, client_name, client_uuid) = match &msg {
        Message::Hello {
            device_id,
            device_name,
            pairing_uuid,
        } => {
            dbg_log!(
                "SERVER: Hello from '{}' ({}), uuid={:?}",
                device_name,
                device_id,
                pairing_uuid
            );
            (device_id.clone(), device_name.clone(), pairing_uuid.clone())
        }
        other => {
            dbg_log!(
                "SERVER: expected Hello, got {:?}",
                std::mem::discriminant(other)
            );
            return Err("Expected Hello".into());
        }
    };

    let server_pub_key;
    let server_priv_key;
    let remote_pub_key;
    {
        let s = store
            .lock()
            .map_err(|e| format!("PeerStore lock poisoned: {}", e))?;
        server_pub_key = s.public_key_pem.clone();
        server_priv_key = s.private_key_pem.clone();
        dbg_log!(
            "SERVER: my device_id='{}', has_priv_key={}, has_pub_key={}",
            s.device_id,
            !s.private_key_pem.is_empty(),
            !s.public_key_pem.is_empty()
        );
    }

    // Check if already paired
    let paired_info = store
        .lock()
        .map_err(|e| format!("PeerStore lock poisoned: {}", e))?
        .find_paired(&client_id)
        .cloned();
    dbg_log!(
        "SERVER: paired_info for '{}': {:?}",
        client_id,
        paired_info.as_ref().map(|p| &p.pairing_uuid)
    );

    if let Some(paired) = paired_info {
        if client_uuid.as_deref() != Some(&paired.pairing_uuid) {
            dbg_log!(
                "SERVER: UUID mismatch, re-pairing. client={:?} stored={}",
                client_uuid,
                paired.pairing_uuid
            );
            let (ecdh_secret, ecdh_pub) = crypto::generate_ecdh_keypair();
            write_message(
                stream,
                &Message::PairNeeded {
                    ecdh_public_key: base64_encode(&ecdh_pub),
                },
            )?;
            return handle_pairing(
                stream,
                pin,
                &client_id,
                &client_name,
                &server_pub_key,
                &server_priv_key,
                data_dir,
                &store,
                &ecdh_secret,
            )
            .and_then(|remote_pk| {
                handle_encrypted_session(
                    stream,
                    &server_priv_key,
                    &remote_pk,
                    data_dir,
                    books_dir,
                    &store,
                    extra_book_paths,
                )
            });
        }

        dbg_log!("SERVER: re-auth via challenge-response");
        let nonce = crypto::generate_nonce();
        let nonce_b64 = base64_encode(&nonce);
        write_message(
            stream,
            &Message::Challenge {
                nonce: nonce_b64,
                pairing_uuid: paired.pairing_uuid.clone(),
            },
        )?;

        let resp = read_message(stream)?;
        match resp {
            Message::ChallengeResponse { signature } => {
                let sig_bytes = base64_decode(&signature)?;
                let ok = crypto::rsa_verify(&paired.remote_public_key_pem, &nonce, &sig_bytes);
                dbg_log!("SERVER: challenge response verify = {}", ok);
                if !ok {
                    return Err("Challenge verification failed".into());
                }
                write_message(
                    stream,
                    &Message::Authenticated {
                        public_key_pem: Some(server_pub_key.clone()),
                    },
                )?;
                remote_pub_key = paired.remote_public_key_pem.clone();
            }
            _ => return Err("Expected ChallengeResponse".into()),
        }
    } else {
        // ── First-time pairing ──
        dbg_log!("SERVER: first-time pairing for '{}'", client_id);
        let (ecdh_secret, ecdh_pub) = crypto::generate_ecdh_keypair();
        write_message(
            stream,
            &Message::PairNeeded {
                ecdh_public_key: base64_encode(&ecdh_pub),
            },
        )?;
        dbg_log!("SERVER: sent PairNeeded with ECDH pub");
        remote_pub_key = handle_pairing(
            stream,
            pin,
            &client_id,
            &client_name,
            &server_pub_key,
            &server_priv_key,
            data_dir,
            &store,
            &ecdh_secret,
        )?;
        dbg_log!("SERVER: pairing complete");
    }

    dbg_log!("SERVER: starting encrypted session");
    handle_encrypted_session(
        stream,
        &server_priv_key,
        &remote_pub_key,
        data_dir,
        books_dir,
        &store,
        extra_book_paths,
    )
}

/// Handle the pairing sub-protocol on server side. Returns client's public key PEM.
/// Uses ECDH-derived key to encrypt PIN exchange.
#[allow(clippy::too_many_arguments)]
fn handle_pairing(
    stream: &mut TcpStream,
    pin: &str,
    client_id: &str,
    client_name: &str,
    server_pub_key: &str,
    _server_priv_key: &str,
    data_dir: &str,
    store: &Arc<Mutex<PeerStore>>,
    ecdh_secret: &[u8; 32],
) -> Result<String, String> {
    dbg_log!("PAIRING: waiting for PairKeyExchange...");
    // Step 1: Receive client's ECDH public key
    let kex_msg = read_message(stream)?;
    let client_ecdh_pub = match kex_msg {
        Message::PairKeyExchange { ecdh_public_key } => {
            dbg_log!(
                "PAIRING: received PairKeyExchange (len={})",
                ecdh_public_key.len()
            );
            let bytes = base64_decode(&ecdh_public_key)?;
            if bytes.len() != 32 {
                return Err("Invalid ECDH public key length".into());
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        other => {
            dbg_log!(
                "PAIRING: expected PairKeyExchange, got {:?}",
                std::mem::discriminant(&other)
            );
            return Err("Expected PairKeyExchange".into());
        }
    };

    // Step 2: Derive shared AES key
    let pair_key = crypto::ecdh_derive_key(ecdh_secret, &client_ecdh_pub)?;
    dbg_log!(
        "PAIRING: derived ECDH key, first 4 bytes: {:02x}{:02x}{:02x}{:02x}",
        pair_key[0],
        pair_key[1],
        pair_key[2],
        pair_key[3]
    );
    let mut pair_recv_ctr: u64 = 0;
    let mut pair_send_ctr: u64 = 1;

    // Step 3: Encrypted PIN exchange
    const MAX_ATTEMPTS: u32 = 5;
    let mut attempts = 0u32;
    loop {
        dbg_log!(
            "PAIRING: reading encrypted PairRequest (attempt {})...",
            attempts + 1
        );
        let pair_msg = crypto::read_encrypted_message(stream, &pair_key, &mut pair_recv_ctr)?;
        match pair_msg {
            Message::PairRequest {
                pin: client_pin,
                public_key_pem,
            } => {
                dbg_log!(
                    "PAIRING: received PairRequest, client_pin_len={} server_pin_len={} match={}",
                    client_pin.len(),
                    pin.len(),
                    client_pin == pin
                );
                if client_pin == pin {
                    let pairing_uuid = Uuid::new_v4().to_string();
                    dbg_log!("PAIRING: PIN matched! pairing_uuid={}", pairing_uuid);
                    let (device_id, device_name) = {
                        let s = store
                            .lock()
                            .map_err(|e| format!("PeerStore lock poisoned: {}", e))?;
                        (s.device_id.clone(), s.device_name.clone())
                    };
                    crypto::write_encrypted_message(
                        stream,
                        &Message::PairAccepted {
                            pairing_uuid: pairing_uuid.clone(),
                            public_key_pem: server_pub_key.to_string(),
                            device_name,
                            device_id: Some(device_id),
                        },
                        &pair_key,
                        &mut pair_send_ctr,
                    )?;
                    dbg_log!("PAIRING: sent PairAccepted, saving pairing info...");
                    // Save pairing
                    let mut s = store
                        .lock()
                        .map_err(|e| format!("PeerStore lock poisoned: {}", e))?;
                    s.add_paired(
                        client_id.to_string(),
                        client_name.to_string(),
                        pairing_uuid,
                        public_key_pem.clone(),
                    );
                    s.save(data_dir);
                    dbg_log!("PAIRING: pairing saved successfully");
                    return Ok(public_key_pem);
                } else {
                    attempts += 1;
                    dbg_log!(
                        "PAIRING: PIN mismatch! attempt {}/{}",
                        attempts,
                        MAX_ATTEMPTS
                    );
                    crypto::write_encrypted_message(
                        stream,
                        &Message::PairRejected,
                        &pair_key,
                        &mut pair_send_ctr,
                    )?;
                    if attempts >= MAX_ATTEMPTS {
                        dbg_log!("PAIRING: max attempts reached, rejecting");
                        return Err("Too many failed PIN attempts".into());
                    }
                }
            }
            _ => return Err("Expected PairRequest".into()),
        }
    }
}

/// After authentication, exchange AES session key and handle encrypted data commands.
fn handle_encrypted_session(
    stream: &mut TcpStream,
    server_priv_key: &str,
    _remote_pub_key: &str,
    data_dir: &str,
    books_dir: &str,
    store: &Arc<Mutex<PeerStore>>,
    extra_book_paths: &[String],
) -> Result<(), String> {
    dbg_log!("SESSION: waiting for SessionKey...");
    // Receive AES session key (encrypted with our RSA public key)
    let sk_msg = read_message(stream)?;
    let aes_key: [u8; 32] = match sk_msg {
        Message::SessionKey { encrypted_key } => {
            dbg_log!(
                "SESSION: received SessionKey (encrypted len={})",
                encrypted_key.len()
            );
            let encrypted = base64_decode(&encrypted_key)?;
            let key_bytes = match crypto::rsa_decrypt(server_priv_key, &encrypted) {
                Ok(k) => k,
                Err(e) => {
                    let _ = write_message(
                        stream,
                        &Message::Error {
                            message: format!("Session key decrypt failed; re-pair required: {e}"),
                        },
                    );
                    return Err(format!("Session key decrypt failed: {e}"));
                }
            };
            if key_bytes.len() != 32 {
                let _ = write_message(
                    stream,
                    &Message::Error {
                        message: "Invalid session key length; re-pair required".into(),
                    },
                );
                return Err("Invalid session key length".into());
            }
            let mut k = [0u8; 32];
            k.copy_from_slice(&key_bytes);
            k
        }
        other => {
            dbg_log!(
                "SESSION: expected SessionKey, got {:?}",
                std::mem::discriminant(&other)
            );
            return Err("Expected SessionKey".into());
        }
    };
    dbg_log!("SESSION: AES key decrypted OK, sending Ack");
    write_message(stream, &Message::SessionKeyAck)?;

    // Counters: server uses even for encrypt (send), odd for decrypt (recv)
    // Client uses odd for encrypt (send), even for decrypt (recv)
    let mut send_counter: u64 = 0; // server sends: 0, 2, 4, ...
    let mut recv_counter: u64 = 1; // server receives: 1, 3, 5, ...

    // Encrypted command loop
    loop {
        dbg_log!("SESSION: waiting for next command...");
        let cmd = match crypto::read_encrypted_message(stream, &aes_key, &mut recv_counter) {
            Ok(m) => m,
            Err(e) => {
                // If the read fails with an EOF-like error it usually means the
                // remote side closed the connection (older client without Goodbye).
                // Log at debug level only – this is not a real error.
                dbg_log!("SESSION: peer disconnected ({})", e);
                break;
            }
        };
        match cmd {
            Message::ListBooks => {
                let books = list_local_books(books_dir, extra_book_paths);
                dbg_log!("SESSION: ListBooks -> returning {} books", books.len());
                crypto::write_encrypted_message(
                    stream,
                    &Message::BookList { books },
                    &aes_key,
                    &mut send_counter,
                )?;
            }
            Message::RequestBook { hash } => {
                match find_book_by_hash(books_dir, extra_book_paths, &hash) {
                    Some((title, data)) => {
                        crypto::write_encrypted_message(
                            stream,
                            &Message::BookData {
                                title,
                                hash: hash.clone(),
                                size: data.len() as u64,
                            },
                            &aes_key,
                            &mut send_counter,
                        )?;
                        crypto::write_encrypted_raw(stream, &aes_key, &mut send_counter, &data)?;
                    }
                    None => {
                        crypto::write_encrypted_message(
                            stream,
                            &Message::BookNotFound { hash },
                            &aes_key,
                            &mut send_counter,
                        )?;
                    }
                }
            }
            Message::SendBook {
                title,
                hash,
                size: _,
            } => {
                let data = crypto::read_encrypted_raw(stream, &aes_key, &mut recv_counter)?;
                let mut library = crate::library::Library::load_from(data_dir);
                let entry = library.add_or_update_from_bytes(data_dir, title, &data, 0, None);
                if std::path::Path::new(&entry.path).exists() {
                    crypto::write_encrypted_message(
                        stream,
                        &Message::BookReceived { hash },
                        &aes_key,
                        &mut send_counter,
                    )?;
                } else {
                    dbg_log!("SESSION: failed to persist received book, not sending ACK");
                    return Err("Failed to persist received book".into());
                }
            }
            Message::SyncProgress { entries } => {
                let all_progress = {
                    let mut s = store
                        .lock()
                        .map_err(|e| format!("PeerStore lock poisoned: {}", e))?;
                    s.merge_progress(&entries);
                    s.save(data_dir);
                    s.progress.clone()
                };
                crypto::write_encrypted_message(
                    stream,
                    &Message::ProgressResponse {
                        entries: all_progress,
                    },
                    &aes_key,
                    &mut send_counter,
                )?;
            }
            Message::Goodbye => {
                dbg_log!("SESSION: received Goodbye, closing session");
                break;
            }
            _ => break,
        }
    }
    Ok(())
}

/// Connect to a peer, authenticate, and establish encrypted session.
/// Returns (TcpStream, aes_key, send_counter, recv_counter).
pub fn connect_to_peer(
    addr: &str,
    store: &mut PeerStore,
    data_dir: &str,
    remote_device_id: Option<&str>,
    pin: Option<&str>,
) -> Result<(TcpStream, [u8; 32], u64, u64), String> {
    dbg_log!(
        "CONNECT: connecting to addr={} remote_id={:?} pin={:?}",
        addr,
        remote_device_id,
        pin
    );
    let mut stream = TcpStream::connect(addr).map_err(|e| {
        dbg_log!("CONNECT: TCP connect failed: {}", e);
        format!("连接失败: {e}")
    })?;
    dbg_log!("CONNECT: TCP connected OK");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();

    // Look up if we're already paired with this device
    let pairing_uuid = remote_device_id
        .and_then(|id| store.find_paired(id))
        .map(|p| p.pairing_uuid.clone());
    dbg_log!("CONNECT: stored pairing_uuid={:?}", pairing_uuid);

    write_message(
        &mut stream,
        &Message::Hello {
            device_id: store.device_id.clone(),
            device_name: store.device_name.clone(),
            pairing_uuid: pairing_uuid.clone(),
        },
    )?;
    dbg_log!("CONNECT: Hello sent, device_id={}", store.device_id);

    let remote_pub_key;
    let resp = read_message(&mut stream)?;
    match resp {
        Message::PairNeeded { ecdh_public_key } => {
            dbg_log!(
                "CONNECT: got PairNeeded, ECDH pub len={}",
                ecdh_public_key.len()
            );
            // Need to pair — requires PIN
            let pin = pin.ok_or("需要配对 PIN")?;
            dbg_log!("CONNECT: using PIN='{}'", pin);

            // Step 1: ECDH key exchange
            let server_ecdh_bytes = base64_decode(&ecdh_public_key)?;
            if server_ecdh_bytes.len() != 32 {
                return Err("Invalid server ECDH key".into());
            }
            let mut server_ecdh_pub = [0u8; 32];
            server_ecdh_pub.copy_from_slice(&server_ecdh_bytes);

            let (client_ecdh_secret, client_ecdh_pub) = crypto::generate_ecdh_keypair();
            write_message(
                &mut stream,
                &Message::PairKeyExchange {
                    ecdh_public_key: base64_encode(&client_ecdh_pub),
                },
            )?;
            dbg_log!("CONNECT: sent PairKeyExchange");

            let pair_key = crypto::ecdh_derive_key(&client_ecdh_secret, &server_ecdh_pub)?;
            dbg_log!(
                "CONNECT: derived ECDH key, first 4 bytes: {:02x}{:02x}{:02x}{:02x}",
                pair_key[0],
                pair_key[1],
                pair_key[2],
                pair_key[3]
            );
            let mut pair_send_ctr: u64 = 0;
            let mut pair_recv_ctr: u64 = 1;

            // Step 2: Send PIN + RSA public key (encrypted with ECDH-derived key)
            let my_pub_key = store.public_key_pem.clone();
            dbg_log!("CONNECT: sending encrypted PairRequest with pin='{}'", pin);
            crypto::write_encrypted_message(
                &mut stream,
                &Message::PairRequest {
                    pin: pin.to_string(),
                    public_key_pem: my_pub_key,
                },
                &pair_key,
                &mut pair_send_ctr,
            )?;
            dbg_log!("CONNECT: waiting for pair response...");
            let pair_resp =
                crypto::read_encrypted_message(&mut stream, &pair_key, &mut pair_recv_ctr)?;
            match pair_resp {
                Message::PairAccepted {
                    pairing_uuid,
                    public_key_pem,
                    device_name,
                    device_id,
                } => {
                    dbg_log!(
                        "CONNECT: PairAccepted! uuid={} device={}",
                        pairing_uuid,
                        device_name
                    );
                    let remote_id = device_id
                        .or_else(|| remote_device_id.map(str::to_string))
                        .unwrap_or_else(|| "unknown".to_string());
                    store.add_paired(remote_id, device_name, pairing_uuid, public_key_pem.clone());
                    store.save(data_dir);
                    remote_pub_key = public_key_pem;
                }
                Message::PairRejected => {
                    dbg_log!("CONNECT: PairRejected! PIN was wrong");
                    return Err("PIN 不正确".into());
                }
                other => {
                    dbg_log!(
                        "CONNECT: unexpected pair response: {:?}",
                        std::mem::discriminant(&other)
                    );
                    return Err("意外响应".into());
                }
            }
        }
        Message::Challenge {
            nonce,
            pairing_uuid: server_uuid,
        } => {
            dbg_log!(
                "CONNECT: got Challenge, server_uuid={} our_uuid={:?}",
                server_uuid,
                pairing_uuid
            );
            // Already paired — verify UUID and respond to challenge
            if pairing_uuid.as_deref() != Some(&server_uuid) {
                dbg_log!("CONNECT: UUID mismatch!");
                return Err("配对 UUID 不匹配".into());
            }
            let nonce_bytes = base64_decode(&nonce)?;
            let sig = crypto::rsa_sign(&store.private_key_pem, &nonce_bytes)?;
            write_message(
                &mut stream,
                &Message::ChallengeResponse {
                    signature: base64_encode(&sig),
                },
            )?;
            dbg_log!("CONNECT: sent ChallengeResponse, waiting for auth...");
            let auth_resp = read_message(&mut stream)?;
            match auth_resp {
                Message::Authenticated { public_key_pem } => {
                    dbg_log!(
                        "CONNECT: Authenticated! server_pub_from_auth={} ",
                        public_key_pem.as_ref().map(|_| "yes").unwrap_or("no")
                    );

                    if let Some(current_server_pub) = public_key_pem.filter(|s| !s.is_empty()) {
                        remote_pub_key = current_server_pub.clone();
                        if let Some(id) = remote_device_id {
                            if let Some(existing) = store.find_paired(id).cloned() {
                                store.add_paired(
                                    existing.device_id,
                                    existing.device_name,
                                    existing.pairing_uuid,
                                    current_server_pub,
                                );
                                store.save(data_dir);
                            }
                        }
                    } else {
                        remote_pub_key = remote_device_id
                            .and_then(|id| store.find_paired(id))
                            .map(|p| p.remote_public_key_pem.clone())
                            .ok_or("No stored public key for peer")?;
                    }
                }
                other => {
                    dbg_log!(
                        "CONNECT: auth failed, got {:?}",
                        std::mem::discriminant(&other)
                    );
                    return Err("认证失败".into());
                }
            }
        }
        other => {
            dbg_log!(
                "CONNECT: unexpected response: {:?}",
                std::mem::discriminant(&other)
            );
            return Err("意外响应".into());
        }
    }

    // ── Session key exchange ──
    dbg_log!("CONNECT: generating AES session key...");
    let aes_key = crypto::generate_aes_key();
    let encrypted_key = crypto::rsa_encrypt(&remote_pub_key, &aes_key)?;
    write_message(
        &mut stream,
        &Message::SessionKey {
            encrypted_key: base64_encode(&encrypted_key),
        },
    )?;
    dbg_log!("CONNECT: SessionKey sent, waiting for Ack...");
    let ack = read_message(&mut stream)?;
    match ack {
        Message::SessionKeyAck => {
            dbg_log!("CONNECT: SessionKeyAck received, session established!");
        }
        Message::Error { message } => {
            dbg_log!("CONNECT: SessionKey error from server: {}", message);
            return Err(message);
        }
        other => {
            dbg_log!(
                "CONNECT: expected SessionKeyAck, got {:?}",
                std::mem::discriminant(&other)
            );
            return Err("Expected SessionKeyAck".into());
        }
    }

    // Client counters: odd for send (1,3,5,...), even for recv (0,2,4,...)
    let send_counter: u64 = 1;
    let recv_counter: u64 = 0;

    Ok((stream, aes_key, send_counter, recv_counter))
}

#[allow(clippy::too_many_arguments)]
pub fn auto_sync_session(
    stream: &mut TcpStream,
    aes_key: &[u8; 32],
    send_ctr: &mut u64,
    recv_ctr: &mut u64,
    store: &mut PeerStore,
    data_dir: &str,
    books_dir: &str,
    extra_book_paths: &[String],
) -> Result<(Vec<ProgressEntry>, Vec<SharedBookInfo>), String> {
    dbg_log!(
        "SYNC: starting auto_sync, progress entries={}",
        store.progress.len()
    );
    crypto::write_encrypted_message(
        stream,
        &Message::SyncProgress {
            entries: store.progress.clone(),
        },
        aes_key,
        send_ctr,
    )?;
    dbg_log!("SYNC: SyncProgress sent, waiting for ProgressResponse...");

    let changed_progress = match crypto::read_encrypted_message(stream, aes_key, recv_ctr)? {
        Message::ProgressResponse { entries } => {
            dbg_log!("SYNC: got ProgressResponse with {} entries", entries.len());
            let changed = store.merge_progress(&entries);
            store.save(data_dir);
            dbg_log!("SYNC: progress merged, {} changed", changed.len());
            changed
        }
        other => {
            dbg_log!(
                "SYNC: expected ProgressResponse, got {:?}",
                std::mem::discriminant(&other)
            );
            return Err("Expected ProgressResponse".into());
        }
    };

    dbg_log!("SYNC: requesting book list...");
    crypto::write_encrypted_message(stream, &Message::ListBooks, aes_key, send_ctr)?;
    match crypto::read_encrypted_message(stream, aes_key, recv_ctr)? {
        Message::BookList {
            books: remote_books,
        } => {
            dbg_log!("SYNC: remote has {} books", remote_books.len());
            let local_books = list_local_books(books_dir, extra_book_paths);
            dbg_log!("SYNC: local has {} books", local_books.len());
            for rb in &remote_books {
                if !local_books.iter().any(|lb| lb.hash == rb.hash) {
                    dbg_log!(
                        "SYNC: downloading missing book: {} (hash={})",
                        rb.title,
                        rb.hash
                    );
                    crypto::write_encrypted_message(
                        stream,
                        &Message::RequestBook {
                            hash: rb.hash.clone(),
                        },
                        aes_key,
                        send_ctr,
                    )?;
                    match crypto::read_encrypted_message(stream, aes_key, recv_ctr)? {
                        Message::BookData {
                            title,
                            hash: _,
                            size: _,
                        } => {
                            let data = crypto::read_encrypted_raw(stream, aes_key, recv_ctr)?;
                            let mut library = crate::library::Library::load_from(data_dir);
                            let _ =
                                library.add_or_update_from_bytes(data_dir, title, &data, 0, None);
                            crypto::write_encrypted_message(
                                stream,
                                &Message::BookReceived {
                                    hash: rb.hash.clone(),
                                },
                                aes_key,
                                send_ctr,
                            )?;
                        }
                        Message::BookNotFound { .. } => {}
                        _ => return Err("Expected BookData or BookNotFound".into()),
                    }
                }
            }
            // Signal the server we're done so it doesn't see an unexpected EOF.
            let _ = crypto::write_encrypted_message(stream, &Message::Goodbye, aes_key, send_ctr);
            Ok((changed_progress, remote_books))
        }
        _ => Err("Expected BookList".into()),
    }
}

fn list_local_books(books_dir: &str, extra_paths: &[String]) -> Vec<SharedBookInfo> {
    let mut books = Vec::new();
    let mut seen_hashes = std::collections::HashSet::new();
    let dir = PathBuf::from(books_dir);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("epub") {
                if let Ok(meta) = std::fs::metadata(&path) {
                    let hash = match crate::epub::EpubBook::file_hash(&path.to_string_lossy()) {
                        Ok(h) if !h.is_empty() => h,
                        _ => {
                            dbg_log!("list_local_books: skipping {:?}, hash failed", path);
                            continue;
                        }
                    };
                    if seen_hashes.insert(hash.clone()) {
                        let title = crate::epub::EpubBook::read_title(&path).unwrap_or_else(|| {
                            path.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        });
                        books.push(SharedBookInfo {
                            title,
                            hash,
                            size: meta.len(),
                        });
                    }
                }
            }
        }
    }
    // Also include books from extra paths (e.g. Library entries)
    for extra in extra_paths {
        let p = PathBuf::from(extra);
        if p.extension().and_then(|e| e.to_str()) == Some("epub") {
            if let Ok(meta) = std::fs::metadata(&p) {
                let hash = match crate::epub::EpubBook::file_hash(extra) {
                    Ok(h) if !h.is_empty() => h,
                    _ => continue,
                };
                if seen_hashes.insert(hash.clone()) {
                    let title = crate::epub::EpubBook::read_title(&p).unwrap_or_else(|| {
                        p.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    });
                    books.push(SharedBookInfo {
                        title,
                        hash,
                        size: meta.len(),
                    });
                }
            }
        }
    }
    books
}

fn find_book_by_hash(
    books_dir: &str,
    extra_paths: &[String],
    target_hash: &str,
) -> Option<(String, Vec<u8>)> {
    let dir = PathBuf::from(books_dir);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("epub") {
                if let Ok(hash) = crate::epub::EpubBook::file_hash(&path.to_string_lossy()) {
                    if hash == target_hash {
                        let title = crate::epub::EpubBook::read_title(&path).unwrap_or_else(|| {
                            path.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string()
                        });
                        if let Ok(data) = std::fs::read(&path) {
                            return Some((title, data));
                        }
                    }
                }
            }
        }
    }
    // Also check extra paths
    for extra in extra_paths {
        let p = PathBuf::from(extra);
        if p.extension().and_then(|e| e.to_str()) == Some("epub") {
            if let Ok(hash) = crate::epub::EpubBook::file_hash(extra) {
                if hash == target_hash {
                    let title = crate::epub::EpubBook::read_title(&p).unwrap_or_else(|| {
                        p.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    });
                    if let Ok(data) = std::fs::read(&p) {
                        return Some((title, data));
                    }
                }
            }
        }
    }
    None
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Test the full pairing flow: client sends correct PIN, server accepts.
    #[test]
    fn test_pairing_correct_pin() {
        let tmp = std::env::temp_dir().join("epub_pair_test");
        let server_dir = tmp.join("server");
        let client_dir = tmp.join("client");
        let _ = std::fs::create_dir_all(&server_dir);
        let _ = std::fs::create_dir_all(&client_dir);

        let server_data = server_dir.to_string_lossy().to_string();
        let client_data = client_dir.to_string_lossy().to_string();

        let server_pin = "1234";

        let server_store = PeerStore::load(&server_data);
        let server_device_id = server_store.device_id.clone();
        let server_store_arc = Arc::new(Mutex::new(server_store));

        let mut client_store = PeerStore::load(&client_data);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let ss = server_store_arc.clone();
        let sd = server_data.clone();
        let pin = server_pin.to_string();
        let server_thread = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .ok();
                handle_client(&mut stream, &sd, &sd, &pin, ss, &[])
            } else {
                Err("accept failed".into())
            }
        });

        let result = connect_to_peer(
            &addr,
            &mut client_store,
            &client_data,
            Some(&server_device_id),
            Some(server_pin),
        );
        assert!(result.is_ok(), "connect_to_peer failed: {:?}", result.err());

        let _ = server_thread.join();

        assert!(
            !client_store.paired.is_empty(),
            "Client should have paired device"
        );
        let server_s = server_store_arc.lock().unwrap();
        assert!(
            !server_s.paired.is_empty(),
            "Server should have paired device"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Test that wrong PIN is rejected.
    #[test]
    fn test_pairing_wrong_pin() {
        let tmp = std::env::temp_dir().join("epub_wrongpin_test");
        let server_dir = tmp.join("server");
        let client_dir = tmp.join("client");
        let _ = std::fs::create_dir_all(&server_dir);
        let _ = std::fs::create_dir_all(&client_dir);

        let server_data = server_dir.to_string_lossy().to_string();
        let client_data = client_dir.to_string_lossy().to_string();

        let server_pin = "1234";

        let server_store = PeerStore::load(&server_data);
        let server_device_id = server_store.device_id.clone();
        let server_store_arc = Arc::new(Mutex::new(server_store));

        let mut client_store = PeerStore::load(&client_data);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let ss = server_store_arc.clone();
        let sd = server_data.clone();
        let pin = server_pin.to_string();
        let server_thread = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .ok();
                let _ = handle_client(&mut stream, &sd, &sd, &pin, ss, &[]);
            }
        });

        let result = connect_to_peer(
            &addr,
            &mut client_store,
            &client_data,
            Some(&server_device_id),
            Some("5678"),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("PIN"));

        let _ = server_thread.join();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Test that re-authentication works after initial pairing.
    #[test]
    fn test_reauth_after_pairing() {
        let tmp = std::env::temp_dir().join("epub_reauth_test");
        let server_dir = tmp.join("server");
        let client_dir = tmp.join("client");
        let _ = std::fs::create_dir_all(&server_dir);
        let _ = std::fs::create_dir_all(&client_dir);

        let server_data = server_dir.to_string_lossy().to_string();
        let client_data = client_dir.to_string_lossy().to_string();

        let server_pin = "9999";

        let server_store = PeerStore::load(&server_data);
        let server_device_id = server_store.device_id.clone();
        let server_store_arc = Arc::new(Mutex::new(server_store));

        let mut client_store = PeerStore::load(&client_data);

        // ── First: pair ──
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let ss = server_store_arc.clone();
        let sd = server_data.clone();
        let pin = server_pin.to_string();
        let t = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .ok();
                handle_client(&mut stream, &sd, &sd, &pin, ss, &[])
            } else {
                Err("accept failed".into())
            }
        });

        let result = connect_to_peer(
            &addr,
            &mut client_store,
            &client_data,
            Some(&server_device_id),
            Some(server_pin),
        );
        assert!(result.is_ok(), "First pairing failed: {:?}", result.err());
        let _ = t.join();

        // ── Second: re-authenticate ──
        let listener2 = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr2 = listener2.local_addr().unwrap().to_string();

        let ss2 = server_store_arc.clone();
        let sd2 = server_data.clone();
        let pin2 = server_pin.to_string();
        let t2 = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener2.accept() {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .ok();
                handle_client(&mut stream, &sd2, &sd2, &pin2, ss2, &[])
            } else {
                Err("accept failed".into())
            }
        });

        // Re-connect — this time should use challenge-response, no PIN needed
        let result2 = connect_to_peer(
            &addr2,
            &mut client_store,
            &client_data,
            Some(&server_device_id),
            None,
        );
        assert!(result2.is_ok(), "Re-auth failed: {:?}", result2.err());
        let _ = t2.join();

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_manual_pairing_without_remote_id_should_store_real_server_id() {
        let tmp = std::env::temp_dir().join("epub_manual_pair_remote_id_test");
        let server_dir = tmp.join("server");
        let client_dir = tmp.join("client");
        let _ = std::fs::create_dir_all(&server_dir);
        let _ = std::fs::create_dir_all(&client_dir);

        let server_data = server_dir.to_string_lossy().to_string();
        let client_data = client_dir.to_string_lossy().to_string();
        let server_pin = "1357";

        let server_store = PeerStore::load(&server_data);
        let server_device_id = server_store.device_id.clone();
        let server_store_arc = Arc::new(Mutex::new(server_store));

        let mut client_store = PeerStore::load(&client_data);

        // First connect manually without knowing remote device_id.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let ss = server_store_arc.clone();
        let sd = server_data.clone();
        let pin = server_pin.to_string();
        let t = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .ok();
            handle_client(&mut stream, &sd, &sd, &pin, ss, &[])
        });

        let first = connect_to_peer(
            &addr,
            &mut client_store,
            &client_data,
            None,
            Some(server_pin),
        );
        assert!(
            first.is_ok(),
            "manual pairing without remote id failed: {:?}",
            first.err()
        );
        let _ = t.join();

        assert!(
            client_store
                .paired
                .iter()
                .any(|p| p.device_id == server_device_id),
            "client should store the server's real device_id after pairing"
        );
        assert!(
            client_store.paired.iter().all(|p| p.device_id != "unknown"),
            "client should no longer fall back to an 'unknown' paired device entry"
        );

        // Then reconnect using the discovered/real device_id without PIN.
        let listener2 = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr2 = listener2.local_addr().unwrap().to_string();
        let ss2 = server_store_arc.clone();
        let sd2 = server_data.clone();
        let pin2 = server_pin.to_string();
        let t2 = std::thread::spawn(move || {
            let (mut stream, _) = listener2.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .ok();
            handle_client(&mut stream, &sd2, &sd2, &pin2, ss2, &[])
        });

        let second = connect_to_peer(
            &addr2,
            &mut client_store,
            &client_data,
            Some(&server_device_id),
            None,
        );
        assert!(
            second.is_ok(),
            "reconnect using stored real device_id should not need PIN: {:?}",
            second.err()
        );
        let _ = t2.join();

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_list_local_books_includes_extra_paths() {
        let tmp = std::env::temp_dir().join("epub_list_local_books_extra_paths_test");
        let books_dir = tmp.join("books");
        let _ = std::fs::create_dir_all(&books_dir);

        let external = tmp.join("external.epub");
        let _ = std::fs::write(&external, b"not-a-real-epub-but-hashable");

        let books_dir_str = books_dir.to_string_lossy().to_string();
        let external_str = external.to_string_lossy().to_string();
        let books = list_local_books(&books_dir_str, std::slice::from_ref(&external_str));

        assert_eq!(books.len(), 1);
        assert_eq!(
            books[0].hash,
            crate::epub::EpubBook::file_hash(&external_str).unwrap()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_merge_progress_should_fill_missing_chapter_title_on_equal_timestamp() {
        let mut store = PeerStore::default();
        store.progress.push(ProgressEntry {
            book_hash: "h1".into(),
            title: "Book".into(),
            chapter: 17,
            chapter_title: None,
            timestamp: 100,
        });

        let changed = store.merge_progress(&[ProgressEntry {
            book_hash: "h1".into(),
            title: "Book".into(),
            chapter: 17,
            chapter_title: Some("第16章 被妹妹讨厌了".into()),
            timestamp: 100,
        }]);

        assert_eq!(changed.len(), 1);
        assert_eq!(store.progress.len(), 1);
        assert_eq!(
            store.progress[0].chapter_title.as_deref(),
            Some("第16章 被妹妹讨厌了")
        );
    }

    #[test]
    fn test_reauth_with_stale_server_pubkey_should_self_heal() {
        let tmp = std::env::temp_dir().join("epub_reauth_stale_server_pubkey_test");
        let server_dir = tmp.join("server");
        let client_dir = tmp.join("client");
        let _ = std::fs::create_dir_all(&server_dir);
        let _ = std::fs::create_dir_all(&client_dir);

        let server_data = server_dir.to_string_lossy().to_string();
        let client_data = client_dir.to_string_lossy().to_string();
        let server_pin = "2468";

        let server_store = PeerStore::load(&server_data);
        let server_device_id = server_store.device_id.clone();
        let server_store_arc = Arc::new(Mutex::new(server_store));

        let mut client_store = PeerStore::load(&client_data);

        // First connect/pair normally.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let ss = server_store_arc.clone();
        let sd = server_data.clone();
        let pin = server_pin.to_string();
        let t = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .ok();
                handle_client(&mut stream, &sd, &sd, &pin, ss, &[])
            } else {
                Err("accept failed".into())
            }
        });

        let first = connect_to_peer(
            &addr,
            &mut client_store,
            &client_data,
            Some(&server_device_id),
            Some(server_pin),
        );
        assert!(first.is_ok(), "first pairing failed: {:?}", first.err());
        let _ = t.join();

        // Corrupt the stored server pubkey on client with another valid pubkey.
        let (_, wrong_pub) = crypto::generate_rsa_keypair().unwrap();
        if let Some(p) = client_store
            .paired
            .iter_mut()
            .find(|p| p.device_id == server_device_id)
        {
            p.remote_public_key_pem = wrong_pub;
        }
        client_store.save(&client_data);

        // Re-auth should still succeed because server now sends current pubkey in Authenticated.
        let listener2 = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr2 = listener2.local_addr().unwrap().to_string();
        let ss2 = server_store_arc.clone();
        let sd2 = server_data.clone();
        let pin2 = server_pin.to_string();
        let t2 = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener2.accept() {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .ok();
                handle_client(&mut stream, &sd2, &sd2, &pin2, ss2, &[])
            } else {
                Err("accept failed".into())
            }
        });

        let second = connect_to_peer(
            &addr2,
            &mut client_store,
            &client_data,
            Some(&server_device_id),
            None,
        );
        assert!(
            second.is_ok(),
            "reauth with stale pubkey should self-heal but failed: {:?}",
            second.err()
        );
        let _ = t2.join();

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
