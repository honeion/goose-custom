//! NotebookEdit tool - Jupyter notebook cell editing
//!
//! Provides cell-level editing for .ipynb files:
//! - Replace cell content
//! - Insert new cells
//! - Delete cells
//! - Change cell type (code/markdown)

use indoc::formatdoc;
use rmcp::model::{Content, ErrorCode, ErrorData, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

use super::file_history::{save_to_history, FileHistory};

/// Cell type in Jupyter notebook
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CellType {
    Code,
    Markdown,
    Raw,
}

impl Default for CellType {
    fn default() -> Self {
        CellType::Code
    }
}

impl std::fmt::Display for CellType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CellType::Code => write!(f, "code"),
            CellType::Markdown => write!(f, "markdown"),
            CellType::Raw => write!(f, "raw"),
        }
    }
}

/// Operation type for notebook editing
#[derive(Debug, Clone, PartialEq)]
pub enum NotebookOperation {
    /// Replace content of existing cell
    Replace,
    /// Insert new cell at position
    Insert,
    /// Delete cell at position
    Delete,
}

impl Default for NotebookOperation {
    fn default() -> Self {
        NotebookOperation::Replace
    }
}

/// Parameters for the notebook_edit tool
#[derive(Debug, Clone)]
pub struct NotebookEditParams {
    /// Path to the .ipynb file
    pub path: PathBuf,
    /// Cell index (0-based)
    pub cell_index: usize,
    /// New content for the cell (lines joined by \n)
    pub content: Option<String>,
    /// Cell type (code, markdown, raw)
    pub cell_type: Option<CellType>,
    /// Operation: replace, insert, delete
    pub operation: NotebookOperation,
}

/// Edits a Jupyter notebook cell
pub async fn notebook_edit(
    params: NotebookEditParams,
    file_history: &FileHistory,
) -> Result<Vec<Content>, ErrorData> {
    let path = &params.path;

    // Validate file extension
    if path.extension().and_then(|e| e.to_str()) != Some("ipynb") {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "File must have .ipynb extension".to_string(),
            None,
        ));
    }

    // Check if file exists
    if !path.is_file() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("File '{}' does not exist", path.display()),
            None,
        ));
    }

    // Save to history before modifying
    save_to_history(path, file_history)?;

    // Read and parse notebook
    let content = std::fs::read_to_string(path).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to read notebook: {}", e),
            None,
        )
    })?;

    let mut notebook: Value = serde_json::from_str(&content).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to parse notebook JSON: {}", e),
            None,
        )
    })?;

    // Get cells array
    let cells = notebook
        .get_mut("cells")
        .and_then(|c| c.as_array_mut())
        .ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Invalid notebook format: missing 'cells' array".to_string(),
                None,
            )
        })?;

    let cell_count = cells.len();
    let result_message: String;

    match params.operation {
        NotebookOperation::Replace => {
            // Validate cell index
            if params.cell_index >= cell_count {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "Cell index {} is out of range. Notebook has {} cells (0-{}).",
                        params.cell_index,
                        cell_count,
                        cell_count.saturating_sub(1)
                    ),
                    None,
                ));
            }

            let cell = &mut cells[params.cell_index];

            // Update content if provided
            if let Some(new_content) = &params.content {
                let source_lines: Vec<Value> = new_content
                    .lines()
                    .enumerate()
                    .map(|(i, line)| {
                        if i < new_content.lines().count() - 1 {
                            json!(format!("{}\n", line))
                        } else {
                            json!(line.to_string())
                        }
                    })
                    .collect();
                cell["source"] = json!(source_lines);
            }

            // Update cell type if provided
            if let Some(cell_type) = &params.cell_type {
                cell["cell_type"] = json!(cell_type.to_string());

                // Ensure proper structure for cell type
                match cell_type {
                    CellType::Code => {
                        if cell.get("execution_count").is_none() {
                            cell["execution_count"] = json!(null);
                        }
                        if cell.get("outputs").is_none() {
                            cell["outputs"] = json!([]);
                        }
                    }
                    CellType::Markdown | CellType::Raw => {
                        // Remove code-specific fields
                        if let Some(obj) = cell.as_object_mut() {
                            obj.remove("execution_count");
                            obj.remove("outputs");
                        }
                    }
                }
            }

            result_message = format!(
                "Updated cell {} in {}",
                params.cell_index,
                path.display()
            );
        }

        NotebookOperation::Insert => {
            // For insert, cell_index is where the new cell will be inserted
            if params.cell_index > cell_count {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "Insert position {} is out of range. Valid range is 0-{}.",
                        params.cell_index, cell_count
                    ),
                    None,
                ));
            }

            let cell_type = params.cell_type.clone().unwrap_or(CellType::Code);
            let content = params.content.clone().unwrap_or_default();

            let source_lines: Vec<Value> = if content.is_empty() {
                vec![]
            } else {
                content
                    .lines()
                    .enumerate()
                    .map(|(i, line)| {
                        if i < content.lines().count() - 1 {
                            json!(format!("{}\n", line))
                        } else {
                            json!(line.to_string())
                        }
                    })
                    .collect()
            };

            let new_cell = match cell_type {
                CellType::Code => json!({
                    "cell_type": "code",
                    "execution_count": null,
                    "metadata": {},
                    "outputs": [],
                    "source": source_lines
                }),
                CellType::Markdown => json!({
                    "cell_type": "markdown",
                    "metadata": {},
                    "source": source_lines
                }),
                CellType::Raw => json!({
                    "cell_type": "raw",
                    "metadata": {},
                    "source": source_lines
                }),
            };

            cells.insert(params.cell_index, new_cell);

            result_message = format!(
                "Inserted new {} cell at position {} in {}",
                cell_type,
                params.cell_index,
                path.display()
            );
        }

        NotebookOperation::Delete => {
            if params.cell_index >= cell_count {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "Cell index {} is out of range. Notebook has {} cells (0-{}).",
                        params.cell_index,
                        cell_count,
                        cell_count.saturating_sub(1)
                    ),
                    None,
                ));
            }

            cells.remove(params.cell_index);

            result_message = format!(
                "Deleted cell {} from {} (now has {} cells)",
                params.cell_index,
                path.display(),
                cell_count - 1
            );
        }
    }

    // Write back to file with pretty formatting
    let output = serde_json::to_string_pretty(&notebook).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to serialize notebook: {}", e),
            None,
        )
    })?;

    std::fs::write(path, &output).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to write notebook: {}", e),
            None,
        )
    })?;

    // Show updated notebook summary
    let updated_notebook: Value = serde_json::from_str(&output).unwrap();
    let updated_cells = updated_notebook["cells"].as_array().unwrap();

    let summary = format_notebook_summary(path, updated_cells);

    Ok(vec![
        Content::text(result_message.clone()).with_audience(vec![Role::Assistant]),
        Content::text(formatdoc! {"
            {message}

            {summary}
            ",
            message = result_message,
            summary = summary,
        })
        .with_audience(vec![Role::User])
        .with_priority(0.2),
    ])
}

/// Formats a summary of notebook cells
fn format_notebook_summary(path: &PathBuf, cells: &[Value]) -> String {
    let mut summary = format!("### {} ({} cells)\n\n", path.display(), cells.len());

    for (i, cell) in cells.iter().enumerate() {
        let cell_type = cell["cell_type"].as_str().unwrap_or("unknown");
        let source = cell["source"].as_array();

        let preview = if let Some(lines) = source {
            let text: String = lines
                .iter()
                .filter_map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join("");

            let first_line = text.lines().next().unwrap_or("");
            if first_line.len() > 60 {
                format!("{}...", &first_line[..60])
            } else {
                first_line.to_string()
            }
        } else {
            "(empty)".to_string()
        };

        let type_icon = match cell_type {
            "code" => "📝",
            "markdown" => "📄",
            "raw" => "📋",
            _ => "❓",
        };

        summary.push_str(&format!(
            "  {} [{}] {}: {}\n",
            type_icon, i, cell_type, preview
        ));
    }

    summary
}

/// Reads a notebook and returns its structure
pub async fn read_notebook(path: &PathBuf) -> Result<Vec<Content>, ErrorData> {
    if !path.is_file() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("File '{}' does not exist", path.display()),
            None,
        ));
    }

    let content = std::fs::read_to_string(path).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to read notebook: {}", e),
            None,
        )
    })?;

    let notebook: Value = serde_json::from_str(&content).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to parse notebook JSON: {}", e),
            None,
        )
    })?;

    let cells = notebook["cells"].as_array().ok_or_else(|| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            "Invalid notebook format: missing 'cells' array".to_string(),
            None,
        )
    })?;

    let mut output = format!("### {} ({} cells)\n\n", path.display(), cells.len());

    for (i, cell) in cells.iter().enumerate() {
        let cell_type = cell["cell_type"].as_str().unwrap_or("unknown");
        let source = cell["source"].as_array();

        let type_icon = match cell_type {
            "code" => "📝",
            "markdown" => "📄",
            "raw" => "📋",
            _ => "❓",
        };

        output.push_str(&format!("\n{} Cell {} [{}]\n", type_icon, i, cell_type));
        output.push_str("```");

        if cell_type == "code" {
            // Try to detect language from notebook metadata
            let lang = notebook
                .get("metadata")
                .and_then(|m| m.get("language_info"))
                .and_then(|l| l.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("python");
            output.push_str(lang);
        }

        output.push('\n');

        if let Some(lines) = source {
            let text: String = lines
                .iter()
                .filter_map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join("");
            output.push_str(&text);
            if !text.ends_with('\n') {
                output.push('\n');
            }
        }

        output.push_str("```\n");

        // Show outputs for code cells
        if cell_type == "code" {
            if let Some(outputs) = cell["outputs"].as_array() {
                if !outputs.is_empty() {
                    output.push_str("Output:\n");
                    for out in outputs {
                        if let Some(text) = out.get("text").and_then(|t| t.as_array()) {
                            let text_str: String = text
                                .iter()
                                .filter_map(|l| l.as_str())
                                .collect::<Vec<_>>()
                                .join("");
                            output.push_str(&format!("```\n{}\n```\n", text_str.trim()));
                        } else if let Some(data) = out.get("data") {
                            if let Some(plain) = data.get("text/plain").and_then(|t| t.as_array()) {
                                let text_str: String = plain
                                    .iter()
                                    .filter_map(|l| l.as_str())
                                    .collect::<Vec<_>>()
                                    .join("");
                                output.push_str(&format!("```\n{}\n```\n", text_str.trim()));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(vec![
        Content::text(output.clone()).with_audience(vec![Role::Assistant]),
        Content::text(output)
            .with_audience(vec![Role::User])
            .with_priority(0.0),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::developer::file_history::new_file_history;
    use std::fs;
    use tempfile::tempdir;

    fn create_test_notebook() -> String {
        r##"{
            "cells": [
                {
                    "cell_type": "code",
                    "execution_count": 1,
                    "metadata": {},
                    "outputs": [],
                    "source": ["print('hello')"]
                },
                {
                    "cell_type": "markdown",
                    "metadata": {},
                    "source": ["# Title"]
                }
            ],
            "metadata": {
                "language_info": {
                    "name": "python"
                }
            },
            "nbformat": 4,
            "nbformat_minor": 5
        }"##
        .to_string()
    }

    #[tokio::test]
    async fn test_notebook_replace_cell() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.ipynb");
        fs::write(&file_path, create_test_notebook()).unwrap();

        let history = new_file_history();

        let params = NotebookEditParams {
            path: file_path.clone(),
            cell_index: 0,
            content: Some("print('world')".to_string()),
            cell_type: None,
            operation: NotebookOperation::Replace,
        };

        let result = notebook_edit(params, &history).await.unwrap();
        assert!(!result.is_empty());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("world"));
    }

    #[tokio::test]
    async fn test_notebook_insert_cell() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.ipynb");
        fs::write(&file_path, create_test_notebook()).unwrap();

        let history = new_file_history();

        let params = NotebookEditParams {
            path: file_path.clone(),
            cell_index: 1,
            content: Some("# New Section".to_string()),
            cell_type: Some(CellType::Markdown),
            operation: NotebookOperation::Insert,
        };

        let result = notebook_edit(params, &history).await.unwrap();
        assert!(!result.is_empty());

        let content = fs::read_to_string(&file_path).unwrap();
        let notebook: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(notebook["cells"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_notebook_delete_cell() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.ipynb");
        fs::write(&file_path, create_test_notebook()).unwrap();

        let history = new_file_history();

        let params = NotebookEditParams {
            path: file_path.clone(),
            cell_index: 0,
            content: None,
            cell_type: None,
            operation: NotebookOperation::Delete,
        };

        let result = notebook_edit(params, &history).await.unwrap();
        assert!(!result.is_empty());

        let content = fs::read_to_string(&file_path).unwrap();
        let notebook: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(notebook["cells"].as_array().unwrap().len(), 1);
    }
}
