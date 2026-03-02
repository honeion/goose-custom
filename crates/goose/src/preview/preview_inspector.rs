//! Preview Inspector - Shows diff preview before file modifications
//!
//! This inspector intercepts edit/write/undo tool calls and generates
//! a diff preview for user approval before the actual modification.

use anyhow::Result;
use async_trait::async_trait;
use similar::{ChangeTag, TextDiff};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

/// Tools that require preview before execution
const PREVIEW_TOOLS: &[&str] = &[
    "developer__edit",
    "developer__write",
    "developer__undo",
];

/// Inspector that generates diff previews for file modification tools
pub struct PreviewInspector {
    /// Set of tool names that require preview
    preview_tools: HashSet<String>,
    /// Whether the inspector is enabled
    enabled: bool,
}

impl PreviewInspector {
    pub fn new() -> Self {
        let preview_tools: HashSet<String> = PREVIEW_TOOLS
            .iter()
            .map(|s| s.to_string())
            .collect();

        Self {
            preview_tools,
            enabled: true,
        }
    }

    /// Check if a tool requires preview
    fn requires_preview(&self, tool_name: &str) -> bool {
        self.preview_tools.contains(tool_name)
    }

    /// Generate diff preview for edit tool
    fn generate_edit_preview(&self, request: &ToolRequest) -> Option<String> {
        let tool_call = request.tool_call.as_ref().ok()?;
        let args = tool_call.arguments.as_ref()?;

        let path = args.get("path")?.as_str()?;
        let old_string = args.get("old_string")?.as_str()?;
        let new_string = args.get("new_string")?.as_str()?;
        let replace_all = args.get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Read current file content
        let file_path = PathBuf::from(path);
        let current_content = match std::fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(e) => {
                return Some(format!(
                    "📝 Edit Preview\n\nFile: {}\n⚠️ Cannot read file: {}\n\nWill search for:\n```\n{}\n```\n\nReplace with:\n```\n{}\n```",
                    path, e, old_string, new_string
                ));
            }
        };

        // Find and replace
        let (new_content, match_count) = if replace_all {
            let count = current_content.matches(old_string).count();
            (current_content.replace(old_string, new_string), count)
        } else {
            let count = if current_content.contains(old_string) { 1 } else { 0 };
            (current_content.replacen(old_string, new_string, 1), count)
        };

        if match_count == 0 {
            return Some(format!(
                "📝 Edit Preview\n\nFile: {}\n⚠️ No matches found for the search string.\n\nSearching for:\n```\n{}\n```",
                path, old_string
            ));
        }

        // Generate unified diff
        let diff = TextDiff::from_lines(&current_content, &new_content);
        let mut diff_output = String::new();

        diff_output.push_str(&format!("📝 Edit Preview\n\nFile: {}\n", path));
        if replace_all && match_count > 1 {
            diff_output.push_str(&format!("Matches: {} (replace all)\n", match_count));
        }
        diff_output.push_str("\n```diff\n");

        for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
            if idx > 0 {
                diff_output.push_str("...\n");
            }
            for op in group {
                for change in diff.iter_inline_changes(op) {
                    let sign = match change.tag() {
                        ChangeTag::Delete => "-",
                        ChangeTag::Insert => "+",
                        ChangeTag::Equal => " ",
                    };
                    diff_output.push_str(sign);
                    for (_, value) in change.iter_strings_lossy() {
                        diff_output.push_str(&value);
                    }
                    if change.missing_newline() {
                        diff_output.push('\n');
                    }
                }
            }
        }
        diff_output.push_str("```");

        Some(diff_output)
    }

    /// Generate diff preview for write tool
    fn generate_write_preview(&self, request: &ToolRequest) -> Option<String> {
        let tool_call = request.tool_call.as_ref().ok()?;
        let args = tool_call.arguments.as_ref()?;

        let path = args.get("path")?.as_str()?;
        let content = args.get("content")?.as_str()?;

        let file_path = PathBuf::from(path);

        // Check if file exists
        if file_path.exists() {
            let current_content = match std::fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(e) => {
                    return Some(format!(
                        "📄 Write Preview\n\nFile: {}\n⚠️ Cannot read existing file: {}\n\nNew content ({} bytes):\n```\n{}...\n```",
                        path, e, content.len(),
                        &content[..std::cmp::min(500, content.len())]
                    ));
                }
            };

            // Generate diff for existing file
            let content_string = content.to_string();
            let diff = TextDiff::from_lines(&current_content, &content_string);
            let mut diff_output = String::new();

            diff_output.push_str(&format!("📄 Write Preview (Overwrite)\n\nFile: {}\n", path));
            diff_output.push_str(&format!("Current: {} bytes → New: {} bytes\n",
                current_content.len(), content.len()));
            diff_output.push_str("\n```diff\n");

            for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
                if idx > 0 {
                    diff_output.push_str("...\n");
                }
                for op in group {
                    for change in diff.iter_inline_changes(op) {
                        let sign = match change.tag() {
                            ChangeTag::Delete => "-",
                            ChangeTag::Insert => "+",
                            ChangeTag::Equal => " ",
                        };
                        diff_output.push_str(sign);
                        for (_, value) in change.iter_strings_lossy() {
                            diff_output.push_str(&value);
                        }
                        if change.missing_newline() {
                            diff_output.push('\n');
                        }
                    }
                }
            }
            diff_output.push_str("```");

            Some(diff_output)
        } else {
            // New file
            let preview_content = if content.len() > 1000 {
                format!("{}...\n\n(truncated, {} more bytes)",
                    &content[..1000], content.len() - 1000)
            } else {
                content.to_string()
            };

            Some(format!(
                "📄 Write Preview (New File)\n\nFile: {}\nSize: {} bytes\n\n```\n{}\n```",
                path, content.len(), preview_content
            ))
        }
    }

    /// Generate preview for undo tool
    fn generate_undo_preview(&self, request: &ToolRequest) -> Option<String> {
        let tool_call = request.tool_call.as_ref().ok()?;
        let args = tool_call.arguments.as_ref()?;

        let path = args.get("path")?.as_str()?;
        let steps = args.get("steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);

        // We can't easily preview undo without access to history
        // So we just show what will be undone
        Some(format!(
            "↩️ Undo Preview\n\nFile: {}\nSteps: {}\n\n⚠️ This will restore the file to a previous state.\nThe exact content will be shown after approval.",
            path, steps
        ))
    }
}

impl Default for PreviewInspector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolInspector for PreviewInspector {
    fn name(&self) -> &'static str {
        "preview"
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        goose_mode: GooseMode,
    ) -> Result<Vec<InspectionResult>> {
        // Only active in Approve or SmartApprove modes
        if goose_mode == GooseMode::Auto {
            return Ok(vec![]);
        }

        let mut results = Vec::new();

        for request in tool_requests {
            let tool_name: &str = match &request.tool_call {
                Ok(call) => &call.name,
                Err(_) => continue,
            };

            if !self.requires_preview(tool_name) {
                continue;
            }

            // Generate preview based on tool type
            let preview = match tool_name {
                "developer__edit" => self.generate_edit_preview(request),
                "developer__write" => self.generate_write_preview(request),
                "developer__undo" => self.generate_undo_preview(request),
                _ => None,
            };

            if let Some(preview_message) = preview {
                results.push(InspectionResult {
                    tool_request_id: request.id.clone(),
                    action: InspectionAction::RequireApproval(Some(preview_message)),
                    reason: "File modification requires preview approval".to_string(),
                    confidence: 1.0,
                    inspector_name: self.name().to_string(),
                    finding_id: None,
                });
            }
        }

        Ok(results)
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    fn create_edit_request(path: &str, old_string: &str, new_string: &str) -> ToolRequest {
        ToolRequest {
            id: "test_1".to_string(),
            tool_call: Ok(CallToolRequestParams {
                meta: None,
                task: None,
                name: "developer__edit".into(),
                arguments: Some(object!({
                    "path": path,
                    "old_string": old_string,
                    "new_string": new_string
                })),
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    fn create_write_request(path: &str, content: &str) -> ToolRequest {
        ToolRequest {
            id: "test_2".to_string(),
            tool_call: Ok(CallToolRequestParams {
                meta: None,
                task: None,
                name: "developer__write".into(),
                arguments: Some(object!({
                    "path": path,
                    "content": content
                })),
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    #[test]
    fn test_requires_preview() {
        let inspector = PreviewInspector::new();

        assert!(inspector.requires_preview("developer__edit"));
        assert!(inspector.requires_preview("developer__write"));
        assert!(inspector.requires_preview("developer__undo"));
        assert!(!inspector.requires_preview("developer__read"));
        assert!(!inspector.requires_preview("developer__shell"));
    }

    #[tokio::test]
    async fn test_inspect_skips_auto_mode() {
        let inspector = PreviewInspector::new();
        let requests = vec![create_edit_request("/tmp/test.txt", "old", "new")];

        let results = inspector.inspect(&requests, &[], GooseMode::Auto).await.unwrap();
        assert!(results.is_empty(), "Should skip inspection in Auto mode");
    }

    #[tokio::test]
    async fn test_inspect_edit_tool() {
        let inspector = PreviewInspector::new();
        let requests = vec![create_edit_request("/tmp/nonexistent.txt", "old", "new")];

        let results = inspector.inspect(&requests, &[], GooseMode::Approve).await.unwrap();
        assert_eq!(results.len(), 1);

        match &results[0].action {
            InspectionAction::RequireApproval(Some(msg)) => {
                assert!(msg.contains("Edit Preview"));
            }
            _ => panic!("Expected RequireApproval with message"),
        }
    }

    #[tokio::test]
    async fn test_inspect_non_preview_tool() {
        let inspector = PreviewInspector::new();
        let request = ToolRequest {
            id: "test_3".to_string(),
            tool_call: Ok(CallToolRequestParams {
                meta: None,
                task: None,
                name: "developer__read".into(),
                arguments: Some(object!({
                    "path": "/tmp/test.txt"
                })),
            }),
            metadata: None,
            tool_meta: None,
        };

        let results = inspector.inspect(&[request], &[], GooseMode::Approve).await.unwrap();
        assert!(results.is_empty(), "Should not inspect non-preview tools");
    }
}
