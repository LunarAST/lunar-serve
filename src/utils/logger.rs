use axum::http::HeaderMap;
use chrono::Utc;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

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
    
    // 🚀 升级：拓展 AI 智能体与开发端 API 客户端识别，精准拦截 Go-http-client、requests、axios、reqwest 等代理
    let is_ai_agent = ua_lower.contains("bot") 
        || ua_lower.contains("agent") 
        || ua_lower.contains("chatgpt") 
        || ua_lower.contains("claude") 
        || ua_lower.contains("gpt") 
        || ua_lower.contains("oai") 
        || ua_lower.contains("deepseek") 
        || ua_lower.contains("cursor") 
        || ua_lower.contains("bridge")
        || ua_lower.contains("go-http-client")
        || ua_lower.contains("reqwest")
        || ua_lower.contains("python")
        || ua_lower.contains("http")
        || ua_lower.contains("axios")
        || ua_lower.contains("fetch");

    let auth_status = if crate::utils::is_authorized(headers) { "ValidBearer" } else { "Public" };

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
            let _ = writeln!(file, "{}", line); // 机器写：保持全息 jsonl 格式不变 [1]

            // 🚀 人类读：全新“双行全息日志结构”，兼顾极致美观与 100% 信息对齐 (HCI 深度重构) [1]
            let time_str = Utc::now().format("%H:%M:%S").to_string();
            let agent_icon = if is_ai_agent { "🤖 AI" } else { "🧑 HU" };
            
            // 终端动作着色
            let method_color = match method {
                "GET" => "\x1b[36m",    // 青色
                "POST" => "\x1b[32m",   // 绿色
                "PUT" => "\x1b[33m",    // 黄色
                "DELETE" => "\x1b[31m", // 红色
                _ => "\x1b[37m",        // 白色
            };
            
            let status_color = if status < 400 {
                "\x1b[32m" // 成功显绿
            } else {
                "\x1b[31m" // 报错显红
            };
            
            let reset = "\x1b[0m";
            let gray = "\x1b[90m"; // 优雅的暗灰色，防止抢占主行视觉焦点

            // 第 1 行：核心链路流 (时间 | 方法 | 路径)
            println!(
                "{} | {}{:<4}{} | {}",
                time_str,
                method_color, method, reset,
                uri
            );
            
            // 第 2 行：全息元数据卡片 (暗灰呈现，保留 100% 精确数据，包括完整 UA 浏览器指纹) [1]
            println!(
                "  {}↳ Status: {}{}{} | Time: {:>3}ms | Project: {} | Auth: {} | Agent: {} | IP: {} | UA: {}{}",
                gray,
                status_color, status, gray,
                duration_ms,
                project,
                auth_status,
                agent_icon,
                client_ip,
                user_agent,
                reset
            );
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
