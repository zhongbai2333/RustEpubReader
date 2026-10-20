use super::*;

#[derive(Serialize, Deserialize)]
struct Actor {
    id: String,
    counter: u64,
}
pub(super) fn current_actor_id(root: &str) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(Path::new(root).join("annotation-device.json")).ok()?;
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    // Display-only reads never recover the counter; recovery must rotate its identity.
    let actor: Actor = serde_json::from_slice(&bytes).ok()?;
    crate::cloud_sync::valid_device_id(&actor.id).then_some(actor.id)
}
fn reserve_revision(root: &str, floor: u64) -> Result<(String, u64), String> {
    let path = Path::new(root).join("annotation-device.json");
    // Recovered identity counters could have already been published. Rotate the identity
    // when recovering a damaged primary rather than issuing a previously used revision.
    let damaged = std::fs::read(&path)
        .ok()
        .is_some_and(|bytes| serde_json::from_slice::<Actor>(&bytes).is_err());
    let stored = crate::storage::load::<Actor>(&path)?;
    let mut actor = if damaged {
        Actor {
            id: uuid::Uuid::new_v4().to_string(),
            counter: 0,
        }
    } else {
        stored.unwrap_or_else(|| Actor {
            id: uuid::Uuid::new_v4().to_string(),
            counter: 0,
        })
    };
    if !crate::cloud_sync::valid_device_id(&actor.id) {
        return Err("Invalid annotation device identity".into());
    }
    actor.counter = actor
        .counter
        .max(floor)
        .checked_add(1)
        .ok_or("Annotation revision exhausted")?;
    // Reserve before committing the annotation. A failed commit may skip a counter,
    // but restoring an older book snapshot must never reuse an already published dot.
    crate::storage::save(&path, &actor)?;
    Ok((actor.id, actor.counter))
}
fn normalize_legacy(config: &mut BookConfig) -> Result<(), String> {
    if !config.annotation_state.is_empty() {
        return Ok(());
    }
    config.annotation_state.validate()?;
    // Historical timestamp-based IDs could collide within one second. Preserve
    // each highlight with a deterministic identity shared by all upgrades.
    let mut seen = std::collections::BTreeSet::new();
    for (index, highlight) in config.highlights.iter_mut().enumerate() {
        if seen.insert(highlight.id.clone()) {
            continue;
        }
        let fingerprint = crate::bytes_hash(
            &serde_json::to_vec(&(index, &*highlight))
                .map_err(|_| "Cannot identify legacy highlight")?,
        );
        highlight.id = format!("legacy-{}", &fingerprint[..48]);
        if !seen.insert(highlight.id.clone()) {
            return Err("Legacy annotation identity collision".into());
        }
    }
    config
        .bookmarks
        .sort_by_key(|b| (b.chapter, b.block, b.created_at));
    config.bookmarks.dedup_by_key(|b| (b.chapter, b.block));
    let mut notes: BTreeMap<String, Note> = BTreeMap::new();
    for note in std::mem::take(&mut config.notes) {
        if let Some(previous) = notes.get_mut(&note.highlight_id) {
            if previous.content != note.content {
                previous.content.push_str("\n\n---\n\n");
                previous.content.push_str(&note.content);
            }
            previous.created_at = previous.created_at.min(note.created_at);
            previous.updated_at = previous.updated_at.max(note.updated_at);
        } else {
            notes.insert(note.highlight_id.clone(), note);
        }
    }
    config.notes = notes.into_values().collect();
    Ok(())
}
fn load_unlocked(root: &str, id: &str) -> Result<Option<BookConfig>, String> {
    let Some(mut config) = crate::storage::load::<BookConfig>(&config_path(root, id)?)? else {
        return Ok(None);
    };
    if config.id != id {
        return Err("Mismatched book identity".into());
    }
    normalize_legacy(&mut config)?;
    let legacy = flat(&config)?;
    config.annotation_state.seed(&legacy)?;
    config.annotation_state.clone().materialize(&mut config);
    snapshot(&mut config)?;
    Ok(Some(config))
}
pub fn load_config(root: &str, id: &str) -> Result<Option<BookConfig>, String> {
    let _access = crate::library::access::lock(root)?;
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load_unlocked(root, id)
}
/// Merge changed fields into the latest persisted configuration while holding the writer lock.
/// An unsaved edit never enters the sync journal. Failed saves leave the caller's baseline intact.
pub fn save_config(root: &str, desired: &mut BookConfig) -> Result<(), String> {
    let _access = crate::library::access::lock(root)?;
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = config_path(root, &desired.id)?;
    let current = load_unlocked(root, &desired.id)?;
    let base: Option<BookConfig> = desired
        .persisted
        .as_deref()
        .map(|value| {
            serde_json::from_value(value.clone()).map_err(|_| "Invalid loaded book snapshot")
        })
        .transpose()?;
    if current.is_none() && base.is_some() {
        return Err("Book configuration was removed; reload the library".into());
    }
    let desired_value =
        serde_json::to_value(&*desired).map_err(|_| "Cannot encode book configuration")?;
    let mut output = if let Some(current) = current.as_ref() {
        let mut value =
            serde_json::to_value(current).map_err(|_| "Cannot encode book configuration")?;
        let fields: std::collections::BTreeSet<_> = desired_value
            .as_object()
            .ok_or("Invalid book configuration")?
            .keys()
            .chain(
                desired
                    .persisted
                    .as_deref()
                    .and_then(Value::as_object)
                    .into_iter()
                    .flat_map(|b| b.keys()),
            )
            .collect();
        for key in fields {
            if matches!(
                key.as_str(),
                "annotation_state" | "bookmarks" | "highlights" | "notes"
            ) {
                continue;
            }
            let field = desired_value.get(key);
            if desired
                .persisted
                .as_ref()
                .is_none_or(|base| base.get(key) != field)
            {
                if let Some(field) = field {
                    value[key] = field.clone();
                } else {
                    value
                        .as_object_mut()
                        .ok_or("Invalid book configuration")?
                        .remove(key);
                }
            }
        }
        serde_json::from_value::<BookConfig>(value)
            .map_err(|_| "Invalid merged book configuration")?
    } else {
        desired.clone()
    };
    let original = base.as_ref().map(flat).transpose()?.unwrap_or_default();
    let changed = flat(desired)?;
    let mut state = current
        .as_ref()
        .map(|c| c.annotation_state.clone())
        .unwrap_or_default();
    // A brand-new/imported configuration may already carry a validated portable state.
    state.merge(&desired.annotation_state)?;
    let keys: std::collections::BTreeSet<_> = original.keys().chain(changed.keys()).collect();
    for key in keys {
        if original.get(key) == changed.get(key) {
            continue;
        }
        let floor = state
            .entries
            .get(key)
            .into_iter()
            .flatten()
            .flat_map(|r| r.clock.values())
            .copied()
            .max()
            .unwrap_or(0);
        let (id, sequence) = reserve_revision(root, floor)?;
        let observed = base
            .as_ref()
            .and_then(|b| b.annotation_state.entries.get(key));
        state.edit(
            key,
            changed.get(key).cloned(),
            observed,
            &id,
            crate::now_secs(),
            sequence - 1,
        )?;
    }
    state.validate()?;
    state.materialize(&mut output);
    output.annotation_state = state;
    crate::storage::save(&path, &output)?;
    snapshot(&mut output)?;
    *desired = output;
    Ok(())
}
/// Apply remote causal state without generating a local edit or touching reading progress.
/// Missing books are retained by the transport cache until their file is downloaded/restored.
pub fn apply_remote(root: &str, id: &str, remote: &AnnotationState) -> Result<bool, String> {
    let _access = crate::library::access::lock(root)?;
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    remote.validate()?;
    let Some(mut config) = load_unlocked(root, id)? else {
        return Ok(false);
    };
    let mut state = config.annotation_state.clone();
    if !state.merge(remote)? {
        return Ok(false);
    }
    state.materialize(&mut config);
    config.annotation_state = state;
    crate::storage::save(&config_path(root, id)?, &config)?;
    Ok(true)
}

/// An editor carries the note it actually saw. New remote text arriving while the
/// draft is open becomes a concurrent version, rather than being silently observed.
/// None is the legacy API; Some(None) means the editor saw no note.
pub fn save_note(
    root: &str,
    id: &str,
    highlight_id: &str,
    content: &str,
    original: Option<Option<&str>>,
) -> Result<BookConfig, String> {
    let _access = crate::library::access::lock(root)?;
    let _guard = CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if content.len() > 32 * 1024 {
        return Err("Note is too long".into());
    }
    let mut config = load_unlocked(root, id)?.ok_or("Book is no longer available")?;
    if !config.highlights.iter().any(|h| h.id == highlight_id) {
        return Err("Highlight is no longer available".into());
    }
    let previous = config.notes.iter().find(|n| n.highlight_id == highlight_id);
    let stale = original.is_some_and(|expected| expected != previous.map(|n| n.content.as_str()));
    if stale
        && config
            .annotation_state
            .entries
            .get(&format!("n:{highlight_id}"))
            .is_some_and(|versions| versions.iter().any(|v| v.value.is_none()))
    {
        return Err("Note was deleted while editing; retain the draft and reopen the note".into());
    }
    let now = crate::now_secs();
    let note = Note {
        highlight_id: highlight_id.into(),
        content: content.into(),
        created_at: previous.map_or(now, |n| n.created_at),
        updated_at: now,
    };
    let key = format!("n:{highlight_id}");
    let mut state = config.annotation_state.clone();
    let floor = state
        .entries
        .get(&key)
        .into_iter()
        .flatten()
        .flat_map(|r| r.clock.values())
        .copied()
        .max()
        .unwrap_or(0);
    let (actor, sequence) = reserve_revision(root, floor)?;
    // A stale draft is a separate causal branch, including two drafts on one install.
    // The prefix still identifies this installation in the details UI.
    let source = if stale {
        format!("{actor}_{}", uuid::Uuid::new_v4())
    } else {
        actor
    };
    let observed = if stale {
        None
    } else {
        config.annotation_state.entries.get(&key)
    };
    state.edit(
        &key,
        if content.trim().is_empty() {
            None
        } else {
            Some(Annotation::Note(note))
        },
        observed,
        &source,
        now,
        sequence - 1,
    )?;
    state.materialize(&mut config);
    config.annotation_state = state;
    crate::storage::save(&config_path(root, id)?, &config)?;
    snapshot(&mut config)?;
    Ok(config)
}
