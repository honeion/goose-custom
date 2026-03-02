//! Undo tool - File change reversion functionality
//!
//! Provides the ability to undo file changes made by Edit and Write tools:
//! - Single step undo
//! - Multi-step undo
//! - History inspection

use rmcp::model::{Content, ErrorCode, ErrorData};
use std::path::PathBuf;

use super::file_history::{get_history_depth, restore_from_history, FileHistory};

/// Parameters for the undo tool
#[derive(Debug, Clone)]
pub struct UndoParams {
    /// Absolute path to file
    pub path: PathBuf,
    /// Number of steps to undo (default: 1)
    pub steps: usize,
}

impl Default for UndoParams {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            steps: 1,
        }
    }
}

/// Undoes file changes by restoring from history
pub async fn undo(params: UndoParams, file_history: &FileHistory) -> Result<Vec<Content>, ErrorData> {
    let path = &params.path;
    let steps = if params.steps == 0 { 1 } else { params.steps };

    // Check available history depth
    let available_steps = get_history_depth(path, file_history);

    if available_steps == 0 {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("No undo history available for '{}'", path.display()),
            None,
        ));
    }

    let actual_steps = std::cmp::min(steps, available_steps);

    // Restore from history
    let restored_content = restore_from_history(path, file_history, actual_steps)?;

    match restored_content {
        Some(content) => {
            if content.is_empty() {
                // File was originally new - delete it
                if path.exists() {
                    std::fs::remove_file(path).map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to delete file during undo: {}", e),
                            None,
                        )
                    })?;
                }

                let message = if actual_steps == 1 {
                    format!("Undid creation of '{}' (file deleted)", path.display())
                } else {
                    format!(
                        "Undid {} steps for '{}' (file deleted)",
                        actual_steps,
                        path.display()
                    )
                };

                Ok(vec![Content::text(message)])
            } else {
                // Restore previous content
                std::fs::write(path, &content).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to write file during undo: {}", e),
                        None,
                    )
                })?;

                let remaining = available_steps - actual_steps;
                let message = if actual_steps == 1 {
                    if remaining > 0 {
                        format!(
                            "Undid the last edit to '{}' ({} more undo steps available)",
                            path.display(),
                            remaining
                        )
                    } else {
                        format!("Undid the last edit to '{}'", path.display())
                    }
                } else {
                    if remaining > 0 {
                        format!(
                            "Undid {} edits to '{}' ({} more undo steps available)",
                            actual_steps,
                            path.display(),
                            remaining
                        )
                    } else {
                        format!("Undid {} edits to '{}'", actual_steps, path.display())
                    }
                };

                Ok(vec![Content::text(message)])
            }
        }
        None => Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("No undo history available for '{}'", path.display()),
            None,
        )),
    }
}

/// Gets the number of available undo steps for a file (utility function)
pub fn get_undo_depth(path: &PathBuf, file_history: &FileHistory) -> usize {
    get_history_depth(path, file_history)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::developer::file_history::{new_file_history, save_to_history};
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_undo_single_step() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Create initial file
        fs::write(&file_path, "version 1").unwrap();

        let history = new_file_history();

        // Save to history and modify
        save_to_history(&file_path, &history).unwrap();
        fs::write(&file_path, "version 2").unwrap();

        // Undo
        let params = UndoParams {
            path: file_path.clone(),
            steps: 1,
        };

        let result = undo(params, &history).await.unwrap();
        assert!(!result.is_empty());

        // Check content was restored
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "version 1");
    }

    #[tokio::test]
    async fn test_undo_multiple_steps() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "v1").unwrap();

        let history = new_file_history();

        // Make multiple changes
        save_to_history(&file_path, &history).unwrap();
        fs::write(&file_path, "v2").unwrap();

        save_to_history(&file_path, &history).unwrap();
        fs::write(&file_path, "v3").unwrap();

        save_to_history(&file_path, &history).unwrap();
        fs::write(&file_path, "v4").unwrap();

        // Undo 2 steps
        let params = UndoParams {
            path: file_path.clone(),
            steps: 2,
        };

        let result = undo(params, &history).await.unwrap();
        assert!(!result.is_empty());

        // Should be at v2 (undid v4 -> v3 -> v2)
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "v2");
    }

    #[tokio::test]
    async fn test_undo_new_file_deletes() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("new_file.txt");

        let history = new_file_history();

        // Save empty history (file doesn't exist yet)
        save_to_history(&file_path, &history).unwrap();

        // Create file
        fs::write(&file_path, "new content").unwrap();
        assert!(file_path.exists());

        // Undo should delete the file
        let params = UndoParams {
            path: file_path.clone(),
            steps: 1,
        };

        let result = undo(params, &history).await.unwrap();
        assert!(!result.is_empty());

        // File should be deleted
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_undo_no_history_fails() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let history = new_file_history();

        let params = UndoParams {
            path: file_path.clone(),
            steps: 1,
        };

        let result = undo(params, &history).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_undo_depth() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "v1").unwrap();

        let history = new_file_history();

        assert_eq!(get_undo_depth(&file_path, &history), 0);

        save_to_history(&file_path, &history).unwrap();
        assert_eq!(get_undo_depth(&file_path, &history), 1);

        save_to_history(&file_path, &history).unwrap();
        assert_eq!(get_undo_depth(&file_path, &history), 2);
    }
}
