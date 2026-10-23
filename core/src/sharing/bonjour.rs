//! Desktop DNS-SD bridge. Mobile adapters use their OS discovery frameworks.
use super::discovery::{DiscoveredPeer, DiscoveryAnnouncement};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
const SERVICE: &str = "_epubreader._tcp.local.";
pub(super) fn advertise(ann: DiscoveryAnnouncement, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
            return;
        };
        let port = ann
            .addr
            .rsplit(':')
            .next()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        if port == 0 {
            let _ = daemon.shutdown();
            return;
        }
        let host = format!("reader-{}.local.", ann.device_id);
        let instance = format!("reader-{}", ann.device_id);
        let properties = [
            ("id", ann.device_id.as_str()),
            ("name", ann.device_name.as_str()),
        ];
        if let Ok(service) =
            mdns_sd::ServiceInfo::new(SERVICE, &instance, &host, "", port, &properties[..])
        {
            let _ = daemon.register(service.enable_addr_auto());
        }
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(250));
        }
        let _ = daemon.shutdown();
    });
}
pub(super) fn browse(own: String, peers: Arc<Mutex<Vec<DiscoveredPeer>>>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
            return;
        };
        let Ok(receiver) = daemon.browse(SERVICE) else {
            let _ = daemon.shutdown();
            return;
        };
        let mut active = std::collections::HashMap::<String, DiscoveredPeer>::new();
        while !stop.load(Ordering::Relaxed) {
            match receiver.recv_timeout(Duration::from_millis(500)) {
                Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                    let Some(id) = info
                        .get_property_val_str("id")
                        .filter(|s| *s != own && uuid::Uuid::parse_str(s).is_ok())
                    else {
                        continue;
                    };
                    let Some(ip) = info
                        .get_addresses_v4()
                        .into_iter()
                        .find(|ip| !ip.is_loopback() && !ip.is_unspecified())
                    else {
                        continue;
                    };
                    let name = info
                        .get_property_val_str("name")
                        .unwrap_or(id)
                        .chars()
                        .take(80)
                        .collect();
                    active.insert(
                        info.get_fullname().to_owned(),
                        DiscoveredPeer {
                            device_id: id.into(),
                            device_name: name,
                            addr: format!("{ip}:{}", info.get_port()),
                            last_seen: crate::now_secs(),
                        },
                    );
                }
                Ok(mdns_sd::ServiceEvent::ServiceRemoved(_, name)) => {
                    active.remove(&name);
                }
                _ => {}
            }
            let now = crate::now_secs();
            let mut list = peers.lock().unwrap_or_else(|e| e.into_inner());
            list.retain(|p| now.saturating_sub(p.last_seen) < 10);
            for peer in active.values() {
                let mut peer = peer.clone();
                peer.last_seen = now;
                if let Some(old) = list.iter_mut().find(|p| p.device_id == peer.device_id) {
                    *old = peer;
                } else {
                    list.push(peer);
                }
            }
        }
        let _ = daemon.shutdown();
    });
}
