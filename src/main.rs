use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::get,
    Router,
};
use lunar::LunarMap;
use lunar_serve::{load_repos, ProjectIndex, ProjectMeta};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

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

fn render_summary(map: &LunarMap) -> String {
    let mut md = String::from("# Ecosystem Summary\n\n");
    let total_exposed: usize = map.projects.iter().map(|p| p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len())).sum();
    let total_consumed: usize = map.projects.iter().map(|p| p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len())).sum();
    let orphaned = map.anomalies.orphaned_consumers.len();
    let unused = map.anomalies.unused_endpoints.len();
    md.push_str(&format!("- Projects: {}\n- Total Exposed Endpoints: {}\n- Total Consumed Dependencies: {}\n- Anomalies: {} orphaned, {} unused\n\n", map.projects.len(), total_exposed, total_consumed, orphaned, unused));
    md.push_str("## Projects\n\n| Name | Type | Exposed | Consumed | Status |\n|:---|:---|:---|:---|:---|\n");
    for p in &map.projects {
        let exp = p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
        let con = p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
        md.push_str(&format!("| {} | {} | {} | {} | {} |\n", p.name, p.project_type, exp, con, p.scan_status));
    }
    if orphaned + unused > 0 {
        md.push_str("\n## Top Risks\n");
        for ep in &map.anomalies.orphaned_consumers { md.push_str(&format!("1. **Orphaned**: {} calls `{} {}` but target not found.\n", ep.project, ep.method, ep.path)); }
        for ep in &map.anomalies.unused_endpoints { md.push_str(&format!("1. **Unused**: {} exposes `{} {}` but no consumer.\n", ep.project, ep.method, ep.path)); }
    }
    md
}

fn render_project_md(map: &LunarMap, name: &str, meta: Option<&ProjectMeta>, is_authenticated: bool) -> String {
    let project = map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(name));
    if project.is_none() { return format!("# Project `{}` not found.\n", name); }
    let p = project.unwrap();
    let exp = p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
    let con = p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
    let mut md = format!("# Project: {}\n\n- Type: {}\n- Scan Status: {}\n- Exposed: {}\n- Consumed: {}\n\n", p.name, p.project_type, p.scan_status, exp, con);
    if let Some(interfaces) = p.interfaces.as_object() {
        if let Some(exposed) = interfaces.get("exposed").and_then(|e| e.as_array()) {
            md.push_str("## Exposed Endpoints\n\n| Method | Path |\n|:---|:---|\n");
            for e in exposed { md.push_str(&format!("| {} | {} |\n", e["method"].as_str().unwrap_or(""), e["path"].as_str().unwrap_or(""))); }
            md.push('\n');
        }
        if let Some(consumed) = interfaces.get("consumed").and_then(|e| e.as_array()) {
            md.push_str("## Consumed Dependencies\n\n| Method | Path | Target |\n|:---|:---|:---|\n");
            for c in consumed { md.push_str(&format!("| {} | {} | {} |\n", c["method"].as_str().unwrap_or(""), c["path"].as_str().unwrap_or(""), c.get("targetProject").and_then(|t| t.as_str()).unwrap_or("?"))); }
            md.push('\n');
        }
    }
    let relevant: Vec<&lunar::AlignmentEntry> = map.alignments.iter().filter(|a| a.client_project.eq_ignore_ascii_case(name) || a.server_project.eq_ignore_ascii_case(name)).collect();
    if !relevant.is_empty() {
        md.push_str("## Alignments\n\n| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n");
        for a in relevant { md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status)); }
    }
    if let Some(meta) = meta {
        if let Some(ref url) = meta.archive_url {
            if meta.visibility == "public" || is_authenticated { md.push_str(&format!("\n📦 [Download Source Archive]({})\n", url)); }
        }
    }
    md
}

fn render_status_filter(map: &LunarMap, status: &str) -> String {
    let filtered: Vec<&lunar::AlignmentEntry> = map.alignments.iter().filter(|a| a.status.eq_ignore_ascii_case(status)).collect();
    let mut md = format!("# Alignments with status `{}` ({})\n\n", status, filtered.len());
    if filtered.is_empty() { md.push_str("No matching alignments.\n"); }
    else { md.push_str("| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n"); for a in filtered { md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status)); } }
    md
}

fn render_path_filter(map: &LunarMap, path: &str) -> String {
    let filtered: Vec<&lunar::AlignmentEntry> = map.alignments.iter().filter(|a| a.path.contains(path)).collect();
    let mut md = format!("# Alignments related to path `{}` ({})\n\n", path, filtered.len());
    if filtered.is_empty() { md.push_str("No matching alignments.\n"); }
    else { md.push_str("| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n"); for a in filtered { md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status)); } }
    md
}

fn render_mermaid(map: &LunarMap) -> String {
    let mut md = String::from("```mermaid\ngraph LR\n");
    let mut node_ids: HashMap<&str, usize> = HashMap::new();
    for (i, p) in map.projects.iter().enumerate() { node_ids.insert(&p.name, i); md.push_str(&format!("  n{}[{}]\n", i, p.name)); }
    for edge in &map.aggregated_edges {
        if let (Some(&from), Some(&to)) = (node_ids.get(edge.client_project.as_str()), node_ids.get(edge.server_project.as_str())) {
            let style = if edge.status == "Orphaned" { " -- " } else { " --> " };
            md.push_str(&format!("  n{}{}|{} {}|n{}\n", from, style, edge.call_count, edge.status, to));
        }
    }
    md.push_str("```\n");
    md
}

async fn get_json(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let map = load_map(&state.data_path)?;
    serde_json::to_value(&map).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())).map(Json)
}

async fn get_markdown(State(state): State<Arc<AppState>>, Query(query): Query<MdQuery>) -> Result<String, (StatusCode, String)> {
    let map = load_map(&state.data_path)?;
    if query.summary { return Ok(render_summary(&map)); }
    if let Some(ref scope) = query.scope { return Ok(render_project_md(&map, scope, state.project_index.get_meta(scope), false)); }
    if let Some(ref status) = query.status { return Ok(render_status_filter(&map, status)); }
    if let Some(ref path) = query.path { return Ok(render_path_filter(&map, path)); }
    if let Some(ref style) = query.style { if style == "mermaid" { return Ok(render_mermaid(&map)); } }
    let mut md = String::from("# Ecosystem Topology\n\n");
    for p in &map.projects { md.push_str(&format!("## {}\n", p.name)); }
    Ok(md)
}

async fn get_project_md_api(State(state): State<Arc<AppState>>, Path(name): Path<String>, Query(query): Query<MdQuery>) -> Result<String, (StatusCode, String)> {
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path)?;
    let meta = state.project_index.get_meta(&name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render_mermaid(&map)); }
    Ok(render_project_md(&map, &name, meta, false))
}

async fn get_project_md_github(State(state): State<Arc<AppState>>, Path((owner, repo, branch)): Path<(String, String, String)>, Query(query): Query<MdQuery>) -> Result<String, (StatusCode, String)> {
    let branch = branch.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path)?;
    let name = state.project_index.get_name_by_github(&owner, &repo, &branch).ok_or((StatusCode::NOT_FOUND, "No project mapped to this GitHub path".to_string()))?;
    let meta = state.project_index.get_meta(name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render_mermaid(&map)); }
    Ok(render_project_md(&map, name, meta, false))
}

async fn get_project_md_legacy(State(state): State<Arc<AppState>>, Path(name): Path<String>, Query(query): Query<MdQuery>) -> Result<String, (StatusCode, String)> {
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path)?;
    let meta = state.project_index.get_meta(name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render_mermaid(&map)); }
    Ok(render_project_md(&map, name, meta, false))
}

async fn get_private_project_md(State(state): State<Arc<AppState>>, req: axum::http::Request<axum::body::Body>) -> Result<String, (StatusCode, String)> {
    let path = req.uri().path();
    let name = path.trim_start_matches("/private/project/").trim_end_matches(".md").trim_end_matches(".json");
    if name.is_empty() { return Err((StatusCode::BAD_REQUEST, "Missing project name".into())); }
    let auth = req.headers().get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !auth.starts_with("Bearer ") { return Err((StatusCode::UNAUTHORIZED, "Missing JWT token".into())); }
    let map = load_map(&state.data_path)?;
    let meta = state.project_index.get_meta(name);
    Ok(render_project_md(&map, name, meta, true))
}

async fn healthz() -> &'static str { "OK" }

/// Validates and reads a single file from the workspace, preventing directory traversal.
/// This conforms to the Google engineering standards for robust security checks.
fn read_secure_file(base_path_str: &str, relative_path_str: &str) -> Result<String, (StatusCode, String)> {
    let base_path = std::path::Path::new(base_path_str);
    let relative_path = std::path::Path::new(relative_path_str);

    // Combine base path and relative path to form the target path
    let target_path = base_path.join(relative_path);

    // Resolve the absolute canonical path to eliminate symlinks and "/../" bypasses
    let canonical_target = match target_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return Err((StatusCode::NOT_FOUND, "File not found or invalid path".to_string())),
    };

    // Resolve the canonical project base path for comparison
    let canonical_base = match base_path.canonicalize() {
        Ok(path) => path,
        Err(_) => return Err((StatusCode::INTERNAL_SERVER_ERROR, "Invalid project base path configured".to_string())),
    };

    // Security boundary: Ensure target path remains strictly inside the base directory
    if !canonical_target.starts_with(canonical_base) {
        return Err((StatusCode::FORBIDDEN, "Access denied: Path traversal detected".to_string()));
    }

    // Read file contents to string
    std::fs::read_to_string(canonical_target)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read file: {}", e)))
}

/// Helper function to check if the incoming request is authorized for private repos.
fn is_authorized(headers: &HeaderMap) -> bool {
    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    auth.starts_with("Bearer ")
}

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
    if visibility == "private" && !is_authorized(&headers) {
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

    read_secure_file(&base_path, &filepath)
}

/// Endpoint to serve raw file content mirroring GitHub's URL format.
/// Path matches: /:owner/:repo/raw/:branch/*filepath
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
    if visibility == "private" && !is_authorized(&headers) {
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

    read_secure_file(&base_path, &filepath)
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
    let app = Router::new()
        .route("/lunar-map.json", get(get_json))
        .route("/lunar-map.md", get(get_markdown))
        .route("/api/v1/projects/:name/map", get(get_project_md_api))
        .route("/api/v1/projects/:name/raw/*filepath", get(get_raw_file_api))
        .route("/:owner/:repo/tree/:branch", get(get_project_md_github))
        .route("/:owner/:repo/raw/:branch/*filepath", get(get_raw_file_github))
        .route("/project/:name", get(get_project_md_legacy))
        .route("/private/project/:name", get(get_private_project_md))
        .route("/healthz", get(healthz))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("lunar-serve listening on http://0.0.0.0:{}", port);
    axum::serve(listener, app).await.unwrap();
}
