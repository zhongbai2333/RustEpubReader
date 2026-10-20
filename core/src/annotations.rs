//! Native, portable annotations. Logical clocks establish causality; wall time is display only.
//! BookConfig remains the atomic commit boundary for both visible data and revisions.
use crate::library::{BookConfig, Bookmark, Highlight, Note};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};

mod markdown;
mod persistence;
pub use markdown::{export_markdown, MarkdownExport};
pub use persistence::{apply_remote, load_config, save_config, save_note};
#[cfg(test)]
mod tests;

pub fn new_highlight_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_CONCURRENT: usize = 32;
const MAX_ACTORS: usize = 128;
static CONFIG_LOCK: Mutex<()> = Mutex::new(());
type Clock = BTreeMap<String, u64>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Annotation {
    Bookmark(Bookmark),
    Highlight(Highlight),
    Note(Note),
}
impl Annotation {
    fn key(&self) -> String {
        match self {
            Self::Bookmark(b) => format!("b:{}:{}", b.chapter, b.block),
            Self::Highlight(h) => format!("h:{}", h.id),
            Self::Note(n) => format!("n:{}", n.highlight_id),
        }
    }
    fn valid(&self) -> bool {
        let position = |chapter, block, offset| {
            chapter <= 100_000 && block <= 10_000_000 && offset <= 100_000_000
        };
        match self {
            Self::Bookmark(b) => position(b.chapter, b.block, 0),
            Self::Highlight(h) => {
                crate::cloud_sync::valid_device_id(&h.id)
                    && position(h.chapter, h.start_block, h.start_offset)
                    && position(h.chapter, h.end_block, h.end_offset)
                    && (h.start_block, h.start_offset) < (h.end_block, h.end_offset)
            }
            Self::Note(n) => {
                crate::cloud_sync::valid_device_id(&n.highlight_id)
                    && n.content.len() <= 1024 * 1024
            }
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Revision {
    pub clock: Clock,
    pub source: String,
    pub edited_at: u64,
    pub value: Option<Annotation>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationState {
    pub schema_version: u32,
    pub entries: BTreeMap<String, Vec<Revision>>,
}
impl Default for AnnotationState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entries: BTreeMap::new(),
        }
    }
}
fn observes(a: &Clock, b: &Clock) -> bool {
    b.iter()
        .all(|(actor, counter)| a.get(actor).copied().unwrap_or(0) >= *counter)
}
fn join(a: &mut Clock, b: &Clock) {
    for (actor, counter) in b {
        a.entry(actor.clone())
            .and_modify(|v| *v = (*v).max(*counter))
            .or_insert(*counter);
    }
}
fn valid_key(key: &str) -> bool {
    let fields: Vec<_> = key.split(':').collect();
    match fields.as_slice() {
        ["b", chapter, block] => {
            chapter.parse::<usize>().is_ok_and(|c| c <= 100_000)
                && block.parse::<usize>().is_ok_and(|b| b <= 10_000_000)
                && key
                    == format!(
                        "b:{}:{}",
                        chapter.parse::<usize>().unwrap(),
                        block.parse::<usize>().unwrap()
                    )
        }
        ["h" | "n", id] => crate::cloud_sync::valid_device_id(id),
        _ => false,
    }
}
impl AnnotationState {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.entries.len() > MAX_ENTRIES {
            return Err("Unsupported or oversized annotation state".into());
        }
        for (key, versions) in &self.entries {
            if !valid_key(key) || versions.is_empty() || versions.len() > MAX_CONCURRENT {
                return Err("Invalid annotation revisions".into());
            }
            for (index, version) in versions.iter().enumerate() {
                if !crate::cloud_sync::valid_device_id(&version.source)
                    || version.clock.is_empty()
                    || version.clock.len() > MAX_ACTORS
                    || version.clock.get(&version.source).copied().unwrap_or(0) == 0
                    || version
                        .clock
                        .iter()
                        .any(|(a, c)| !crate::cloud_sync::valid_device_id(a) || *c == 0)
                    || version
                        .value
                        .as_ref()
                        .is_some_and(|v| !v.valid() || v.key() != *key)
                    || versions[..index].iter().any(|v| {
                        observes(&v.clock, &version.clock) || observes(&version.clock, &v.clock)
                    })
                {
                    return Err("Invalid or redundant annotation revision".into());
                }
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| "Cannot encode annotations")?
            .len()
            > MAX_BYTES
        {
            return Err("Annotations exceed size limit".into());
        }
        Ok(())
    }
    /// Deterministic union of causal maxima. Deletion is retained as a first-class revision.
    pub fn merge(&mut self, other: &Self) -> Result<bool, String> {
        self.validate()?;
        other.validate()?;
        let before = self.clone();
        let mut next = self.clone();
        for (key, incoming) in &other.entries {
            let existing = next.entries.entry(key.clone()).or_default();
            for version in incoming {
                if existing
                    .iter()
                    .any(|old| old.clock == version.clock && old != version)
                {
                    return Err("Conflicting annotation revision identity".into());
                }
                if existing
                    .iter()
                    .any(|old| observes(&old.clock, &version.clock))
                {
                    continue;
                }
                existing.retain(|old| !observes(&version.clock, &old.clock));
                existing.push(version.clone());
            }
            existing.sort_by_key(|v| serde_json::to_string(v).unwrap_or_default());
        }
        next.validate()?;
        let changed = next != before;
        *self = next;
        Ok(changed)
    }
    /// Upgrade legacy vectors deterministically without publishing an edit or reading the clock.
    fn seed(&mut self, flat: &BTreeMap<String, Annotation>) -> Result<(), String> {
        for (key, value) in flat {
            if self.entries.contains_key(key) {
                continue;
            }
            if !value.valid() {
                return Err("Invalid local annotation".into());
            }
            let bytes = serde_json::to_vec(value).map_err(|_| "Cannot encode annotation")?;
            let actor = format!("legacy-{}", &crate::bytes_hash(&bytes)[..32]);
            self.entries.insert(
                key.clone(),
                vec![Revision {
                    clock: BTreeMap::from([(actor.clone(), 1)]),
                    source: actor,
                    edited_at: 0,
                    value: Some(value.clone()),
                }],
            );
        }
        self.validate()
    }
    fn edit(
        &mut self,
        key: &str,
        value: Option<Annotation>,
        observed: Option<&Vec<Revision>>,
        actor: &str,
        now: u64,
        counter_floor: u64,
    ) -> Result<(), String> {
        let mut clock = Clock::new();
        if let Some(observed) = observed {
            for revision in observed {
                join(&mut clock, &revision.clock);
            }
        }
        // An actor can have multiple stale UI snapshots. Never issue the same dot twice.
        let current_counter = self
            .entries
            .get(key)
            .into_iter()
            .flatten()
            .filter_map(|v| v.clock.get(actor))
            .copied()
            .max()
            .unwrap_or(0);
        let next = clock
            .get(actor)
            .copied()
            .unwrap_or(0)
            .max(current_counter)
            .max(counter_floor)
            .checked_add(1)
            .ok_or("Annotation revision exhausted")?;
        clock.insert(actor.into(), next);
        let delta = Self {
            schema_version: 1,
            entries: BTreeMap::from([(
                key.into(),
                vec![Revision {
                    clock,
                    source: actor.into(),
                    edited_at: now,
                    value,
                }],
            )]),
        };
        self.merge(&delta)?;
        Ok(())
    }
    pub fn materialize(&self, config: &mut BookConfig) {
        config.bookmarks.clear();
        config.highlights.clear();
        config.notes.clear();
        for versions in self.entries.values() {
            // Concurrent delete wins. A later deliberate add observes and supersedes the delete.
            if versions.iter().any(|v| v.value.is_none()) {
                continue;
            }
            let Some(value) = versions.last().and_then(|v| v.value.as_ref()) else {
                continue;
            };
            match value {
                Annotation::Bookmark(b) => config.bookmarks.push(b.clone()),
                Annotation::Highlight(h) => config.highlights.push(h.clone()),
                Annotation::Note(n) => {
                    let mut note = n.clone();
                    let mut contents = Vec::new();
                    for version in versions {
                        if let Some(Annotation::Note(n)) = &version.value {
                            if !contents.contains(&n.content) {
                                contents.push(n.content.clone());
                            }
                        }
                    }
                    note.content = contents.join("\n\n---\n\n");
                    config.notes.push(note);
                }
            }
        }
        config
            .notes
            .retain(|n| config.highlights.iter().any(|h| h.id == n.highlight_id));
        config.bookmarks.sort_by_key(|b| (b.chapter, b.block));
        config
            .highlights
            .sort_by_key(|h| (h.chapter, h.start_block, h.start_offset, h.id.clone()));
    }
}
fn flat(config: &BookConfig) -> Result<BTreeMap<String, Annotation>, String> {
    let mut result = BTreeMap::new();
    for value in config
        .bookmarks
        .iter()
        .cloned()
        .map(Annotation::Bookmark)
        .chain(config.highlights.iter().cloned().map(Annotation::Highlight))
        .chain(config.notes.iter().cloned().map(Annotation::Note))
    {
        if result.insert(value.key(), value).is_some() {
            return Err("Invalid or duplicate local annotation".into());
        }
    }
    Ok(result)
}
fn snapshot(config: &mut BookConfig) -> Result<(), String> {
    config.persisted = Some(Arc::new(
        serde_json::to_value(&*config).map_err(|_| "Cannot encode book configuration")?,
    ));
    Ok(())
}
fn config_path(root: &str, id: &str) -> Result<std::path::PathBuf, String> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err("Invalid book identity".into());
    }
    Ok(Path::new(root).join("books").join(format!("{id}.json")))
}

/// Display-only conflict markers. Never infer concurrency from user-authored note text.
pub fn concurrent_note_ids(config: &BookConfig) -> Vec<&str> {
    config
        .notes
        .iter()
        .filter_map(|note| {
            config
                .annotation_state
                .entries
                .get(&format!("n:{}", note.highlight_id))
                .is_some_and(|versions| versions.len() > 1)
                .then_some(note.highlight_id.as_str())
        })
        .collect()
}

/// Read-only native presentation payload, distinct from the persisted BookConfig.
pub fn view(config: &BookConfig) -> Value {
    serde_json::json!({"highlights":config.highlights,"notes":config.notes,
        "bookmarks":config.bookmarks,"corrections":config.corrections,
        "concurrent_notes":concurrent_note_ids(config)})
}

/// Shared annotation tools bridge contract. UI-specific mutation adapters still use BookConfig saves.
pub fn command(root: &str, request: &Value) -> Result<Value, String> {
    let id = request["id"]
        .as_str()
        .ok_or("Missing annotation book identity")?;
    match request["action"].as_str() {
        Some("export") => serde_json::to_value(export_markdown(root, id)?)
            .map_err(|_| "Cannot encode Markdown export".into()),
        Some("note") => {
            let highlight = request["highlight_id"]
                .as_str()
                .ok_or("Missing highlight identity")?;
            let content = request["content"].as_str().ok_or("Missing note content")?;
            let original = match request.get("original_note") {
                None => None,
                Some(Value::Null) => Some(None),
                Some(Value::String(text)) => Some(Some(text.as_str())),
                _ => return Err("Invalid original note".into()),
            };
            let config = save_note(root, id, highlight, content, original)?;
            Ok(view(&config))
        }
        Some("details") => {
            let key = request["key"]
                .as_str()
                .filter(|key| valid_key(key))
                .ok_or("Invalid annotation identity")?;
            let config = load_config(root, id)?.ok_or("Book is no longer available")?;
            let local = persistence::current_actor_id(root);
            let revisions = config
                .annotation_state
                .entries
                .get(key)
                .ok_or("Annotation is no longer available")?;
            Ok(
                serde_json::json!({"key":key,"concurrent":revisions.len()>1,"revisions":revisions.iter().map(|v|{
                let legacy=v.source.starts_with("legacy-");
                serde_json::json!({"source":if legacy {None}else{Some(&v.source)},"local":local.as_deref().is_some_and(|id|v.source==id || v.source.starts_with(&format!("{id}_"))),"edited_at":if v.edited_at==0 {None}else{Some(v.edited_at)},"value":v.value})
            }).collect::<Vec<_>>()}),
            )
        }
        _ => Err("Unknown annotation request".into()),
    }
}
