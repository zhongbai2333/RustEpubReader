//! UDP broadcast-based LAN discovery.
//!
//! Servers periodically broadcast their TCP address on a fixed UDP port.
//! Clients listen passively and build a live list of reachable peers.

use serde::{Deserialize, Serialize};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::now_secs;

/// UDP port used for discovery broadcasts (chosen to avoid common port conflicts).
pub const DISCOVERY_PORT: u16 = 14527;

/// Payload broadcast by each server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryAnnouncement {
    pub device_id: String,
    pub device_name: String,
    /// Resolved TCP address (real LAN IP:port) clients can connect to.
    pub addr: String,
}

/// A peer discovered via UDP broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    pub device_id: String,
    pub device_name: String,
    pub addr: String,
    /// Unix timestamp (seconds) when the last announcement was received.
    pub last_seen: u64,
}

/// Find the non-loopback outbound IP by "connecting" a UDP socket.
/// No data is actually sent — this just probes the routing table.
pub fn get_local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    let ip = addr.ip().to_string();
    if ip.starts_with("0.") || ip == "127.0.0.1" {
        None
    } else {
        Some(ip)
    }
}

/// Collect all broadcast addresses for local LAN interfaces.
/// Returns a list of subnet-directed broadcast addresses (e.g. "192.168.1.255").
fn get_broadcast_addresses() -> Vec<String> {
    let mut addrs = Vec::new();
    // Always include the limited broadcast as fallback
    addrs.push(format!("255.255.255.255:{DISCOVERY_PORT}"));

    // Probe routing table via UDP "connect" to various common gateway IPs.
    // Each connect reveals which local interface would be used for that destination.
    let targets = [
        "8.8.8.8:80",  // internet route (default gateway)
        "10.0.0.1:80", // common 10.x
        "10.255.255.1:80",
        "172.16.0.1:80", // common 172.x
        "172.28.0.1:80",
        "192.168.0.1:80", // common 192.168.x
        "192.168.1.1:80",
        "192.168.2.1:80",
        "192.168.3.1:80",
        "192.168.4.1:80",
        "192.168.5.1:80",
        "192.168.10.1:80",
        "192.168.31.1:80", // Xiaomi routers
        "192.168.50.1:80", // ASUS routers
        "192.168.100.1:80",
        "192.168.123.1:80",
        "192.168.199.1:80",
    ];
    for target in &targets {
        if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
            if socket.connect(target).is_ok() {
                if let Ok(local) = socket.local_addr() {
                    if let std::net::IpAddr::V4(v4) = local.ip() {
                        let o = v4.octets();
                        if o[0] != 127 && o[0] != 0 {
                            let bcast = format!("{}.{}.{}.255:{DISCOVERY_PORT}", o[0], o[1], o[2]);
                            if !addrs.contains(&bcast) {
                                addrs.push(bcast);
                            }
                        }
                    }
                }
            }
        }
    }

    addrs
}

/// Replace `0.0.0.0:port` (wildcard bind result) with the real local LAN IP.
pub fn resolve_broadcast_addr(bound_addr: &str) -> String {
    if let Some(port) = bound_addr.strip_prefix("0.0.0.0:") {
        if let Some(ip) = get_local_ip() {
            return format!("{ip}:{port}");
        }
    }
    bound_addr.to_string()
}

/// Get all local non-loopback IPv4 addresses by probing the routing table.
pub fn get_all_local_ips() -> Vec<String> {
    let mut ips = Vec::new();
    let targets = [
        "8.8.8.8:80",
        "10.0.0.1:80",
        "10.255.255.1:80",
        "172.16.0.1:80",
        "172.28.0.1:80",
        "192.168.0.1:80",
        "192.168.1.1:80",
        "192.168.2.1:80",
        "192.168.3.1:80",
        "192.168.4.1:80",
        "192.168.5.1:80",
        "192.168.10.1:80",
        "192.168.31.1:80",
        "192.168.50.1:80",
        "192.168.100.1:80",
        "192.168.123.1:80",
        "192.168.199.1:80",
    ];
    for target in &targets {
        if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
            if socket.connect(target).is_ok() {
                if let Ok(local) = socket.local_addr() {
                    let ip = local.ip().to_string();
                    if !ip.starts_with("127.") && !ip.starts_with("0.") && !ips.contains(&ip) {
                        ips.push(ip);
                    }
                }
            }
        }
    }
    ips
}

/// Spawn a thread that broadcasts `ann` every 2 seconds until `stop` is set.
/// Broadcasts on all detected network interfaces and includes multiple IP
/// addresses so peers on different subnets can reach us.
pub fn start_broadcast(ann: DiscoveryAnnouncement, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            dbg_log!("BROADCAST: failed to bind UDP socket");
            return;
        };
        socket.set_broadcast(true).ok();

        // Extract port from the announcement address
        let port = ann.addr.rsplit(':').next().unwrap_or("0").to_string();
        let local_ips = get_all_local_ips();
        let targets = get_broadcast_addresses();
        dbg_log!(
            "BROADCAST: starting, port={}, local_ips={:?}, targets={:?}, device_id={}",
            port,
            local_ips,
            targets,
            ann.device_id
        );

        while !stop.load(Ordering::SeqCst) {
            // Send an announcement for each local IP so peers in any subnet can connect
            for ip in &local_ips {
                let mut ann_copy = ann.clone();
                ann_copy.addr = format!("{ip}:{port}");
                if let Ok(data) = serde_json::to_vec(&ann_copy) {
                    for target in &targets {
                        let _ = socket.send_to(&data, target);
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        dbg_log!("BROADCAST: stopped");
    });
}

/// Check if a peer address shares the /24 subnet with any of our IPs.
fn is_same_subnet(own_ips: &[String], addr: &str) -> bool {
    let peer_ip = addr.rsplit(':').next_back().unwrap_or("");
    let peer_prefix = match peer_ip.rfind('.') {
        Some(i) => &peer_ip[..i],
        None => return false,
    };
    own_ips
        .iter()
        .any(|ip| ip.rfind('.').map(|i| &ip[..i]) == Some(peer_prefix))
}

/// Spawn a background thread that listens for peer announcements.
/// Returns the shared list, updated continuously in the background.
/// Entries older than 10 seconds are automatically evicted.
pub fn start_listener(
    own_device_id: &str,
    stop: Arc<AtomicBool>,
) -> Arc<Mutex<Vec<DiscoveredPeer>>> {
    let peers: Arc<Mutex<Vec<DiscoveredPeer>>> = Arc::new(Mutex::new(Vec::new()));
    let peers_out = peers.clone();
    let own_id = own_device_id.to_string();

    std::thread::spawn(move || {
        let Ok(socket) = UdpSocket::bind(format!("0.0.0.0:{DISCOVERY_PORT}")) else {
            dbg_log!("LISTENER: failed to bind UDP port {}", DISCOVERY_PORT);
            return;
        };
        socket.set_broadcast(true).ok();
        socket.set_read_timeout(Some(Duration::from_secs(1))).ok();
        let own_ips = get_all_local_ips();
        dbg_log!(
            "LISTENER: started, own_id={}, own_ips={:?}",
            own_id,
            own_ips
        );

        let mut buf = [0u8; 4096];
        let mut known_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            match socket.recv_from(&mut buf) {
                Ok((len, src_addr)) => {
                    let Ok(ann) = serde_json::from_slice::<DiscoveryAnnouncement>(&buf[..len])
                    else {
                        dbg_log!("LISTENER: failed to parse announcement from {}", src_addr);
                        continue;
                    };
                    // Ignore our own broadcasts
                    if ann.device_id == own_id {
                        continue;
                    }
                    // Only log first time we see a device (reduce spam)
                    if known_ids.insert(ann.device_id.clone()) {
                        dbg_log!(
                            "LISTENER: NEW peer from {} -> id={} name='{}' addr={}",
                            src_addr,
                            ann.device_id,
                            ann.device_name,
                            ann.addr
                        );
                    }
                    let now = now_secs();
                    let mut list = match peers.lock() {
                        Ok(l) => l,
                        Err(e) => {
                            dbg_log!("LISTENER: peers lock poisoned: {}", e);
                            break;
                        }
                    };
                    // Evict stale entries
                    list.retain(|p| now.saturating_sub(p.last_seen) < 10);
                    if let Some(p) = list.iter_mut().find(|p| p.device_id == ann.device_id) {
                        p.last_seen = now;
                        p.device_name = ann.device_name;
                        // Prefer addresses in our own subnet over others
                        if is_same_subnet(&own_ips, &ann.addr) || !is_same_subnet(&own_ips, &p.addr)
                        {
                            p.addr = ann.addr;
                        }
                    } else {
                        list.push(DiscoveredPeer {
                            device_id: ann.device_id,
                            device_name: ann.device_name,
                            addr: ann.addr,
                            last_seen: now,
                        });
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => break,
            }
        }
    });

    peers_out
}
