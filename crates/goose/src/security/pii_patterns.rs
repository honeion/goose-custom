//! PII 패턴 정의
//!
//! LLM에 전송하기 전에 마스킹할 민감 정보 패턴

/// 마스킹 토큰 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaskType {
    /// 비밀번호, API 키 등 일반 시크릿
    Secret,
    /// DB 연결 문자열 등 자격증명
    Credential,
    /// 인증서, 개인키
    Certificate,
    /// Bearer 토큰, JWT 등
    Token,
}

impl MaskType {
    /// 마스킹 토큰 접두사
    pub fn prefix(&self) -> &'static str {
        match self {
            MaskType::Secret => "SECRET",
            MaskType::Credential => "CRED",
            MaskType::Certificate => "CERT",
            MaskType::Token => "TOKEN",
        }
    }
}

/// PII 패턴 정의
#[derive(Debug, Clone)]
pub struct PiiPattern {
    /// 패턴 이름 (로깅용)
    pub name: &'static str,
    /// 정규식 패턴
    pub pattern: &'static str,
    /// 마스킹 타입
    pub mask_type: MaskType,
    /// 설명
    pub description: &'static str,
    /// 값만 추출할 캡처 그룹 인덱스 (None이면 전체 매치)
    pub value_group: Option<usize>,
}

/// 기본 PII 패턴 목록
pub const PII_PATTERNS: &[PiiPattern] = &[
    // === 비밀번호 ===
    PiiPattern {
        name: "password_assignment",
        pattern: r"(?i)(password|passwd|pwd|pass)\s*[=:]\s*(\S+)",
        mask_type: MaskType::Secret,
        description: "Password assignment (password=xxx)",
        value_group: Some(2),
    },
    PiiPattern {
        name: "password_flag",
        pattern: r"(?i)-p\s*(\S+)",
        mask_type: MaskType::Secret,
        description: "Password flag (-p xxx)",
        value_group: Some(1),
    },
    // === API 키 ===
    PiiPattern {
        name: "api_key",
        pattern: r"(?i)(api[_-]?key|apikey|secret[_-]?key|access[_-]?key)\s*[=:]\s*(\S+)",
        mask_type: MaskType::Secret,
        description: "API key assignment",
        value_group: Some(2),
    },
    PiiPattern {
        name: "openai_key",
        pattern: r"sk-[A-Za-z0-9]{32,}",
        mask_type: MaskType::Secret,
        description: "OpenAI API key",
        value_group: None,
    },
    // === Bearer 토큰 ===
    PiiPattern {
        name: "bearer_token",
        pattern: r"(?i)Bearer\s+([A-Za-z0-9\-_\.]+)",
        mask_type: MaskType::Token,
        description: "Bearer token",
        value_group: Some(1),
    },
    PiiPattern {
        name: "jwt_token",
        pattern: r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+",
        mask_type: MaskType::Token,
        description: "JWT token",
        value_group: None,
    },
    // === 인증 토큰 ===
    PiiPattern {
        name: "auth_token",
        pattern: r"(?i)(auth[_-]?token|access[_-]?token|refresh[_-]?token)\s*[=:]\s*(\S+)",
        mask_type: MaskType::Token,
        description: "Authentication token",
        value_group: Some(2),
    },
    // === DB 연결 문자열 ===
    PiiPattern {
        name: "connection_string_password",
        pattern: r"(?i)(Password|Pwd)\s*=\s*([^;]+)",
        mask_type: MaskType::Credential,
        description: "Connection string password",
        value_group: Some(2),
    },
    PiiPattern {
        name: "jdbc_password",
        pattern: r"(?i)jdbc:[^:]+://[^:]+:([^@]+)@",
        mask_type: MaskType::Credential,
        description: "JDBC connection password",
        value_group: Some(1),
    },
    PiiPattern {
        name: "mongodb_password",
        pattern: r"mongodb://[^:]+:([^@]+)@",
        mask_type: MaskType::Credential,
        description: "MongoDB connection password",
        value_group: Some(1),
    },
    // === 개인키/인증서 ===
    PiiPattern {
        name: "private_key",
        pattern: r"-----BEGIN\s+(RSA\s+)?PRIVATE KEY-----[\s\S]*?-----END\s+(RSA\s+)?PRIVATE KEY-----",
        mask_type: MaskType::Certificate,
        description: "Private key (PEM)",
        value_group: None,
    },
    PiiPattern {
        name: "ssh_private_key",
        pattern: r"-----BEGIN OPENSSH PRIVATE KEY-----[\s\S]*?-----END OPENSSH PRIVATE KEY-----",
        mask_type: MaskType::Certificate,
        description: "SSH private key",
        value_group: None,
    },
    // === 클라우드 자격증명 ===
    PiiPattern {
        name: "aws_access_key",
        pattern: r"AKIA[0-9A-Z]{16}",
        mask_type: MaskType::Secret,
        description: "AWS Access Key ID",
        value_group: None,
    },
    PiiPattern {
        name: "aws_secret_key",
        pattern: r"(?i)(aws[_-]?secret[_-]?access[_-]?key)\s*[=:]\s*([A-Za-z0-9/+=]{40})",
        mask_type: MaskType::Secret,
        description: "AWS Secret Access Key",
        value_group: Some(2),
    },
    PiiPattern {
        name: "azure_storage_key",
        pattern: r"[A-Za-z0-9+/]{86}==",
        mask_type: MaskType::Secret,
        description: "Azure Storage Key",
        value_group: None,
    },
    // === 내부망 특화 ===
    PiiPattern {
        name: "internal_token",
        pattern: r"(?i)(token|auth)\s*[=:]\s*([A-Za-z0-9\-_]{20,})",
        mask_type: MaskType::Token,
        description: "Internal token (20+ chars)",
        value_group: Some(2),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn test_patterns_compile() {
        for pattern in PII_PATTERNS {
            let result = Regex::new(pattern.pattern);
            assert!(
                result.is_ok(),
                "Pattern '{}' failed to compile: {:?}",
                pattern.name,
                result.err()
            );
        }
    }

    #[test]
    fn test_password_pattern() {
        let pattern = &PII_PATTERNS[0]; // password_assignment
        let re = Regex::new(pattern.pattern).unwrap();

        assert!(re.is_match("password=MyP@ss123"));
        assert!(re.is_match("PASSWORD = secret"));
        assert!(re.is_match("pwd:abc123"));
        assert!(!re.is_match("passwordless auth"));
    }

    #[test]
    fn test_bearer_token_pattern() {
        let pattern = PII_PATTERNS
            .iter()
            .find(|p| p.name == "bearer_token")
            .unwrap();
        let re = Regex::new(pattern.pattern).unwrap();

        assert!(re.is_match("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(re.is_match("bearer abc123-xyz"));
        assert!(!re.is_match("Bear token"));
    }

    #[test]
    fn test_openai_key_pattern() {
        let pattern = PII_PATTERNS
            .iter()
            .find(|p| p.name == "openai_key")
            .unwrap();
        let re = Regex::new(pattern.pattern).unwrap();

        assert!(re.is_match("sk-1234567890abcdef1234567890abcdef"));
        assert!(!re.is_match("sk-short"));
    }
}
