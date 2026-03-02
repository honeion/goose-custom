//! Edit tool - File modification functionality
//!
//! Provides file editing capabilities:
//! - String replacement (old_string -> new_string)
//! - Diff application (unified diff format)
//! - Fuzzy matching via mpatch (70% similarity)

use indoc::formatdoc;
use mpatch::{apply_patch, parse_diffs, PatchError};
use rmcp::model::{Content, ErrorCode, ErrorData, Role};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use super::editor_models::EditorModel;
use super::file_history::{save_to_history, FileHistory};
use super::lang;
use super::shell::normalize_line_endings;

// Constants
pub const MAX_DIFF_SIZE: usize = 1024 * 1024; // 1MB max diff size
pub const MAX_FILES_IN_DIFF: usize = 100;

/// Parameters for the edit tool
#[derive(Debug, Clone)]
pub struct EditParams {
    /// Absolute path to file
    pub path: PathBuf,
    /// String to find and replace
    pub old_string: String,
    /// Replacement string
    pub new_string: String,
    /// Replace all occurrences (default: false, requires exactly one match)
    pub replace_all: bool,
    /// Optional unified diff to apply instead of string replacement
    pub diff: Option<String>,
}

/// Edits a file by replacing text or applying a diff
pub async fn edit(
    params: EditParams,
    editor_model: &Option<EditorModel>,
    file_history: &FileHistory,
) -> Result<Vec<Content>, ErrorData> {
    // If diff is provided, use diff application
    if let Some(diff_content) = &params.diff {
        return apply_diff(&params.path, diff_content, file_history).await;
    }

    // Otherwise, use string replacement
    replace_string(
        &params.path,
        &params.old_string,
        &params.new_string,
        params.replace_all,
        editor_model,
        file_history,
    )
    .await
}

/// Replaces a string in a file
async fn replace_string(
    path: &PathBuf,
    old_str: &str,
    new_str: &str,
    replace_all: bool,
    editor_model: &Option<EditorModel>,
    file_history: &FileHistory,
) -> Result<Vec<Content>, ErrorData> {
    // Check if file exists
    if !path.exists() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "File '{}' does not exist. Use the write tool to create a new file.",
                path.display()
            ),
            None,
        ));
    }

    // Read content
    let content = std::fs::read_to_string(path).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to read file: {}", e),
            None,
        )
    })?;

    // Try Editor API if configured
    if let Some(ref editor) = editor_model {
        save_to_history(path, file_history)?;

        match editor.edit_code(&content, old_str, new_str).await {
            Ok(updated_content) => {
                let mut normalized_content = normalize_line_endings(&updated_content);
                if !normalized_content.ends_with('\n') {
                    normalized_content.push('\n');
                }

                std::fs::write(path, &normalized_content).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to write file: {}", e),
                        None,
                    )
                })?;

                return Ok(vec![
                    Content::text(format!("Successfully replaced text in {}.", path.display()))
                        .with_audience(vec![Role::Assistant]),
                    Content::text(format!("Successfully replaced text in {}.", path.display()))
                        .with_audience(vec![Role::User])
                        .with_priority(0.2),
                ]);
            }
            Err(e) => {
                tracing::debug!(
                    "Editor API call failed: {}, falling back to string replacement",
                    e
                );
            }
        }
    }

    // Traditional string replacement
    let match_count = content.matches(old_str).count();

    if match_count == 0 {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "'old_string' not found in file. Make sure it exactly matches existing content, including whitespace."
            ),
            None,
        ));
    }

    if !replace_all && match_count > 1 {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "'old_string' appears {} times in the file. Set replace_all=true to replace all occurrences, or provide more context to make it unique.",
                match_count
            ),
            None,
        ));
    }

    // Save history for undo
    save_to_history(path, file_history)?;

    let new_content = if replace_all {
        content.replace(old_str, new_str)
    } else {
        content.replacen(old_str, new_str, 1)
    };

    let mut normalized_content = normalize_line_endings(&new_content);
    if !normalized_content.ends_with('\n') {
        normalized_content.push('\n');
    }

    std::fs::write(path, &normalized_content).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to write file: {}", e),
            None,
        )
    })?;

    // Calculate line number for display
    let replacement_line = content
        .split(old_str)
        .next()
        .map(|s| s.matches('\n').count() + 1)
        .unwrap_or(1);

    let language = lang::get_language_identifier(path);
    let new_line_count = new_str.lines().count();

    // Show snippet with context
    const SNIPPET_LINES: usize = 4;
    let start_line = replacement_line.saturating_sub(SNIPPET_LINES + 1);
    let end_line = replacement_line + SNIPPET_LINES + new_line_count;
    let lines: Vec<&str> = new_content.lines().collect();
    let snippet = lines
        .iter()
        .skip(start_line)
        .take(end_line - start_line + 1)
        .cloned()
        .collect::<Vec<&str>>()
        .join("\n");

    let summary = if replace_all && match_count > 1 {
        format!(
            "Successfully replaced {} occurrences in {}.",
            match_count,
            path.display()
        )
    } else {
        format!("Successfully replaced text in {}.", path.display())
    };

    let user_output = formatdoc! {r#"
        {summary} (line {line})
        ```{language}
        {snippet}
        ```
        "#,
        summary=summary,
        line=replacement_line,
        language=language,
        snippet=snippet
    };

    Ok(vec![
        Content::text(summary).with_audience(vec![Role::Assistant]),
        Content::text(user_output)
            .with_audience(vec![Role::User])
            .with_priority(0.2),
    ])
}

// ============================================================================
// Diff Application
// ============================================================================

/// Results from applying a diff
#[derive(Debug, Default)]
struct DiffResults {
    files_created: usize,
    files_modified: usize,
    files_deleted: usize,
    lines_added: usize,
    lines_removed: usize,
}

/// Applies a unified diff to files
pub async fn apply_diff(
    base_path: &Path,
    diff_content: &str,
    file_history: &FileHistory,
) -> Result<Vec<Content>, ErrorData> {
    validate_diff_size(diff_content)?;
    let patches = parse_diff_content(diff_content)?;

    if patches.len() > MAX_FILES_IN_DIFF {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "Too many files in diff ({}). Maximum is {} files.",
                patches.len(),
                MAX_FILES_IN_DIFF
            ),
            None,
        ));
    }

    let base_dir = if base_path.is_file() {
        base_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        base_path.to_path_buf()
    };

    let mut results = DiffResults::default();
    let mut failed_hunks = Vec::new();

    for patch in &patches {
        apply_single_patch(
            patch,
            &base_dir,
            file_history,
            &mut results,
            &mut failed_hunks,
        )?;
    }

    ensure_trailing_newlines(&patches, &base_dir)?;
    report_partial_failures(&failed_hunks);

    let (lines_added, lines_removed) = count_line_changes(diff_content);
    results.lines_added = lines_added;
    results.lines_removed = lines_removed;

    let is_single_file = patches.len() == 1;
    Ok(generate_summary(&results, is_single_file, base_path))
}

fn validate_diff_size(diff_content: &str) -> Result<(), ErrorData> {
    if diff_content.len() > MAX_DIFF_SIZE {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "Diff is too large ({} bytes). Maximum size is {} bytes (1MB).",
                diff_content.len(),
                MAX_DIFF_SIZE
            ),
            None,
        ));
    }
    Ok(())
}

fn parse_diff_content(diff_content: &str) -> Result<Vec<mpatch::Patch>, ErrorData> {
    let wrapped_diff = if diff_content.contains("```diff") || diff_content.contains("```patch") {
        diff_content.to_string()
    } else {
        format!("```diff\n{}\n```", diff_content)
    };

    parse_diffs(&wrapped_diff).map_err(|e| match e {
        PatchError::MissingFileHeader => ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "Invalid diff format: Missing file header (e.g., '--- a/path/to/file')".to_string(),
            None,
        ),
        PatchError::Io { path, source } => ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("I/O error processing {}: {}", path.display(), source),
            None,
        ),
        PatchError::PathTraversal(path) => ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "Security: Path '{}' would escape the base directory",
                path.display()
            ),
            None,
        ),
        PatchError::TargetNotFound(path) => ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Target file not found: {}", path.display()),
            None,
        ),
    })
}

fn adjust_base_dir_for_overlap(base_dir: &Path, file_path: &Path) -> PathBuf {
    let base_components: Vec<_> = base_dir.components().collect();
    let file_components: Vec<_> = file_path.components().collect();

    let min_len = base_components.len().min(file_components.len());
    let max_k = (1..=min_len)
        .rfind(|&k| file_components[0..k] == base_components[base_components.len() - k..])
        .unwrap_or(0);

    if max_k > 0 {
        let adjusted_components = base_components[..base_components.len() - max_k].to_vec();
        PathBuf::from_iter(adjusted_components)
    } else {
        base_dir.to_path_buf()
    }
}

fn apply_single_patch(
    patch: &mpatch::Patch,
    base_dir: &Path,
    file_history: &FileHistory,
    results: &mut DiffResults,
    failed_hunks: &mut Vec<String>,
) -> Result<(), ErrorData> {
    let adjusted_base_dir = adjust_base_dir_for_overlap(base_dir, &patch.file_path);
    let file_path = adjusted_base_dir.join(&patch.file_path);

    validate_path_safety(&adjusted_base_dir, &file_path)?;

    let file_existed = file_path.exists();
    if file_existed {
        save_to_history(&file_path, file_history)?;
    }

    let success = apply_patch(patch, &adjusted_base_dir, false, 0.7).map_err(|e| match e {
        PatchError::Io { path, source } => ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to process '{}': {}", path.display(), source),
            None,
        ),
        PatchError::PathTraversal(path) => ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "Security: Path '{}' would escape the base directory",
                path.display()
            ),
            None,
        ),
        PatchError::TargetNotFound(path) => ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!(
                "File '{}' not found and patch doesn't create it",
                path.display()
            ),
            None,
        ),
        PatchError::MissingFileHeader => ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "Invalid patch format".to_string(),
            None,
        ),
    })?;

    if !success {
        let hunk_count = patch.hunks.len();
        let context_preview = patch
            .hunks
            .first()
            .and_then(|h| {
                let match_block = h.get_match_block();
                match_block.first().map(|s| s.to_string())
            })
            .unwrap_or_else(|| "(empty context)".to_string());

        failed_hunks.push(format!(
            "Failed to apply some hunks to '{}' ({} hunks total). First expected line: '{}'",
            patch.file_path.display(),
            hunk_count,
            context_preview
        ));
    }

    if file_existed {
        results.files_modified += 1;
    } else {
        results.files_created += 1;
    }

    Ok(())
}

fn validate_path_safety(base_dir: &Path, target_path: &Path) -> Result<(), ErrorData> {
    if target_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "Path traversal detected: paths cannot contain '..'".to_string(),
            None,
        ));
    }

    if let (Ok(canonical_target), Ok(canonical_base)) =
        (target_path.canonicalize(), base_dir.canonicalize())
    {
        if !canonical_target.starts_with(&canonical_base) {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "Path '{}' is outside the base directory",
                    target_path.display()
                ),
                None,
            ));
        }
    } else if !target_path.exists() {
        if let Some(parent) = target_path.parent() {
            if let (Ok(canonical_parent), Ok(canonical_base)) =
                (parent.canonicalize(), base_dir.canonicalize())
            {
                if !canonical_parent.starts_with(&canonical_base) {
                    return Err(ErrorData::new(
                        ErrorCode::INVALID_PARAMS,
                        format!(
                            "Path '{}' would be outside the base directory",
                            target_path.display()
                        ),
                        None,
                    ));
                }
            }
        }
    }

    if target_path.exists() {
        let metadata = target_path.symlink_metadata().map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to check symlink status: {}", e),
                None,
            )
        })?;

        if metadata.is_symlink() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "Cannot modify symlink '{}'. Please operate on the actual file.",
                    target_path.display()
                ),
                None,
            ));
        }
    }

    Ok(())
}

fn ensure_trailing_newlines(patches: &[mpatch::Patch], base_dir: &Path) -> Result<(), ErrorData> {
    for patch in patches {
        let adjusted_base_dir = adjust_base_dir_for_overlap(base_dir, &patch.file_path);
        let file_path = adjusted_base_dir.join(&patch.file_path);

        if file_path.exists() {
            let content = std::fs::read_to_string(&file_path).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to read file for post-processing: {}", e),
                    None,
                )
            })?;

            if !content.ends_with('\n') {
                let content_with_newline = format!("{}\n", content);
                std::fs::write(&file_path, content_with_newline).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to add trailing newline: {}", e),
                        None,
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn count_line_changes(diff_content: &str) -> (usize, usize) {
    let lines_added = diff_content
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .count();
    let lines_removed = diff_content
        .lines()
        .filter(|l| l.starts_with('-') && !l.starts_with("---"))
        .count();
    (lines_added, lines_removed)
}

fn report_partial_failures(failed_hunks: &[String]) {
    if !failed_hunks.is_empty() {
        let error_msg = format!(
            "Some patches were only partially applied (fuzzy matching at 70% similarity):\n\n{}\n\n\
            The files have been modified but some hunks couldn't find their context.\n\
            Use 'undo' on individual files to revert if needed.",
            failed_hunks.join("\n")
        );
        tracing::warn!("{}", error_msg);
    }
}

fn generate_summary(results: &DiffResults, is_single_file: bool, base_path: &Path) -> Vec<Content> {
    let summary = if is_single_file {
        format!(
            "Successfully applied diff to {}:\n• Lines added: {}\n• Lines removed: {}",
            base_path.display(),
            results.lines_added,
            results.lines_removed
        )
    } else if results.files_created + results.files_modified + results.files_deleted > 1 {
        format!(
            "Successfully applied multi-file diff:\n\
            • Files created: {}\n\
            • Files modified: {}\n\
            • Files deleted: {}\n\
            • Lines added: {}\n\
            • Lines removed: {}",
            results.files_created,
            results.files_modified,
            results.files_deleted,
            results.lines_added,
            results.lines_removed
        )
    } else {
        format!(
            "Successfully applied diff:\n\
            • Files created: {}\n\
            • Files modified: {}\n\
            • Files deleted: {}\n\
            • Lines added: {}\n\
            • Lines removed: {}",
            results.files_created,
            results.files_modified,
            results.files_deleted,
            results.lines_added,
            results.lines_removed
        )
    };

    let user_message = if is_single_file {
        format!("{}\n\nUse 'undo' to revert if needed.\n\n", summary)
    } else {
        format!(
            "{}\n\nUse 'undo' on individual files to revert if needed.\n\n",
            summary
        )
    };

    vec![
        Content::text(summary.clone()).with_audience(vec![Role::Assistant]),
        Content::text(user_message)
            .with_audience(vec![Role::User])
            .with_priority(0.2),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::developer::file_history::new_file_history;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_replace_string() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let history = new_file_history();

        let params = EditParams {
            path: file_path.clone(),
            old_string: "world".to_string(),
            new_string: "rust".to_string(),
            replace_all: false,
            diff: None,
        };

        let result = edit(params, &None, &history).await.unwrap();
        assert!(!result.is_empty());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("hello rust"));
    }

    #[tokio::test]
    async fn test_replace_all() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let history = new_file_history();

        let params = EditParams {
            path: file_path.clone(),
            old_string: "foo".to_string(),
            new_string: "qux".to_string(),
            replace_all: true,
            diff: None,
        };

        let result = edit(params, &None, &history).await.unwrap();
        assert!(!result.is_empty());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content.matches("qux").count(), 3);
        assert_eq!(content.matches("foo").count(), 0);
    }

    #[tokio::test]
    async fn test_replace_multiple_without_flag_fails() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "foo bar foo").unwrap();

        let history = new_file_history();

        let params = EditParams {
            path: file_path.clone(),
            old_string: "foo".to_string(),
            new_string: "qux".to_string(),
            replace_all: false,
            diff: None,
        };

        let result = edit(params, &None, &history).await;
        assert!(result.is_err());
    }
}
