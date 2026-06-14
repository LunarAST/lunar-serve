use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Original types (unchanged)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SourceType { Github, Local }

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GithubSource { pub owner: String, pub repo: String, pub branch: String }

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSource {
    pub r#type: SourceType,
    pub github: Option<GithubSource>,
    #[serde(rename = "archiveUrl")]
    pub archive_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProjectRegistryEntry {
    Simple(String),
    #[serde(rename_all = "camelCase")]
    Detailed {
        name: String,
        display_name: Option<String>,
        source: Option<ProjectSource>,
        visibility: Option<String>,
        path: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReposConfig { pub version: String, pub projects: Vec<ProjectRegistryEntry> }

#[derive(Debug, Clone)]
pub struct ProjectMeta {
    pub display_name: String,
    pub github: Option<GithubSource>,
    pub visibility: String,
    pub archive_url: Option<String>,
    pub path: Option<String>,
}

pub struct ProjectIndex {
    by_name: HashMap<String, ProjectMeta>,
    by_github_path: HashMap<String, String>,
}

impl ProjectIndex {
    pub fn from_config(config: &ReposConfig) -> Self {
        let mut by_name = HashMap::new();
        let mut by_github_path = HashMap::new();
        for entry in &config.projects {
            let (name, display, source, visibility, path) = match entry {
                ProjectRegistryEntry::Simple(name) => (name.clone(), name.clone(), None, "public".to_string(), None),
                ProjectRegistryEntry::Detailed { name, display_name, source, visibility, path } => {
                    (name.clone(), display_name.clone().unwrap_or_else(|| name.clone()), source.clone(), visibility.clone().unwrap_or_else(|| "public".to_string()), path.clone())
                }
            };
            let github = source.as_ref().and_then(|s| if s.r#type == SourceType::Github { s.github.clone() } else { None });
            let archive_url = source.as_ref().and_then(|s| s.archive_url.clone());
            if let Some(ref gh) = github {
                let key = format!("{}/{}/{}", gh.owner, gh.repo, gh.branch).to_lowercase();
                by_github_path.insert(key, name.clone());
            }
            by_name.insert(name, ProjectMeta { display_name: display, github, visibility, archive_url, path });
        }
        Self { by_name, by_github_path }
    }

    pub fn get_name_by_github(&self, owner: &str, repo: &str, branch: &str) -> Option<&str> {
        let key = format!("{}/{}/{}", owner, repo, branch).to_lowercase();
        self.by_github_path.get(&key).map(|s| s.as_str())
    }

    pub fn get_meta(&self, name: &str) -> Option<&ProjectMeta> { self.by_name.get(name) }
}

pub fn load_repos(base_dir: &std::path::Path) -> ReposConfig {
    let path = base_dir.join("repos.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<ReposConfig>(&content) { return config; }
        }
    }
    ReposConfig { version: "0.5.0".to_string(), projects: vec![] }
}

// ---------------------------------------------------------------------------
// v3.0 Security modules – re-exported for main.rs convenience
// ---------------------------------------------------------------------------

pub mod session;
pub mod lct;
pub mod totp;
pub mod rate_limiter;
pub mod patch;
pub mod utils;

// Re-export the most-used items so main.rs can just `use lunar_serve::*`
pub use session::{create_session, validate_session, invalidate_session, spawn_cleanup_task};
pub use lct::{LctPayload, generate_lct, verify_lct, load_signing_key};
pub use totp::verify_totp;
pub use rate_limiter::{check as check_rate_limit, record_failure};
pub use patch::{parse_lunar_patch, ParsedPatch};
pub use utils::*;
