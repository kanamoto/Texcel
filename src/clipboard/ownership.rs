use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::*;

pub(super) const PAYLOAD_PREFIX: &str = "<!--texcel-payload:";

pub(super) const PAYLOAD_SUFFIX: &str = "-->";

pub(super) static NEXT_OWNER_ID: AtomicU64 = AtomicU64::new(1);

pub(super) static OWNED_PAYLOAD: OnceLock<Mutex<Option<OwnedPayloadMark>>> = OnceLock::new();

#[derive(Clone, Debug)]
pub(super) struct OwnedPayloadMark {
    pub(super) owner_id: String,
    pub(super) html_digest: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(super) struct InternalPayload {
    pub(super) version: u8,
    pub(super) owner_id: String,
    pub(super) text: String,
    pub(super) ranges: Vec<Vec<Vec<ClipboardCell>>>,
}

/// Reads Texcel-owned clipboard data from the system clipboard.
pub(super) fn read_internal_payload(html: &str) -> Option<InternalPayload> {
    let start = html.find(PAYLOAD_PREFIX)? + PAYLOAD_PREFIX.len();
    let end = html[start..].find(PAYLOAD_SUFFIX)? + start;
    let encoded = &html[start..end];
    if encoded.len() > MAX_CLIPBOARD_BYTES {
        return None;
    }
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).ok()?;
    if bytes.len() > MAX_CLIPBOARD_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// Creates the next unique clipboard owner ID.
pub(super) fn next_owner_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed);
    format!("{timestamp:x}-{sequence:x}")
}

/// Checks whether an owner ID is valid and current.
pub(super) fn valid_owner_id(owner_id: &str) -> bool {
    !owner_id.is_empty()
        && owner_id.len() <= 128
        && owner_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Stores clipboard data and its ownership marker.
pub(super) fn set_owned_payload(mark: Option<OwnedPayloadMark>) {
    if let Ok(mut current) = OWNED_PAYLOAD.get_or_init(|| Mutex::new(None)).lock() {
        *current = mark;
    }
}

/// Returns the owner ID for the current HTML clipboard payload.
pub(super) fn owned_html_owner_id(html: &str) -> Option<String> {
    let current = OWNED_PAYLOAD
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|current| current.clone())?;
    owned_html_owner_id_for_mark(html, &current)
}

/// Returns an owner ID only when the HTML marker matches.
pub(super) fn owned_html_owner_id_for_mark(html: &str, mark: &OwnedPayloadMark) -> Option<String> {
    let digest: [u8; 32] = Sha256::digest(html.as_bytes()).into();
    (digest == mark.html_digest).then(|| mark.owner_id.clone())
}
