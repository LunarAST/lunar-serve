use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ed25519_dalek::SigningKey;
use std::path::PathBuf;

// ---- NEW IMPORTS for AI raw handler ----
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use tokio::fs;
// ----------------------------------------

// ---------------------------------------------------------------------------
// Original types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SourceType { Github, Local }

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GithubSource { pub owner: String, pub repo: String, pub branch: String }

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSource {
    pub r#type: SourceType,
    pub github: Option<GithubSource>,
    #[serde(rename = "archiveUrl")]
    pub archive_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProjectRegistryEntry {
    Simple(String),
    #[serde(rename_all = "camelCase")]
    Detailed {
        name: String,
        display_name: Option<String>,
        source: Option<ProjectSource>,
        visibility: Option<String>,
        path: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReposConfig { pub version: String, pub projects: Vec<ProjectRegistryEntry> }

#[derive(Debug, Clone)]
pub struct ProjectMeta {
    pub display_name: String,
    pub github: Option<GithubSource>,
    pub visibility: String,
    pub archive_url: Option<String>,
    pub path: Option<String>,
}

pub struct ProjectIndex {
    by_name: HashMap<String, ProjectMeta>,
    by_github_path: HashMap<String, String>,
}

impl ProjectIndex {
    pub fn from_config(config: &ReposConfig) -> Self {
        let mut by_name = HashMap::new();
        let mut by_github_path = HashMap::new();
        for entry in &config.projects {
            let (name, display, source, visibility, path) = match entry {
                ProjectRegistryEntry::Simple(name) => (name.clone(), name.clone(), None, "public".to_string(), None),
                ProjectRegistryEntry::Detailed { name, display_name, source, visibility, path } => {
                    (name.clone(), display_name.clone().unwrap_or_else(|| name.clone()), source.clone(), visibility.clone().unwrap_or_else(|| "public".to_string()), path.clone())
                }
            };
            let github = source.as_ref().and_then(|s| if s.r#type == SourceType::Github { s.github.clone() } else { None });
            let archive_url = source.as_ref().and_then(|s| s.archive_url.clone());
            if let Some(ref gh) = github {
                let key = format!("{}/{}/{}", gh.owner, gh.repo, gh.branch).to_lowercase();
                by_github_path.insert(key, name.clone());
            }
            by_name.insert(name, ProjectMeta { display_name: display, github, visibility, archive_url, path });
        }
        Self { by_name, by_github_path }
    }

    pub fn get_name_by_github(&self, owner: &str, repo: &str, branch: &str) -> Option<&str> {
        let key = format!("{}/{}/{}", owner, repo, branch).to_lowercase();
        self.by_github_path.get(&key).map(|s| s.as_str())
    }

    pub fn get_meta(&self, name: &str) -> Option<&ProjectMeta> { self.by_name.get(name) }
}

pub fn load_repos(base_dir: &std::path::Path) -> ReposConfig {
    let path = base_dir.join("repos.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<ReposConfig>(&content) { return config; }
        }
    }
    ReposConfig { version: "0.5.0".to_string(), projects: vec![] }
}

// ---------------------------------------------------------------------------
// v3.0 Security modules – re-exported for convenience
// ---------------------------------------------------------------------------

pub mod session;
pub mod lct;
pub mod totp;
pub mod patch;
pub mod utils;
pub mod render;
pub mod handlers;

pub use session::{create_session, validate_session, invalidate_session, spawn_cleanup_task};
pub use lct::{LctPayload, generate_lct, verify_lct, load_signing_key};
pub use totp::verify_totp;
pub use patch::{parse_lunar_patch, ParsedPatch};
pub use utils::*;

// ---------------------------------------------------------------------------
// Shared application state (used by all route handlers)
// ---------------------------------------------------------------------------

pub struct AppState {
    pub data_path: PathBuf,
    pub project_index: ProjectIndex,
    pub signing_key: SigningKey,
}

// ---------------------------------------------------------------------------
// Shared helper functions (used across handler modules)
// ---------------------------------------------------------------------------

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response, Json};

pub fn make_error_response(status: StatusCode, error_msg: &str, hint: &str) -> Response {
    let body = serde_json::json!({ "error": error_msg, "hint": hint });
    let mut resp = (status, Json(body)).into_response();
    resp.headers_mut().insert("X-Lunar-Diagnostic", axum::http::HeaderValue::from_str(hint).unwrap_or(axum::http::HeaderValue::from_static("error")));
    resp
}

use lunar_interface::LunarMap;

pub fn load_map(path: &std::path::Path) -> Result<LunarMap, (StatusCode, String)> {
    let content = std::fs::read_to_string(path)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read lunar-map.json".into()))?;
    serde_json::from_str(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid JSON: {}", e)))
}

// ---------------------------------------------------------------------------
// Application router builder (extracted so integration tests can use it)
// ---------------------------------------------------------------------------

use axum::{routing::{get, post}, Router};
use std::sync::Arc;

pub fn build_app(state: Arc<AppState>) -> Router {
    use crate::handlers::{
        get_index, get_json, get_markdown,
        get_project_md_api, get_project_md_github,
        get_project_md_legacy, get_private_project_md,
        healthz,
        get_raw_file_api, get_raw_file_github,
        get_project_todo, post_project_todo, get_project_todo_diff,
        handle_setup, handle_setup_post, handle_login, handle_csrf_token,
        handle_token_generate, handle_dispatch,
    };

    Router::new()
        .route("/", get(get_index))
        .route("/lunar-map.json", get(get_json))
        .route("/lunar-map.md", get(get_markdown))
        .route("/api/v1/projects/:name/map", get(get_project_md_api))
        .route("/api/v1/projects/:name/raw/*filepath", get(get_raw_file_api))
        .route("/api/v1/projects/:name/todo", get(get_project_todo).post(post_project_todo))
        .route("/api/v1/projects/:name/todo/diff", get(get_project_todo_diff))
        .route("/:owner/:repo/tree/:branch", get(get_project_md_github))
        .route("/:owner/:repo/raw/:branch/*filepath", get(get_raw_file_github))
        .route("/:owner/:repo/blob/:branch/*filepath", get(get_raw_file_github))
        .route("/project/:name", get(get_project_md_legacy))
        .route("/private/project/:name", get(get_private_project_md))
        .route("/healthz", get(healthz))
        // v3.0 Security routes
        .route("/setup", get(handle_setup).post(handle_setup_post))
        .route("/login", post(handle_login))
        .route("/csrf-token", get(handle_csrf_token))
        .route("/token/generate", post(handle_token_generate))
        .route("/dispatch", post(handle_dispatch))
        // AI tree routes – order matters: more specific (with /*rest) MUST come first
        .route("/t/:token/:owner/:repo/tree/:branch/*rest", get(crate::handle_ai_tree_file))
        .route("/t/:token/:owner/:repo/tree/:branch", get(crate::handle_ai_tree_root))
        // AI raw and blob routes
        .route("/t/:token/:owner/:repo/raw/:branch/*filepath", get(crate::handle_ai_raw_file))
        .route("/t/:token/:owner/:repo/blob/:branch/*filepath", get(crate::handle_ai_raw_file))
        .with_state(state)
}

// ============================================================================
// NEW: AI Read‑only Raw/Blob Handler (LCT token in URL)
// ============================================================================

/// Path parameters for AI raw/blob requests.
#[derive(serde::Deserialize)]
pub struct AiFileParams {
    pub token: String,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub filepath: String,
}

/// Simple inline MIME type mapping (no external crate)
fn guess_content_type(path: &PathBuf) -> &'static str {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" | "log" | "md" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        _ => "application/octet-stream",
    }
}

/// Internal function to serve a file from a repository given validated parameters.
async fn serve_file_from_repo(
    state: &AppState,
    params: &AiFileParams,
) -> Response {
    let verifying_key = state.signing_key.verifying_key();
    let _payload = match verify_lct(
        &params.token,
        &verifying_key,
        Some(&params.owner),
        Some(&params.repo),
        Some(&params.branch),
    ) {
        Ok(p) => p,
        Err(e) => return make_error_response(
            StatusCode::UNAUTHORIZED,
            "Invalid or mismatched token",
            &e,
        ),
    };

    // 使用 fallback 逻辑查找项目名（与 tree root 一致）
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((s, e)) => return make_error_response(s, &e, ""),
    };
    let project_name = match state.project_index.get_name_by_github(
        &params.owner,
        &params.repo,
        &params.branch,
    ) {
        Some(name) => name.to_string(),
        None => {
            match map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&params.repo)) {
                Some(p) => p.name.clone(),
                None => return make_error_response(
                    StatusCode::NOT_FOUND,
                    "Repository not found in index",
                    "Check owner/repo/branch",
                ),
            }
        }
    };

    let meta = match state.project_index.get_meta(&project_name) {
        Some(m) => m,
        None => return make_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Project meta missing",
            "Index inconsistency",
        ),
    };

    let repo_root = match &meta.path {
        Some(p) => PathBuf::from(p),
        None => return make_error_response(
            StatusCode::NOT_FOUND,
            "Repository not cloned locally",
            "No path field",
        ),
    };

    // ✅ FIX: Axum 的通配符 (*filepath) 会带有前导斜杠 '/'
    // 在 Rust 中，Path::join 拼接绝对路径时会直接丢弃并覆盖 repo_root。
    // 我们必须剔除前导斜杠以确保安全的相对路径拼接。
    let clean_filepath = params.filepath.trim_start_matches('/');
    let file_path = repo_root.join(clean_filepath);

    let canonical_root = match repo_root.canonicalize() {
        Ok(p) => p,
        Err(_) => return make_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot resolve repo root",
            "Check filesystem",
        ),
    };
    let canonical_file = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return make_error_response(
            StatusCode::NOT_FOUND,
            "File not found or inaccessible",
            "Check path",
        ),
    };
    if !canonical_file.starts_with(&canonical_root) {
        return make_error_response(
            StatusCode::FORBIDDEN,
            "Path traversal attempt",
            "Only files under repo root are allowed",
        );
    }

    match fs::read(&canonical_file).await {
        Ok(content) => {
            let mime = guess_content_type(&canonical_file);
            (
                [(axum::http::header::CONTENT_TYPE, mime)],
                content,
            )
                .into_response()
        }
        Err(e) => make_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read file",
            &e.to_string(),
        ),
    }
}

pub async fn handle_ai_raw_file(
    State(state): State<Arc<AppState>>,
    Path(params): Path<AiFileParams>,
) -> Response {
    serve_file_from_repo(&state, &params).await
}

#[derive(serde::Deserialize)]
pub struct AiTreeRootParams {
    pub token: String,
    pub owner: String,
    pub repo: String,
    pub branch: String,
}

pub async fn handle_ai_tree_root(
    State(state): State<Arc<AppState>>,
    Path(params): Path<AiTreeRootParams>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    use crate::render;
    use std::time::Instant;

    let start = Instant::now();
    let verifying_key = state.signing_key.verifying_key();
    let _payload = verify_lct(
        &params.token,
        &verifying_key,
        Some(&params.owner),
        Some(&params.repo),
        Some(&params.branch),
    )
    .map_err(|e| make_error_response(StatusCode::UNAUTHORIZED, &format!("Invalid token: {}", e), "Check LCT."))?;

    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let name = state.project_index.get_name_by_github(&params.owner, &params.repo, &params.branch)
        .or_else(|| map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&params.repo)).map(|p| p.name.as_str()))
        .ok_or_else(|| make_error_response(StatusCode::NOT_FOUND, "Project not found", "Check owner/repo/branch."))?;
    let meta = state.project_index.get_meta(name);
    let resp = render::render_negotiated_tree(
        &headers,
        &map,
        name,
        meta,
        false,
        Some(&params.token),
        Some(&params.branch),
    )
    .map_err(|(status, err)| make_error_response(status, &err, ""))?;
    write_audit_log("127.0.0.1", &headers, &format!("/t/.../{}/{}/tree/{}", params.owner, params.repo, params.branch), "GET", name, None, 200, start.elapsed().as_millis());
    Ok(resp)
}

pub async fn handle_ai_tree_file(
    State(state): State<Arc<AppState>>,
    Path((token, owner, repo, branch, rest)): Path<(String, String, String, String, String)>,
) -> Response {
    let params = AiFileParams {
        token,
        owner,
        repo,
        branch,
        filepath: rest,
    };
    serve_file_from_repo(&state, &params).await
}
