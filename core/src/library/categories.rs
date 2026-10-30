//! Device-local shelf organization. Book UUIDs survive rename and archive/restore.
use super::Library;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Category {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Classification {
    pub categories: Vec<Category>,
    pub assignments: BTreeMap<String, String>,
}

impl Classification {
    pub fn is_empty(&self) -> bool {
        self.categories.is_empty() && self.assignments.is_empty()
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.categories.len() > 200 || self.assignments.len() > 100_000 {
            return Err("category.error_limit".into());
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for category in &self.categories {
            if uuid::Uuid::parse_str(&category.id).is_err()
                || !ids.insert(category.id.as_str())
                || valid_name(&category.name)? != category.name
                || !names.insert(category.name.to_lowercase())
            {
                return Err("category.error_name".into());
            }
        }
        if self.assignments.iter().any(|(id, category)| {
            uuid::Uuid::parse_str(id).is_err() || !ids.contains(category.as_str())
        }) {
            return Err("category.error_missing".into());
        }
        Ok(())
    }

    /// None = all books; empty = uncategorized; otherwise a stable category UUID.
    pub fn matches(&self, book: &str, filter: Option<&str>) -> bool {
        match filter {
            None => true,
            Some("") => !self.assignments.contains_key(book),
            Some(id) => self.assignments.get(book).is_some_and(|value| value == id),
        }
    }
}

fn valid_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err("category.error_name".into());
    }
    Ok(name.to_owned())
}

/// One locked read/modify/write for both native bridges and desktop. A stale UI
/// never overwrites a concurrent reading-progress/import/archive update.
pub fn command(root: &str, request: &Value) -> Result<Value, String> {
    let _lease = super::access::lock(root)?;
    let mut library = Library::load_from(root)?;
    let state = &mut library.classification;
    state.validate()?;
    let action = request["action"].as_str().unwrap_or("list");
    match action {
        "list" => return Ok(json!(state)),
        "create" | "rename" => {
            let name = valid_name(request["name"].as_str().unwrap_or_default())?;
            let id = request["id"].as_str().unwrap_or_default();
            if state.categories.iter().any(|c| {
                (action == "create" || c.id != id) && c.name.to_lowercase() == name.to_lowercase()
            }) {
                return Err("category.error_duplicate".into());
            }
            if action == "create" {
                if state.categories.len() >= 200 {
                    return Err("category.error_limit".into());
                }
                state.categories.push(Category {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                });
            } else {
                state
                    .categories
                    .iter_mut()
                    .find(|c| c.id == id)
                    .ok_or("category.error_missing")?
                    .name = name;
            }
        }
        "delete" => {
            let id = request["id"].as_str().ok_or("category.error_missing")?;
            if !state.categories.iter().any(|c| c.id == id) {
                return Err("category.error_missing".into());
            }
            state.categories.retain(|c| c.id != id);
            state.assignments.retain(|_, value| value != id);
        }
        "assign" => {
            let category = request["id"].as_str().unwrap_or_default();
            if !category.is_empty() && !state.categories.iter().any(|c| c.id == category) {
                return Err("category.error_missing".into());
            }
            let ids = request["books"]
                .as_array()
                .filter(|ids| !ids.is_empty() && ids.len() <= 100_000)
                .ok_or("category.error_books")?;
            let ids: Vec<_> = ids
                .iter()
                .map(|id| {
                    id.as_str()
                        .filter(|id| library.books.iter().any(|b| b.id == *id))
                        .ok_or("category.error_books")
                })
                .collect::<Result<_, _>>()?;
            for id in ids {
                if category.is_empty() {
                    state.assignments.remove(id);
                } else {
                    state.assignments.insert(id.to_owned(), category.to_owned());
                }
            }
        }
        _ => return Err("Unknown category action".into()),
    }
    state.categories.sort_by_key(|c| c.name.to_lowercase());
    let result = json!(state);
    library.save_to(root)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_catalog_and_atomic_assignment_validation() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().to_str().unwrap();
        std::fs::write(super::super::library_path(root), r#"{"books":[]}"#).unwrap();
        assert_eq!(
            command(root, &json!({})).unwrap(),
            json!(Classification::default())
        );
        let state = command(root, &json!({"action":"create","name":"  Fiction  "})).unwrap();
        let id = state["categories"][0]["id"].as_str().unwrap();
        assert!(command(root, &json!({"action":"create","name":"fiction"})).is_err());
        assert!(command(root, &json!({"action":"create","id":id,"name":"fiction"})).is_err());
        for name in ["", "\n", "a\nb", &"a".repeat(41)] {
            assert!(command(root, &json!({"action":"rename","id":id,"name":name})).is_err());
        }
        let before = std::fs::read(super::super::library_path(root)).unwrap();
        assert!(command(
            root,
            &json!({"action":"assign","id":id,"books":["missing"]})
        )
        .is_err());
        assert_eq!(
            before,
            std::fs::read(super::super::library_path(root)).unwrap()
        );
        command(root, &json!({"action":"delete","id":id})).unwrap();
        assert!(Library::load_from(root)
            .unwrap()
            .classification
            .categories
            .is_empty());
    }

    #[test]
    fn categories_survive_stale_progress_writes_and_archive_restore() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().to_str().unwrap();
        let mut library = Library::default();
        let book = library
            .add_or_update_from_bytes(root, "Book".into(), b"book", 0, None)
            .unwrap();
        let category = command(root, &json!({"action":"create","name":"小说"})).unwrap()
            ["categories"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        command(
            root,
            &json!({"action":"assign","id":category,"books":[book.id]}),
        )
        .unwrap();
        library.rename_book(root, &book.id, "Renamed").unwrap();
        library.hide_by_path(root, &book.path).unwrap();
        library.restore_entry(root, book.clone()).unwrap();
        let state = Library::load_from(root).unwrap().classification;
        assert!(state.matches(&book.id, Some(&category)));
        assert!(!state.matches(&book.id, Some("")));
        assert!(state.matches(&book.id, None));
        command(root, &json!({"action":"delete","id":category})).unwrap();
        let library = Library::load_from(root).unwrap();
        assert_eq!(library.books.len(), 1);
        assert!(library.classification.matches(&book.id, Some("")));
        assert!(std::path::Path::new(&book.path).exists());
    }
}
