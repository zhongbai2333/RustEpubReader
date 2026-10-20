//! Recoverable publication of a validated backup. Caller holds the root lease.
//! The journal is durable before any live file changes; rollback is repeatable.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const DIRECTORY: &str = ".library-restore";
const MAX_FILES: usize = 20_003;
const MAX_JOURNAL: u64 = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    path: String,
    previous: Option<String>,
    incoming: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    committed: bool,
    changes: Vec<Change>,
}
fn regular(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "Cannot inspect restore file")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Restore file must be a regular file".into());
    }
    Ok(())
}
fn directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "Cannot inspect restore directory")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Restore directory must not be a symbolic link".into());
    }
    Ok(())
}
fn target(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if !matches!(
        relative,
        "library.json" | "book-lifecycle.json" | "reading-activity.json"
    ) {
        let name = relative
            .strip_prefix("books/")
            .ok_or("Invalid restore target")?;
        let id = name
            .strip_suffix(".json")
            .or_else(|| name.strip_suffix(".epub"))
            .ok_or("Invalid restore target")?;
        if uuid::Uuid::parse_str(id).is_err() || id.contains(['/', '\\']) {
            return Err("Invalid restore identity".into());
        }
        let books = root.join("books");
        if books.exists() {
            directory(&books)?;
        }
    }
    let path = root.join(relative);
    if fs::symlink_metadata(&path).is_ok() {
        regular(&path)?;
    }
    Ok(path)
}
fn hash(path: &Path) -> Result<String, String> {
    regular(path)?;
    crate::file_hash(&path.to_string_lossy())
}
fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Cannot flush restore directory")?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn validate(root: &Path, scratch: &Path, journal: &Journal) -> Result<(), String> {
    if journal.schema_version != 1 || journal.changes.len() > MAX_FILES {
        return Err("Unsupported restore journal".into());
    }
    let mut seen = BTreeSet::new();
    for (index, change) in journal.changes.iter().enumerate() {
        target(root, &change.path)?;
        if !seen.insert(&change.path)
            || !crate::cloud_sync::valid_hash(&change.incoming)
            || change
                .previous
                .as_ref()
                .is_some_and(|h| !crate::cloud_sync::valid_hash(h))
        {
            return Err("Invalid restore journal".into());
        }
        if let Some(expected) = &change.previous {
            if hash(&scratch.join("old").join(index.to_string()))? != *expected {
                return Err("Damaged restore rollback file; original journal retained".into());
            }
        }
    }
    Ok(())
}
/// Called once after acquiring the outer process/file lease, before exposing data.
/// Failure leaves the journal intact and prevents reads from seeing a partial restore.
pub(crate) fn recover(root: &Path) -> Result<(), String> {
    let scratch = root.join(DIRECTORY);
    match fs::symlink_metadata(&scratch) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Cannot inspect interrupted restore".into()),
        Ok(_) => directory(&scratch)?,
    }
    let journal_path = scratch.join("journal.json");
    if !journal_path.exists() {
        // No live changes are allowed before the journal is published.
        fs::remove_dir_all(&scratch).map_err(|_| "Cannot remove unfinished restore staging")?;
        return sync_directory(root);
    }
    regular(&journal_path)?;
    let mut bytes = Vec::new();
    fs::File::open(&journal_path)
        .map_err(|_| "Cannot read restore journal")?
        .take(MAX_JOURNAL + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read restore journal")?;
    if bytes.len() as u64 > MAX_JOURNAL {
        return Err("Restore journal exceeds limit".into());
    }
    let journal: Journal =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid restore journal; retained")?;
    if !journal.committed {
        directory(&scratch.join("old"))?;
        validate(root, &scratch, &journal)?;
        // Originals remain in staging during rollback so another crash can repeat it.
        for (index, change) in journal.changes.iter().enumerate().rev() {
            let path = target(root, &change.path)?;
            if change.previous.is_some() {
                crate::storage::copy_file(&scratch.join("old").join(index.to_string()), &path)?;
            } else if path.exists() {
                fs::remove_file(&path).map_err(|_| "Cannot roll back restored file")?;
                sync_directory(path.parent().ok_or("Invalid restore target")?)?;
            }
        }
    } else if journal.schema_version != 1 {
        return Err("Unsupported committed restore journal".into());
    }
    // Rename the completed journal directory before cleanup. A crash during removal
    // must never leave a prepared journal whose rollback originals were removed.
    let retired = root.join(format!(".restore-retired-{}", uuid::Uuid::new_v4()));
    fs::rename(&scratch, &retired).map_err(|_| "Cannot finish restore recovery")?;
    sync_directory(root)?;
    // Cleanup failure does not make the already committed/rolled-back data invalid.
    let _ = fs::remove_dir_all(retired);
    Ok(())
}
/// Only called for a newly acquired outer lease, after transaction recovery.
/// No other cooperating operation can still own these private staging folders.
pub(crate) fn reclaim_staging(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let id = name
            .strip_prefix(".backup-staging-")
            .or_else(|| name.strip_prefix(".restore-retired-"));
        if id.is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
            && entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}
pub(super) fn publish(root: &str, updates: &[(String, PathBuf)]) -> Result<(), String> {
    publish_with(root, updates, |_| Ok(()))
}
fn publish_with(
    root: &str,
    updates: &[(String, PathBuf)],
    mut after: impl FnMut(usize) -> Result<(), String>,
) -> Result<(), String> {
    let _lease = crate::library::access::lock(root)?;
    let root = Path::new(root);
    if updates.is_empty() || updates.len() > MAX_FILES {
        return Err("Invalid restore batch".into());
    }
    let mut seen = BTreeSet::new();
    for (name, source) in updates {
        target(root, name)?;
        regular(source)?;
        if !seen.insert(name) {
            return Err("Repeated restore target".into());
        }
    }
    let scratch = root.join(DIRECTORY);
    fs::create_dir(&scratch).map_err(|_| "Cannot prepare restore transaction")?;
    let result = (|| {
        fs::create_dir(scratch.join("old")).map_err(|_| "Cannot prepare rollback storage")?;
        fs::create_dir(scratch.join("new")).map_err(|_| "Cannot prepare restore storage")?;
        let mut journal = Journal {
            schema_version: 1,
            committed: false,
            changes: Vec::new(),
        };
        for (index, (relative, source)) in updates.iter().enumerate() {
            let path = target(root, relative)?;
            let previous = if path.exists() {
                let old = scratch.join("old").join(index.to_string());
                crate::storage::copy_file(&path, &old)?;
                Some(hash(&old)?)
            } else {
                None
            };
            let incoming = scratch.join("new").join(index.to_string());
            crate::storage::copy_file(source, &incoming)?;
            journal.changes.push(Change {
                path: relative.clone(),
                previous,
                incoming: hash(&incoming)?,
            });
        }
        sync_directory(&scratch.join("old"))?;
        sync_directory(&scratch.join("new"))?;
        crate::storage::replace(
            &scratch.join("journal.json"),
            &serde_json::to_vec(&journal).map_err(|_| "Cannot encode restore journal")?,
        )?;
        sync_directory(root)?;
        validate(root, &scratch, &journal)?;
        for (index, change) in journal.changes.iter().enumerate() {
            super::progress::update(
                root.to_str().ok_or("Invalid restore root")?,
                "publishing",
                index,
                journal.changes.len(),
            );
            let source = scratch.join("new").join(index.to_string());
            if hash(&source)? != change.incoming {
                return Err("Changed restore staging file".into());
            }
            crate::storage::copy_file(&source, &target(root, &change.path)?)?;
            after(index)?;
        }
        journal.committed = true;
        crate::storage::replace(
            &scratch.join("journal.json"),
            &serde_json::to_vec(&journal).map_err(|_| "Cannot encode restore commit")?,
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            recover(root)?;
            Ok(())
        }
        Err(error) => {
            recover(root).map_err(|rollback| format!("{error}; rollback pending: {rollback}"))?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_prepared_journal_blocks_reads_and_keeps_all_recovery_material() {
        let root = tempfile::tempdir().unwrap();
        let scratch = root.path().join(DIRECTORY);
        fs::create_dir(&scratch).unwrap();
        fs::write(root.path().join("library.json"), br#"{"books":[]}"#).unwrap();
        fs::write(scratch.join("journal.json"), b"damaged journal").unwrap();
        assert!(crate::library::Library::read_snapshot(root.path().to_str().unwrap()).is_err());
        assert_eq!(
            fs::read(scratch.join("journal.json")).unwrap(),
            b"damaged journal"
        );
        assert_eq!(
            fs::read(root.path().join("library.json")).unwrap(),
            br#"{"books":[]}"#
        );
    }
    #[test]
    fn abandoned_private_staging_is_reclaimed_without_touching_other_folders() {
        let root = tempfile::tempdir().unwrap();
        let staged = root
            .path()
            .join(format!(".backup-staging-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("partial"), b"owned").unwrap();
        let other = root.path().join(".backup-staging-not-an-owned-uuid");
        fs::create_dir(&other).unwrap();
        fs::write(other.join("keep"), b"kept").unwrap();
        let _ = crate::library::Library::read_snapshot(root.path().to_str().unwrap()).unwrap();
        assert!(!staged.exists());
        assert_eq!(fs::read(other.join("keep")).unwrap(), b"kept");
    }
    #[test]
    #[ignore = "Subprocess entry exercised by abrupt_process_exit_is_rolled_back_before_the_next_library_read"]
    fn child_exit_during_publication() {
        let root = std::env::var("READER_RESTORE_CHILD_ROOT").unwrap();
        let input = Path::new(&root).join("input.json");
        let _ = publish_with(
            &root,
            &[
                ("library.json".into(), input.clone()),
                ("book-lifecycle.json".into(), input),
            ],
            |index| {
                if index == 0 {
                    std::process::exit(77);
                }
                Ok(())
            },
        );
        panic!("child should exit in the middle of publication");
    }
    #[test]
    fn abrupt_process_exit_is_rolled_back_before_the_next_library_read() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("library.json"), br#"{"books":[]}"#).unwrap();
        fs::write(root.path().join("input.json"), br#"{"changed":true}"#).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "backup::transaction::tests::child_exit_during_publication",
                "--ignored",
            ])
            .env("READER_RESTORE_CHILD_ROOT", root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(77));
        assert!(root.path().join(DIRECTORY).join("journal.json").is_file());
        assert!(fs::read_to_string(root.path().join("library.json"))
            .unwrap()
            .contains("changed"));
        assert!(
            crate::library::Library::read_snapshot(root.path().to_str().unwrap())
                .unwrap()
                .books
                .is_empty()
        );
        assert_eq!(
            fs::read(root.path().join("library.json")).unwrap(),
            br#"{"books":[]}"#
        );
        assert!(!root.path().join("book-lifecycle.json").exists());
        assert!(!root.path().join(DIRECTORY).exists());
    }
    #[test]
    fn error_after_each_published_file_rolls_back_the_entire_batch() {
        for fail_at in 0..3 {
            let root = tempfile::tempdir().unwrap();
            let staged = tempfile::tempdir().unwrap();
            let id = uuid::Uuid::new_v4();
            let names = [
                "library.json".to_owned(),
                "book-lifecycle.json".into(),
                format!("books/{id}.json"),
            ];
            fs::write(root.path().join(&names[0]), b"old catalog").unwrap();
            let updates: Vec<_> = names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let p = staged.path().join(i.to_string());
                    fs::write(&p, b"replacement").unwrap();
                    (name.clone(), p)
                })
                .collect();
            let result = publish_with(root.path().to_str().unwrap(), &updates, |index| {
                if index == fail_at {
                    Err("injected interrupted publication".into())
                } else {
                    Ok(())
                }
            });
            assert!(result.is_err());
            assert_eq!(
                fs::read(root.path().join(&names[0])).unwrap(),
                b"old catalog"
            );
            assert!(!root.path().join(&names[1]).exists());
            assert!(!root.path().join(&names[2]).exists());
            assert!(!root.path().join(DIRECTORY).exists());
        }
    }
    #[test]
    fn repeated_and_traversing_targets_are_rejected_before_publication() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("input");
        fs::write(&source, b"keep").unwrap();
        let r = root.path().to_str().unwrap();
        assert!(publish(r, &[("../library.json".into(), source.clone())]).is_err());
        assert!(publish(
            r,
            &[
                ("library.json".into(), source.clone()),
                ("library.json".into(), source)
            ]
        )
        .is_err());
        assert!(!root.path().join("library.json").exists());
    }
}
