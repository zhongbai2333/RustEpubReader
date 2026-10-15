//! Single-recipient, foreground sharing. No PeerStore, pairing, or progress messages.
use super::{
    package::{self, Package, Received, Scratch},
    ticket::{local_address, Location, Ticket},
    Work,
};
use crate::sharing::{aes_decrypt, aes_encrypt};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use serde_json::json;
use sha2::Sha256;
use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};
const MAGIC: &[u8; 8] = b"RERLAN1\0";
fn derived(secret: &[u8; 32], challenge: &[u8; 32], direction: &[u8]) -> Result<[u8; 32], String> {
    let mut key = [0; 32];
    Hkdf::<Sha256>::new(Some(challenge), secret)
        .expand(direction, &mut key)
        .map_err(|_| "Cannot establish temporary share")?;
    Ok(key)
}
fn write_control(
    stream: &mut TcpStream,
    key: &[u8; 32],
    value: &serde_json::Value,
) -> Result<(), String> {
    let bytes = aes_encrypt(
        key,
        0,
        &serde_json::to_vec(value).map_err(|_| "Cannot encode share request")?,
    )?;
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&bytes))
        .map_err(|_| "Local share connection interrupted".into())
}
fn read_control(stream: &mut TcpStream, key: &[u8; 32]) -> Result<serde_json::Value, String> {
    let mut len = [0; 4];
    stream
        .read_exact(&mut len)
        .map_err(|_| "Local share connection interrupted")?;
    let size = u32::from_be_bytes(len) as usize;
    if !(16..=4096).contains(&size) {
        return Err("Invalid temporary share request".into());
    }
    let mut bytes = vec![0; size];
    stream
        .read_exact(&mut bytes)
        .map_err(|_| "Local share connection interrupted")?;
    serde_json::from_slice(
        &aes_decrypt(key, 0, &bytes).map_err(|_| "Invalid temporary share credential")?,
    )
    .map_err(|_| "Invalid temporary share request".into())
}
pub fn listen(package: Package, ticket: Ticket, listener: TcpListener, work: Arc<Work>) {
    let result = (|| -> Result<(), String> {
        listener
            .set_nonblocking(true)
            .map_err(|_| "Cannot start local share listener")?;
        let started = Instant::now();
        let (session, pin, expires) = match &ticket.location {
            Location::Lan {
                session,
                pin,
                expires,
                ..
            } => (session, pin, *expires),
            _ => return Err("Invalid temporary share".into()),
        };
        let mut failed = 0;
        let mut failed_since = Instant::now();
        loop {
            work.check()?;
            if started.elapsed() > Duration::from_secs(600) || crate::now_secs() >= expires {
                work.phase("expired", 0);
                return Ok(());
            }
            if let Ok((mut stream, remote)) = listener.accept() {
                // macOS can inherit O_NONBLOCK from the listener. Control frames
                // must wait for their encrypted receipt rather than fail with EAGAIN.
                stream
                    .set_nonblocking(false)
                    .map_err(|_| "Cannot configure local connection")?;
                if !super::ticket::local_address(&remote.to_string()).is_ok() {
                    continue;
                }
                if failed >= 8 && failed_since.elapsed() < Duration::from_secs(10) {
                    continue;
                }
                if failed_since.elapsed() >= Duration::from_secs(10) {
                    failed = 0;
                    failed_since = Instant::now();
                }
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .map_err(|_| "Cannot configure local connection")?;
                stream
                    .set_write_timeout(Some(Duration::from_secs(15)))
                    .map_err(|_| "Cannot configure local connection")?;
                work.attach(&stream)?;
                let attempt = (|| -> Result<bool, String> {
                    let mut header = [0; 8];
                    let mut challenge = [0; 32];
                    stream
                        .read_exact(&mut header)
                        .and_then(|_| stream.read_exact(&mut challenge))
                        .map_err(|_| "Invalid share handshake")?;
                    if &header != MAGIC {
                        return Err("Unsupported share protocol".into());
                    }
                    let request = read_control(
                        &mut stream,
                        &derived(&package.key, &challenge, b"RER share request")?,
                    )?;
                    if request["session"].as_str() != Some(session)
                        || request["pin"].as_str() != Some(pin)
                    {
                        return Err("Temporary share PIN is incorrect".into());
                    }
                    work.check()?;
                    let size = std::fs::metadata(&package.path)
                        .map_err(|_| "Cannot read share package")?
                        .len();
                    write_control(
                        &mut stream,
                        &derived(&package.key, &challenge, b"RER share response")?,
                        &json!({"size":size}),
                    )?;
                    work.phase("transferring", size);
                    let mut input =
                        File::open(&package.path).map_err(|_| "Cannot read share package")?;
                    let mut buf = [0; 65536];
                    let transfer_started = Instant::now();
                    loop {
                        work.check()?;
                        if transfer_started.elapsed() > Duration::from_secs(900) {
                            return Err("Local share transfer timed out".into());
                        }
                        let n = input
                            .read(&mut buf)
                            .map_err(|_| "Cannot read share package")?;
                        if n == 0 {
                            break;
                        }
                        stream
                            .write_all(&buf[..n])
                            .map_err(|_| "Local share connection interrupted")?;
                        work.advance(n as u64);
                    }
                    stream
                        .set_read_timeout(Some(Duration::from_secs(180)))
                        .map_err(|_| "Cannot configure local connection")?;
                    let ack = read_control(
                        &mut stream,
                        &derived(&package.key, &challenge, b"RER share receipt")?,
                    )?;
                    if ack["received"].as_bool() != Some(true) {
                        return Ok(false);
                    }
                    write_control(
                        &mut stream,
                        &derived(&package.key, &challenge, b"RER share closed")?,
                        &json!({"closed":true}),
                    )?;
                    Ok(true)
                })();
                work.detach();
                match attempt {
                    Ok(true) => {
                        work.phase("completed", 0);
                        return Ok(());
                    }
                    failed_attempt => {
                        if crate::sharing::is_debug_logging_enabled() {
                            if let Err(error) = failed_attempt {
                                // No ticket, key, book names, or peer addresses in diagnostics.
                                eprintln!("[BOOK-SHARE] Local transfer attempt failed: {error}");
                            }
                        }
                        failed += 1;
                        work.check()?;
                        work.phase("waiting", 0);
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    })();
    if let Err(error) = result {
        work.fail(error);
    }
    // Package owns its encrypted snapshot: drop removes it even on cancel/failure.
    drop(package);
}
pub fn receive(root: &str, ticket: &Ticket, work: &Arc<Work>) -> Result<Received, String> {
    let (address, session, pin) = match &ticket.location {
        Location::Lan {
            address,
            session,
            pin,
            ..
        } => (address, session, pin),
        _ => return Err("Invalid local share link".into()),
    };
    work.phase("connecting", 0);
    let mut stream = TcpStream::connect_timeout(&local_address(address)?, Duration::from_secs(8))
        .map_err(|_| "Cannot reach the sharing device; use the same local network")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .map_err(|_| "Cannot configure local connection")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|_| "Cannot configure local connection")?;
    work.attach(&stream)?;
    let result = (|| {
        let key = ticket.secret()?;
        let mut challenge = [0; 32];
        OsRng.fill_bytes(&mut challenge);
        stream
            .write_all(MAGIC)
            .and_then(|_| stream.write_all(&challenge))
            .map_err(|_| "Local share connection interrupted")?;
        write_control(
            &mut stream,
            &derived(&key, &challenge, b"RER share request")?,
            &json!({"session":session,"pin":pin}),
        )?;
        let response = read_control(
            &mut stream,
            &derived(&key, &challenge, b"RER share response")?,
        )?;
        let size = response["size"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= package::MAX_PACKAGE)
            .ok_or("Invalid share package size")?;
        let scratch = Scratch::new(root)?;
        let path = scratch.0.join("download.rershare");
        let mut output = package::create_private(&path)?;
        let mut remaining = size;
        let mut buf = [0; 65536];
        let started = Instant::now();
        work.phase("receiving", size);
        while remaining > 0 {
            work.check()?;
            if started.elapsed() > Duration::from_secs(900) {
                return Err("Local share transfer timed out".into());
            }
            let n = (remaining as usize).min(buf.len());
            stream
                .read_exact(&mut buf[..n])
                .map_err(|_| "Local share connection interrupted")?;
            output
                .write_all(&buf[..n])
                .map_err(|_| "Cannot save received package")?;
            remaining -= n as u64;
            work.advance(n as u64);
        }
        output
            .sync_all()
            .map_err(|_| "Cannot save received package")?;
        let received = package::decode(root, &path, ticket, work)?;
        work.check()?;
        write_control(
            &mut stream,
            &derived(&key, &challenge, b"RER share receipt")?,
            &json!({"received":true}),
        )?;
        let closed = read_control(
            &mut stream,
            &derived(&key, &challenge, b"RER share closed")?,
        )?;
        if closed["closed"].as_bool() != Some(true) {
            return Err("Local share receipt was not confirmed".into());
        }
        Ok(received)
    })();
    work.detach();
    result
}
