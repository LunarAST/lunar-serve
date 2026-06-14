use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response, Json},
};
use crate::{AppState, make_error_response, load_map, write_audit_log, is_authorized};
use std::sync::Arc;
use std::fs;
use std::time::Instant;
use chrono::Utc;

pub async fn get_project_todo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let meta = state.project_index.get_meta(&name);
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !is_authorized(&headers) {
        return Err(make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Unauthorized", "Private project."));
    }
    let base_path = meta.and_then(|m| m.path.as_deref()).map(|p| p.to_string())
        .or_else(|| map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&name)).and_then(|p| p.path.clone()));
    let base_path = match base_path {
        Some(p) => p,
        None => return Err(make_error_response(axum::http::StatusCode::BAD_REQUEST, "Path missing", "")),
    };
    let todo_path = std::path::Path::new(&base_path).join(".lunar/ai-todo.json");
    if !todo_path.exists() {
        let empty = serde_json::json!({ "project": name, "status": "idle", "lastHandover": Utc::now().to_rfc3339(), "tasks": [] });
        write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/todo", name), "GET", &name, Some(".lunar/ai-todo.json"), 200, start.elapsed().as_millis());
        return Ok(Json(empty).into_response());
    }
    match fs::read_to_string(&todo_path) {
        Ok(content) => {
            let val: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
            write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/todo", name), "GET", &name, Some(".lunar/ai-todo.json"), 200, start.elapsed().as_millis());
            Ok(Json(val).into_response())
        }
        Err(e) => Err(make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Failed to read todo", &e.to_string())),
    }
}

pub async fn post_project_todo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let meta = state.project_index.get_meta(&name);
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !is_authorized(&headers) {
        return Err(make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Unauthorized", ""));
    }
    let base_path = meta.and_then(|m| m.path.as_deref()).map(|p| p.to_string())
        .or_else(|| map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&name)).and_then(|p| p.path.clone()));
    let base_path = match base_path {
        Some(p) => p,
        None => return Err(make_error_response(axum::http::StatusCode::BAD_REQUEST, "Path missing", "")),
    };
    let todo_dir = std::path::Path::new(&base_path).join(".lunar");
    let _ = fs::create_dir_all(&todo_dir);
    let todo_path = todo_dir.join("ai-todo.json");
    match serde_json::to_string_pretty(&payload) {
        Ok(formatted) => {
            if let Err(e) = fs::write(&todo_path, formatted) {
                return Err(make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Failed to save todo", &e.to_string()));
            }
            write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/todo", name), "POST", &name, Some(".lunar/ai-todo.json"), 200, start.elapsed().as_millis());
            Ok((axum::http::StatusCode::OK, "✓ Todo updated").into_response())
        }
        Err(e) => Err(make_error_response(axum::http::StatusCode::BAD_REQUEST, "Invalid JSON", &e.to_string())),
    }
}

pub async fn get_project_todo_diff(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let meta = state.project_index.get_meta(&name);
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !is_authorized(&headers) {
        return Err(make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Unauthorized", ""));
    }
    let base_path = meta.and_then(|m| m.path.as_deref()).map(|p| p.to_string())
        .or_else(|| map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&name)).and_then(|p| p.path.clone()));
    let base_path = match base_path {
        Some(p) => p,
        None => return Err(make_error_response(axum::http::StatusCode::BAD_REQUEST, "Path missing", "")),
    };
    let bp = std::path::Path::new(&base_path);
    let iface_path = bp.join(".lunar/interfaces.yml");
    let todo_path = bp.join(".lunar/ai-todo.json");
    let current = if iface_path.exists() { fs::read_to_string(&iface_path).unwrap_or_default() } else { "**No interfaces.yml found.**".into() };
    let proposed = if todo_path.exists() {
        if let Ok(c) = fs::read_to_string(&todo_path) {
            let v: serde_json::Value = serde_json::from_str(&c).unwrap_or_default();
            v.get("tasks").and_then(|t| t.as_array()).and_then(|a| a.get(0)).and_then(|f| f.get("patch")).and_then(|p| p.as_str()).unwrap_or("**No pending patch.**").to_string()
        } else { "**Read error.**".into() }
    } else { "**No Todo file.**".into() };
    let mut md = format!("# Pending API Contract Diff Analysis: {}\n\n", name);
    md.push_str("As a Peer-Review AI Architect, please examine and audit the proposed YAML patch against the current active contract below.\n\n");
    md.push_str("## 📄 Current interfaces.yml\n```yaml\n");
    md.push_str(&current);
    md.push_str("\n```\n\n## 🚀 Proposed AI Patch (Task Handover)\n```yaml\n");
    md.push_str(&proposed);
    md.push_str("\n```\n");
    write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/todo/diff", name), "GET", &name, Some(".lunar/ai-todo.json"), 200, start.elapsed().as_millis());
    Ok(md.into_response())
}
