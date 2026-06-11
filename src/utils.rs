use axum::http::StatusCode;

/// Helper function to check if the incoming request is authorized for private repos.
pub fn is_authorized(headers: &axum::http::HeaderMap) -> bool {
    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    auth.starts_with("Bearer ")
}

/// Validates and reads a single file from the workspace, returning descriptive diagnostics on error.
pub fn read_secure_file(base_path_str: &str, relative_path_str: &str) -> Result<String, (StatusCode, String, String)> {
    let base_path = std::path::Path::new(base_path_str);
    let relative_path = std::path::Path::new(relative_path_str);
    let target_path = base_path.join(relative_path);

    let canonical_target = match target_path.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            let hint = format!(
                "The requested file path '{}' does not exist in workspace '{}'. Please double-check your workspace file tree.",
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

    std::fs::read_to_string(canonical_target)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file".to_string(), e.to_string()))
}

/// Helper function to build a sorted, filtered directory tree for AI context (Markdown format).
pub fn render_directory_tree(base_path_str: &str) -> String {
    let mut tree_string = String::new();
    let path = std::path::Path::new(base_path_str);
    let mut file_count = 0;
    if let Err(e) = traverse_for_tree(path, path, 0, &mut tree_string, &mut file_count) {
        return format!("*Error generating file tree: {}*", e);
    }
    tree_string
}

/// Recursive directory traversal with strict ignore rules and limits to prevent token bloating.
fn traverse_for_tree(
    root: &std::path::Path,
    current: &std::path::Path,
    depth: usize,
    output: &mut String,
    file_count: &mut usize,
) -> std::io::Result<()> {
    if depth > 5 || *file_count > 300 {
        return Ok(());
    }

    if current.is_dir() {
        let mut entries = std::fs::read_dir(current)?
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

/// Helper function to build a sorted, filtered flat file list in JSON array format.
pub fn render_directory_tree_json(base_path_str: &str) -> serde_json::Value {
    let mut files = Vec::new();
    let path = std::path::Path::new(base_path_str);
    let mut file_count = 0;
    if let Err(_) = traverse_for_json(path, path, 0, &mut files, &mut file_count) {
        return serde_json::Value::Null;
    }
    serde_json::to_value(files).unwrap_or(serde_json::Value::Null)
}

fn traverse_for_json(
    root: &std::path::Path,
    current: &std::path::Path,
    depth: usize,
    output: &mut Vec<String>,
    file_count: &mut usize,
) -> std::io::Result<()> {
    if depth > 5 || *file_count > 300 {
        return Ok(());
    }

    if current.is_dir() {
        let mut entries = std::fs::read_dir(current)?
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
