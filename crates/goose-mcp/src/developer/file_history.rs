//! Shared file history module for undo functionality
//!
//! This module provides a shared history store that can be used by
//! Edit, Write, and Undo tools to track file changes and enable undo operations.

use rmcp::model::{ErrorCode, ErrorData};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// Shared file history for undo operations
///
/// Stores previous file contents keyed by path.
/// Each path can have multiple history entries (stack).
pub type FileHistory = Arc<Mutex<HashMap<PathBuf, Vec<String>>>>;

/// Creates a new empty file history
pub fn new_file_history() -> FileHistory {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Saves the current content of a file to history before modification
///
/// Call this before any write/edit operation to enable undo.
pub fn save_to_history(path: &PathBuf, history: &FileHistory) -> Result<(), ErrorData> {
    let mut history_guard = history.lock().map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to acquire history lock: {}", e),
            None,
        )
    })?;

    let content = if path.exists() {
        std::fs::read_to_string(path).map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to read file for history: {}", e),
                None,
            )
        })?
    } else {
        // Empty string for new files - undo will delete the file
        String::new()
    };

    history_guard.entry(path.clone()).or_default().push(content);
    Ok(())
}

/// Restores the previous content of a file from history
///
/// Returns Ok(Some(content)) if history exists, Ok(None) if no history.
pub fn restore_from_history(
    path: &PathBuf,
    history: &FileHistory,
    steps: usize,
) -> Result<Option<String>, ErrorData> {
    let mut history_guard = history.lock().map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to acquire history lock: {}", e),
            None,
        )
    })?;

    if let Some(contents) = history_guard.get_mut(path) {
        let mut restored_content = None;

        for _ in 0..steps {
            if let Some(content) = contents.pop() {
                restored_content = Some(content);
            } else {
                break;
            }
        }

        Ok(restored_content)
    } else {
        Ok(None)
    }
}

/// Gets the number of undo steps available for a file
pub fn get_history_depth(path: &PathBuf, history: &FileHistory) -> usize {
    history
        .lock()
        .ok()
        .and_then(|guard| guard.get(path).map(|v| v.len()))
        .unwrap_or(0)
}

/// Clears history for a specific file
pub fn clear_history(path: &PathBuf, history: &FileHistory) {
    if let Ok(mut guard) = history.lock() {
        guard.remove(path);
    }
}

/// Clears all history
pub fn clear_all_history(history: &FileHistory) {
    if let Ok(mut guard) = history.lock() {
        guard.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_restore_history() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Create initial file
        fs::write(&file_path, "initial content").unwrap();

        let history = new_file_history();

        // Save to history
        save_to_history(&file_path, &history).unwrap();

        // Modify file
        fs::write(&file_path, "modified content").unwrap();

        // Restore from history
        let restored = restore_from_history(&file_path, &history, 1).unwrap();
        assert_eq!(restored, Some("initial content".to_string()));
    }

    #[test]
    fn test_multiple_undo_steps() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "v1").unwrap();
        let history = new_file_history();

        // Save v1
        save_to_history(&file_path, &history).unwrap();
        fs::write(&file_path, "v2").unwrap();

        // Save v2
        save_to_history(&file_path, &history).unwrap();
        fs::write(&file_path, "v3").unwrap();

        // Should have 2 history entries
        assert_eq!(get_history_depth(&file_path, &history), 2);

        // Restore 1 step - should get v2
        let restored = restore_from_history(&file_path, &history, 1).unwrap();
        assert_eq!(restored, Some("v2".to_string()));

        // Restore 1 more step - should get v1
        let restored = restore_from_history(&file_path, &history, 1).unwrap();
        assert_eq!(restored, Some("v1".to_string()));

        // No more history
        let restored = restore_from_history(&file_path, &history, 1).unwrap();
        assert_eq!(restored, None);
    }

    #[test]
    fn test_new_file_history() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("new_file.txt");

        let history = new_file_history();

        // Save history for non-existent file
        save_to_history(&file_path, &history).unwrap();

        // Should have empty string in history
        let restored = restore_from_history(&file_path, &history, 1).unwrap();
        assert_eq!(restored, Some(String::new()));
    }
}
