//! Ratatui 기반 TUI 모듈
//!
//! Phase 5: Ratatui UI 고도화
//! TEA (Elm Architecture) 패턴 기반 TUI 구현

pub mod animation;
pub mod app;
pub mod events;
pub mod markdown;
pub mod offscreen_buffer;
pub mod render;
pub mod runner;
pub mod theme;

pub use app::{ChatMessage, InputMode, MessageRole, ToolStatus, TuiApp};
pub use events::{Action, UpdateResult, event_to_action};
pub use offscreen_buffer::{OffscreenBuffer, PanelId, PanelManager};
pub use runner::{run_tui_demo, AgentMessage, TuiRunner};
pub use theme::Theme;
