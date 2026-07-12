use axum::http::StatusCode;
use std::fs;
use std::path::Path;

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
