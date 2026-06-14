use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ed25519_dalek::SigningKey;
use std::path::PathBuf;

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
pub mod rate_limiter;
pub mod patch;
pub mod utils;
pub mod render;
pub mod handlers; // <--- ADDED: handlers as a library module

pub use session::{create_session, validate_session, invalidate_session, spawn_cleanup_task};
pub use lct::{LctPayload, generate_lct, verify_lct, load_signing_key};
pub use totp::verify_totp;
pub use rate_limiter::{check as check_rate_limit, record_failure};
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
        handle_ai_readonly_tree,
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
        .route("/t/:token/:owner/:repo/tree/:branch", get(handle_ai_readonly_tree))
        .route("/t/:token/:owner/:repo/tree/:branch/*rest", get(handle_ai_readonly_tree))
        .with_state(state)
}
