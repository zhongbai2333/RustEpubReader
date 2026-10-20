//! Durable JSON snapshots. Invalid originals are never silently replaced by defaults.
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".bak");
    name.into()
}

/// Write beside the destination, sync its bytes, then atomically replace it.
/// Unique temporary names avoid collisions between independent writers.
pub fn replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    publish(path, |file| file.write_all(bytes))
}

/// Stream large imports into a sibling file; a failed copy cannot truncate an
/// existing destination or allocate a full publication in memory.
pub fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    let mut source =
        fs::File::open(source).map_err(|e| format!("Cannot read {}: {e}", source.display()))?;
    publish(destination, |file| {
        std::io::copy(&mut source, file).map(|_| ())
    })
}

fn publish(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> std::io::Result<()>,
) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid storage path")?;
    fs::create_dir_all(parent).map_err(|e| format!("Cannot create storage: {e}"))?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        {
            // A completed rename must survive a power loss as well as a process crash.
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("Cannot save {}: {e}", path.display()))
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<(T, Vec<u8>)>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
    };
    let value = serde_json::from_slice(&bytes)
        .map_err(|_| format!("Invalid stored data: {}", path.display()))?;
    Ok(Some((value, bytes)))
}

/// Missing primary files represent new/deleted state. A backup alone must not
/// resurrect a deliberately deleted record. Recovery is only for damaged JSON.
pub fn load<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match read::<T>(path) {
        Ok(value) => Ok(value.map(|(value, _)| value)),
        Err(error) => {
            // Permission and IO errors must not be treated as corruption.
            let original = fs::read(path).map_err(|_| error.clone())?;
            if serde_json::from_slice::<T>(&original).is_ok() {
                return Err(error);
            }
            let Some((value, bytes)) = read::<T>(&backup_path(path))? else {
                return Err(error);
            };
            let evidence = path.with_extension(format!("json.corrupt.{}", uuid::Uuid::new_v4()));
            replace(&evidence, &original)?;
            replace(path, &bytes)?;
            Ok(Some(value))
        }
    }
}

/// Retain the previous schema-valid snapshot before publishing a new one.
/// Reading/recovery must succeed first, so invalid files cannot become empty data.
pub fn save<T: Serialize + DeserializeOwned>(path: &Path, value: &T) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|e| format!("Cannot encode stored data: {e}"))?;
    let _: T = serde_json::from_slice(&bytes).map_err(|_| "Invalid stored data")?;
    if load::<T>(path)?.is_some() {
        let old = fs::read(path).map_err(|e| e.to_string())?;
        replace(&backup_path(path), &old)?;
    } else {
        // A first successful save also has a recovery copy.
        replace(&backup_path(path), &bytes)?;
    }
    replace(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn interrupted_streaming_copy_preserves_destination_and_removes_its_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        fs::write(&path, b"original").unwrap();
        let result = publish(&path, |file| {
            file.write_all(b"partial")?;
            Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn backup_failure_cannot_truncate_a_healthy_primary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.json");
        fs::write(&path, b"{\"note\":\"kept\"}").unwrap();
        fs::create_dir(backup_path(&path)).unwrap();
        assert!(save(&path, &json!({"note":"lost"})).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{\"note\":\"kept\"}");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn parseable_json_with_invalid_schema_is_not_a_valid_backup() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Document {
            notes: Vec<String>,
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.json");
        fs::write(&path, b"{}").unwrap();
        fs::write(backup_path(&path), b"{}").unwrap();
        assert!(load::<Document>(&path).is_err());
        assert!(save(&path, &Document { notes: vec![] }).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{}");
    }
    #[test]
    fn previous_valid_snapshot_recovers_without_destroying_corrupt_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.json");
        save(&path, &json!({"note":"original"})).unwrap();
        save(&path, &json!({"note":"new"})).unwrap();
        fs::write(&path, b"interrupted").unwrap();
        assert_eq!(
            load::<Value>(&path).unwrap(),
            Some(json!({"note":"original"}))
        );
        assert!(fs::read_dir(dir.path())
            .unwrap()
            .any(|e| fs::read(e.unwrap().path()).unwrap() == b"interrupted"));
        assert_eq!(
            load::<Value>(&path).unwrap(),
            Some(json!({"note":"original"}))
        );
    }
    #[test]
    fn unrecoverable_data_blocks_writes_and_retains_both_originals() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.json");
        fs::write(&path, b"bad primary").unwrap();
        fs::write(backup_path(&path), b"bad backup").unwrap();
        assert!(save(&path, &json!({})).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"bad primary");
        assert_eq!(fs::read(backup_path(&path)).unwrap(), b"bad backup");
    }
    #[test]
    fn deleted_primary_does_not_reappear_and_failed_replace_preserves_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.json");
        fs::write(backup_path(&path), b"{}").unwrap();
        assert!(load::<Value>(&path).unwrap().is_none());
        fs::create_dir(&path).unwrap();
        assert!(save(&path, &json!({})).is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
