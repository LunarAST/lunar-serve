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

// ============================================================================
// 🚀 自适应 .gitignore 匹配器 (0 硬编码、自动加载生态契约、不产生多余垃圾)
// ============================================================================
pub struct GitIgnoreMatcher {
    patterns: Vec<String>,
}

impl GitIgnoreMatcher {
    pub fn load_from_dir(dir: &Path) -> Self {
        let mut patterns = Vec::new();

        // 默认全局底线忽略规则
        patterns.push(".git".to_string());
        patterns.push("target".to_string());
        patterns.push("node_modules".to_string());
        patterns.push("dist".to_string());
        patterns.push("tmp".to_string());
        patterns.push("CACHEDIR.TAG".to_string());
        patterns.push("__pycache__".to_string());
        patterns.push(".venv".to_string());
        patterns.push("venv".to_string());
        patterns.push(".idea".to_string());
        patterns.push(".vscode".to_string());
        patterns.push("*.pyc".to_string());
        patterns.push("*.pyo".to_string());
        patterns.push("*.pyd".to_string());
        patterns.push("*.class".to_string());
        patterns.push("*.o".to_string());
        patterns.push("*.exe".to_string());
        patterns.push("*.dll".to_string());
        patterns.push("*.so".to_string());
        patterns.push("*.ds_store".to_string());
        patterns.push("*.lock".to_string());

        // 如果项目存在 .gitignore，则动态读取并追加本地规则 (做对，不做快)
        let ignore_path = dir.join(".gitignore");
        if ignore_path.exists() {
            if let Ok(content) = fs::read_to_string(&ignore_path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let pattern = line.trim_start_matches('/').trim_end_matches('/').to_string();
                    if !pattern.is_empty() {
                        patterns.push(pattern);
                    }
                }
            }
        }
        Self { patterns }
    }

    /// 核心匹配熔断检查
    pub fn is_ignored(&self, path: &Path, root: &Path) -> bool {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.is_empty() {
            return false;
        }

        // 获取当前扫描路径相对于根目录的相对路径，以便支持目录级规则匹配
        let rel_path = path.strip_prefix(root).ok();
        let rel_path_str = rel_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default();

        for pat in &self.patterns {
            // 1. 精确文件名/目录名匹配
            if name == pat {
                return true;
            }
            // 2. 通配符后缀匹配 (e.g. *.jsonl)
            if pat.starts_with("*.") {
                let ext = pat.trim_start_matches("*.");
                if path.extension().and_then(|e| e.to_str()).map_or(false, |e| e.eq_ignore_ascii_case(ext)) {
                    return true;
                }
            }
            // 3. 相对路径前后缀匹配
            if !rel_path_str.is_empty() {
                if rel_path_str == *pat || rel_path_str.starts_with(&format!("{}/", pat)) || rel_path_str.ends_with(&format!("/{}", pat)) {
                    return true;
                }
            }
        }
        false
    }
}

/// Helper function to build a sorted, filtered directory tree for AI context.
pub fn render_directory_tree(base_path_str: &str) -> String {
    let mut tree_string = String::new();
    let path = Path::new(base_path_str);
    let matcher = GitIgnoreMatcher::load_from_dir(path);
    let mut file_count = 0;
    if let Err(e) = traverse_for_tree(path, path, 0, &mut tree_string, &mut file_count, &matcher) {
        return format!("*Error generating file tree: {}*", e);
    }
    // 🚀 清洗掉由于折叠闭合在最末尾或中间产生的空 ```` ``` 代码块，确保 Markdown 语法完美纯净
    tree_string.replace("```\n```\n", "")
}

fn traverse_for_tree(
    root: &Path,
    current: &Path,
    depth: usize,
    output: &mut String,
    file_count: &mut usize,
    matcher: &GitIgnoreMatcher,
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
            
            // 🚀 尊重 .gitignore：自适应匹配熔断
            if matcher.is_ignored(&path, root) {
                continue;
            }

            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let indent = "  ".repeat(depth);

            if path.is_dir() {
                // 判断是否为高噪音元数据目录，是则使用 <details> 静态折叠
                let is_noise = name == ".lunar" 
                    || name == ".backup" 
                    || name == "access-logs" 
                    || name == "suggestions"
                    || name == ".github";

                if is_noise {
                    output.push_str("```\n"); // 闭合外部代码块
                    output.push_str(&format!("{}<details><summary>📁 {}/ (Folded for Humans)</summary>\n\n", indent, name));
                    output.push_str("```\n"); // 重启内部代码块以保持等宽格式
                    
                    output.push_str(&format!("{}- {}/\n", indent, name));
                    traverse_for_tree(root, &path, depth + 1, output, file_count, matcher)?;
                    
                    output.push_str("```\n"); // 闭合内部代码块
                    output.push_str(&format!("{}</details>\n\n", indent));
                    output.push_str("```\n"); // 重开外部代码块，无缝衔接后续节点
                } else {
                    output.push_str(&format!("{}- {}/\n", indent, name));
                    traverse_for_tree(root, &path, depth + 1, output, file_count, matcher)?;
                }
            } else {
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
    let matcher = GitIgnoreMatcher::load_from_dir(path);
    let mut file_count = 0;
    if let Err(_) = traverse_for_json(path, path, 0, &mut files, &mut file_count, &matcher) {
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
    matcher: &GitIgnoreMatcher,
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
            
            // 🚀 JSON 接口也需要对齐 .gitignore 规则，保证全景一致、剔除垃圾
            if matcher.is_ignored(&path, root) {
                continue;
            }

            if path.is_dir() {
                traverse_for_json(root, &path, depth + 1, output, file_count, matcher)?;
            } else {
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
