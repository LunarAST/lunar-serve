use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, Json, Html},
    routing::get,
    Router,
};
use lunar_interface::LunarMap;
use lunar_serve::{load_repos, ProjectIndex};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use chrono::Utc;

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

/// Generates a standardized, AI-friendly diagnostic error response with X-Lunar-Diagnostic headers.
fn make_error_response(status: StatusCode, error_msg: &str, hint: &str) -> Response {
    let body_val = serde_json::json!({
        "error": error_msg,
        "hint": hint
    });
    
    let mut response = (status, Json(body_val)).into_response();
    response.headers_mut().insert(
        "X-Lunar-Diagnostic",
        axum::http::HeaderValue::from_str(hint).unwrap_or(axum::http::HeaderValue::from_static("error"))
    );
    response
}

/// [ADDED] Serves the single-file bundled React Canvas (lunar-scope) natively at root URL.
async fn get_index(State(state): State<Arc<AppState>>) -> Result<Html<String>, Response> {
    let base_dir = state.data_path.parent().unwrap_or(std::path::Path::new("/"));
    
    // Fallback search to locate the compiled index.html on disk
    let candidate_paths = vec![
        base_dir.join("LunarAST/lunar-scope/dist/index.html"),
        base_dir.join("lunar-scope/dist/index.html"),
        PathBuf::from("/opt/LunarAST/lunar-scope/dist/index.html"),
    ];
    
    let mut resolved_path = None;
    for path in candidate_paths {
        if path.exists() {
            resolved_path = Some(path);
            break;
        }
    }
    
    let index_path = match resolved_path {
        Some(p) => p,
        None => {
            return Err(make_error_response(
                StatusCode::NOT_FOUND,
                "lunar-scope compiled index.html not found",
                "Please compile the React frontend first by running 'npm run build' inside your lunar-scope directory."
            ));
        }
    };
    
    fs::read_to_string(index_path)
        .map(Html)
        .map_err(|e| make_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read index.html",
            &e.to_string()
        ))
}

async fn get_json(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, Response> {
    let start = Instant::now();
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    
    utils::write_audit_log("127.0.0.1", &HeaderMap::new(), "/lunar-map.json", "GET", "global", None, 200, start.elapsed().as_millis());
    
    serde_json::to_value(&map)
        .map_err(|e| make_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Serialization failed", &e.to_string()))
        .map(Json)
}

async fn get_markdown(State(state): State<Arc<AppState>>, Query(query): Query<MdQuery>) -> Result<String, Response> {
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    if query.summary { return Ok(render::render_summary(&map)); }
    if let Some(ref scope) = query.scope { return Ok(render::render_project_md(&map, scope, state.project_index.get_meta(scope), false)); }
    if let Some(ref status) = query.status { return Ok(render::render_status_filter(&map, status)); }
    if let Some(ref path) = query.path { return Ok(render::render_path_filter(&map, path)); }
    if let Some(ref style) = query.style { if style == "mermaid" { return Ok(render::render_mermaid(&map)); } }
    let mut md = String::from("# Ecosystem Topology\n\n");
    for p in &map.projects { md.push_str(&format!("## {}\n", p.name)); }
    Ok(md)
}

async fn get_project_md_api(
    State(state): State<Arc<AppState>>, 
    headers: HeaderMap,
    Path(name): Path<String>, 
    Query(query): Query<MdQuery>
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let meta = state.project_index.get_meta(&name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map).into_response()); }
    
    let resp = render::render_negotiated_tree(&headers, &map, &name, meta, false)
        .map_err(|(status, err)| make_error_response(status, &err, "Failed to render negotiated tree on API request"));

    let status_code = match &resp { Ok(r) => r.status().as_u16(), Err(e) => e.status().as_u16() };
    utils::write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/map", name), "GET", &name, None, status_code, start.elapsed().as_millis());

    resp
}

async fn get_project_md_github(
    State(state): State<Arc<AppState>>, 
    headers: HeaderMap,
    Path((owner, repo, branch)): Path<(String, String, String)>, 
    Query(query): Query<MdQuery>
) -> Result<Response, Response> {
    let start = Instant::now();
    let branch = branch.trim_end_matches(".md").trim_end_matches(".json");
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let name = match state.project_index.get_name_by_github(&owner, &repo, &branch) {
        Some(n) => n,
        None => {
            let resp = make_error_response(
                StatusCode::NOT_FOUND,
                "No project mapped to this GitHub path",
                &format!("Verify your repos.json coordinates. Ensure owner '{}', repo '{}' and branch '{}' match.", owner, repo, branch)
            );
            utils::write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/tree/{}", owner, repo, branch), "GET", "unknown", None, 404, start.elapsed().as_millis());
            return Err(resp);
        }
    };
    let meta = state.project_index.get_meta(name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map).into_response()); }
    
    let resp = render::render_negotiated_tree(&headers, &map, name, meta, false)
        .map_err(|(status, err)| make_error_response(status, &err, "Failed to render negotiated tree on GitHub coordinates request"));

    let status_code = match &resp { Ok(r) => r.status().as_u16(), Err(e) => e.status().as_u16() };
    utils::write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/tree/{}", owner, repo, branch), "GET", name, None, status_code, start.elapsed().as_millis());

    resp
}

async fn get_project_md_legacy(
    State(state): State<Arc<AppState>>, 
    headers: HeaderMap,
    Path(name): Path<String>, 
    Query(query): Query<MdQuery>
) -> Result<Response, Response> {
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let meta = state.project_index.get_meta(&name);
    if query.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map).into_response()); }
    
    render::render_negotiated_tree(&headers, &map, &name, meta, false)
        .map_err(|(status, err)| make_error_response(status, &err, "Failed to render negotiated tree on legacy request"))
}

async fn get_private_project_md(
    State(state): State<Arc<AppState>>, 
    headers: HeaderMap,
    req: axum::http::Request<axum::body::Body>
) -> Result<Response, Response> {
    let path = req.uri().path();
    let name = path.trim_start_matches("/private/project/").trim_end_matches(".md").trim_end_matches(".json");
    if name.is_empty() { return Err(make_error_response(StatusCode::BAD_REQUEST, "Missing project name", "Check your request path.")); }
    
    let auth = req.headers().get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !auth.starts_with("Bearer ") { return Err(make_error_response(StatusCode::UNAUTHORIZED, "Missing JWT token", "Private routes require a valid Bearer token.")); }
    
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let meta = state.project_index.get_meta(name);
    
    render::render_negotiated_tree(&headers, &map, name, meta, true)
        .map_err(|(status, err)| make_error_response(status, &err, "Failed to render negotiated tree on private request"))
}

async fn healthz() -> &'static str { "OK" }

/// Endpoint to serve raw file content by project name via standard API.
/// Path matches: /api/v1/projects/:name/raw/*filepath
async fn get_raw_file_api(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, filepath)): Path<(String, String)>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let meta = state.project_index.get_meta(&name);

    // Authorization barrier for private projects
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !utils::is_authorized(&headers) {
        let resp = make_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized access",
            "This project is private. Please provide a valid Bearer JWT token in your Authorization header."
        );
        utils::write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/raw/{}", name, filepath), "GET", &name, Some(&filepath), 401, start.elapsed().as_millis());
        return Err(resp);
    }

    // Auto-discover path
    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
                .and_then(|p| p.path.clone())
        });
        
    let base_path = match base_path {
        Some(p) => p,
        None => {
            let resp = make_error_response(
                StatusCode::BAD_REQUEST,
                "Workspace path missing",
                &format!("No physical workspace path could be auto-detected or mapped for project '{}'.", name)
            );
            utils::write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/raw/{}", name, filepath), "GET", &name, Some(&filepath), 400, start.elapsed().as_millis());
            return Err(resp);
        }
    };

    let result = utils::read_secure_file(&base_path, &filepath);
    let status_code = match &result { Ok(_) => 200, Err((status, _, _)) => status.as_u16() };
    utils::write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/raw/{}", name, filepath), "GET", &name, Some(&filepath), status_code, start.elapsed().as_millis());

    result
        .map(|content| content.into_response())
        .map_err(|(status, err, hint)| make_error_response(status, &err, &hint))
}

/// Endpoint to serve raw file content mirroring GitHub's URL format.
/// Supports both /raw/ and /blob/ path aliases dynamically for zero-friction AI consumption.
async fn get_raw_file_github(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, repo, branch, filepath)): Path<(String, String, String, String)>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let branch = branch.trim_end_matches(".md").trim_end_matches(".json");
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let name = match state.project_index.get_name_by_github(&owner, &repo, &branch) {
        Some(n) => n,
        None => {
            let resp = make_error_response(
                StatusCode::NOT_FOUND,
                "No project mapped to this GitHub path",
                &format!("Verify your repos.json coordinates. Ensure owner '{}', repo '{}' and branch '{}' match.", owner, repo, branch)
            );
            utils::write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/raw/{}/{}", owner, repo, branch, filepath), "GET", "unknown", Some(&filepath), 404, start.elapsed().as_millis());
            return Err(resp);
        }
    };

    let meta = state.project_index.get_meta(name);

    // Authorization barrier for private projects
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !utils::is_authorized(&headers) {
        let resp = make_error_response(
            StatusCode::UNAUTHORIZED,
            "Unauthorized access",
            "This project is private. Please provide a valid Bearer JWT token in your Authorization header."
        );
        utils::write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/raw/{}/{}", owner, repo, branch, filepath), "GET", name, Some(&filepath), 401, start.elapsed().as_millis());
        return Err(resp);
    }

    // Auto-discover path
    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .and_then(|p| p.path.clone())
        });
        
    let base_path = match base_path {
        Some(p) => p,
        None => {
            let resp = make_error_response(
                StatusCode::BAD_REQUEST,
                "Workspace path missing",
                &format!("No physical workspace path could be auto-detected or mapped for project '{}'.", name)
            );
            utils::write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/raw/{}/{}", owner, repo, branch, filepath), "GET", name, Some(&filepath), 400, start.elapsed().as_millis());
            return Err(resp);
        }
    };

    let result = utils::read_secure_file(&base_path, &filepath);
    let status_code = match &result { Ok(_) => 200, Err((status, _, _)) => status.as_u16() };
    utils::write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/raw/{}/{}", owner, repo, branch, filepath), "GET", name, Some(&filepath), status_code, start.elapsed().as_millis());

    result
        .map(|content| content.into_response())
        .map_err(|(status, err, hint)| make_error_response(status, &err, &hint))
}

/// Task B: Get the current AI Todo handover scratchpad.
async fn get_project_todo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let meta = state.project_index.get_meta(&name);

    // Authorization barrier for private projects
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !utils::is_authorized(&headers) {
        return Err(make_error_response(StatusCode::UNAUTHORIZED, "Unauthorized access", "Todo is private. Authorization required."));
    }

    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
                .and_then(|p| p.path.clone())
        });

    let base_path = match base_path {
        Some(p) => p,
        None => return Err(make_error_response(StatusCode::BAD_REQUEST, "Workspace path missing", "No path mapped.")),
    };

    let todo_path = std::path::Path::new(&base_path).join(".lunar/ai-todo.json");
    if !todo_path.exists() {
        let empty_todo = serde_json::json!({
            "project": name,
            "status": "idle",
            "lastHandover": Utc::now().to_rfc3339(),
            "tasks": []
        });
        utils::write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/todo", name), "GET", &name, Some(".lunar/ai-todo.json"), 200, start.elapsed().as_millis());
        return Ok(Json(empty_todo).into_response());
    }

    match fs::read_to_string(&todo_path) {
        Ok(content) => {
            let val: serde_json::Value = serde_json::from_str(&content).unwrap_or(serde_json::Value::Null);
            utils::write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/todo", name), "GET", &name, Some(".lunar/ai-todo.json"), 200, start.elapsed().as_millis());
            Ok(Json(val).into_response())
        }
        Err(e) => Err(make_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to read todo file", &e.to_string())),
    }
}

/// [ADDED] Task D: Serves a side-by-side comparative Markdown diff for AI contract auditing.
async fn get_project_todo_diff(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let meta = state.project_index.get_meta(&name);

    // Authorization barrier for private projects
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !utils::is_authorized(&headers) {
        return Err(make_error_response(StatusCode::UNAUTHORIZED, "Unauthorized access", "Todo is private. Authorization required."));
    }

    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
                .and_then(|p| p.path.clone())
        });

    let base_path = match base_path {
        Some(p) => p,
        None => return Err(make_error_response(StatusCode::BAD_REQUEST, "Workspace path missing", "No path mapped.")),
    };

    let base_path_obj = std::path::Path::new(&base_path);
    let interfaces_path = base_path_obj.join(".lunar/interfaces.yml");
    let todo_path = base_path_obj.join(".lunar/ai-todo.json");

    let current_yml = if interfaces_path.exists() {
        fs::read_to_string(&interfaces_path).unwrap_or_default()
    } else {
        "**No existing interfaces.yml file found.**".to_string()
    };

    let proposed_patch = if todo_path.exists() {
        if let Ok(todo_content) = fs::read_to_string(&todo_path) {
            let val: serde_json::Value = serde_json::from_str(&todo_content).unwrap_or(serde_json::Value::Null);
            val.get("tasks")
                .and_then(|t| t.as_array())
                .and_then(|arr| arr.get(0))
                .and_then(|first| first.get("patch"))
                .and_then(|p| p.as_str())
                .unwrap_or("**No pending contract patch inside the active Todo scratchpad.**")
                .to_string()
        } else {
            "**Failed to read active Todo file.**".to_string()
        }
    } else {
        "**No active Todo handover file found.**".to_string()
    };

    let mut md = format!("# Pending API Contract Diff Analysis: {}\n\n", name);
    md.push_str("As a Peer-Review AI Architect, please examine and audit the proposed YAML patch against the current active contract below.\n\n");
    md.push_str("## 📄 Current interfaces.yml\n");
    md.push_str("```yaml\n");
    md.push_str(&current_yml);
    md.push_str("\n```\n\n");
    md.push_str("## 🚀 Proposed AI Patch (Task Handover)\n");
    md.push_str("```yaml\n");
    md.push_str(&proposed_patch);
    md.push_str("\n```\n");

    utils::write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/todo/diff", name), "GET", &name, Some(".lunar/ai-todo.json"), 200, start.elapsed().as_millis());

    Ok(md.into_response())
}

/// Task B: Update the AI Todo handover scratchpad.
async fn post_project_todo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = match load_map(&state.data_path) {
        Ok(m) => m,
        Err((status, err)) => return Err(make_error_response(status, &err, "Failed to read global lunar-map.json")),
    };
    let meta = state.project_index.get_meta(&name);

    // Authorization barrier for private projects
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !utils::is_authorized(&headers) {
        return Err(make_error_response(StatusCode::UNAUTHORIZED, "Unauthorized access", "Todo is private. Authorization required."));
    }

    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|p| p.name.eq_ignore_ascii_case(&name))
                .and_then(|p| p.path.clone())
        });

    let base_path = match base_path {
        Some(p) => p,
        None => return Err(make_error_response(StatusCode::BAD_REQUEST, "Workspace path missing", "No path mapped.")),
    };

    let todo_dir = std::path::Path::new(&base_path).join(".lunar");
    if !todo_dir.exists() {
        let _ = fs::create_dir_all(&todo_dir);
    }
    let todo_path = todo_dir.join("ai-todo.json");

    match serde_json::to_string_pretty(&payload) {
        Ok(formatted) => {
            if let Err(e) = fs::write(&todo_path, formatted) {
                return Err(make_error_response(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save todo", &e.to_string()));
            }
            utils::write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/todo", name), "POST", &name, Some(".lunar/ai-todo.json"), 200, start.elapsed().as_millis());
            Ok((StatusCode::OK, "✓ Todo scratchpad updated successfully").into_response())
        }
        Err(e) => Err(make_error_response(StatusCode::BAD_REQUEST, "Invalid JSON payload", &e.to_string())),
    }
}

#[tokio::main]
async fn main() {
    // Run the automated Log Purge Daemon on startup to clear expired access logs
    utils::purge_old_logs();

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
        .with_state(state);
        
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("lunar-serve listening on http://0.0.0.0:{}", port);
    axum::serve(listener, app).await.unwrap();
}
