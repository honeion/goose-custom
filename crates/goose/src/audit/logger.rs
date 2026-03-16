//! 감사 로거 (전역 싱글톤)
//!
//! 애플리케이션 전체에서 사용되는 감사 로그 기록기입니다.
//! 스레드 안전하며, 여러 곳에서 동시에 로그를 기록할 수 있습니다.

use super::event::*;
use super::writer::AuditWriter;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use uuid::Uuid;

/// 전역 AuditLogger 인스턴스
static AUDIT_LOGGER: OnceLock<Arc<AuditLogger>> = OnceLock::new();

/// 감사 로거 설정
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// 감사 로그 활성화
    pub enabled: bool,
    /// 보관 기간 (일)
    pub retention_days: u32,
    /// 사용자 입력 기록
    pub log_user_input: bool,
    /// PII 미리보기 기록
    pub log_pii_preview: bool,
    /// API payload 기록
    pub log_api_payload: bool,
    /// 도구 인자 기록
    pub log_tool_args: bool,
    /// 민감 모드 (최소 기록)
    pub sensitive_mode: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 30,
            log_user_input: true,
            log_pii_preview: true,
            log_api_payload: true,
            log_tool_args: true,
            sensitive_mode: false,
        }
    }
}

/// 세션 통계
#[derive(Debug, Default)]
pub struct SessionStats {
    pub start_time: Option<Instant>,
    pub total_tokens: TokenUsage,
    pub tool_calls: usize,
    pub pii_masked_count: usize,
    pub security_events: usize,
}

/// 감사 로거
pub struct AuditLogger {
    writer: Mutex<Option<AuditWriter>>,
    config: AuditConfig,
    session_stats: Mutex<HashMap<String, SessionStats>>,
}

impl AuditLogger {
    /// 새 AuditLogger 생성
    pub fn new(config: AuditConfig) -> Result<Self> {
        let writer = if config.enabled {
            Some(AuditWriter::new(Some(config.retention_days))?)
        } else {
            None
        };

        Ok(Self {
            writer: Mutex::new(writer),
            config,
            session_stats: Mutex::new(HashMap::new()),
        })
    }

    /// 전역 로거 초기화
    pub fn init(config: AuditConfig) -> Result<()> {
        let logger = Arc::new(Self::new(config)?);
        let _ = AUDIT_LOGGER.set(logger);
        Ok(())
    }

    /// 전역 로거 가져오기
    pub fn global() -> Option<Arc<AuditLogger>> {
        AUDIT_LOGGER.get().cloned()
    }

    /// 설정 가져오기
    pub fn config(&self) -> &AuditConfig {
        &self.config
    }

    /// 이벤트 기록
    pub fn log(&self, event: AuditEvent) {
        if !self.config.enabled {
            return;
        }

        // 세션 통계 업데이트
        self.update_stats(&event);

        // 파일에 기록
        if let Ok(mut writer_guard) = self.writer.lock() {
            if let Some(writer) = writer_guard.as_mut() {
                if let Err(e) = writer.write_event(&event) {
                    tracing::error!("Failed to write audit event: {}", e);
                }
            }
        }
    }

    /// 세션 통계 업데이트
    fn update_stats(&self, event: &AuditEvent) {
        if let Ok(mut stats) = self.session_stats.lock() {
            let session_stats = stats.entry(event.session_id.clone()).or_default();

            match &event.data {
                AuditEventData::SessionStart(_) => {
                    session_stats.start_time = Some(Instant::now());
                }
                AuditEventData::PiiMasked(data) => {
                    session_stats.pii_masked_count += data.masked_count;
                }
                AuditEventData::ApiResponse(data) => {
                    session_stats.total_tokens.input += data.usage.input;
                    session_stats.total_tokens.output += data.usage.output;
                }
                AuditEventData::ToolExecution(_) => {
                    session_stats.tool_calls += 1;
                }
                AuditEventData::SecurityEvent(_) => {
                    session_stats.security_events += 1;
                }
                _ => {}
            }
        }
    }

    /// 세션 통계 가져오기
    pub fn get_session_stats(&self, session_id: &str) -> Option<SessionStats> {
        self.session_stats
            .lock()
            .ok()
            .and_then(|stats| stats.get(session_id).cloned())
    }

    /// 세션 시작 로그
    pub fn log_session_start(&self, session_id: &str, working_directory: &str) {
        self.log(AuditEvent::session_start(session_id, working_directory));
    }

    /// 세션 종료 로그
    pub fn log_session_end(&self, session_id: &str) {
        if let Some(stats) = self.get_session_stats(session_id) {
            let duration_secs = stats
                .start_time
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0);

            self.log(AuditEvent::session_end(
                session_id,
                duration_secs,
                stats.total_tokens,
                stats.tool_calls,
                stats.pii_masked_count,
                stats.security_events,
            ));
        }

        // 세션 통계 제거
        if let Ok(mut stats) = self.session_stats.lock() {
            stats.remove(session_id);
        }
    }

    /// 사용자 입력 로그 (마스킹된 상태)
    pub fn log_user_input(
        &self,
        session_id: &str,
        content_masked: &str,
        original_length: usize,
        pii_count: usize,
    ) {
        if !self.config.log_user_input {
            return;
        }

        self.log(AuditEvent::user_input(
            session_id,
            content_masked,
            original_length,
            pii_count > 0,
            pii_count,
        ));
    }

    /// PII 마스킹 로그
    pub fn log_pii_masked(&self, session_id: &str, items: Vec<MaskedPiiItem>) {
        if items.is_empty() {
            return;
        }

        // 민감 모드에서는 미리보기 제거
        let items = if self.config.sensitive_mode || !self.config.log_pii_preview {
            items
                .into_iter()
                .map(|mut item| {
                    item.preview = "***".to_string();
                    item
                })
                .collect()
        } else {
            items
        };

        self.log(AuditEvent::pii_masked(session_id, items));
    }

    /// PII 언마스킹 로그
    pub fn log_pii_unmasked(&self, session_id: &str, tokens: Vec<String>) {
        if tokens.is_empty() {
            return;
        }

        self.log(AuditEvent::pii_unmasked(session_id, tokens, true));
    }

    /// API 요청 로그
    pub fn log_api_request(&self, session_id: &str, data: ApiRequestData) {
        if !self.config.log_api_payload && !self.config.sensitive_mode {
            // 최소 정보만 기록
            let minimal_data = ApiRequestData {
                provider: data.provider,
                model: data.model,
                endpoint: None,
                message_preview: "[redacted]".to_string(),
                tools: vec![],
                token_estimate: data.token_estimate,
                pii_masked: data.pii_masked,
                masked_tokens: data.masked_tokens,
            };
            self.log(AuditEvent::api_request(session_id, minimal_data));
        } else {
            self.log(AuditEvent::api_request(session_id, data));
        }
    }

    /// API 응답 로그
    pub fn log_api_response(&self, session_id: &str, data: ApiResponseData) {
        self.log(AuditEvent::api_response(session_id, data));
    }

    /// 도구 실행 로그
    pub fn log_tool_execution(&self, session_id: &str, data: ToolExecutionData) {
        let data = if !self.config.log_tool_args {
            ToolExecutionData {
                args_masked: HashMap::new(),
                result_preview: None,
                ..data
            }
        } else {
            data
        };

        self.log(AuditEvent::tool_execution(session_id, data));
    }

    /// Hook 실행 로그
    pub fn log_hook_execution(&self, session_id: &str, data: HookExecutionData) {
        self.log(AuditEvent::hook_execution(session_id, data));
    }

    /// 보안 이벤트 로그
    pub fn log_security_event(&self, session_id: &str, data: SecurityEventData) {
        self.log(AuditEvent::security_event(session_id, data));
    }
}

impl Clone for SessionStats {
    fn clone(&self) -> Self {
        Self {
            start_time: self.start_time,
            total_tokens: self.total_tokens.clone(),
            tool_calls: self.tool_calls,
            pii_masked_count: self.pii_masked_count,
            security_events: self.security_events,
        }
    }
}

/// 새 세션 ID 생성
pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

/// 편의 함수: 전역 로거로 이벤트 기록
pub fn audit_log(event: AuditEvent) {
    if let Some(logger) = AuditLogger::global() {
        logger.log(event);
    }
}

/// 편의 매크로: 감사 로그 기록
#[macro_export]
macro_rules! audit {
    (session_start, $session_id:expr, $working_dir:expr) => {
        if let Some(logger) = $crate::audit::logger::AuditLogger::global() {
            logger.log_session_start($session_id, $working_dir);
        }
    };
    (session_end, $session_id:expr) => {
        if let Some(logger) = $crate::audit::logger::AuditLogger::global() {
            logger.log_session_end($session_id);
        }
    };
    (user_input, $session_id:expr, $content:expr, $len:expr, $pii_count:expr) => {
        if let Some(logger) = $crate::audit::logger::AuditLogger::global() {
            logger.log_user_input($session_id, $content, $len, $pii_count);
        }
    };
    (pii_masked, $session_id:expr, $items:expr) => {
        if let Some(logger) = $crate::audit::logger::AuditLogger::global() {
            logger.log_pii_masked($session_id, $items);
        }
    };
    (pii_unmasked, $session_id:expr, $tokens:expr) => {
        if let Some(logger) = $crate::audit::logger::AuditLogger::global() {
            logger.log_pii_unmasked($session_id, $tokens);
        }
    };
    (security_event, $session_id:expr, $data:expr) => {
        if let Some(logger) = $crate::audit::logger::AuditLogger::global() {
            logger.log_security_event($session_id, $data);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_id() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();

        assert_eq!(id1.len(), 8);
        assert_eq!(id2.len(), 8);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_default_config() {
        let config = AuditConfig::default();

        assert!(config.enabled);
        assert_eq!(config.retention_days, 30);
        assert!(config.log_user_input);
        assert!(config.log_pii_preview);
        assert!(!config.sensitive_mode);
    }
}
