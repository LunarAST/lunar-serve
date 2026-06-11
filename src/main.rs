use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::get,
    Router,
};
use lunar_interface::LunarMap;
use lunar_serve::{load_repos, ProjectIndex};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

pub mod render;
pub mod utils;

struct AppState {
    data_path: PathBuf,
    project_index: ProjectIndex,
}

#[derive(Deserialize, Default)]
struct MdQuery {
    #[serde(default)]
    summary: bool,
    scope: Option<String>,
    status: Option<String>,
    path: Option<String>,
    style: Option<String>,
}

fn load_map(path: &std::path::Path) -> Result<LunarMap, (StatusCode, String)> {
    let content = fs::read_to_string(path)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read lunar-map.json".into()))?;
    serde_json::from_str(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid JSON: {}", e)))
}

async fn get_json(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let map = load_map(&state.data_path)?;
    serde_json::to_value(&map).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())).map(Json)
}

async fn get_markdown(State(state): State<Arc<AppState>>, Query(query): Query<MdQuery>) -> Result<String, (StatusCode, String)> {
    let map = load_map(&state.data_path)?;
    if query.summary { return Ok(render::render_summary(&map)); }
    if let Some(ref scope) = query.scope { return Ok(render::render_project_md(&map, scope, state.project_index.get_meta(scope), false)); }
    if let Some(ref status) = query.status { return Ok(render::render_status_filter(&map, status)); }
    if let Some(ref path) = query.path { return Ok(render::render_path_filter(&map, path)); }
    if let Some(ref style) = query.style { if style == "mermaid" { return Ok(render::render_mermaid(&map)); } }
    let mut md = String::from("# Ecosystem Topology\n\n");
    for p in &map.projects { md.push_str(&format!("## {}\n", p.name)); }
    Ok(md)
}

async fn get_project_md_api(State(state): State<Arc<AppState>>, Path(name): Path<String>, Query(query): Query<MdQuery>) -> Result<String, (StatusCode, String)> {
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path)?;
    let meta = state.project_index.get_meta(&name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map)); }
    Ok(render::render_project_md(&map, &name, meta, false))
}

async fn get_project_md_github(State(state): State<Arc<AppState>>, Path((owner, repo, branch)): Path<(String, String, String)>, Query(query): Query<MdQuery>) -> Result<String, (StatusCode, String)> {
    let branch = branch.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path)?;
    let name = state.project_index.get_name_by_github(&owner, &repo, &branch).ok_or((StatusCode::NOT_FOUND, "No project mapped to this GitHub path".to_string()))?;
    let meta = state.project_index.get_meta(name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map)); }
    Ok(render::render_project_md(&map, name, meta, false))
}

async fn get_project_md_legacy(State(state): State<Arc<AppState>>, Path(name): Path<String>, Query(query): Query<MdQuery>) -> Result<String, (StatusCode, String)> {
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path)?;
    let meta = state.project_index.get_meta(name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map)); }
    Ok(render::render_project_md(&map, name, meta, false))
}

async fn get_private_project_md(State(state): State<Arc<AppState>>, req: axum::http::Request<axum::body::Body>) -> Result<String, (StatusCode, String)> {
    let path = req.uri().path();
    let name = path.trim_start_matches("/private/project/").trim_end_matches(".md").trim_end_matches(".json");
    if name.is_empty() { return Err((StatusCode::BAD_REQUEST, "Missing project name".into())); }
    let auth = req.headers().get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !auth.starts_with("Bearer ") { return Err((StatusCode::UNAUTHORIZED, "Missing JWT token".into())); }
    let map = load_map(&state.data_path)?;
    let meta = state.project_index.get_meta(name);
    Ok(render::render_project_md(&map, name, meta, true))
}

async fn healthz() -> &'static str { "OK" }

/// Endpoint to serve raw file content by project name via standard API.
/// Path matches: /api/v1/projects/:name/raw/*filepath
async fn get_raw_file_api(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, filepath)): Path<(String, String)>,
) -> Result<String, (StatusCode, String)> {
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path)?;
    let meta = state.project_index.get_meta(&name);

    // Authorization barrier for private projects
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !utils::is_authorized(&headers) {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized access to private workspace".to_string()));
    }

    // Auto-discover path: 1. check repos.json override, 2. fallback to automated map configuration
    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
                .and_then(|p| p.path.clone())
        })
        .ok_or((StatusCode::BAD_REQUEST, "No physical workspace path mapped for this project".to_string()))?;

    utils::read_secure_file(&base_path, &filepath)
}

/// Endpoint to serve raw file content mirroring GitHub's URL format.
/// Supports both /raw/ and /blob/ path aliases dynamically for zero-friction AI consumption.
async fn get_raw_file_github(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, repo, branch, filepath)): Path<(String, String, String, String)>,
) -> Result<String, (StatusCode, String)> {
    let branch = branch.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path)?;
    let name = state.project_index.get_name_by_github(&owner, &repo, &branch)
        .ok_or((StatusCode::NOT_FOUND, "No project mapped to this GitHub path".to_string()))?;

    let meta = state.project_index.get_meta(name);

    // Authorization barrier for private projects
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !utils::is_authorized(&headers) {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized access to private workspace".to_string()));
    }

    // Auto-discover path: 1. check repos.json override, 2. fallback to automated map configuration
    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .and_then(|p| p.path.clone())
        })
        .ok_or((StatusCode::BAD_REQUEST, "No physical workspace path mapped for this project".to_string()))?;

    utils::read_secure_file(&base_path, &filepath)
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("LUNAR_SERVE_PORT").unwrap_or_else(|_| "8787".to_string()).parse().unwrap_or(8787);
    let args: Vec<String> = std::env::args().collect();
    let data_path = if args.len() > 1 { PathBuf::from(&args[1]) } else { PathBuf::from("lunar-map.json") };
    if !data_path.exists() { eprintln!("Error: lunar-map.json not found at {}.", data_path.display()); std::process::exit(1); }
    let base_dir = data_path.parent().unwrap_or(std::path::Path::new("/"));
    let repos_config = load_repos(base_dir);
    let project_index = ProjectIndex::from_config(&repos_config);
    let state = Arc::new(AppState { data_path, project_index });
    
    // Wire up handlers, mapping both `/raw/` and `/blob/` to fulfill AI projection needs
    let app = Router::new()
        .route("/lunar-map.json", get(get_json))
        .route("/lunar-map.md", get(get_markdown))
        .route("/api/v1/projects/:name/map", get(get_project_md_api))
        .route("/api/v1/projects/:name/raw/*filepath", get(get_raw_file_api))
        .route("/:owner/:repo/tree/:branch", get(get_project_md_github))
        .route("/:owner/:repo/raw/:branch/*filepath", get(get_raw_file_github))
        .route("/:owner/:repo/blob/:branch/*filepath", get(get_raw_file_github))
        .route("/project/:name", get(get_project_md_legacy))
        .route("/private/project/:name", get(get_private_project_md))
        .route("/healthz", get(healthz))
        .with_state(state);
        
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("lunar-serve listening on http://0.0.0.0:{}", port);
    axum::serve(listener, app).await.unwrap();
}
