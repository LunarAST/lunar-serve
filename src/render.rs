use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use lunar_interface::{AlignmentEntry, LunarMap};
use lunar_serve::ProjectMeta;
use crate::utils::{render_directory_tree, render_directory_tree_json};
use std::collections::HashMap;
use std::fs;

/// Renders either JSON or Markdown project tree based on the HTTP Accept header.
pub fn render_negotiated_tree(
    headers: &HeaderMap,
    map: &LunarMap,
    name: &str,
    meta: Option<&ProjectMeta>,
    is_authenticated: bool,
) -> Result<Response, (StatusCode, String)> {
    let accept_header = headers.get("Accept").and_then(|v| v.to_str().ok()).unwrap_or("");

    // Resolve base path via secondary fallback strategy
    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|proj| proj.name.eq_ignore_ascii_case(name))
                .and_then(|proj| proj.path.clone())
        });

    if accept_header.contains("application/json") {
        if let Some(ref path) = base_path {
            if std::path::Path::new(path).exists() {
                let json_tree = render_directory_tree_json(path);
                return Ok(Json(json_tree).into_response());
            }
        }
        return Ok(Json(serde_json::Value::Null).into_response());
    }

    // Default to highly cohesive Markdown with embedded file tree outline
    let md = render_project_md(headers, map, name, meta, is_authenticated);
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
) -> String {
    let project = map.projects.iter().find(|p| p.name.eq_ignore_ascii_case(name));
    if project.is_none() { return format!("# Project `{}` not found.\n", name); }
    let p = project.unwrap();
    let exp = p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
    let con = p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
    
    // [MODIFIED] Section 8.4: Sane declared domain with progressive dynamic sniffing fallback
    let request_host = std::env::var("LUNAR_SERVE_DOMAIN").ok()
        .and_then(|d| d.strip_prefix("https://").or_else(|| d.strip_prefix("http://")).map(|s| s.to_string()))
        .unwrap_or_else(|| {
            headers.get("Host").and_then(|v| v.to_str().ok()).unwrap_or("127.0.0.1:8787").to_string()
        });
        
    let git_owner = meta.and_then(|m| m.github.as_ref()).map(|g| g.owner.as_str()).unwrap_or("Jasonmilk");
    let git_repo = meta.and_then(|m| m.github.as_ref()).map(|g| g.repo.as_str()).unwrap_or(name);
    let git_branch = meta.and_then(|m| m.github.as_ref()).map(|g| g.branch.as_str()).unwrap_or("rs2");

    let base_path = meta.and_then(|m| m.path.as_deref())
        .map(|p| p.to_string())
        .or_else(|| {
            map.projects.iter()
                .find(|proj| proj.name.eq_ignore_ascii_case(name))
                .and_then(|proj| proj.path.clone())
        });

    let mut md = String::new();

    // Read AI Agent System Instruction from local workspace path if available on VPS disk
    if let Some(ref path) = base_path {
        let instruction_path = std::path::Path::new(path).join(".lunar/ai-instruction.md");
        if instruction_path.exists() {
            if let Ok(inst_content) = fs::read_to_string(&instruction_path) {
                md.push_str(&inst_content);
                md.push_str("\n\n---\n\n");
            }
        } else {
            // [FALLBACK] Sane default English AI Instruction matching dynamically sniffed host coordinates
            md.push_str("> 🤖 **AI Agent Instruction (LunarAST Decoupled Protocol)**\n");
            md.push_str(&format!("> You are currently analyzing the codebase of `{}` (from the `CommonIntents` protocol family).\n", name));
            md.push_str("> 1. To explore the directory layout, inspect the **Workspace File Tree** at the bottom of this page.\n");
            md.push_str(&format!("> 2. To read any file contents, do NOT guess. Fetch it dynamically on-demand via:\n>    `https://{}/{}/{}/raw/{}/<filepath>`\n", request_host, git_owner, git_repo, git_branch));
            md.push_str(&format!("> 3. Read or update active tasks on the Handover TODO board via:\n>    `https://{}/api/v1/projects/{}/todo`\n", request_host, name.to_lowercase()));
            md.push_str("\n\n---\n\n");
        }
    }

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
            // Append directory tree
            md.push_str("\n---\n## 📂 Workspace File Tree (AI Navigation Guide)\n\n");
            md.push_str("To view raw file content on-demand, use the `/raw` endpoint matching these paths.\n\n");
            md.push_str("```\n");
            
            // Inject dynamic, intuitive `#` header comments with sniffed host coordinates
            md.push_str(&format!("# Repository: {}/{}\n", git_owner, git_repo));
            md.push_str(&format!("# Branch: {} (Ecosystem automatic path discovery)\n", git_branch));
            md.push_str(&format!("# To read project manual: fetch /raw/{}/README.md\n", git_branch));
            md.push_str(&format!("# To read any source file: request /raw/{}/<filepath>\n\n", git_branch));
            
            md.push_str(&render_directory_tree(path));
            md.push_str("```\n");

            // Append Collapsible Handover TODOs at the bottom
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
