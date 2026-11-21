//! Stores document-keyed and table-level saved view preferences.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::document::{DocumentDisplayMode, TableLayout};

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DocumentKey {
    #[serde(with = "super::path_serde::settings_path_serde")]
    pub path: std::path::PathBuf,
    pub size: u64,
    pub sha256: [u8; 32],
}

/// View settings saved for one table, matched again by its position and name.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TablePreferences {
    /// Position in the saved file's ordered table list.
    pub table_index: usize,
    pub table_name: String,
    pub wrap_mode: super::WrapMode,
    pub view_layout: Arc<TableLayout>,
}

/// View settings saved for one file, found again by its path, size, and content hash.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DocumentPreferences {
    pub key: DocumentKey,
    pub display_mode: DocumentDisplayMode,
    pub tables: Vec<TablePreferences>,
}
