use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode, header},
};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rand::RngCore;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

use lunar_serve::{
    AppState, ProjectIndex, ReposConfig,
    generate_lct, LctPayload, build_app,
};
use lunar_serve::session::clear_all_sessions;
use lunar_serve::rate_limiter::clear_all_limits;

struct TestContext {
    _temp: TempDir,
    _guard: ChangeDir,
    state: Arc<AppState>,
    totp_secret: String,
}

struct ChangeDir(Option<std::path::PathBuf>);
impl ChangeDir {
    fn new(target: &Path) -> Self {
        let original = std::env::current_dir().ok();
        std::env::set_current_dir(target).expect("Failed to set current dir");
        Self(original)
    }
}
impl Drop for ChangeDir {
    fn drop(&mut self) {
        if let Some(original) = self.0.take() {
            let _ = std::env::set_current_dir(&original);
        }
    }
}

async fn setup_test_env() -> TestContext {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();

    let public_dir = root.join("public-proj");
    let private_dir = root.join("private-proj");
    fs::create_dir_all(&public_dir).unwrap();
    fs::create_dir_all(&private_dir).unwrap();

    fs::write(public_dir.join("README.md"), "# Public").unwrap();
    fs::write(private_dir.join("README.md"), "# Private").unwrap();
    let lunar_priv = private_dir.join(".lunar");
    fs::create_dir_all(&lunar_priv).unwrap();
    fs::write(lunar_priv.join("ai-instruction.md"), "> AI instruction").unwrap();
    fs::write(lunar_priv.join("interfaces.yml"), "").unwrap();
    fs::create_dir_all(root.join(".lunar/access-logs")).unwrap();

    // Fixed: add required "sha" field to project objects
    let map = serde_json::json!({
        "version": "1.0",
        "projects": [
            {
                "name": "public-proj",
                "type": "mixed",
                "sha": "",
                "scanStatus": "success",
                "interfaces": { "exposed": [], "consumed": [] },
                "path": public_dir.to_str().unwrap()
            },
            {
                "name": "private-proj",
                "type": "service",
                "sha": "",
                "scanStatus": "success",
                "interfaces": { "exposed": [], "consumed": [] },
                "path": private_dir.to_str().unwrap()
            }
        ],
        "alignments": [],
        "aggregatedEdges": [],
        "anomalies": { "unusedEndpoints": [], "orphanedConsumers": [], "crossLayerViolations": [] }
    });
    fs::write(root.join("lunar-map.json"), map.to_string()).unwrap();

    let repos = serde_json::json!({
        "version": "0.5.0",
        "projects": [
            {
                "name": "public-proj",
                "visibility": "public",
                "path": public_dir.to_str().unwrap()
            },
            {
                "name": "private-proj",
                "visibility": "private",
                "path": private_dir.to_str().unwrap()
            }
        ]
    });
    fs::write(root.join("repos.json"), repos.to_string()).unwrap();

    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let key_path = root.join(".lunar/lct-secret.key");
    fs::create_dir_all(key_path.parent().unwrap()).unwrap();
    fs::write(&key_path, base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, seed)).unwrap();

    let totp_seed = "JBSWY3DPEHPK3PXP";
    let totp_path = root.join(".lunar/totp.secret");
    fs::write(&totp_path, totp_seed).unwrap();

    let _guard = ChangeDir::new(&root);

    let data_path = root.join("lunar-map.json");
    let repos_config: ReposConfig = serde_json::from_str(&fs::read_to_string(root.join("repos.json")).unwrap()).unwrap();
    let project_index = ProjectIndex::from_config(&repos_config);
    let state = Arc::new(AppState {
        data_path,
        project_index,
        signing_key,
    });

    TestContext {
        _temp: tmp,
        _guard,
        state,
        totp_secret: totp_seed.to_string(),
    }
}

#[tokio::test]
async fn test_login_valid_totp() {
    let ctx = setup_test_env().await;
    clear_all_sessions();
    clear_all_limits();
    let app = build_app(ctx.state);

    let expected = totp_lite::totp::<totp_lite::Sha1>(ctx.totp_secret.as_bytes(), chrono::Utc::now().timestamp() as u64);
    let req = Request::post("/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({ "totp": expected }).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_login_invalid_totp() {
    let ctx = setup_test_env().await;
    clear_all_sessions();
    clear_all_limits();
    let app = build_app(ctx.state);
    let req = Request::post("/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({ "totp": "000000" }).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_csrf_protection_on_dispatch() {
    let ctx = setup_test_env().await;
    clear_all_sessions();
    clear_all_limits();
    let app = build_app(ctx.state.clone());

    let expected = totp_lite::totp::<totp_lite::Sha1>(ctx.totp_secret.as_bytes(), chrono::Utc::now().timestamp() as u64);
    let req = Request::post("/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({ "totp": expected }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let cookies = resp.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_owned();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let session_id = cookies.split(';').next().unwrap().trim().strip_prefix("session_id=").unwrap().to_string();
    let csrf_token = json["csrf_token"].as_str().unwrap().to_string();

    let req = Request::post("/dispatch")
        .header("Content-Type", "application/json")
        .header("Cookie", format!("session_id={}", session_id))
        .body(Body::from(serde_json::json!({ "totp": expected, "patch_content": "test" }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let req = Request::post("/dispatch")
        .header("Content-Type", "application/json")
        .header("Cookie", format!("session_id={}", session_id))
        .header("X-CSRF-Token", csrf_token)
        .body(Body::from(serde_json::json!({ "totp": expected, "patch_content": "test" }).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_lct_private_project_access() {
    let ctx = setup_test_env().await;
    clear_all_sessions();
    clear_all_limits();
    let app = build_app(ctx.state.clone());

    let exp = chrono::Utc::now().timestamp() as u64 + 3600;
    let payload = LctPayload {
        exp,
        owner: "me".into(),
        repo: "private-proj".into(),
        branch: "main".into(),
        scope: "readonly".into(),
    };
    let token = generate_lct(&payload, &ctx.state.signing_key).unwrap();

    let uri = format!("/t/{}/me/private-proj/tree/main", token);
    let req = Request::get(&uri).body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    if !resp.status().is_success() {
        let status = resp.status();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap_or_default();
        panic!("LCT access failed with {}: {}", status, String::from_utf8_lossy(&body));
    }
}

#[tokio::test]
async fn test_setup_requires_auth() {
    let ctx = setup_test_env().await;
    clear_all_sessions();
    clear_all_limits();
    let app = build_app(ctx.state.clone());

    let req = Request::get("/setup").body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let expected = totp_lite::totp::<totp_lite::Sha1>(ctx.totp_secret.as_bytes(), chrono::Utc::now().timestamp() as u64);
    let req = Request::post("/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({ "totp": expected }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let cookies = resp.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap();
    let session_id = cookies.split(';').next().unwrap().trim().strip_prefix("session_id=").unwrap();

    let req = Request::get("/setup")
        .header("Cookie", format!("session_id={}", session_id))
        .body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_rate_limiter_blocks_after_failures() {
    let ctx = setup_test_env().await;
    clear_all_sessions();
    clear_all_limits();
    let app = build_app(ctx.state);

    for _ in 0..5 {
        let req = Request::post("/login")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::json!({ "totp": "000000" }).to_string()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    let req = Request::post("/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::json!({ "totp": "000000" }).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(!resp.status().is_success());
    assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
}
