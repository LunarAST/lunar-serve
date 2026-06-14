mod handlers;

use axum::{
    routing::{get, post},
    Router,
};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rand::RngCore;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use lunar_serve::{
    load_repos, ProjectIndex, AppState,
    load_signing_key, spawn_cleanup_task, purge_old_logs,
};

use handlers::{
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

#[tokio::main]
async fn main() {
    purge_old_logs();
    spawn_cleanup_task();

    let port: u16 = std::env::var("LUNAR_SERVE_PORT").unwrap_or_else(|_| "8787".to_string()).parse().unwrap_or(8787);
    let args: Vec<String> = std::env::args().collect();
    let data_path = if args.len() > 1 { PathBuf::from(&args[1]) } else { PathBuf::from("lunar-map.json") };
    if !data_path.exists() { eprintln!("Error: lunar-map.json not found at {}.", data_path.display()); std::process::exit(1); }
    let base_dir = data_path.parent().unwrap_or(std::path::Path::new("/"));
    let repos_config = load_repos(base_dir);
    let project_index = ProjectIndex::from_config(&repos_config);

    let signing_key = load_signing_key(".lunar/lct-secret.key").unwrap_or_else(|_| {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let key = SigningKey::from_bytes(&seed);
        if let Some(parent) = std::path::Path::new(".lunar/lct-secret.key").parent() { let _ = fs::create_dir_all(parent); }
        let _ = fs::write(".lunar/lct-secret.key", base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, seed));
        key
    });

    let state = Arc::new(AppState { data_path, project_index, signing_key });

    let app = Router::new()
        // Original routes
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
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("lunar-serve listening on http://0.0.0.0:{}", port);
    axum::serve(listener, app).await.unwrap();
}
