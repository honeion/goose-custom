//! Read tool - File reading functionality
//!
//! Provides read-only file access with support for:
//! - Reading file contents with line numbers
//! - Directory listing
//! - View range specification (offset/limit)
//! - Large file handling

use indoc::formatdoc;
use rmcp::model::{Content, ErrorCode, ErrorData, Role};
use std::{
    fs::File,
    io::Read as IoRead,
    path::{Path, PathBuf},
};

use super::lang;

// Constants
pub const LINE_READ_LIMIT: usize = 2000;
pub const MAX_FILE_SIZE: u64 = 400 * 1024; // 400KB

/// Parameters for the read tool
#[derive(Debug, Clone)]
pub struct ReadParams {
    /// Absolute path to file or directory
    pub path: PathBuf,
    /// Starting line number (0-indexed). If None, starts from beginning.
    pub offset: Option<usize>,
    /// Number of lines to read. If None, reads to end (up to limit).
    pub limit: Option<usize>,
}

/// Reads a file or lists directory contents
pub async fn read(params: ReadParams) -> Result<Vec<Content>, ErrorData> {
    let path = &params.path;

    // Check if path is a directory
    if path.is_dir() {
        return list_directory_contents(path);
    }

    read_file(path, params.offset, params.limit).await
}

/// Reads file contents with optional range
async fn read_file(
    path: &Path,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<Vec<Content>, ErrorData> {
    if !path.is_file() {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!(
                "The path '{}' does not exist or is not accessible.",
                path.display()
            ),
            None,
        ));
    }

    let f = File::open(path).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to open file: {}", e),
            None,
        )
    })?;

    let file_size = f
        .metadata()
        .map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to get file metadata: {}", e),
                None,
            )
        })?
        .len();

    if file_size > MAX_FILE_SIZE {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!(
                "File '{}' is too large ({:.2}KB). Maximum size is 400KB.",
                path.display(),
                file_size as f64 / 1024.0
            ),
            None,
        ));
    }

    // Ensure we never read over the limit
    let mut f = f.take(MAX_FILE_SIZE);

    let mut content = String::new();
    f.read_to_string(&mut content).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to read file: {}", e),
            None,
        )
    })?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Calculate range
    let start_idx = offset.unwrap_or(0);
    let end_idx = if let Some(lim) = limit {
        std::cmp::min(start_idx + lim, total_lines)
    } else {
        total_lines
    };

    // Check if we should recommend using range for large files
    if offset.is_none() && limit.is_none() && total_lines > LINE_READ_LIMIT {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!(
                "File '{}' is {} lines long. Use offset/limit to read in smaller chunks, or pass offset=0, limit={} to read all.",
                path.display(),
                total_lines,
                total_lines
            ),
            None,
        ));
    }

    // Validate range
    if start_idx >= total_lines && total_lines > 0 {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "Offset {} is beyond the end of the file (total lines: {})",
                start_idx, total_lines
            ),
            None,
        ));
    }

    let formatted = format_file_content(path, &lines, start_idx, end_idx, offset, limit);

    Ok(vec![
        Content::text(formatted.clone()).with_audience(vec![Role::Assistant]),
        Content::text(formatted)
            .with_audience(vec![Role::User])
            .with_priority(0.0),
    ])
}

/// Formats file content with line numbers
fn format_file_content(
    path: &Path,
    lines: &[&str],
    start_idx: usize,
    end_idx: usize,
    offset: Option<usize>,
    limit: Option<usize>,
) -> String {
    let display_content = if lines.is_empty() {
        String::new()
    } else {
        let actual_end = std::cmp::min(end_idx, lines.len());
        let selected_lines: Vec<String> = lines[start_idx..actual_end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}: {}", start_idx + i + 1, line))
            .collect();

        selected_lines.join("\n")
    };

    let language = lang::get_language_identifier(path);

    if offset.is_some() || limit.is_some() {
        let start_display = start_idx + 1;
        let end_display = end_idx;
        formatdoc! {"
            ### {path} (lines {start}-{end})
            ```{language}
            {content}
            ```
            ",
            path=path.display(),
            start=start_display,
            end=end_display,
            language=language,
            content=display_content,
        }
    } else {
        formatdoc! {"
            ### {path}
            ```{language}
            {content}
            ```
            ",
            path=path.display(),
            language=language,
            content=display_content,
        }
    }
}

/// Lists the contents of a directory
fn list_directory_contents(path: &Path) -> Result<Vec<Content>, ErrorData> {
    const MAX_ITEMS: usize = 50;

    let entries = std::fs::read_dir(path).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to read directory: {}", e),
            None,
        )
    })?;

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total_count = 0;

    for entry in entries {
        let entry = entry.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to read directory entry: {}", e),
                None,
            )
        })?;

        total_count += 1;

        if dirs.len() + files.len() < MAX_ITEMS {
            let metadata = entry.metadata().map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to read metadata: {}", e),
                    None,
                )
            })?;

            let name = entry.file_name().to_string_lossy().to_string();

            if metadata.is_dir() {
                dirs.push(format!("{}/", name));
            } else {
                files.push(name);
            }
        }
    }

    dirs.sort();
    files.sort();

    let mut output = format!("'{}' is a directory. Contents:\n\n", path.display());

    if !dirs.is_empty() {
        output.push_str("Directories:\n");
        for dir in &dirs {
            output.push_str(&format!("  {}\n", dir));
        }
        output.push('\n');
    }

    if !files.is_empty() {
        output.push_str("Files:\n");
        for file in &files {
            output.push_str(&format!("  {}\n", file));
        }
    }

    if dirs.is_empty() && files.is_empty() {
        output.push_str("  (empty directory)\n");
    }

    if total_count > MAX_ITEMS {
        output.push_str(&format!(
            "\n... and {} more items (showing first {} items)\n",
            total_count - MAX_ITEMS,
            MAX_ITEMS
        ));
    }

    Ok(vec![Content::text(output)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3").unwrap();

        let params = ReadParams {
            path: file_path,
            offset: None,
            limit: None,
        };

        let result = read(params).await.unwrap();
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_read_file_with_offset_limit() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3\nline 4\nline 5").unwrap();

        let params = ReadParams {
            path: file_path,
            offset: Some(1), // Start from line 2 (0-indexed)
            limit: Some(2),  // Read 2 lines
        };

        let result = read(params).await.unwrap();
        let content = &result[0];
        if let rmcp::model::RawContent::Text(text) = &content.raw {
            assert!(text.text.contains("line 2"));
            assert!(text.text.contains("line 3"));
            assert!(!text.text.contains("line 1"));
            assert!(!text.text.contains("line 4"));
        }
    }

    #[tokio::test]
    async fn test_read_directory() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file1.txt"), "content").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let params = ReadParams {
            path: dir.path().to_path_buf(),
            offset: None,
            limit: None,
        };

        let result = read(params).await.unwrap();
        let content = &result[0];
        if let rmcp::model::RawContent::Text(text) = &content.raw {
            assert!(text.text.contains("file1.txt"));
            assert!(text.text.contains("subdir/"));
        }
    }
}
