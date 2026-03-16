//! PII 마스킹/복원 로직
//!
//! LLM에 전송하기 전에 민감 정보를 마스킹하고,
//! 응답에서 마스킹된 토큰을 원본으로 복원합니다.
//!
//! ## 감사 로그 연동
//!
//! 마스킹 결과를 감사 로그에 기록할 때는 `MaskedItem::to_audit_item()`을
//! 사용하여 `MaskedPiiItem`으로 변환합니다. 원본 값은 절대 로그에 기록되지 않습니다.

use regex::Regex;
use std::collections::HashMap;

use super::pii_patterns::{MaskType, PiiPattern, PII_PATTERNS};
use crate::audit::event::{MaskedPiiItem, PiiPosition};

/// 컴파일된 패턴
struct CompiledPattern {
    name: String,
    regex: Regex,
    mask_type: MaskType,
    value_group: Option<usize>,
}

/// 마스킹된 항목 정보 (UI 표시용)
#[derive(Debug, Clone)]
pub struct MaskedItem {
    /// 마스킹 토큰 (예: [SECRET_1])
    pub token: String,
    /// 마스킹 타입
    pub mask_type: MaskType,
    /// 패턴 이름
    pub pattern_name: String,
    /// 원본 값의 일부 (UI 표시용, 앞뒤 일부만)
    pub partial_original: String,
    /// 원본 길이
    pub original_length: usize,
    /// 원본 텍스트 내 위치 (옵션)
    pub position: Option<(usize, usize)>,
}

impl MaskedItem {
    /// 감사 로그용 MaskedPiiItem으로 변환
    ///
    /// 원본 값은 포함하지 않고, 미리보기만 포함합니다.
    pub fn to_audit_item(&self) -> MaskedPiiItem {
        MaskedPiiItem {
            token: self.token.clone(),
            pii_type: self.pattern_name.clone(),
            preview: self.partial_original.clone(),
            length: self.original_length,
            position: self.position.map(|(start, end)| PiiPosition { start, end }),
        }
    }
}

/// 마스킹 결과
#[derive(Debug, Clone)]
pub struct MaskResult {
    /// 마스킹된 텍스트
    pub masked_text: String,
    /// 마스킹된 항목 수
    pub masked_count: usize,
    /// 마스킹된 항목 목록 (UI 표시용)
    pub masked_items: Vec<MaskedItem>,
}

impl MaskResult {
    /// 감사 로그용 MaskedPiiItem 목록으로 변환
    pub fn to_audit_items(&self) -> Vec<MaskedPiiItem> {
        self.masked_items.iter().map(|item| item.to_audit_item()).collect()
    }

    /// 마스킹된 토큰 목록 반환
    pub fn masked_tokens(&self) -> Vec<String> {
        self.masked_items.iter().map(|item| item.token.clone()).collect()
    }
}

/// PII 마스커
pub struct PiiMasker {
    /// 컴파일된 패턴 목록
    patterns: Vec<CompiledPattern>,
    /// 마스킹 매핑 테이블 (토큰 -> 원본)
    mappings: HashMap<String, String>,
    /// 역매핑 테이블 (원본 -> 토큰) - 중복 방지용
    reverse_mappings: HashMap<String, String>,
    /// 타입별 카운터
    counters: HashMap<MaskType, usize>,
    /// 활성화 여부
    enabled: bool,
}

impl PiiMasker {
    /// 새 PiiMasker 생성
    pub fn new() -> Self {
        Self::with_patterns(PII_PATTERNS)
    }

    /// 커스텀 패턴으로 생성
    pub fn with_patterns(patterns: &[PiiPattern]) -> Self {
        let compiled: Vec<CompiledPattern> = patterns
            .iter()
            .filter_map(|p| {
                match Regex::new(p.pattern) {
                    Ok(regex) => Some(CompiledPattern {
                        name: p.name.to_string(),
                        regex,
                        mask_type: p.mask_type,
                        value_group: p.value_group,
                    }),
                    Err(e) => {
                        tracing::warn!("Failed to compile PII pattern '{}': {}", p.name, e);
                        None
                    }
                }
            })
            .collect();

        Self {
            patterns: compiled,
            mappings: HashMap::new(),
            reverse_mappings: HashMap::new(),
            counters: HashMap::new(),
            enabled: true,
        }
    }

    /// 마스킹 활성화/비활성화
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// 마스킹 활성화 여부
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 입력 텍스트에서 PII를 마스킹
    pub fn mask(&mut self, input: &str) -> MaskResult {
        if !self.enabled {
            return MaskResult {
                masked_text: input.to_string(),
                masked_count: 0,
                masked_items: Vec::new(),
            };
        }

        let mut masked_text = input.to_string();
        let mut masked_items = Vec::new();

        // 패턴 정보를 먼저 복사 (borrow 문제 해결)
        let pattern_infos: Vec<_> = self
            .patterns
            .iter()
            .map(|p| (p.regex.clone(), p.name.clone(), p.mask_type, p.value_group))
            .collect();

        // 각 패턴에 대해 마스킹 수행
        for (regex, pattern_name, mask_type, value_group) in pattern_infos {
            // 모든 매치를 먼저 찾음 (소유권 있는 데이터로 복사)
            let matches: Vec<(usize, usize, String)> = regex
                .find_iter(&masked_text)
                .map(|m| (m.start(), m.end(), m.as_str().to_string()))
                .collect();

            // 뒤에서부터 교체해야 인덱스가 안 밀림
            for (match_start, match_end, full_match) in matches.into_iter().rev() {
                let full_match_str = full_match.as_str();

                // 값 추출 (캡처 그룹 또는 전체 매치)
                let value = if let Some(group_idx) = value_group {
                    regex
                        .captures(full_match_str)
                        .and_then(|caps| caps.get(group_idx))
                        .map(|m| m.as_str())
                        .unwrap_or(full_match_str)
                } else {
                    full_match_str
                };

                // 이미 마스킹된 토큰이 포함되어 있으면 스킵 (중복 처리 방지)
                // 토큰 형식: [SECRET_1], [CRED_2], [CERT_3], [TOKEN_4]
                if value.contains("[SECRET_") || value.contains("[CRED_")
                    || value.contains("[CERT_") || value.contains("[TOKEN_") {
                    continue;
                }

                // 이미 마스킹된 값인지 확인
                let token = if let Some(existing_token) = self.reverse_mappings.get(value) {
                    existing_token.clone()
                } else {
                    // 새 토큰 생성
                    let new_token = self.generate_token(mask_type);

                    // 매핑 저장
                    self.mappings.insert(new_token.clone(), value.to_string());
                    self.reverse_mappings.insert(value.to_string(), new_token.clone());

                    // 마스킹된 항목 정보 저장
                    masked_items.push(MaskedItem {
                        token: new_token.clone(),
                        mask_type,
                        pattern_name: pattern_name.clone(),
                        partial_original: Self::partial_mask(value),
                        original_length: value.len(),
                        position: Some((match_start, match_end)),
                    });

                    new_token
                };

                // 텍스트 대체
                let replacement = if value_group.is_some() {
                    full_match_str.replace(value, &token)
                } else {
                    token
                };

                // 문자열 직접 교체 (바이트 인덱스 사용)
                masked_text = format!(
                    "{}{}{}",
                    &masked_text[..match_start],
                    replacement,
                    &masked_text[match_end..]
                );
            }
        }

        MaskResult {
            masked_text,
            masked_count: masked_items.len(),
            masked_items,
        }
    }

    /// 마스킹된 텍스트를 원본으로 복원
    pub fn unmask(&self, input: &str) -> String {
        let mut result = input.to_string();

        for (token, original) in &self.mappings {
            result = result.replace(token, original);
        }

        result
    }

    /// 세션 종료 시 매핑 테이블 초기화
    pub fn clear(&mut self) {
        self.mappings.clear();
        self.reverse_mappings.clear();
        self.counters.clear();
    }

    /// 현재 마스킹된 항목 수
    pub fn masked_count(&self) -> usize {
        self.mappings.len()
    }

    /// 마스킹 토큰 생성
    fn generate_token(&mut self, mask_type: MaskType) -> String {
        let counter = self.counters.entry(mask_type).or_insert(0);
        *counter += 1;
        format!("[{}_{}]", mask_type.prefix(), counter)
    }

    /// 원본 값의 일부만 표시 (예: MyP@ss123 -> MyP@****23)
    fn partial_mask(value: &str) -> String {
        let len = value.chars().count();
        if len <= 4 {
            return "*".repeat(len);
        }

        let visible_chars = 2.min(len / 4);
        let chars: Vec<char> = value.chars().collect();

        let start: String = chars[..visible_chars].iter().collect();
        let end: String = chars[len - visible_chars..].iter().collect();
        let middle = "*".repeat(4.min(len - visible_chars * 2));

        format!("{}{}{}", start, middle, end)
    }
}

impl Default for PiiMasker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_password_masking() {
        let mut masker = PiiMasker::new();
        let result = masker.mask("password=MyP@ss123 연결 안 돼");

        assert!(result.masked_text.contains("[SECRET_1]"));
        assert!(!result.masked_text.contains("MyP@ss123"));
        assert_eq!(result.masked_count, 1);
    }

    #[test]
    fn test_unmask() {
        let mut masker = PiiMasker::new();
        masker.mask("password=MyP@ss123");

        let restored = masker.unmask("[SECRET_1] 형식이 잘못됐네요");
        assert_eq!(restored, "MyP@ss123 형식이 잘못됐네요");
    }

    #[test]
    fn test_multiple_masking() {
        let mut masker = PiiMasker::new();
        let result = masker.mask("password=secret123 api_key=sk-abcdef");

        assert!(result.masked_text.contains("[SECRET_1]"));
        assert!(result.masked_text.contains("[SECRET_2]"));
        assert!(!result.masked_text.contains("secret123"));
        assert!(!result.masked_text.contains("sk-abcdef"));
        assert_eq!(result.masked_count, 2);
    }

    #[test]
    fn test_bearer_token_masking() {
        let mut masker = PiiMasker::new();
        let result = masker.mask("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.xxx");

        assert!(result.masked_text.contains("[TOKEN_"));
        assert!(!result.masked_text.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn test_connection_string_masking() {
        let mut masker = PiiMasker::new();
        let result = masker.mask("Server=localhost;Database=test;Password=supersecret123;");

        // password_assignment 패턴이 먼저 매칭되어 [SECRET_1] 생성
        assert!(result.masked_text.contains("[SECRET_"));
        assert!(!result.masked_text.contains("supersecret123"));
        // 키워드는 유지
        assert!(result.masked_text.contains("Password="));
    }

    #[test]
    fn test_same_value_same_token() {
        let mut masker = PiiMasker::new();
        let result = masker.mask("password=secret password=secret");

        // 같은 값은 같은 토큰으로 마스킹
        let count = result.masked_text.matches("[SECRET_1]").count();
        assert_eq!(count, 2);
        assert_eq!(result.masked_count, 1); // 실제 마스킹은 1번만
    }

    #[test]
    fn test_disabled_masking() {
        let mut masker = PiiMasker::new();
        masker.set_enabled(false);

        let result = masker.mask("password=secret123");
        assert_eq!(result.masked_text, "password=secret123");
        assert_eq!(result.masked_count, 0);
    }

    #[test]
    fn test_clear() {
        let mut masker = PiiMasker::new();
        masker.mask("password=secret123");

        assert_eq!(masker.masked_count(), 1);

        masker.clear();
        assert_eq!(masker.masked_count(), 0);

        // 복원 불가
        let restored = masker.unmask("[SECRET_1]");
        assert_eq!(restored, "[SECRET_1]");
    }

    #[test]
    fn test_partial_mask() {
        assert_eq!(PiiMasker::partial_mask("MyP@ss123"), "My****23");
        assert_eq!(PiiMasker::partial_mask("abc"), "***");
        assert_eq!(PiiMasker::partial_mask("abcdefghij"), "ab****ij");
    }

    #[test]
    fn test_private_key_masking() {
        let mut masker = PiiMasker::new();
        let key = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7
-----END PRIVATE KEY-----"#;

        let result = masker.mask(&format!("내 키는 {} 야", key));

        assert!(result.masked_text.contains("[CERT_1]"));
        assert!(!result.masked_text.contains("BEGIN PRIVATE KEY"));
        assert_eq!(result.masked_count, 1);
    }

    #[test]
    fn test_openai_key_masking() {
        let mut masker = PiiMasker::new();
        let result = masker.mask("OPENAI_API_KEY=sk-1234567890abcdef1234567890abcdef");

        assert!(result.masked_text.contains("[SECRET_"));
        assert!(!result.masked_text.contains("sk-1234567890abcdef1234567890abcdef"));
    }

    #[test]
    fn test_masked_items_info() {
        let mut masker = PiiMasker::new();
        let result = masker.mask("password=MySecretPassword123");

        assert_eq!(result.masked_items.len(), 1);
        let item = &result.masked_items[0];
        assert_eq!(item.mask_type, MaskType::Secret);
        assert_eq!(item.pattern_name, "password_assignment");
        // partial_original은 일부만 표시
        assert!(item.partial_original.contains("*"));
    }

    // ============================================================
    // LLM 응답 언마스킹 시나리오 테스트
    // GPT-4o 등 LLM이 민감정보 출력을 거부하므로 수동 테스트 불가
    // 아래 테스트들이 언마스킹 로직을 검증함
    // ============================================================

    #[test]
    fn test_llm_response_unmask_single_token() {
        // 시나리오: 사용자가 password=secret123 입력
        // LLM이 "[SECRET_1]이 올바르지 않습니다" 응답
        let mut masker = PiiMasker::new();

        // 1. 사용자 메시지 마스킹
        let user_input = "password=secret123 이게 왜 안되지?";
        let mask_result = masker.mask(user_input);
        assert_eq!(mask_result.masked_text, "password=[SECRET_1] 이게 왜 안되지?");

        // 2. LLM 응답에서 토큰이 포함된 경우 언마스킹
        let llm_response = "[SECRET_1]이 올바른 형식이 아닙니다. 특수문자를 포함해야 합니다.";
        let unmasked = masker.unmask(llm_response);
        assert_eq!(unmasked, "secret123이 올바른 형식이 아닙니다. 특수문자를 포함해야 합니다.");
    }

    #[test]
    fn test_llm_response_unmask_multiple_tokens() {
        // 시나리오: 여러 비밀번호/키가 포함된 요청
        let mut masker = PiiMasker::new();

        // 1. 사용자 메시지 마스킹
        let user_input = "password=pass1 api_key=sk-abcdef1234567890abcdef1234567890 둘다 안됨";
        let mask_result = masker.mask(user_input);
        assert!(mask_result.masked_text.contains("[SECRET_1]"));
        assert!(mask_result.masked_text.contains("[SECRET_2]"));

        // 2. LLM이 두 토큰 모두 언급하는 응답
        let llm_response = "[SECRET_1]은 너무 짧고, [SECRET_2]는 만료되었습니다.";
        let unmasked = masker.unmask(llm_response);
        assert!(unmasked.contains("pass1"));
        assert!(unmasked.contains("sk-abcdef1234567890abcdef1234567890"));
        assert!(!unmasked.contains("[SECRET_"));
    }

    #[test]
    fn test_llm_response_unmask_with_context() {
        // 시나리오: 실제 LLM 응답 형태 시뮬레이션
        let mut masker = PiiMasker::new();

        // 사용자: DB 연결 문자열 문제 질문
        let user_input = "Server=localhost;Database=mydb;Password=MyP@ssw0rd123; 연결이 안 돼요";
        let mask_result = masker.mask(user_input);
        assert!(mask_result.masked_count >= 1);

        // LLM 응답 (토큰 포함)
        let llm_response = r#"연결 문자열을 확인해보니, 비밀번호 [SECRET_1]에 특수문자가 포함되어 있습니다.
URL 인코딩이 필요할 수 있습니다. 다음과 같이 시도해보세요:
Password=%4D%79%50%40%73%73%77%30%72%64%31%32%33"#;

        let unmasked = masker.unmask(llm_response);
        assert!(unmasked.contains("MyP@ssw0rd123"));
        assert!(!unmasked.contains("[SECRET_1]"));
    }

    #[test]
    fn test_llm_response_no_token_unchanged() {
        // 시나리오: LLM 응답에 토큰이 없는 경우 (일반적인 경우)
        let mut masker = PiiMasker::new();

        // 마스킹 수행
        masker.mask("password=secret123");

        // 토큰이 없는 응답은 그대로 유지
        let llm_response = "비밀번호 형식이 올바르지 않습니다.";
        let unmasked = masker.unmask(llm_response);
        assert_eq!(unmasked, llm_response);
    }

    #[test]
    fn test_llm_response_partial_token_unchanged() {
        // 시나리오: 토큰처럼 보이지만 실제 토큰이 아닌 경우
        let mut masker = PiiMasker::new();

        masker.mask("password=secret123");

        // [SECRET_99]는 매핑에 없으므로 그대로 유지
        let llm_response = "오류 코드 [SECRET_99]가 발생했습니다.";
        let unmasked = masker.unmask(llm_response);
        assert_eq!(unmasked, llm_response); // 변경 없음
    }

    #[test]
    fn test_round_trip_mask_unmask() {
        // 전체 라운드트립 테스트
        let mut masker = PiiMasker::new();
        let original_secrets = vec![
            "MyP@ssword123",
            "sk-1234567890abcdef1234567890abcdef",
            "supersecretkey123",
        ];

        // 마스킹 (패턴에 맞는 형식 사용)
        let input = format!(
            "password={} api_key={} secret_key={}",
            original_secrets[0], original_secrets[1], original_secrets[2]
        );
        let mask_result = masker.mask(&input);

        // 마스킹된 텍스트에 원본 없음 확인
        for secret in &original_secrets {
            assert!(!mask_result.masked_text.contains(secret),
                "Expected '{}' to be masked, but found in: {}",
                secret, mask_result.masked_text);
        }

        // 언마스킹으로 원본 복원
        let restored = masker.unmask(&mask_result.masked_text);
        for secret in &original_secrets {
            assert!(restored.contains(secret),
                "Expected '{}' to be restored, but not found in: {}",
                secret, restored);
        }
    }
}
