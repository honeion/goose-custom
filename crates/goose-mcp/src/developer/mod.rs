pub mod analyze;
mod editor_models;
mod lang;
pub mod paths;
mod shell;
mod text_editor;

// New separated tools (Phase 1: tool separation)
pub mod file_history;
pub mod read;
pub mod edit;
pub mod write;
pub mod undo;
pub mod notebook_edit;

pub mod rmcp_developer;

#[cfg(test)]
mod tests;
