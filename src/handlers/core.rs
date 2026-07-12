use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response, Json, Html},
};
use crate::{AppState, make_error_response, load_map, write_audit_log, is_authorized};
use crate::render;
use serde::Deserialize;
use std::fs;
use std::sync::Arc;
use std::time::Instant;
use std::path::PathBuf;

#[derive(Deserialize, Default)]
pub struct MdQuery {
    #[serde(default)]
    pub summary: bool,
    pub scope: Option<String>,
    pub status: Option<String>,
    pub path: Option<String>,
    pub style: Option<String>,
    // New structured query params
    #[serde(default)]
    pub q: Option<String>,      // search term for path/method
    pub method: Option<String>, // filter by HTTP method
    #[serde(rename = "type")]
    pub filter_type: Option<String>, // "exposed" or "consumed"
    #[serde(default)]
    pub format: Option<String>, // "json" to force JSON response
}

pub async fn get_index(State(state): State<Arc<AppState>>) -> Result<Html<String>, Response> {
    let base_dir = state.data_path.parent().unwrap_or(std::path::Path::new("/"));
    let candidates = vec![
        base_dir.join("LunarAST/lunar-scope/dist/index.html"),
        base_dir.join("lunar-scope/dist/index.html"),
        PathBuf::from("/opt/LunarAST/lunar-scope/dist/index.html"),
    ];
    let path = candidates.into_iter().find(|p| p.exists()).ok_or_else(|| {
        make_error_response(axum::http::StatusCode::NOT_FOUND, "lunar-scope compiled index.html not found", "Run 'npm run build' in lunar-scope.")
    })?;
    fs::read_to_string(path).map(Html).map_err(|e| make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Read index.html error", &e.to_string()))
}

pub async fn get_json(State(state): State<Arc<AppState>>, headers: HeaderMap, Query(q): Query<MdQuery>) -> Result<Json<serde_json::Value>, Response> {
    let start = Instant::now();
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    write_audit_log("127.0.0.1", &headers, "/lunar-map.json", "GET", "global", None, 200, start.elapsed().as_millis());

    if q.summary {
        let projects: Vec<serde_json::Value> = map.projects.iter().map(|p| {
            let exp_count = p.interfaces.get("exposed").and_then(|e| e.as_array()).map_or(0, |a| a.len());
            let con_count = p.interfaces.get("consumed").and_then(|c| c.as_array()).map_or(0, |a| a.len());
            serde_json::json!({
                "name": p.name,
                "type": p.project_type,
                "exposed_count": exp_count,
                "consumed_count": con_count,
                "scan_status": p.scan_status,
                "path": p.path,
            })
        }).collect();
        let summary = serde_json::json!({
            "projects": projects,
            "total_projects": map.projects.len(),
        });
        return Ok(Json(summary));
    }

    serde_json::to_value(&map)
        .map_err(|e| make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Serialization error", &e.to_string()))
        .map(Json)
}

pub async fn get_markdown(State(state): State<Arc<AppState>>, Query(q): Query<MdQuery>) -> Result<String, Response> {
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    if q.summary { return Ok(render::render_summary(&map)); }
    if let Some(ref scope) = q.scope { return Ok(render::render_project_md(&HeaderMap::new(), &map, scope, state.project_index.get_meta(scope), false, None, None)); }
    if let Some(ref status) = q.status { return Ok(render::render_status_filter(&map, status)); }
    if let Some(ref path) = q.path { return Ok(render::render_path_filter(&map, path)); }
    if let Some(ref style) = q.style { if style == "mermaid" { return Ok(render::render_mermaid(&map)); } }
    let mut md = String::from("# Ecosystem Topology\n\n");
    for p in &map.projects { md.push_str(&format!("## {}\n", p.name)); }
    Ok(md)
}

pub async fn get_project_md_api(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(q): Query<MdQuery>
) -> Result<Response, Response> {
    let start = Instant::now();
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let meta = state.project_index.get_meta(&name);

    // Authentication check for private projects
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !is_authorized(&headers) {
        // Check for LCT token in Authorization header
        let has_lct = headers.get("Authorization")
            .and_then(|v| v.to_str().ok())
            .map_or(false, |a| a.starts_with("Bearer "));
        if !has_lct {
            return Err(make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Private project", "Login required or provide LCT token."));
        }
    }

    // Determine if JSON response is desired
    let want_json = q.format.as_deref() == Some("json") || headers.get("Accept").and_then(|v| v.to_str().ok()).map_or(false, |a| a.contains("application/json"));

    if q.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map).into_response()); }

    // If structured query requested, produce filtered JSON
    if want_json && (q.q.is_some() || q.method.is_some() || q.filter_type.is_some()) {
        let project = map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&name));
        if project.is_none() {
            return Ok(Json(serde_json::json!({"error": "Project not found"})).into_response());
        }
        let p = project.unwrap();
        let exposed = p.interfaces.get("exposed").and_then(|e| e.as_array());
        let consumed = p.interfaces.get("consumed").and_then(|e| e.as_array());
        let mut results = Vec::new();

        let filter_method = q.method.as_deref().map(|m| m.to_uppercase());
        let search_term = q.q.as_deref().map(|s| s.to_lowercase());

        // Helper to check a single interface entry
        let matches = |item: &serde_json::Value, default_status: &str| -> bool {
            let method = item.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let path = item.get("path").and_then(|p| p.as_str()).unwrap_or("");
            let status = item.get("status").and_then(|v| v.as_str()).unwrap_or(default_status);
            if let Some(ref ft) = filter_method {
                if method.to_uppercase() != *ft { return false; }
            }
            if let Some(ref q) = search_term {
                if !method.to_lowercase().contains(q) && !path.to_lowercase().contains(q) && !status.to_lowercase().contains(q) {
                    return false;
                }
            }
            true
        };

        // Exposed
        if q.filter_type.as_deref() != Some("consumed") {
            if let Some(exp) = exposed {
                for item in exp {
                    if matches(item, "aligned") {
                        results.push(serde_json::json!({
                            "type": "exposed",
                            "method": item["method"],
                            "path": item["path"],
                            "status": item.get("status").and_then(|v| v.as_str()).unwrap_or("aligned")
                        }));
                    }
                }
            }
        }
        // Consumed
        if q.filter_type.as_deref() != Some("exposed") {
            if let Some(con) = consumed {
                for item in con {
                    if matches(item, "aligned") {
                        results.push(serde_json::json!({
                            "type": "consumed",
                            "method": item["method"],
                            "path": item["path"],
                            "target_project": item.get("targetProject"),
                            "status": item.get("status").and_then(|v| v.as_str()).unwrap_or("aligned")
                        }));
                    }
                }
            }
        }

        let response = serde_json::json!({
            "project": name,
            "query": {
                "method": q.method,
                "q": q.q,
                "type": q.filter_type,
            },
            "count": results.len(),
            "results": results
        });
        let sc = 200;
        write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/map", name), "GET", &name, None, sc, start.elapsed().as_millis());
        return Ok(Json(response).into_response());
    }

    // Default behavior: Markdown or JSON without filters
    let resp = render::render_negotiated_tree(&headers, &map, &name, meta, false, None, None)
        .map_err(|(status, err)| make_error_response(status, &err, ""));
    let sc = resp.as_ref().map(|r| r.status().as_u16()).unwrap_or(500);
    write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/map", name), "GET", &name, None, sc, start.elapsed().as_millis());
    resp
}

// Remainder unchanged
pub async fn get_project_md_github(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((owner, repo, branch)): Path<(String, String, String)>,
    Query(q): Query<MdQuery>
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
                    write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/tree/{}", owner, repo, branch), "GET", "unknown", None, 404, start.elapsed().as_millis());
                    return Err(r);
                }
            }
        }
    };
    let meta = state.project_index.get_meta(&name);
    let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
    if visibility == "private" && !is_authorized(&headers) {
        return Err(make_error_response(
            axum::http::StatusCode::UNAUTHORIZED,
            "Private project",
            "This project is private. Please login or provide a valid LCT token."
        ));
    }
    if q.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map).into_response()); }
    let resp = render::render_negotiated_tree(&headers, &map, &name, meta, visibility == "private", None, Some(&branch))
        .map_err(|(status, err)| make_error_response(status, &err, ""));
    let sc = resp.as_ref().map(|r| r.status().as_u16()).unwrap_or(500);
    write_audit_log("127.0.0.1", &headers, &format!("/{}/{}/tree/{}", owner, repo, branch), "GET", &name, None, sc, start.elapsed().as_millis());
    resp
}

pub async fn get_project_md_legacy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(q): Query<MdQuery>
) -> Result<Response, Response> {
    let name = name.trim_end_matches(".md").trim_end_matches(".json");
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let meta = state.project_index.get_meta(&name);
    if q.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map).into_response()); }
    render::render_negotiated_tree(&headers, &map, &name, meta, false, None, None)
        .map_err(|(status, err)| make_error_response(status, &err, ""))
}

pub async fn get_private_project_md(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: axum::http::Request<axum::body::Body>
) -> Result<Response, Response> {
    let path = req.uri().path();
    let name = path.trim_start_matches("/private/project/").trim_end_matches(".md").trim_end_matches(".json");
    if name.is_empty() { return Err(make_error_response(axum::http::StatusCode::BAD_REQUEST, "Missing project name", "")); }
    let auth = req.headers().get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !auth.starts_with("Bearer ") { return Err(make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Missing JWT", "Private routes require Bearer token.")); }
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let meta = state.project_index.get_meta(name);
    render::render_negotiated_tree(&headers, &map, name, meta, true, None, None)
        .map_err(|(status, err)| make_error_response(status, &err, ""))
}

pub async fn healthz() -> &'static str { "OK" }
