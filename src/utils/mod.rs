mod fs;
mod tree;
mod logger;
mod crypto;

pub use fs::read_secure_file;
pub use tree::{render_directory_tree, render_directory_tree_json};
pub use logger::{write_audit_log, purge_old_logs};
pub use crypto::hex_decode;

// 将简单的权限校验放在 mod.rs 中保持高轻量性
pub fn is_authorized(headers: &axum::http::HeaderMap) -> bool {
    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    auth.starts_with("Bearer ")
}
