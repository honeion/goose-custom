//! Write tool - File writing functionality
//!
//! Provides file creation and overwrite capabilities:
//! - Full file content writing
//! - Line ending normalization
//! - Optional directory creation
//! - Optional backup creation

use indoc::formatdoc;
use rmcp::model::{Content, ErrorCode, ErrorData, Role};
use std::path::PathBuf;

use super::file_history::{save_to_history, FileHistory};
use super::lang;
use super::shell::normalize_line_endings;

/// Parameters for the write tool
#[derive(Debug, Clone)]
pub struct WriteParams {
    /// Absolute path to file
    pub path: PathBuf,
    /// Content to write
    pub content: String,
    /// Create parent directories if they don't exist (default: false)
    pub create_directories: bool,
    /// Create a .bak backup before overwriting (default: false)
    pub backup: bool,
}

/// Writes content to a file
pub async fn write(params: WriteParams, file_history: &FileHistory) -> Result<Vec<Content>, ErrorData> {
    let path = &params.path;

    // Create parent directories if requested
    if params.create_directories {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to create directories: {}", e),
                        None,
                    )
                })?;
            }
        }
    } else {
        // Check if parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "Parent directory '{}' does not exist. Set create_directories=true to create it.",
                        parent.display()
                    ),
                    None,
                ));
            }
        }
    }

    // Save to history before modifying (for existing files)
    if path.exists() {
        // Create backup if requested
        if params.backup {
            let backup_path = PathBuf::from(format!("{}.bak", path.display()));
            std::fs::copy(path, &backup_path).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to create backup: {}", e),
                    None,
                )
            })?;
        }

        save_to_history(path, file_history)?;
    }

    // Normalize line endings based on platform
    let mut normalized_text = normalize_line_endings(&params.content);

    // Ensure the text ends with a newline
    if !normalized_text.ends_with('\n') {
        normalized_text.push('\n');
    }

    // Write to the file
    std::fs::write(path, &normalized_text).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to write file: {}", e),
            None,
        )
    })?;

    // Detect language for syntax highlighting
    let language = lang::get_language_identifier(path);

    let summary = format!("Successfully wrote to {}", path.display());

    // Show content preview for user
    let user_output = formatdoc! {
        r#"
        ### {path}
        ```{language}
        {content}
        ```
        "#,
        path=path.display(),
        language=language,
        content=&normalized_text
    };

    Ok(vec![
        Content::text(summary).with_audience(vec![Role::Assistant]),
        Content::text(user_output)
            .with_audience(vec![Role::User])
            .with_priority(0.2),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::developer::file_history::new_file_history;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_write_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("new_file.txt");

        let history = new_file_history();

        let params = WriteParams {
            path: file_path.clone(),
            content: "hello world".to_string(),
            create_directories: false,
            backup: false,
        };

        let result = write(params, &history).await.unwrap();
        assert!(!result.is_empty());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("hello world"));
        assert!(content.ends_with('\n'));
    }

    #[tokio::test]
    async fn test_write_overwrite_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("existing.txt");
        fs::write(&file_path, "old content").unwrap();

        let history = new_file_history();

        let params = WriteParams {
            path: file_path.clone(),
            content: "new content".to_string(),
            create_directories: false,
            backup: false,
        };

        let result = write(params, &history).await.unwrap();
        assert!(!result.is_empty());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("new content"));
        assert!(!content.contains("old content"));
    }

    #[tokio::test]
    async fn test_write_with_backup() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("backup_test.txt");
        fs::write(&file_path, "original").unwrap();

        let history = new_file_history();

        let params = WriteParams {
            path: file_path.clone(),
            content: "modified".to_string(),
            create_directories: false,
            backup: true,
        };

        let result = write(params, &history).await.unwrap();
        assert!(!result.is_empty());

        // Check backup was created
        let backup_path = dir.path().join("backup_test.txt.bak");
        assert!(backup_path.exists());
        let backup_content = fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, "original");

        // Check original was modified
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("modified"));
    }

    #[tokio::test]
    async fn test_write_create_directories() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("new_dir").join("subdir").join("file.txt");

        let history = new_file_history();

        let params = WriteParams {
            path: file_path.clone(),
            content: "content".to_string(),
            create_directories: true,
            backup: false,
        };

        let result = write(params, &history).await.unwrap();
        assert!(!result.is_empty());

        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("content"));
    }

    #[tokio::test]
    async fn test_write_without_create_directories_fails() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("nonexistent").join("file.txt");

        let history = new_file_history();

        let params = WriteParams {
            path: file_path.clone(),
            content: "content".to_string(),
            create_directories: false,
            backup: false,
        };

        let result = write(params, &history).await;
        assert!(result.is_err());
    }
}
