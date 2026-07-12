use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, Json},
};
use lunar_interface::{AlignmentEntry, LunarMap};
use crate::{ProjectMeta, utils::{render_directory_tree, render_directory_tree_json}};
use std::collections::HashMap;
use std::fs;
use std::sync::{Mutex, OnceLock};

// 🚀 全局静态目录树缓存（进程生存期内仅首次扫描磁盘，后续 0ms 内存秒回，极致节省 CPU/IO 算力）
static TREE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
static JSON_TREE_CACHE: OnceLock<Mutex<HashMap<String, serde_json::Value>>> = OnceLock::new();

fn get_cached_directory_tree(path: &str) -> String {
    let cache = TREE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut lock = cache.lock().unwrap();
    if let Some(tree) = lock.get(path) {
        return tree.clone();
    }
    let tree = render_directory_tree(path);
    lock.insert(path.to_string(), tree.clone());
    tree
}

fn get_cached_directory_tree_json(path: &str) -> serde_json::Value {
    let cache = JSON_TREE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut lock = cache.lock().unwrap();
    if let Some(tree) = lock.get(path) {
        return tree.clone();
    }
    let tree = render_directory_tree_json(path);
    lock.insert(path.to_string(), tree.clone());
    tree
}

/// Renders either JSON or Markdown project tree based on the HTTP Accept header.
/// Accepts an optional LCT token for AI-friendly URL generation and a request branch.
pub fn render_negotiated_tree(
    headers: &HeaderMap,
    map: &LunarMap,
    name: &str,
    meta: Option<&ProjectMeta>,
    is_authenticated: bool,
    token: Option<&str>,
    request_branch: Option<&str>, // NEW: branch from request, overrides meta
) -> Result<Response, (StatusCode, String)> {
    let accept_header = headers.get("Accept").and_then(|v| v.to_str().ok()).unwrap_or("");

    // Resolve base path via secondary fallback strategy
    let base_path: Option<String> = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|proj| proj.name.eq_ignore_ascii_case(name))
                .and_then(|proj| proj.path.clone())
        });

    if accept_header.contains("application/json") {
        if let Some(ref path) = base_path {
            if std::path::Path::new(path).exists() {
                // 🚀 使用缓存的 JSON 目录树
                let json_tree = get_cached_directory_tree_json(path);
                return Ok(Json(json_tree).into_response());
            }
        }
        return Ok(Json(serde_json::Value::Null).into_response());
    }

    // Default to Markdown with embedded file tree outline
    let md = render_project_md(headers, map, name, meta, is_authenticated, token, request_branch);
    Ok(md.into_response())
}

pub fn render_summary(map: &LunarMap) -> String {
    let mut md = String::from("# Ecosystem Summary\n\n");
    let total_exposed: usize = map.projects.iter().map(|p| p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len())).sum();
    let total_consumed: usize = map.projects.iter().map(|p| p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len())).sum();
    let orphaned = map.anomalies.orphaned_consumers.len();
    let unused = map.anomalies.unused_endpoints.len();
    md.push_str(&format!("- Projects: {}\n- Total Exposed Endpoints: {}\n- Total Consumed Dependencies: {}\n- Anomalies: {} orphaned, {} unused\n\n", map.projects.len(), total_exposed, total_consumed, orphaned, unused));
    md.push_str("## Projects\n\n| Name | Type | Exposed | Consumed | Status |\n|:---|:---|:---|:---|:---|\n");
    for p in &map.projects {
        let exp = p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
        let con = p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
        md.push_str(&format!("| {} | {} | {} | {} | {} |\n", p.name, p.project_type, exp, con, p.scan_status));
    }
    if orphaned + unused > 0 {
        md.push_str("\n## Top Risks\n");
        for ep in &map.anomalies.orphaned_consumers { md.push_str(&format!("1. **Orphaned**: {} calls `{} {}` but target not found.\n", ep.project, ep.method, ep.path)); }
        for ep in &map.anomalies.unused_endpoints { md.push_str(&format!("1. **Unused**: {} exposes `{} {}` but no consumer.\n", ep.project, ep.method, ep.path)); }
    }
    md
}

pub fn render_project_md(
    headers: &HeaderMap,
    map: &LunarMap,
    name: &str,
    meta: Option<&ProjectMeta>,
    is_authenticated: bool,
    token: Option<&str>,
    request_branch: Option<&str>, // NEW: branch from request, overrides meta
) -> String {
    let project = map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(name));
    if project.is_none() { return format!("# Project `{}` not found.\n", name); }
    let p = project.unwrap();
    let exp = p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
    let con = p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
    
    let request_host = std::env::var("LUNAR_SERVE_DOMAIN").ok()
        .and_then(|d| d.strip_prefix("https://").or_else(|| d.strip_prefix("http://")).map(|s| s.to_string()))
        .unwrap_or_else(|| {
            headers.get("Host").and_then(|v| v.to_str().ok()).unwrap_or("127.0.0.1:8787").to_string()
        });
        
    let git_owner = meta.and_then(|m| m.github.as_ref()).map(|g| g.owner.as_str()).unwrap_or("Jasonmilk");
    let git_repo = meta.and_then(|m| m.github.as_ref()).map(|g| g.repo.as_str()).unwrap_or(name);
    // Use request_branch if provided, otherwise fallback to meta branch or default
    let git_branch = request_branch
        .or_else(|| meta.and_then(|m| m.github.as_ref()).map(|g| g.branch.as_str()))
        .unwrap_or("rs2");

    let base_path: Option<String> = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|proj| proj.name.eq_ignore_ascii_case(name))
                .and_then(|proj| proj.path.clone())
        });

    let mut md = String::new();

    // ------------------------------------------------------------------------
    // AI Instruction Loading — Config-Driven, Project-Agnostic
    // ------------------------------------------------------------------------
    let instruction_path = std::path::Path::new("lunar-serve/config/ai-instruction.md");
    if instruction_path.exists() {
        if let Ok(inst_content) = fs::read_to_string(instruction_path) {
            let rendered = inst_content
                .replace("{project_name}", name)
                .replace("{base_url}", &request_host)
                .replace("{branch}", git_branch)
                .replace("{owner}", git_owner)
                .replace("{repo}", git_repo);
            md.push_str(&rendered);
            md.push_str("\n\n---\n\n");
        }
    }
    // ------------------------------------------------------------------------

    md.push_str(&format!("# Project: {}\n\n- Type: {}\n- Scan Status: {}\n- Exposed: {}\n- Consumed: {}\n\n", p.name, p.project_type, p.scan_status, exp, con));
    if let Some(interfaces) = p.interfaces.as_object() {
        if let Some(exposed) = interfaces.get("exposed").and_then(|e| e.as_array()) {
            md.push_str("## Exposed Endpoints\n\n| Method | Path |\n|:---|:---|\n");
            for e in exposed { md.push_str(&format!("| {} | {} |\n", e["method"].as_str().unwrap_or(""), e["path"].as_str().unwrap_or(""))); }
            md.push('\n');
        }
        if let Some(consumed) = interfaces.get("consumed").and_then(|e| e.as_array()) {
            md.push_str("## Consumed Dependencies\n\n| Method | Path | Target |\n|:---|:---|:---|\n");
            for c in consumed { md.push_str(&format!("| {} | {} | {} |\n", c["method"].as_str().unwrap_or(""), c["path"].as_str().unwrap_or(""), c.get("targetProject").and_then(|t| t.as_str()).unwrap_or("?"))); }
            md.push('\n');
        }
    }
    let relevant: Vec<&AlignmentEntry> = map.alignments.iter().filter(|a| a.client_project.eq_ignore_ascii_case(name) || a.server_project.eq_ignore_ascii_case(name)).collect();
    if !relevant.is_empty() {
        md.push_str("## Alignments\n\n| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n");
        for a in relevant { md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status)); }
    }
    if let Some(meta) = meta {
        if let Some(ref url) = meta.archive_url {
            if meta.visibility == "public" || is_authenticated { md.push_str(&format!("\n📦 [Download Source Archive]({})\n", url)); }
        }
    }

    if let Some(ref path) = base_path {
        let path_obj = std::path::Path::new(path);
        if path_obj.exists() {
            md.push_str("\n---\n## 📂 Workspace File Tree (AI Navigation Guide)\n\n");
            md.push_str("To view raw file content on-demand, use the `/raw` endpoint matching these paths.\n\n");
            md.push_str("```\n");
            md.push_str(&format!("# Repository: {}/{}\n", git_owner, git_repo));
            md.push_str(&format!("# Branch: {} (Ecosystem automatic path discovery)\n", git_branch));
            
            let base_url = if let Some(tok) = token {
                format!("/t/{}/{}/{}/raw/{}", tok, git_owner, git_repo, git_branch)
            } else {
                format!("/raw/{}", git_branch)
            };
            md.push_str(&format!("# To read project manual: fetch {}/README.md\n", base_url));
            md.push_str(&format!("# To read any source file: request {}/<filepath>\n\n", base_url));
            
            // 🚀 使用带静态缓存的扫盘渲染
            md.push_str(&get_cached_directory_tree(path));
            md.push_str("```\n");

            let todo_path = path_obj.join(".lunar/ai-todo.json");
            if todo_path.exists() {
                if let Ok(todo_content) = std::fs::read_to_string(&todo_path) {
                    if let Ok(todo_val) = serde_json::from_str::<serde_json::Value>(&todo_content) {
                        if let Some(tasks) = todo_val.get("tasks").and_then(|t| t.as_array()) {
                            if !tasks.is_empty() {
                                md.push_str("\n<details>\n<summary>📋 Active Handover TODOs (Click to expand)</summary>\n\n");
                                md.push_str(&format!("**Project Status**: {}\n", todo_val.get("status").and_then(|s| s.as_str()).unwrap_or("idle")));
                                md.push_str(&format!("**Last Handover**: {}\n\n", todo_val.get("lastHandover").and_then(|s| s.as_str()).unwrap_or("unknown")));
                                for task in tasks {
                                    let task_desc = task.get("task").and_then(|t| t.as_str()).unwrap_or("unknown task");
                                    let assigned = task.get("assignedTo").and_then(|a| a.as_str()).unwrap_or("unassigned");
                                    let task_status = task.get("status").and_then(|s| s.as_str()).unwrap_or("pending");
                                    let checked = if task_status == "completed" || task_status == "done" { "[x]" } else { "[ ]" };
                                    md.push_str(&format!("- {} **{}** (Assigned: {}, Status: {})\n", checked, task_desc, assigned, task_status));
                                }
                                md.push_str("\n</details>\n");
                            }
                        }
                    }
                }
            }
        }
    }
    md
}

pub fn render_status_filter(map: &LunarMap, status: &str) -> String {
    let filtered: Vec<&AlignmentEntry> = map.alignments.iter().filter(|a| a.status.eq_ignore_ascii_case(status)).collect();
    let mut md = format!("# Alignments with status `{}` ({})\n\n", status, filtered.len());
    if filtered.is_empty() { md.push_str("No matching alignments.\n"); }
    else { md.push_str("| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n"); for a in filtered { md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status)); } }
    md
}

pub fn render_path_filter(map: &LunarMap, path: &str) -> String {
    let filtered: Vec<&AlignmentEntry> = map.alignments.iter().filter(|a| a.path.contains(path)).collect();
    let mut md = format!("# Alignments related to path `{}` ({})\n\n", path, filtered.len());
    if filtered.is_empty() { md.push_str("No matching alignments.\n"); }
    else { md.push_str("| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n"); for a in filtered { md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status)); } }
    md
}

pub fn render_mermaid(map: &LunarMap) -> String {
    let mut md = String::from("```mermaid\ngraph LR\n");
    let mut node_ids: HashMap<&str, usize> = HashMap::new();
    for (i, p) in map.projects.iter().enumerate() { node_ids.insert(&p.name, i); md.push_str(&format!("  n{}[{}]\n", i, p.name)); }
    for edge in &map.aggregated_edges {
        if let (Some(&from), Some(&to)) = (node_ids.get(edge.client_project.as_str()), node_ids.get(edge.server_project.as_str())) {
            let style = if edge.status == "Orphaned" { " -- " } else { " --> " };
            md.push_str(&format!("  n{}{}|{} {}|n{}\n", from, style, edge.call_count, edge.status, to));
        }
    }
    md.push_str("```\n");
    md
}
