use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response, Json},
};
use lunar_serve::{
    AppState, make_error_response, load_map, write_audit_log,
    create_session, validate_session, generate_lct, verify_lct, LctPayload,
    verify_totp, check_rate_limit, record_failure, parse_lunar_patch,
};
use lunar_serve::render;
use crate::handlers::core::MdQuery;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::Deserialize;
use std::sync::Arc;
use std::fs;
use std::time::Instant;
use chrono::Utc;

fn extract_session_id(headers: &HeaderMap) -> Option<String> {
    headers.get(header::COOKIE).and_then(|v| v.to_str().ok())
        .and_then(|c| c.split(';').find_map(|pair| {
            let mut kv = pair.trim().splitn(2, '=');
            if kv.next()? == "session_id" { kv.next().map(|s| s.to_string()) } else { None }
        }))
}

pub async fn handle_setup(State(_state): State<Arc<AppState>>) -> Result<Response, Response> {
    let secret_path = ".lunar/totp.secret";
    if std::path::Path::new(secret_path).exists() {
        let secret = fs::read_to_string(secret_path)
            .map_err(|e| make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Read secret error", &e.to_string()))?;
        let secret = secret.trim();
        let uri = format!("otpauth://totp/LunarAST?secret={}&issuer=LunarAST&digits=6", secret);
        return Ok(Json(serde_json::json!({ "message": "TOTP already configured", "otpauth_uri": uri })).into_response());
    }
    let mut seed = [0u8; 20];
    OsRng.fill_bytes(&mut seed);
    let secret = data_encoding::BASE32_NOPAD.encode(&seed);
    if let Some(parent) = std::path::Path::new(secret_path).parent() { let _ = fs::create_dir_all(parent); }
    fs::write(secret_path, &secret)
        .map_err(|e| make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Write secret error", &e.to_string()))?;
    let uri = format!("otpauth://totp/LunarAST?secret={}&issuer=LunarAST&digits=6", secret);
    Ok(Json(serde_json::json!({ "message": "TOTP secret generated", "otpauth_uri": uri })).into_response())
}

#[derive(Deserialize)]
pub struct LoginRequest { pub totp: String }

pub async fn handle_login(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<Response, Response> {
    let ip = headers.get("X-Forwarded-For").and_then(|v| v.to_str().ok()).unwrap_or("127.0.0.1");
    if let Err(reason) = check_rate_limit(ip, 5, 15, 900) {
        return Err(make_error_response(axum::http::StatusCode::TOO_MANY_REQUESTS, &reason, ""));
    }
    match verify_totp(&body.totp) {
        Ok(true) => {
            let (session_id, csrf_token) = create_session();
            let cookie = format!("session_id={}; HttpOnly; Secure; SameSite=Strict; Path=/", session_id);
            let mut resp = Json(serde_json::json!({ "csrf_token": csrf_token })).into_response();
            resp.headers_mut().insert(header::SET_COOKIE, axum::http::HeaderValue::from_str(&cookie).unwrap());
            Ok(resp)
        }
        Ok(false) => {
            record_failure(ip, 5, 15, 900);
            Err(make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Invalid TOTP", "Check authenticator."))
        }
        Err(e) => {
            record_failure(ip, 5, 15, 900);
            Err(make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "TOTP error", &e))
        }
    }
}

pub async fn handle_csrf_token(headers: HeaderMap) -> Result<Response, Response> {
    let session_id = extract_session_id(&headers).unwrap_or_default();
    match validate_session(&session_id) {
        Some(csrf_token) => Ok(Json(serde_json::json!({ "csrf_token": csrf_token })).into_response()),
        None => Err(make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Invalid session", "Login again.")),
    }
}

#[derive(Deserialize)]
pub struct TokenGenerateRequest { pub duration_minutes: u64, pub owner: String, pub repo: String, pub branch: String }

pub async fn handle_token_generate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<TokenGenerateRequest>,
) -> Result<Response, Response> {
    let session_id = extract_session_id(&headers).unwrap_or_default();
    let csrf_token = headers.get("X-CSRF-Token").and_then(|v| v.to_str().ok()).unwrap_or("");
    let stored_csrf = validate_session(&session_id).ok_or_else(|| make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Invalid session", ""))?;
    if stored_csrf != csrf_token {
        return Err(make_error_response(axum::http::StatusCode::FORBIDDEN, "CSRF token mismatch", "Reload page."));
    }
    let exp = Utc::now().timestamp() as u64 + (body.duration_minutes * 60);
    let payload = LctPayload { exp, owner: body.owner.clone(), repo: body.repo.clone(), branch: body.branch.clone(), scope: "readonly".into() };
    let token = generate_lct(&payload, &state.signing_key)
        .map_err(|e| make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Token generation error", &e))?;
    let base_url = std::env::var("LUNAR_SERVE_DOMAIN").unwrap_or_else(|_| "http://localhost:8787".into());
    let url = format!("{}/t/{}/{}/{}/tree/{}", base_url.trim_end_matches('/'), token, body.owner, body.repo, body.branch);
    Ok(Json(serde_json::json!({ "token": token, "url": url, "expires_at": exp })).into_response())
}

#[derive(Deserialize)]
pub struct DispatchRequest { pub totp: String, pub patch_content: String }

pub async fn handle_dispatch(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<DispatchRequest>,
) -> Result<Response, Response> {
    let ip = headers.get("X-Forwarded-For").and_then(|v| v.to_str().ok()).unwrap_or("127.0.0.1");
    if let Err(reason) = check_rate_limit(ip, 5, 15, 900) {
        return Err(make_error_response(axum::http::StatusCode::TOO_MANY_REQUESTS, &reason, ""));
    }
    let session_id = extract_session_id(&headers).unwrap_or_default();
    let csrf_token = headers.get("X-CSRF-Token").and_then(|v| v.to_str().ok()).unwrap_or("");
    let stored_csrf = validate_session(&session_id).ok_or_else(|| make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Invalid session", ""))?;
    if stored_csrf != csrf_token {
        return Err(make_error_response(axum::http::StatusCode::FORBIDDEN, "CSRF token mismatch", ""));
    }
    match verify_totp(&body.totp) {
        Ok(true) => {},
        Ok(false) => {
            record_failure(ip, 5, 15, 900);
            return Err(make_error_response(axum::http::StatusCode::UNAUTHORIZED, "Invalid TOTP", ""));
        }
        Err(e) => {
            record_failure(ip, 5, 15, 900);
            return Err(make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "TOTP error", &e));
        }
    }
    let parsed = parse_lunar_patch(&body.patch_content);
    let ai_agent = parsed.as_ref().map(|p| p.ai_agent.as_str()).unwrap_or("unknown");
    let patch_type = parsed.as_ref().map(|p| p.patch_type.as_str()).unwrap_or("unknown");
    let staging_dir = ".lunar/suggestions";
    let _ = fs::create_dir_all(staging_dir);
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let filename = format!("{}-{}-ai.yaml", timestamp, patch_type);
    let staging_path = std::path::Path::new(staging_dir).join(&filename);
    fs::write(&staging_path, &body.patch_content)
        .map_err(|e| make_error_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Save patch error", &e.to_string()))?;
    write_audit_log(ip, &headers, "/dispatch", "POST", "staging", Some(&filename), 200, 0);
    Ok(Json(serde_json::json!({ "status": "staged", "file": filename, "ai_agent": ai_agent })).into_response())
}

pub async fn handle_ai_readonly_tree(
    State(state): State<Arc<AppState>>,
    Path((token, owner, repo, branch)): Path<(String, String, String, String)>,
    headers: HeaderMap,
    _query: Query<MdQuery>,
) -> Result<Response, Response> {
    let start = Instant::now();
    let verifying_key = state.signing_key.verifying_key();
    let _lct = verify_lct(&token, &verifying_key, Some(&owner), Some(&repo), Some(&branch))
        .map_err(|e| make_error_response(axum::http::StatusCode::UNAUTHORIZED, &format!("Invalid token: {}", e), "Check LCT."))?;
    let map = load_map(&state.data_path).map_err(|(s,e)| make_error_response(s, &e, ""))?;
    let name = state.project_index.get_name_by_github(&owner, &repo, &branch)
        .or_else(|| map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(&repo)).map(|p| p.name.as_str()))
        .ok_or_else(|| make_error_response(axum::http::StatusCode::NOT_FOUND, "Project not found", "Check owner/repo/branch."))?;
    let meta = state.project_index.get_meta(name);
    let resp = render::render_negotiated_tree(&headers, &map, name, meta, false)
        .map_err(|(status, err)| make_error_response(status, &err, ""));
    write_audit_log("127.0.0.1", &headers, &format!("/t/.../{}/{}/tree/{}", owner, repo, branch), "GET", name, None, 200, start.elapsed().as_millis());
    resp
}
