//! 감사 로그 이벤트 정의
//!
//! 모든 감사 이벤트는 마스킹된 데이터만 포함합니다.
//! 원본 PII 값은 절대 로그에 기록되지 않습니다.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 감사 이벤트 타입
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// 세션 시작
    SessionStart,
    /// 세션 종료
    SessionEnd,
    /// 사용자 입력 (마스킹된 상태)
    UserInput,
    /// PII 마스킹 발생
    PiiMasked,
    /// PII 언마스킹 (표시용)
    PiiUnmasked,
    /// LLM API 요청 (마스킹된 상태)
    ApiRequest,
    /// LLM API 응답
    ApiResponse,
    /// 도구 실행
    ToolExecution,
    /// Hook 실행
    HookExecution,
    /// 보안 이벤트 (탐지/차단)
    SecurityEvent,
}

impl std::fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditEventType::SessionStart => write!(f, "SESSION_START"),
            AuditEventType::SessionEnd => write!(f, "SESSION_END"),
            AuditEventType::UserInput => write!(f, "USER_INPUT"),
            AuditEventType::PiiMasked => write!(f, "PII_MASKED"),
            AuditEventType::PiiUnmasked => write!(f, "PII_UNMASKED"),
            AuditEventType::ApiRequest => write!(f, "API_REQUEST"),
            AuditEventType::ApiResponse => write!(f, "API_RESPONSE"),
            AuditEventType::ToolExecution => write!(f, "TOOL_EXECUTION"),
            AuditEventType::HookExecution => write!(f, "HOOK_EXECUTION"),
            AuditEventType::SecurityEvent => write!(f, "SECURITY_EVENT"),
        }
    }
}

/// 보안 이벤트 심각도
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SecuritySeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// 도구 실행 결과 상태
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolResultStatus {
    Success,
    Error,
    Blocked,
    Timeout,
}

/// 마스킹된 PII 항목 (원본 없음)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskedPiiItem {
    /// 마스킹 토큰 (예: [SECRET_1])
    pub token: String,
    /// PII 타입 (예: password, api_key)
    pub pii_type: String,
    /// 부분 마스킹된 미리보기 (예: myP@****rd)
    pub preview: String,
    /// 원본 길이
    pub length: usize,
    /// 원본 텍스트 내 위치
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<PiiPosition>,
}

/// PII 위치 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiPosition {
    pub start: usize,
    pub end: usize,
}

/// 보안 검사 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityCheckResult {
    pub passed: bool,
    pub patterns_checked: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked_patterns: Vec<String>,
}

/// 감사 이벤트
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// 이벤트 타임스탬프
    pub timestamp: DateTime<Utc>,
    /// 세션 ID
    pub session_id: String,
    /// 이벤트 타입
    pub event_type: AuditEventType,
    /// 이벤트 데이터
    pub data: AuditEventData,
}

/// 이벤트별 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuditEventData {
    SessionStart(SessionStartData),
    SessionEnd(SessionEndData),
    UserInput(UserInputData),
    PiiMasked(PiiMaskedData),
    PiiUnmasked(PiiUnmaskedData),
    ApiRequest(ApiRequestData),
    ApiResponse(ApiResponseData),
    ToolExecution(ToolExecutionData),
    HookExecution(HookExecutionData),
    SecurityEvent(SecurityEventData),
}

/// 세션 시작 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartData {
    pub working_directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// 세션 종료 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndData {
    pub duration_secs: u64,
    pub total_tokens: TokenUsage,
    pub tool_calls: usize,
    pub pii_masked_count: usize,
    pub security_events: usize,
}

/// 토큰 사용량
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
}

/// 사용자 입력 데이터 (마스킹됨)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputData {
    /// 마스킹된 입력 내용
    pub content_masked: String,
    /// 원본 길이
    pub content_length: usize,
    /// PII 탐지 여부
    pub pii_detected: bool,
    /// 탐지된 PII 개수
    pub pii_count: usize,
}

/// PII 마스킹 데이터 (원본 없음)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiMaskedData {
    /// 마스킹된 항목들
    pub items: Vec<MaskedPiiItem>,
    /// 총 마스킹 개수
    pub masked_count: usize,
}

/// PII 언마스킹 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiUnmaskedData {
    /// 언마스킹된 토큰들
    pub tokens: Vec<String>,
    /// 사용자 화면 표시 여부
    pub display_to_user: bool,
}

/// API 요청 데이터 (마스킹됨)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequestData {
    /// LLM 프로바이더
    pub provider: String,
    /// 모델명
    pub model: String,
    /// API 엔드포인트
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// 마스킹된 메시지 (전체 payload는 너무 크므로 요약)
    pub message_preview: String,
    /// 사용 가능한 도구 목록
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// 예상 토큰 수
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_estimate: Option<u64>,
    /// PII 마스킹 적용됨
    pub pii_masked: bool,
    /// 마스킹된 토큰 목록
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub masked_tokens: Vec<String>,
}

/// API 응답 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponseData {
    /// 토큰 사용량
    pub usage: TokenUsage,
    /// 응답 시간 (ms)
    pub latency_ms: u64,
    /// 도구 호출 목록
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallSummary>,
    /// 응답 텍스트 미리보기 (truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_preview: Option<String>,
    /// 에러 여부
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 도구 호출 요약
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallSummary {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_preview: Option<String>,
}

/// 도구 실행 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionData {
    /// 도구 이름
    pub tool_name: String,
    /// 마스킹된 인자
    pub args_masked: HashMap<String, serde_json::Value>,
    /// 실행 시간 (ms)
    pub execution_time_ms: u64,
    /// 결과 상태
    pub result_status: ToolResultStatus,
    /// 결과 미리보기 (truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<String>,
    /// 결과 길이
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_length: Option<usize>,
    /// 보안 검사 결과
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_check: Option<SecurityCheckResult>,
}

/// Hook 실행 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookExecutionData {
    /// Hook 이름
    pub hook_name: String,
    /// Hook 타입 (pre/post)
    pub hook_type: String,
    /// 실행 시간 (ms)
    pub execution_time_ms: u64,
    /// 성공 여부
    pub success: bool,
    /// 수정 여부
    pub modified: bool,
    /// 에러 메시지
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 보안 이벤트 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEventData {
    /// 심각도
    pub severity: SecuritySeverity,
    /// 이벤트 이름
    pub event_name: String,
    /// 상세 정보
    pub details: HashMap<String, serde_json::Value>,
    /// 취한 조치
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_taken: Option<String>,
}

impl AuditEvent {
    /// 새 감사 이벤트 생성
    pub fn new(session_id: impl Into<String>, event_type: AuditEventType, data: AuditEventData) -> Self {
        Self {
            timestamp: Utc::now(),
            session_id: session_id.into(),
            event_type,
            data,
        }
    }

    /// 세션 시작 이벤트
    pub fn session_start(session_id: impl Into<String>, working_directory: impl Into<String>) -> Self {
        Self::new(
            session_id,
            AuditEventType::SessionStart,
            AuditEventData::SessionStart(SessionStartData {
                working_directory: working_directory.into(),
                provider: None,
                model: None,
            }),
        )
    }

    /// 세션 종료 이벤트
    pub fn session_end(
        session_id: impl Into<String>,
        duration_secs: u64,
        total_tokens: TokenUsage,
        tool_calls: usize,
        pii_masked_count: usize,
        security_events: usize,
    ) -> Self {
        Self::new(
            session_id,
            AuditEventType::SessionEnd,
            AuditEventData::SessionEnd(SessionEndData {
                duration_secs,
                total_tokens,
                tool_calls,
                pii_masked_count,
                security_events,
            }),
        )
    }

    /// 사용자 입력 이벤트 (마스킹됨)
    pub fn user_input(
        session_id: impl Into<String>,
        content_masked: impl Into<String>,
        content_length: usize,
        pii_detected: bool,
        pii_count: usize,
    ) -> Self {
        Self::new(
            session_id,
            AuditEventType::UserInput,
            AuditEventData::UserInput(UserInputData {
                content_masked: content_masked.into(),
                content_length,
                pii_detected,
                pii_count,
            }),
        )
    }

    /// PII 마스킹 이벤트
    pub fn pii_masked(session_id: impl Into<String>, items: Vec<MaskedPiiItem>) -> Self {
        let masked_count = items.len();
        Self::new(
            session_id,
            AuditEventType::PiiMasked,
            AuditEventData::PiiMasked(PiiMaskedData { items, masked_count }),
        )
    }

    /// PII 언마스킹 이벤트
    pub fn pii_unmasked(session_id: impl Into<String>, tokens: Vec<String>, display_to_user: bool) -> Self {
        Self::new(
            session_id,
            AuditEventType::PiiUnmasked,
            AuditEventData::PiiUnmasked(PiiUnmaskedData { tokens, display_to_user }),
        )
    }

    /// API 요청 이벤트
    pub fn api_request(session_id: impl Into<String>, data: ApiRequestData) -> Self {
        Self::new(session_id, AuditEventType::ApiRequest, AuditEventData::ApiRequest(data))
    }

    /// API 응답 이벤트
    pub fn api_response(session_id: impl Into<String>, data: ApiResponseData) -> Self {
        Self::new(session_id, AuditEventType::ApiResponse, AuditEventData::ApiResponse(data))
    }

    /// 도구 실행 이벤트
    pub fn tool_execution(session_id: impl Into<String>, data: ToolExecutionData) -> Self {
        Self::new(session_id, AuditEventType::ToolExecution, AuditEventData::ToolExecution(data))
    }

    /// Hook 실행 이벤트
    pub fn hook_execution(session_id: impl Into<String>, data: HookExecutionData) -> Self {
        Self::new(session_id, AuditEventType::HookExecution, AuditEventData::HookExecution(data))
    }

    /// 보안 이벤트
    pub fn security_event(session_id: impl Into<String>, data: SecurityEventData) -> Self {
        Self::new(session_id, AuditEventType::SecurityEvent, AuditEventData::SecurityEvent(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_user_input_event() {
        let event = AuditEvent::user_input(
            "session123",
            "비밀번호 [SECRET_1]로 서버 [IP_1]에 접속해줘",
            42,
            true,
            2,
        );

        assert_eq!(event.session_id, "session123");
        assert_eq!(event.event_type, AuditEventType::UserInput);

        if let AuditEventData::UserInput(data) = &event.data {
            assert!(data.content_masked.contains("[SECRET_1]"));
            assert!(data.pii_detected);
            assert_eq!(data.pii_count, 2);
        } else {
            panic!("Expected UserInput data");
        }
    }

    #[test]
    fn test_pii_masked_event() {
        let items = vec![
            MaskedPiiItem {
                token: "[SECRET_1]".to_string(),
                pii_type: "password".to_string(),
                preview: "myP@****rd".to_string(),
                length: 10,
                position: Some(PiiPosition { start: 5, end: 15 }),
            },
        ];

        let event = AuditEvent::pii_masked("session123", items);

        if let AuditEventData::PiiMasked(data) = &event.data {
            assert_eq!(data.masked_count, 1);
            assert_eq!(data.items[0].token, "[SECRET_1]");
            // 원본 값은 포함되지 않음
            assert_eq!(data.items[0].preview, "myP@****rd");
        } else {
            panic!("Expected PiiMasked data");
        }
    }

    #[test]
    fn test_serialize_event() {
        let event = AuditEvent::user_input("session123", "test [SECRET_1]", 15, true, 1);
        let json = serde_json::to_string(&event).unwrap();

        // JSON에 원본 PII 값이 없어야 함
        assert!(!json.contains("myP@ssw0rd"));
        assert!(json.contains("[SECRET_1]"));
        assert!(json.contains("session123"));
    }
}
