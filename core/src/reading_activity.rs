//! Reading activity shared by native clients. Device/day counters merge by maximum.
use crate::{cloud_sync::valid_hash, library::Library};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::Mutex};

static LOCK: Mutex<()> = Mutex::new(());
fn bounded_title(title: &str) -> String {
    let mut end = title.len().min(4096);
    while !title.is_char_boundary(end) {
        end -= 1;
    }
    title[..end].to_owned()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DayActivity {
    pub reading: u64,
    pub listening: u64,
}
impl DayActivity {
    pub fn seconds(&self) -> u64 {
        self.reading.saturating_add(self.listening)
    }
    fn merge(&mut self, other: &Self) {
        self.reading = self.reading.max(other.reading);
        self.listening = self.listening.max(other.listening);
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeviceActivity {
    pub title: String,
    pub days: BTreeMap<String, DayActivity>,
    /// Imported legacy seconds whose date was absent or invalid.
    #[serde(default)]
    pub undated: u64,
}
impl DeviceActivity {
    pub fn validate(&self) -> bool {
        self.title.len() <= 4096
            && self.days.len() <= 20000
            && self.undated <= 10_000_000_000
            && self
                .days
                .iter()
                .all(|(day, value)| parse_day(day).is_some() && value.seconds() <= 172800)
    }
    pub fn merge(&mut self, other: &Self) {
        if self.title.is_empty() {
            self.title = other.title.clone();
        }
        self.undated = self.undated.max(other.undated);
        for (day, value) in &other.days {
            self.days.entry(day.clone()).or_default().merge(value);
        }
    }
}
fn store_version() -> u32 {
    1
}
#[derive(Serialize, Deserialize)]
struct ActivityStore {
    #[serde(default = "store_version")]
    schema_version: u32,
    books: BTreeMap<String, BTreeMap<String, DeviceActivity>>,
    #[serde(default)]
    checkpoints: BTreeMap<String, u64>,
    #[serde(default)]
    migrated: std::collections::BTreeSet<String>,
}
impl Default for ActivityStore {
    fn default() -> Self {
        Self {
            schema_version: store_version(),
            books: Default::default(),
            checkpoints: Default::default(),
            migrated: Default::default(),
        }
    }
}
impl ActivityStore {
    fn load(root: &str) -> Result<Self, String> {
        let path = Path::new(root).join("reading-activity.json");
        match std::fs::read(path) {
            Ok(bytes) => {
                let store: Self = serde_json::from_slice(&bytes)
                    .map_err(|_| "Cannot read reading activity; original file retained")?;
                if store.schema_version != store_version()
                    || store.books.iter().any(|(hash, sources)| {
                        !valid_hash(hash)
                            || sources.iter().any(|(source, value)| {
                                !crate::cloud_sync::valid_device_id(
                                    source.strip_prefix("legacy_").unwrap_or(source),
                                ) || !value.validate()
                            })
                    })
                {
                    return Err(
                        "Invalid or unsupported reading activity; original file retained".into(),
                    );
                }
                Ok(store)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("Cannot read reading activity".into()),
        }
    }
    fn save(&self, root: &str) -> Result<(), String> {
        let path = Path::new(root).join("reading-activity.json");
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(self).map_err(|_| "Cannot encode reading activity")?;
        std::fs::write(&temporary, bytes)
            .and_then(|_| std::fs::rename(temporary, path))
            .map_err(|_| "Cannot save reading activity".into())
    }
    fn migrate(&mut self, root: &str, device: &str) {
        if self.migrated.contains(device) {
            return;
        }
        for entry in Library::load_from(root).books {
            let Some(config) = Library::read_book_config(root, &entry.id) else {
                continue;
            };
            let Some(old) = config.reading_stats else {
                continue;
            };
            let Some(hash) = config
                .file_hash
                .or_else(|| crate::epub::EpubBook::file_hash(&entry.path).ok())
            else {
                continue;
            };
            let mut legacy = DeviceActivity {
                title: bounded_title(&entry.title),
                ..Default::default()
            };
            // The old timer included idle time and used UTC day keys. Keep its total
            // distinct from the new activity trend; the original sessions stay in config.
            legacy.undated = old.total_seconds;
            // Separate legacy identity: repeating migration cannot add history twice.
            self.books
                .entry(hash)
                .or_default()
                .entry(format!("legacy_{device}"))
                .or_default()
                .merge(&legacy);
        }
        self.migrated.insert(device.into());
    }
}

/// Idempotent heartbeat. `elapsed` is cumulative active seconds for a random session.
/// The timestamp and local offset describe this heartbeat, not an inferred historical date.
#[allow(clippy::too_many_arguments)] // Mirrors the shared heartbeat fields exposed by both FFI bridges.
pub fn checkpoint(
    root: &str,
    device: &str,
    book_id: &str,
    session: &str,
    elapsed: u64,
    timestamp: u64,
    offset: i32,
    listening: bool,
) -> Result<(), String> {
    if !crate::cloud_sync::valid_device_id(device)
        || session.len() > 80
        || session.is_empty()
        || !session
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || offset.unsigned_abs() > 86400
    {
        return Err("Invalid reading activity checkpoint".into());
    }
    let local = i128::from(timestamp) + i128::from(offset);
    let last_supported_day = i128::from(parse_day("2200-12-31").unwrap()) * 86400 + 86399;
    if !(0..=last_supported_day).contains(&local) {
        return Err("Invalid reading activity date".into());
    }
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = ActivityStore::load(root)?;
    store.migrate(root, device);
    if !Library::load_from(root)
        .books
        .iter()
        .any(|b| b.id == book_id)
    {
        return Err("Book not found".into());
    }
    let config = Library::read_book_config(root, book_id).ok_or("Book not found")?;
    let hash = config
        .file_hash
        .or_else(|| crate::epub::EpubBook::file_hash(&config.epub_path).ok())
        .ok_or("Book hash unavailable")?;
    let key = format!("{device}/{hash}/{session}");
    let previous = store.checkpoints.get(&key).copied().unwrap_or(0);
    if elapsed <= previous {
        return Ok(());
    }
    let delta = elapsed - previous;
    if delta > 60 {
        return Err("Reading heartbeat exceeded 60 seconds".into());
    }
    let activity = store
        .books
        .entry(hash)
        .or_default()
        .entry(device.into())
        .or_default();
    activity.title = bounded_title(&config.title);
    // Split the short interval at local midnight, rather than attributing it all to today.
    for second in timestamp.saturating_sub(delta)..timestamp {
        let days = (second.min(i64::MAX as u64) as i64 + i64::from(offset)).div_euclid(86400);
        let day = activity.days.entry(date_from_days(days)).or_default();
        if listening {
            day.listening = day.listening.saturating_add(1)
        } else {
            day.reading = day.reading.saturating_add(1)
        }
    }
    store.checkpoints.insert(key, elapsed);
    store.save(root)
}

/// Export includes legacy records; their distinct source identities remain stable.
pub fn export(root: &str, device: &str) -> Result<BTreeMap<String, DeviceActivity>, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = ActivityStore::load(root)?;
    store.migrate(root, device);
    store.save(root)?;
    Ok(store
        .books
        .into_iter()
        .filter_map(|(hash, mut devices)| {
            let mut activity = devices.remove(device).unwrap_or_default();
            if let Some(legacy) = devices.remove(&format!("legacy_{device}")) {
                activity.undated = activity.undated.saturating_add(legacy.undated);
                for (day, legacy) in legacy.days {
                    let value = activity.days.entry(day).or_default();
                    value.reading = value.reading.saturating_add(legacy.reading);
                }
            }
            (!activity.days.is_empty() || activity.undated > 0).then_some((hash, activity))
        })
        .collect())
}

pub fn import(
    root: &str,
    device: &str,
    snapshots: &[crate::cloud_sync::DeviceSnapshot],
) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = ActivityStore::load(root)?;
    for snapshot in snapshots {
        snapshot.validate()?;
        if snapshot.device_id == device {
            continue;
        }
        for (hash, value) in &snapshot.activity {
            store
                .books
                .entry(hash.clone())
                .or_default()
                .entry(snapshot.device_id.clone())
                .or_default()
                .merge(value);
        }
    }
    store.save(root)
}

/// LAN forwards source components, rather than republishing received time as local time.
pub fn snapshots(
    root: &str,
    device: &str,
) -> Result<Vec<crate::cloud_sync::DeviceSnapshot>, String> {
    let own = export(root, device)?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store = ActivityStore::load(root)?;
    let mut snapshots = BTreeMap::<String, crate::cloud_sync::DeviceSnapshot>::new();
    let mut local = crate::cloud_sync::DeviceSnapshot::new(device.into());
    local.activity = own;
    snapshots.insert(device.into(), local);
    for (hash, sources) in store.books {
        for (source, value) in sources {
            if source == device || source.starts_with("legacy_") {
                continue;
            }
            snapshots
                .entry(source.clone())
                .or_insert_with(|| crate::cloud_sync::DeviceSnapshot::new(source))
                .activity
                .insert(hash.clone(), value);
        }
    }
    Ok(snapshots.into_values().collect())
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct ActivityBook {
    pub hash: String,
    pub title: String,
    pub reading: u64,
    pub listening: u64,
    pub undated: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ActivityDay {
    pub date: String,
    pub reading: u64,
    pub listening: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ActivityReport {
    pub today: u64,
    pub week: u64,
    pub total: u64,
    pub reading: u64,
    pub listening: u64,
    pub undated: u64,
    pub active_days: usize,
    pub streak: usize,
    pub days: Vec<ActivityDay>,
    pub books: Vec<ActivityBook>,
    pub devices: usize,
}
pub fn report(
    root: &str,
    device: &str,
    today: &str,
    filter: Option<&str>,
) -> Result<ActivityReport, String> {
    let day_number = parse_day(today).ok_or("Invalid statistics date")?;
    if filter.is_some_and(|h| !valid_hash(h)) {
        return Err("Invalid book hash".into());
    }
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = ActivityStore::load(root)?;
    store.migrate(root, device);
    let mut days = BTreeMap::<String, DayActivity>::new();
    let mut books = Vec::new();
    let mut devices = std::collections::BTreeSet::new();
    for (hash, sources) in store.books {
        if filter.is_some_and(|f| f != hash) {
            continue;
        }
        let mut book = ActivityBook {
            hash,
            ..Default::default()
        };
        for (source, activity) in sources {
            devices.insert(source.strip_prefix("legacy_").unwrap_or(&source).to_owned());
            if book.title.is_empty() {
                book.title = activity.title;
            }
            book.undated = book.undated.saturating_add(activity.undated);
            for (date, value) in activity.days {
                book.reading = book.reading.saturating_add(value.reading);
                book.listening = book.listening.saturating_add(value.listening);
                let aggregate = days.entry(date).or_default();
                aggregate.reading = aggregate.reading.saturating_add(value.reading);
                aggregate.listening = aggregate.listening.saturating_add(value.listening);
            }
        }
        books.push(book);
    }
    books.sort_by_key(|b| {
        std::cmp::Reverse(
            b.reading
                .saturating_add(b.listening)
                .saturating_add(b.undated),
        )
    });
    let reading = books
        .iter()
        .fold(0u64, |sum, b| sum.saturating_add(b.reading));
    let listening = books
        .iter()
        .fold(0u64, |sum, b| sum.saturating_add(b.listening));
    let undated = books
        .iter()
        .fold(0u64, |sum, b| sum.saturating_add(b.undated));
    let mut streak = 0;
    let end = if days.get(today).is_some_and(|v| v.seconds() > 0) {
        day_number
    } else {
        day_number - 1
    };
    while days
        .get(&date_from_days(end - streak as i64))
        .is_some_and(|v| v.seconds() > 0)
    {
        streak += 1;
    }
    let active_days = days.values().filter(|v| v.seconds() > 0).count();
    let week = (0..7)
        .filter_map(|n| days.get(&date_from_days(day_number - n)))
        .fold(0u64, |sum, v| sum.saturating_add(v.seconds()));
    let today = days.get(today).map_or(0, DayActivity::seconds);
    let days = (0..30)
        .rev()
        .map(|n| {
            let date = date_from_days(day_number - n);
            let v = days.get(&date).cloned().unwrap_or_default();
            ActivityDay {
                date,
                reading: v.reading,
                listening: v.listening,
            }
        })
        .collect();
    Ok(ActivityReport {
        today,
        week,
        total: reading.saturating_add(listening).saturating_add(undated),
        reading,
        listening,
        undated,
        active_days,
        streak,
        days,
        books,
        devices: devices.len(),
    })
}

// Gregorian calendar conversion, shared across platforms without timezone dependencies.
pub fn date_from_days(days: i64) -> String {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    format!("{:04}-{m:02}-{d:02}", y + i64::from(m <= 2))
}
fn parse_day(value: &str) -> Option<i64> {
    if !value.is_ascii() || value.len() != 10 || &value[4..5] != "-" || &value[7..8] != "-" {
        return None;
    }
    let y: i64 = value[..4].parse().ok()?;
    let m: i64 = value[5..7].parse().ok()?;
    let d: i64 = value[8..].parse().ok()?;
    if !(1970..=2200).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    (date_from_days(days) == value).then_some(days)
}

pub fn dispatch(
    root: &str,
    device: &str,
    request: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let text = |name: &str| {
        request[name]
            .as_str()
            .ok_or_else(|| format!("Missing {name}"))
    };
    if request["op"] == "activity_checkpoint" {
        checkpoint(
            root,
            device,
            text("id")?,
            text("session")?,
            request["elapsed"].as_u64().ok_or("Missing elapsed")?,
            request["timestamp"].as_u64().ok_or("Missing timestamp")?,
            request["offset"]
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or("Invalid timezone offset")?,
            request["listening"].as_bool().unwrap_or(false),
        )?;
        Ok(serde_json::json!({"saved":true}))
    } else {
        let hash = if let Some(id) = request["id"].as_str() {
            let e = Library::load_from(root)
                .books
                .into_iter()
                .find(|b| b.id == id)
                .ok_or("Book not found")?;
            Some(
                Library::read_book_config(root, id)
                    .and_then(|c| c.file_hash)
                    .or_else(|| crate::epub::EpubBook::file_hash(&e.path).ok())
                    .ok_or("Book hash unavailable")?,
            )
        } else {
            None
        };
        serde_json::to_value(report(root, device, text("today")?, hash.as_deref())?)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::load_from(dir.path().to_str().unwrap());
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/illustrations.epub");
        let e = lib.add_or_update(
            dir.path().to_str().unwrap(),
            "Book".into(),
            fixture.to_string_lossy().into_owned(),
            0,
            None,
        );
        (dir, e.id)
    }
    #[test]
    fn heartbeat_is_idempotent_out_of_order_safe_and_splits_midnight() {
        let (dir, id) = setup();
        let root = dir.path().to_str().unwrap();
        let midnight = parse_day("2026-10-14").unwrap() as u64 * 86400;
        checkpoint(root, "phone", &id, "session", 15, midnight + 5, 0, false).unwrap();
        checkpoint(root, "phone", &id, "session", 15, midnight + 5, 0, false).unwrap();
        checkpoint(root, "phone", &id, "session", 12, midnight + 5, 0, false).unwrap();
        let report = report(root, "phone", "2026-10-14", None).unwrap();
        assert_eq!(report.total, 15);
        assert_eq!(report.today, 5);
        assert_eq!(report.days[28].reading, 10);
        assert!(checkpoint(
            root,
            "phone",
            &id,
            "session",
            1000,
            midnight + 1000,
            0,
            false
        )
        .is_err());
        assert_eq!(report.total, report.reading);
    }
    #[test]
    fn devices_merge_without_repeat_growth_and_keep_reading_and_listening_separate() {
        let (dir, id) = setup();
        let root = dir.path().to_str().unwrap();
        let now = parse_day("2026-10-14").unwrap() as u64 * 86400 + 120;
        checkpoint(root, "phone", &id, "read", 10, now, 0, false).unwrap();
        let hash = export(root, "phone")
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        let mut remote = crate::cloud_sync::DeviceSnapshot::new("desktop".into());
        remote.activity.insert(
            hash,
            DeviceActivity {
                title: "Book".into(),
                days: BTreeMap::from([(
                    "2026-10-14".into(),
                    DayActivity {
                        reading: 20,
                        listening: 30,
                    },
                )]),
                undated: 0,
            },
        );
        import(root, "phone", &[remote.clone(), remote.clone()]).unwrap();
        import(root, "phone", &[remote.clone()]).unwrap();
        let report = report(root, "phone", "2026-10-14", None).unwrap();
        assert_eq!(
            (
                report.total,
                report.reading,
                report.listening,
                report.devices
            ),
            (60, 30, 30, 2)
        );
        remote
            .activity
            .values_mut()
            .next()
            .unwrap()
            .days
            .values_mut()
            .next()
            .unwrap()
            .reading = 5;
        import(root, "phone", &[remote]).unwrap();
        assert_eq!(
            report.total,
            super::report(root, "phone", "2026-10-14", None)
                .unwrap()
                .total
        );
    }
    #[test]
    fn legacy_is_preserved_once_and_excluded_from_new_trend() {
        let (dir, id) = setup();
        let root = dir.path().to_str().unwrap();
        let mut config = Library::read_book_config(root, &id).unwrap();
        config.reading_stats = Some(crate::library::ReadingStats {
            total_seconds: 3600,
            sessions: vec![crate::library::ReadingSession {
                date: "20740".into(),
                seconds: 3600,
            }],
        });
        config.save(root);
        export(root, "phone").unwrap();
        export(root, "phone").unwrap();
        let report = report(root, "phone", "2026-10-14", None).unwrap();
        assert_eq!(report.total, 3600);
        assert_eq!(report.undated, 3600);
        assert_eq!(report.week, 0);
        assert_eq!(
            Library::read_book_config(root, &id)
                .unwrap()
                .reading_stats
                .unwrap()
                .sessions
                .len(),
            1
        );
    }
    #[test]
    fn dates_validate_without_unicode_panics_and_offset_is_local() {
        assert_eq!(date_from_days(0), "1970-01-01");
        assert!(parse_day("2024-02-29").is_some());
        assert!(parse_day("2025-02-29").is_none());
        assert!(parse_day("测测测a").is_none());
        let (dir, id) = setup();
        let root = dir.path().to_str().unwrap();
        let midnight = parse_day("2026-10-14").unwrap() as u64 * 86400;
        checkpoint(
            root,
            "phone",
            &id,
            "r",
            15,
            midnight - 8 * 3600 + 5,
            8 * 3600,
            false,
        )
        .unwrap();
        assert_eq!(report(root, "phone", "2026-10-14", None).unwrap().today, 5);
        assert!(checkpoint(root, "phone", "../bad", "r", 20, midnight, 0, false).is_err());
    }
    #[test]
    fn lan_roundtrip_preserves_sources_and_old_messages_remain_readable() {
        let (dir, id) = setup();
        let root = dir.path().to_str().unwrap();
        let now = parse_day("2026-10-14").unwrap() as u64 * 86400 + 120;
        checkpoint(root, "phone", &id, "read", 10, now, 0, false).unwrap();
        let hash = export(root, "phone")
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        let mut remote = crate::cloud_sync::DeviceSnapshot::new("desktop".into());
        remote.activity.insert(
            hash,
            DeviceActivity {
                title: "Book".into(),
                days: BTreeMap::from([(
                    "2026-10-14".into(),
                    DayActivity {
                        reading: 20,
                        listening: 0,
                    },
                )]),
                undated: 0,
            },
        );
        import(root, "phone", &[remote]).unwrap();
        let message = crate::sharing::protocol::Message::SyncProgress {
            entries: vec![],
            activity: snapshots(root, "phone").unwrap(),
        };
        let wire = serde_json::to_vec(&message).unwrap();
        let decoded: crate::sharing::protocol::Message = serde_json::from_slice(&wire).unwrap();
        let other = tempfile::tempdir().unwrap();
        let other_root = other.path().to_str().unwrap();
        if let crate::sharing::protocol::Message::SyncProgress { activity, .. } = decoded {
            import(other_root, "tablet", &activity).unwrap();
            import(other_root, "tablet", &activity).unwrap();
            let result = report(other_root, "tablet", "2026-10-14", None).unwrap();
            assert_eq!(result.total, 30);
            assert_eq!(result.devices, 2);
        } else {
            panic!("Wrong wire message");
        }
        let old: crate::sharing::protocol::Message =
            serde_json::from_str(r#"{"type":"SyncProgress","entries":[]}"#).unwrap();
        assert!(
            matches!(old,crate::sharing::protocol::Message::SyncProgress{activity,..} if activity.is_empty())
        );
        assert_eq!(
            export(root, "phone").unwrap().values().next().unwrap().days["2026-10-14"].reading,
            10
        );
    }
    #[test]
    fn invalid_remote_data_and_corrupt_local_files_do_not_replace_records() {
        let (dir, id) = setup();
        let root = dir.path().to_str().unwrap();
        let now = parse_day("2026-10-14").unwrap() as u64 * 86400 + 120;
        checkpoint(root, "phone", &id, "read", 10, now, 0, false).unwrap();
        let path = dir.path().join("reading-activity.json");
        let before = std::fs::read(&path).unwrap();
        assert!(checkpoint(root, "phone", &id, "read", 20, u64::MAX, 86400, false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let mut remote = crate::cloud_sync::DeviceSnapshot::new("desktop".into());
        remote
            .activity
            .insert("../bad".into(), DeviceActivity::default());
        assert!(import(root, "phone", &[remote]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::write(&path, b"broken original").unwrap();
        assert!(checkpoint(root, "phone", &id, "read", 20, now, 0, false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken original");
    }
    #[cfg(not(feature = "keychain"))]
    #[test]
    fn encrypted_lan_sync_exchanges_activity_in_both_directions() {
        use crate::sharing::{
            auto_sync_session, connect_to_peer, handle_client, start_server, PeerStore,
        };
        use std::sync::Arc;
        let (server_dir, server_book) = setup();
        let server_root = server_dir.path().to_str().unwrap();
        let (client_dir, client_book) = setup();
        let client_root = client_dir.path().to_str().unwrap();
        let server = PeerStore::load(server_root);
        let mut client = PeerStore::load(client_root);
        let server_id = server.device_id.clone();
        let client_id = client.device_id.clone();
        let now = 1_700_000_120u64;
        let date = date_from_days((now / 86400) as i64);
        checkpoint(
            server_root,
            &server_id,
            &server_book,
            "server",
            20,
            now,
            0,
            false,
        )
        .unwrap();
        checkpoint(
            client_root,
            &client_id,
            &client_book,
            "client",
            10,
            now,
            0,
            false,
        )
        .unwrap();
        let shared = Arc::new(Mutex::new(server));
        let (listener, address) = start_server(
            "127.0.0.1:0",
            server_root,
            server_root,
            "1234",
            shared.clone(),
        )
        .unwrap();
        let root = server_root.to_owned();
        let books = server_dir.path().join("books");
        let server_thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            handle_client(
                &mut stream,
                &root,
                books.to_str().unwrap(),
                "1234",
                shared,
                &[],
            )
        });
        let (mut stream, key, mut send, mut receive) = connect_to_peer(
            &address,
            &mut client,
            client_root,
            Some(&server_id),
            Some("1234"),
        )
        .unwrap();
        auto_sync_session(
            &mut stream,
            &key,
            &mut send,
            &mut receive,
            &mut client,
            client_root,
            client_dir.path().join("books").to_str().unwrap(),
            &[],
        )
        .unwrap();
        crate::sharing::crypto::write_encrypted_message(
            &mut stream,
            &crate::sharing::protocol::Message::Goodbye,
            &key,
            &mut send,
        )
        .unwrap();
        server_thread.join().unwrap().unwrap();
        assert_eq!(
            report(client_root, &client_id, &date, None).unwrap().total,
            30
        );
        assert_eq!(
            report(server_root, &server_id, &date, None).unwrap().total,
            30
        );
    }
}
