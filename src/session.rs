//! In-memory session manager with 24h TTL, lazy cleanup, and background expiry.
//! Each session carries a CSRF token bound at creation time.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use rand::rngs::OsRng;
use rand::RngCore;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

/// Metadata stored alongside each session.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub created_at: Instant,
    pub last_seen: Instant,
    pub csrf_token: String,
}

/// Global session store.
static SESSION_STORE: OnceLock<RwLock<HashMap<String, SessionMeta>>> = OnceLock::new();

fn store() -> &'static RwLock<HashMap<String, SessionMeta>> {
    SESSION_STORE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Generate a cryptographically secure random token (Base64URL, no padding).
fn gen_random_token(byte_len: usize) -> String {
    let mut buf = vec![0u8; byte_len];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Create a new session, returning (session_id, csrf_token).
pub fn create_session() -> (String, String) {
    let session_id = gen_random_token(32);
    let csrf_token = gen_random_token(32);
    let now = Instant::now();
    let meta = SessionMeta {
        created_at: now,
        last_seen: now,
        csrf_token: csrf_token.clone(),
    };
    store().write().unwrap().insert(session_id.clone(), meta);
    (session_id, csrf_token)
}

/// Validate a session and bump `last_seen`. Returns the CSRF token if valid.
pub fn validate_session(session_id: &str) -> Option<String> {
    let mut store = store().write().unwrap();
    let meta = store.get_mut(session_id)?;
    if meta.created_at.elapsed() > Duration::from_secs(86400) {
        store.remove(session_id);
        return None;
    }
    meta.last_seen = Instant::now();
    Some(meta.csrf_token.clone())
}

/// Explicitly remove a session (logout).
pub fn invalidate_session(session_id: &str) {
    store().write().unwrap().remove(session_id);
}

/// Spawn a background task that evicts expired sessions every 30 minutes.
pub fn spawn_cleanup_task() {
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(1800)).await;
            let mut store = store().write().unwrap();
            store.retain(|_, meta| meta.created_at.elapsed() < Duration::from_secs(86400));
        }
    });
}

/// Clear all sessions (used for testing).
pub fn clear_all_sessions() {
    store().write().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_validate_session() {
        clear_all_sessions();
        let (sid, csrf) = create_session();
        assert!(!sid.is_empty());
        assert!(!csrf.is_empty());
        let stored_csrf = validate_session(&sid);
        assert_eq!(stored_csrf, Some(csrf));
    }

    #[test]
    fn test_invalidate_session() {
        clear_all_sessions();
        let (sid, _) = create_session();
        invalidate_session(&sid);
        assert!(validate_session(&sid).is_none());
    }

    #[test]
    fn test_expired_session_not_valid() {
        clear_all_sessions();
        let (sid, _) = create_session();
        // Manually set created_at to 25 hours ago by manipulating the store
        {
            let mut store = store().write().unwrap();
            if let Some(meta) = store.get_mut(&sid) {
                meta.created_at = Instant::now() - Duration::from_secs(90000); // 25h
            }
        }
        assert!(validate_session(&sid).is_none());
    }

    #[test]
    fn test_cleanup_removes_expired() {
        clear_all_sessions();
        let (sid1, _) = create_session();
        let (sid2, _) = create_session();
        // Make sid1 expired
        {
            let mut store = store().write().unwrap();
            if let Some(meta) = store.get_mut(&sid1) {
                meta.created_at = Instant::now() - Duration::from_secs(90000);
            }
        }
        // Run cleanup
        store().write().unwrap().retain(|_, meta| meta.created_at.elapsed() < Duration::from_secs(86400));
        assert!(validate_session(&sid1).is_none());
        assert!(validate_session(&sid2).is_some());
    }
}
