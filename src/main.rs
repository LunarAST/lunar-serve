use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rand::RngCore;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use lunar_serve::{
    load_repos, ProjectIndex, AppState,
    load_signing_key, spawn_cleanup_task, purge_old_logs, build_app,
};

#[tokio::main]
async fn main() {
    purge_old_logs();
    spawn_cleanup_task();

    // 写入 PID 文件
    let pid = std::process::id();
    if let Some(parent) = std::path::Path::new(".lunar").parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(".lunar/lunar-serve.pid", pid.to_string()).unwrap();

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
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("lunar-serve listening on http://0.0.0.0:{}", port);
    axum::serve(listener, app).await.unwrap();
}
