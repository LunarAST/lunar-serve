use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use crate::{AppState, make_error_response, load_map, write_audit_log, is_authorized, read_secure_file};
use std::sync::Arc;
use std::time::Instant;

pub async fn get_raw_file_api(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((name, filepath)): Path<(String, String)>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let meta = state.project_index.get_meta(&name);
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !is_authorized(&headers) {
        let r = make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Unauthorized", "Bearer token required.");
        write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/raw/{}", name, filepath), "GET", &name, Some(&filepath), 401, start.elapsed().as_millis());
        return Err(r);
    }
    let base_path = meta.and_then(|m| m.path.as_deref()).map(|p| p.to_string())
        .or_else(|| map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&name)).and_then(|p| p.path.clone()));
    let base_path = match base_path {
        Some(p) => p,
        None => {
            let r = make_error_response(axum::http::StatusCode::BAD_REQUEST, "Path missing", "No workspace path.");
            write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/raw/{}", name, filepath), "GET", &name, Some(&filepath), 400, start.elapsed().as_millis());
            return Err(r);
        }
    };
    let result = read_secure_file(&base_path, &filepath);
    let sc = match &result { Ok(_) => 200, Err((s,_,_)) => s.as_u16() };
    write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/raw/{}", name, filepath), "GET", &name, Some(&filepath), sc, start.elapsed().as_millis());
    result.map(|c| c.into_response()).map_err(|(status, err, hint)| make_error_response(status, &err, &hint))
}

pub async fn get_raw_file_github(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, repo, branch, filepath)): Path<(String, String, String, String)>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let branch = branch.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let name = match state.project_index.get_name_by_github(&owner, &repo, &branch) {
        Some(n) => n.to_string(),
        None => {
            let matched = map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&repo)).map(|p| p.name.clone());
            match matched {
                Some(n) => n,
                None => {
                    let r = make_error_response(axum::http::StatusCode::NOT_FOUND, "Project not found", "Check owner/repo/branch.");
                    write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/raw/{}/{}", owner, repo, branch, filepath), "GET", "unknown", Some(&filepath), 404, start.elapsed().as_millis());
                    return Err(r);
                }
            }
        }
    };
    let meta = state.project_index.get_meta(&name);
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !is_authorized(&headers) {
        let r = make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Unauthorized", "Bearer token required.");
        write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/raw/{}/{}", owner, repo, branch, filepath), "GET", &name, Some(&filepath), 401, start.elapsed().as_millis());
        return Err(r);
    }
    let base_path = meta.and_then(|m| m.path.as_deref()).map(|p| p.to_string())
        .or_else(|| map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&name)).and_then(|p| p.path.clone()));
    let base_path = match base_path {
        Some(p) => p,
        None => {
            let r = make_error_response(axum::http::StatusCode::BAD_REQUEST, "Path missing", "No workspace path.");
            write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/raw/{}/{}", owner, repo, branch, filepath), "GET", &name, Some(&filepath), 400, start.elapsed().as_millis());
            return Err(r);
        }
    };
    let result = read_secure_file(&base_path, &filepath);
    let sc = match &result { Ok(_) => 200, Err((s,_,_)) => s.as_u16() };
    write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/raw/{}/{}", owner, repo, branch, filepath), "GET", &name, Some(&filepath), sc, start.elapsed().as_millis());
    result.map(|c| c.into_response()).map_err(|(status, err, hint)| make_error_response(status, &err, &hint))
}
