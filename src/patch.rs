//! Parses AI-generated patches in LunarAST patch block format.

/// Structured representation of a parsed patch.
#[derive(Debug, Clone)]
pub struct ParsedPatch {
    pub patch_type: String,
    pub ai_agent: String,
    pub session_context: String,
    pub timestamp: String,
    pub content: String,
}

/// Attempt to extract a LunarAST patch block from `text`.
/// Format:
///   ---LUNAR_PATCH_START---
///   type: code_patch
///   ai_agent: deepseek-chat
///   session_context: ...
///   timestamp: 2026-06-14T12:00:00Z
///   ---CONTENT---
///   <patch body>
///   ---LUNAR_PATCH_END---
pub fn parse_lunar_patch(text: &str) -> Option<ParsedPatch> {
    let start_tag = "---LUNAR_PATCH_START---";
    let content_tag = "---CONTENT---";
    let end_tag = "---LUNAR_PATCH_END---";

    let start_pos = text.find(start_tag)?;
    let after_start = &text[start_pos + start_tag.len()..];
    let content_start = after_start.find(content_tag)?;
    let header_part = &after_start[..content_start];
    let after_content = &after_start[content_start + content_tag.len()..];
    let content_end = after_content.find(end_tag)?;
    let content = after_content[..content_end].trim().to_string();

    let mut patch_type = String::new();
    let mut ai_agent = String::new();
    let mut session_context = String::new();
    let mut timestamp = String::new();
    for line in header_part.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_lowercase().as_str() {
                "type" => patch_type = v.trim().to_string(),
                "ai_agent" => ai_agent = v.trim().to_string(),
                "session_context" => session_context = v.trim().to_string(),
                "timestamp" => timestamp = v.trim().to_string(),
                _ => {}
            }
        }
    }

    if patch_type.is_empty() || content.is_empty() {
        return None;
    }

    Some(ParsedPatch { patch_type, ai_agent, session_context, timestamp, content })
}
