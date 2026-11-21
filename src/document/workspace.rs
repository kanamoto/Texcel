//!  Orders open documents and tracks the active document tab.

use super::*;
use std::collections::BTreeMap;

// A workspace that manages document tab order and the selected document.
#[derive(Clone, Debug, Default)]
pub struct Workspace {
    pub documents: BTreeMap<DocumentId, Document>,
    pub order: Vec<DocumentId>,
    pub active_document: Option<DocumentId>,
    next_document_id: DocumentId,
}

impl Workspace {
    // Reserves an ID for async loading. A failed load can leave a gap in the sequence.
    pub fn reserve_document_id(&mut self) -> DocumentId {
        self.next_document_id = self.next_document_id.saturating_add(1).max(1);
        self.next_document_id
    }

    // Adds the document built by the load worker at the end of the tab order and selects it.
    pub fn insert_loaded_document(&mut self, document: Document) {
        let id = document.id;
        self.next_document_id = self.next_document_id.max(id);
        self.documents.insert(id, document);
        if !self.order.contains(&id) {
            self.order.push(id);
        }
        self.active_document = Some(id);
    }

    // Returns the selected document.
    pub fn active_document(&self) -> Option<&Document> {
        self.documents.get(&self.active_document?)
    }

    // Returns a mutable reference to the selected document.
    pub fn active_document_mut(&mut self) -> Option<&mut Document> {
        self.documents.get_mut(&self.active_document?)
    }

    // Selects a document by ID. Returns `false` when the ID does not exist.
    pub fn select_document(&mut self, id: DocumentId) -> bool {
        if self.documents.contains_key(&id) {
            self.active_document = Some(id);
            true
        } else {
            false
        }
    }

    /// Removes one document and selects the nearest remaining tab if it was active.
    pub fn remove_document(&mut self, id: DocumentId) -> Option<Document> {
        let position = self.order.iter().position(|candidate| *candidate == id)?;
        self.order.remove(position);
        let document = self.documents.remove(&id)?;
        if self.active_document == Some(id) {
            self.active_document = self
                .order
                .get(position.min(self.order.len().saturating_sub(1)))
                .copied();
        }
        Some(document)
    }
}

#[cfg(test)]
impl Workspace {
    // Adds and selects a document, then returns its ID.
    pub fn add_document(&mut self, name: impl Into<String>, table_names: &[&str]) -> DocumentId {
        let id = self.reserve_document_id();
        self.documents
            .insert(id, Document::new(id, name, table_names));
        self.order.push(id);
        self.active_document = Some(id);
        id
    }
}
