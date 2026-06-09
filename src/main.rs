use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use lunar::{LunarMap, AlignmentEntry};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

// ── Shared state ──

struct AppState {
    data_path: PathBuf,
}

// ── Query parameters ──

#[derive(Deserialize, Default)]
struct MdQuery {
    #[serde(default)]
    summary: bool,
    scope: Option<String>,
    status: Option<String>,
    path: Option<String>,
    style: Option<String>,
}

// ── Helpers ──

fn load_map(path: &std::path::Path) -> Result<LunarMap, (StatusCode, String)> {
    let content = fs::read_to_string(path)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read lunar-map.json".into()))?;
    serde_json::from_str(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid JSON: {}", e)))
}

fn render_summary(map: &LunarMap) -> String {
    let total_exposed: usize = map.projects.iter().map(|p| p.interfaces.as_object().map_or(0, |i| i.get("exposed").and_then(|e| e.as_array()).map_or(0, |a| a.len()))).sum();
    let total_consumed: usize = map.projects.iter().map(|p| p.interfaces.as_object().map_or(0, |i| i.get("consumed").and_then(|e| e.as_array()).map_or(0, |a| a.len()))).sum();
    let orphaned = map.anomalies.orphaned_consumers.len();
    let unused = map.anomalies.unused_endpoints.len();

    let mut md = String::new();
    md.push_str("# Ecosystem Summary\n\n");
    md.push_str(&format!("- Projects: {}\n", map.projects.len()));
    md.push_str(&format!("- Total Exposed Endpoints: {}\n", total_exposed));
    md.push_str(&format!("- Total Consumed Dependencies: {}\n", total_consumed));
    md.push_str(&format!("- Anomalies: {} orphaned, {} unused\n\n", orphaned, unused));

    md.push_str("## Projects\n\n");
    md.push_str("| Name | Type | Exposed | Consumed | Status |\n");
    md.push_str("|:---|:---|:---|:---|:---|\n");
    for p in &map.projects {
        let exp = p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
        let con = p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
        md.push_str(&format!("| {} | {} | {} | {} | {} |\n", p.name, p.project_type, exp, con, p.scan_status));
    }

    if orphaned + unused > 0 {
        md.push_str("\n## Top Risks\n\n");
        for ep in &map.anomalies.orphaned_consumers {
            md.push_str(&format!("1. **Orphaned**: {} calls `{} {}` but target service not found.\n", ep.project, ep.method, ep.path));
        }
        for ep in &map.anomalies.unused_endpoints {
            md.push_str(&format!("1. **Unused**: {} exposes `{} {}` but no consumer calls it.\n", ep.project, ep.method, ep.path));
        }
    }

    md
}

fn render_project_scope(map: &LunarMap, scope: &str) -> String {
    let project = map.projects.iter().find(|p| p.name == scope);
    if project.is_none() {
        return format!("# Project `{}` not found in ecosystem.\n", scope);
    }
    let p = project.unwrap();
    let exp = p.interfaces.as_object().and_then(|i| i.get("exposed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
    let con = p.interfaces.as_object().and_then(|i| i.get("consumed")).and_then(|e| e.as_array()).map_or(0, |a| a.len());
    let mut md = format!("# Project: {}\n\n- Type: {}\n- Scan Status: {}\n- Exposed: {}\n- Consumed: {}\n\n", p.name, p.project_type, p.scan_status, exp, con);

    if let Some(interfaces) = p.interfaces.as_object() {
        if let Some(exposed) = interfaces.get("exposed").and_then(|e| e.as_array()) {
            md.push_str("## Exposed Endpoints\n\n| Method | Path |\n|:---|:---|\n");
            for e in exposed {
                md.push_str(&format!("| {} | {} |\n", e["method"].as_str().unwrap_or(""), e["path"].as_str().unwrap_or("")));
            }
            md.push('\n');
        }
        if let Some(consumed) = interfaces.get("consumed").and_then(|e| e.as_array()) {
            md.push_str("## Consumed Dependencies\n\n| Method | Path | Target |\n|:---|:---|:---|\n");
            for c in consumed {
                md.push_str(&format!("| {} | {} | {} |\n", c["method"].as_str().unwrap_or(""), c["path"].as_str().unwrap_or(""), c.get("targetProject").and_then(|t| t.as_str()).unwrap_or("?")));
            }
            md.push('\n');
        }
    }

    // Related alignments
    let relevant: Vec<&AlignmentEntry> = map.alignments.iter().filter(|a| a.client_project == scope || a.server_project == scope).collect();
    if !relevant.is_empty() {
        md.push_str("## Alignments Involving This Project\n\n| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n");
        for a in relevant {
            md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status));
        }
    }

    md
}

fn render_status_filter(map: &LunarMap, status: &str) -> String {
    let filtered: Vec<&AlignmentEntry> = map.alignments.iter().filter(|a| a.status.eq_ignore_ascii_case(status)).collect();
    let mut md = format!("# Alignments with status `{}` ({})\n\n", status, filtered.len());
    if filtered.is_empty() {
        md.push_str("No matching alignments.\n");
    } else {
        md.push_str("| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n");
        for a in filtered {
            md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status));
        }
    }
    md
}

fn render_path_filter(map: &LunarMap, path: &str) -> String {
    let filtered: Vec<&AlignmentEntry> = map.alignments.iter().filter(|a| a.path.contains(path)).collect();
    let mut md = format!("# Alignments related to path `{}` ({})\n\n", path, filtered.len());
    if filtered.is_empty() {
        md.push_str("No matching alignments.\n");
    } else {
        md.push_str("| Client | Server | Method | Path | Status |\n|:---|:---|:---|:---|:---|\n");
        for a in filtered {
            md.push_str(&format!("| {} | {} | {} | {} | {} |\n", a.client_project, a.server_project, a.method, a.path, a.status));
        }
    }
    md
}

fn render_mermaid(map: &LunarMap) -> String {
    let mut md = String::from("```mermaid\ngraph LR\n");
    let mut node_ids: HashMap<&str, usize> = HashMap::new();
    for (i, p) in map.projects.iter().enumerate() {
        node_ids.insert(&p.name, i);
        md.push_str(&format!("  n{}[{}]\n", i, p.name));
    }
    for edge in &map.aggregated_edges {
        if let (Some(&from), Some(&to)) = (node_ids.get(edge.client_project.as_str()), node_ids.get(edge.server_project.as_str())) {
            let status = &edge.status;
            let style = if status == "Orphaned" { " -- " } else { " --> " };
            md.push_str(&format!("  n{}{}|{} {}|n{}\n", from, style, edge.call_count, status, to));
        }
    }
    md.push_str("```\n");
    md
}

// ── Handlers ──

async fn get_json(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let map = load_map(&state.data_path)?;
    let json = serde_json::to_value(&map).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json))
}

async fn get_markdown(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MdQuery>,
) -> Result<String, (StatusCode, String)> {
    let map = load_map(&state.data_path)?;

    if query.summary {
        return Ok(render_summary(&map));
    }

    if let Some(ref scope) = query.scope {
        return Ok(render_project_scope(&map, scope));
    }

    if let Some(ref status) = query.status {
        return Ok(render_status_filter(&map, status));
    }

    if let Some(ref path) = query.path {
        return Ok(render_path_filter(&map, path));
    }

    if let Some(ref style) = query.style {
        if style == "mermaid" {
            return Ok(render_mermaid(&map));
        }
    }

    // Default: full topology as plain list
    let mut md = String::from("# Ecosystem Topology\n\n");
    for p in &map.projects {
        md.push_str(&format!("## {}\n", p.name));
        if let Some(interfaces) = p.interfaces.as_object() {
            if let Some(exposed) = interfaces.get("exposed").and_then(|e| e.as_array()) {
                md.push_str("### Exposed\n");
                for e in exposed {
                    md.push_str(&format!("- {} {}\n", e["method"].as_str().unwrap_or(""), e["path"].as_str().unwrap_or("")));
                }
            }
            if let Some(consumed) = interfaces.get("consumed").and_then(|e| e.as_array()) {
                md.push_str("### Consumed\n");
                for c in consumed {
                    md.push_str(&format!("- {} {} -> {}\n", c["method"].as_str().unwrap_or(""), c["path"].as_str().unwrap_or(""), c.get("targetProject").and_then(|t| t.as_str()).unwrap_or("?")));
                }
            }
        }
    }
    md.push_str("\n## Alignments\n");
    for a in &map.alignments {
        md.push_str(&format!("- {} → {}: {} {} ({})\n", a.client_project, a.server_project, a.method, a.path, a.status));
    }
    Ok(md)
}

async fn healthz() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("lunar-map.json")
    };

    if !data_path.exists() {
        eprintln!("Error: lunar-map.json not found at {}. Generate it with `lunar map`.", data_path.display());
        std::process::exit(1);
    }

    let state = Arc::new(AppState { data_path });

    let app = Router::new()
        .route("/lunar-map.json", get(get_json))
        .route("/lunar-map.md", get(get_markdown))
        .route("/healthz", get(healthz))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787").await.unwrap();
    println!("lunar-serve listening on http://0.0.0.0:8787");
    axum::serve(listener, app).await.unwrap();
}
