use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response, Json, Html},
};
use lunar_serve::{AppState, make_error_response, load_map, write_audit_log};
use lunar_serve::render;
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

pub async fn get_json(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, Response> {
    let start = Instant::now();
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    write_audit_log("127.0.0.1", &HeaderMap::new(), "/lunar-map.json", "GET", "global", None, 200, start.elapsed().as_millis());
    serde_json::to_value(&map).map_err(|e| make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Serialization error", &e.to_string())).map(Json)
}

pub async fn get_markdown(State(state): State<Arc<AppState>>, Query(q): Query<MdQuery>) -> Result<String, Response> {
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    if q.summary { return Ok(render::render_summary(&map)); }
    if let Some(ref scope) = q.scope { return Ok(render::render_project_md(&HeaderMap::new(), &map, scope, state.project_index.get_meta(scope), false)); }
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
    if q.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map).into_response()); }
    let resp = render::render_negotiated_tree(&headers, &map, &name, meta, false)
        .map_err(|(status, err)| make_error_response(status, &err, ""));
    let sc = resp.as_ref().map(|r| r.status().as_u16()).unwrap_or(500);
    write_audit_log("127.0.0.1", &headers, &format!("/api/v1/projects/{}/map", name), "GET", &name, None, sc, start.elapsed().as_millis());
    resp
}

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
    if q.style.as_deref() == Some("mermaid") { return Ok(render::render_mermaid(&map).into_response()); }
    let resp = render::render_negotiated_tree(&headers, &map, &name, meta, false)
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
    render::render_negotiated_tree(&headers, &map, &name, meta, false)
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
    render::render_negotiated_tree(&headers, &map, name, meta, true)
        .map_err(|(status, err)| make_error_response(status, &err, ""))
}

pub async fn healthz() -> &'static str { "OK" }
