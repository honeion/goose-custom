//! 감사 로그 시스템
//!
//! ## 핵심 원칙
//!
//! **모든 로그에서 민감정보는 마스킹된 상태로만 기록됩니다.**
//!
//! - 원본 PII 값은 절대 로그에 기록되지 않습니다
//! - 외부 LLM으로 전송되는 데이터는 명확히 구분하여 표시됩니다
//! - 마스킹이 정상 작동 중임을 시각적으로 확인할 수 있습니다
//!
//! ## 이벤트 타입
//!
//! - `SessionStart` / `SessionEnd`: 세션 시작/종료
//! - `UserInput`: 사용자 입력 (마스킹된 상태)
//! - `PiiMasked`: PII 마스킹 발생 (원본 값 없음, 미리보기만)
//! - `PiiUnmasked`: PII 언마스킹 (사용자 화면 표시용)
//! - `ApiRequest`: LLM API 요청 (마스킹된 상태)
//! - `ApiResponse`: LLM API 응답
//! - `ToolExecution`: 도구 실행
//! - `HookExecution`: Hook 실행
//! - `SecurityEvent`: 보안 이벤트 (탐지/차단)
//!
//! ## 사용법
//!
//! ```rust,ignore
//! use goose::audit::{AuditLogger, AuditConfig};
//!
//! // 초기화 (애플리케이션 시작 시 한 번)
//! AuditLogger::init(AuditConfig::default())?;
//!
//! // 전역 로거 사용
//! if let Some(logger) = AuditLogger::global() {
//!     logger.log_session_start("session123", "/home/user/project");
//!     logger.log_user_input("session123", "마스킹된 내용", 100, 2);
//! }
//!
//! // 매크로 사용
//! audit!(session_start, "session123", "/home/user/project");
//! audit!(user_input, "session123", "마스킹된 내용", 100, 2);
//! ```
//!
//! ## 로그 파일 위치
//!
//! ```text
//! ~/.local/state/goose/logs/audit/
//! ├── audit.2026-03-13.jsonl
//! ├── audit.2026-03-12.jsonl
//! └── ...
//! ```

pub mod event;
pub mod logger;
pub mod writer;

// Re-exports
pub use event::*;
pub use logger::{audit_log, generate_session_id, AuditConfig, AuditLogger};
pub use writer::AuditWriter;
