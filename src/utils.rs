use axum::http::{StatusCode, HeaderMap};
use chrono::Utc;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Helper function to check if the incoming request is authorized for private repos.
pub fn is_authorized(headers: &HeaderMap) -> bool {
    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    auth.starts_with("Bearer ")
}

/// Validates and reads a single file from the workspace, preventing directory traversal.
pub fn read_secure_file(base_path_str: &str, relative_path_str: &str) -> Result<String, (StatusCode, String, String)> {
    let base_path = Path::new(base_path_str);
    let relative_path = Path::new(relative_path_str);
    let target_path = base_path.join(relative_path);

    let canonical_target = match target_path.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            let hint = format!(
                "The requested file path '{}' does not exist in workspace '{}'. Please double-check your directory tree.",
                relative_path_str, base_path_str
            );
            return Err((StatusCode::NOT_FOUND, "File not found".to_string(), hint));
        }
    };

    let canonical_base = match base_path.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid project base path".to_string(),
                "The workspace path configured in your project map is physically invalid or inaccessible on the VPS disk.".to_string()
            ));
        }
    };

    if !canonical_target.starts_with(canonical_base) {
        return Err((
            StatusCode::FORBIDDEN,
            "Access denied: Path traversal detected".to_string(),
            "Path traversal checks failed. The requested file path lies outside the canonical project workspace.".to_string()
        ));
    }

    fs::read_to_string(canonical_target)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file".to_string(), e.to_string()))
}

/// Helper function to build a sorted, filtered directory tree for AI context.
pub fn render_directory_tree(base_path_str: &str) -> String {
    let mut tree_string = String::new();
    let path = Path::new(base_path_str);
    let mut file_count = 0;
    if let Err(e) = traverse_for_tree(path, path, 0, &mut tree_string, &mut file_count) {
        return format!("*Error generating file tree: {}*", e);
    }
    tree_string
}

fn traverse_for_tree(
    root: &Path,
    current: &Path,
    depth: usize,
    output: &mut String,
    file_count: &mut usize,
) -> std::io::Result<()> {
    if depth > 5 || *file_count > 300 {
        return Ok(());
    }

    if current.is_dir() {
        let mut entries = fs::read_dir(current)?
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();

        entries.sort_by_key(|e| {
            let path = e.path();
            (
                !path.is_dir(),
                path.file_name().unwrap_or_default().to_string_lossy().into_owned()
            )
        });

        for entry in entries {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if name == ".git"
                || name == "target"
                || name == "node_modules"
                || name == "dist"
                || name == "tmp"
                || name == "CACHEDIR.TAG"
                || name == "__pycache__"
                || name == ".venv"
                || name == "venv"
                || name == ".idea"
                || name == ".vscode"
            {
                continue;
            }

            let indent = "  ".repeat(depth);
            if path.is_dir() {
                output.push_str(&format!("{}- {}/\n", indent, name));
                traverse_for_tree(root, &path, depth + 1, output, file_count)?;
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if ext == "pyc"
                    || ext == "pyo"
                    || ext == "pyd"
                    || ext == "class"
                    || ext == "o"
                    || ext == "exe"
                    || ext == "dll"
                    || ext == "so"
                    || ext == "ds_store"
                    || ext == "lock"
                {
                    continue;
                }

                output.push_str(&format!("{}- {}\n", indent, name));
                *file_count += 1;
                if *file_count > 300 {
                    output.push_str(&format!("{}... (truncated due to file count limit)\n", indent));
                    break;
                }
            }
        }
    }
    Ok(())
}

pub fn render_directory_tree_json(base_path_str: &str) -> serde_json::Value {
    let mut files = Vec::new();
    let path = Path::new(base_path_str);
    let mut file_count = 0;
    if let Err(_) = traverse_for_json(path, path, 0, &mut files, &mut file_count) {
        return serde_json::Value::Null;
    }
    serde_json::to_value(files).unwrap_or(serde_json::Value::Null)
}

fn traverse_for_json(
    root: &Path,
    current: &Path,
    depth: usize,
    output: &mut Vec<String>,
    file_count: &mut usize,
) -> std::io::Result<()> {
    if depth > 5 || *file_count > 300 {
        return Ok(());
    }

    if current.is_dir() {
        let mut entries = fs::read_dir(current)?
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();

        entries.sort_by_key(|e| {
            let path = e.path();
            (
                !path.is_dir(),
                path.file_name().unwrap_or_default().to_string_lossy().into_owned()
            )
        });

        for entry in entries {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if name == ".git"
                || name == "target"
                || name == "node_modules"
                || name == "dist"
                || name == "tmp"
                || name == "CACHEDIR.TAG"
                || name == "__pycache__"
                || name == ".venv"
                || name == "venv"
                || name == ".idea"
                || name == ".vscode"
            {
                continue;
            }

            if path.is_dir() {
                traverse_for_json(root, &path, depth + 1, output, file_count)?;
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if ext == "pyc"
                    || ext == "pyo"
                    || ext == "pyd"
                    || ext == "class"
                    || ext == "o"
                    || ext == "exe"
                    || ext == "dll"
                    || ext == "so"
                    || ext == "ds_store"
                    || ext == "lock"
                {
                    continue;
                }

                if let Ok(rel) = path.strip_prefix(root) {
                    output.push(rel.to_string_lossy().into_owned());
                }
                *file_count += 1;
                if *file_count > 300 {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Task A: Write a structured audit log entry in JSON Lines format daily AND print to stdout.
pub fn write_audit_log(
    client_ip: &str,
    headers: &HeaderMap,
    uri: &str,
    method: &str,
    project: &str,
    file_accessed: Option<&str>,
    status: u16,
    duration_ms: u128,
) {
    let log_dir_str = std::env::var("LUNAR_SERVE_LOG_DIR").unwrap_or_else(|_| ".lunar/access-logs".to_string());
    let log_dir = Path::new(&log_dir_str);
    if !log_dir.exists() {
        let _ = fs::create_dir_all(log_dir);
    }

    let user_agent = headers.get("User-Agent").and_then(|v| v.to_str().ok()).unwrap_or("unknown");
    let ua_lower = user_agent.to_lowercase();
    let is_ai_agent = ua_lower.contains("bot") 
        || ua_lower.contains("agent") 
        || ua_lower.contains("chatgpt") 
        || ua_lower.contains("claude") 
        || ua_lower.contains("gpt") 
        || ua_lower.contains("oai") 
        || ua_lower.contains("deepseek") 
        || ua_lower.contains("cursor") 
        || ua_lower.contains("bridge");

    let auth_status = if is_authorized(headers) { "ValidBearer" } else { "Public" };

    let log_entry = json!({
        "timestamp": Utc::now().to_rfc3339(),
        "clientIp": client_ip,
        "userAgent": user_agent,
        "isAiAgent": is_ai_agent,
        "method": method,
        "uri": uri,
        "project": project,
        "fileAccessed": file_accessed,
        "status": status,
        "durationMs": duration_ms,
        "authStatus": auth_status
    });

    let today = Utc::now().format("%Y-%m-%d").to_string();
    let log_file_path = log_dir.join(format!("{}.jsonl", today));

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file_path) {
        if let Ok(line) = serde_json::to_string(&log_entry) {
            let _ = writeln!(file, "{}", line);
            println!("{}", line);
        }
    }
}

/// Task A: Background clean up daemon to purge logs older than the retention configuration.
pub fn purge_old_logs() {
    let log_dir_str = std::env::var("LUNAR_SERVE_LOG_DIR").unwrap_or_else(|_| ".lunar/access-logs".to_string());
    let retention_days_str = std::env::var("LUNAR_SERVE_LOG_RETENTION_DAYS").unwrap_or_else(|_| "30".to_string());
    let retention_days: i64 = retention_days_str.parse().unwrap_or(30);

    let log_dir = Path::new(&log_dir_str);
    if !log_dir.is_dir() {
        return;
    }

    println!("🧹 Running automated Log Purge Daemon (Retention: {} days)...", retention_days);
    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.ends_with(".jsonl") {
                        let date_str = file_name.trim_end_matches(".jsonl");
                        if let Ok(file_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                            let today = Utc::now().date_naive();
                            let age = today.signed_duration_since(file_date).num_days();
                            if age > retention_days {
                                if let Ok(_) = fs::remove_file(&path) {
                                    println!("  ✓ Purged expired log file: {}", file_name);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Helper date parser to resolve date calculations securely.
struct NaiveDate;
impl NaiveDate {
    fn parse_from_str(s: &str, fmt: &str) -> Result<chrono::NaiveDate, chrono::ParseError> {
        chrono::NaiveDate::parse_from_str(s, fmt)
    }
}

/// Decode a hex string into bytes (used by lct module).
pub fn hex_decode(hex_str: &str) -> Result<Vec<u8>, String> {
    let hex_str = hex_str.trim();
    if hex_str.len() % 2 != 0 {
        return Err("Odd hex string length".into());
    }
    (0..hex_str.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_str[i..i+2], 16).map_err(|e| e.to_string()))
        .collect()
}
