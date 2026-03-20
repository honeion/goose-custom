---
title: PII 마스킹 설계
status: implemented
created: 2026-02-27
updated: 2026-03-09
author: claude
priority: high
---

# PII 마스킹 설계

## 1. 개요

### 1.1 목적

LLM에 전송되는 프롬프트에서 민감 정보(PII, Secrets)를 마스킹하여:
- 네트워크 전송 시 원본 노출 방지
- 로그 파일에 민감 정보 미기록
- Azure OpenAI 보안과 함께 이중 방어 구현

### 1.2 동작 흐름

```
┌─────────────────────────────────────────────────────────────┐
│ 사용자 입력                                                  │
│ "password=MyP@ss123 연결 안 돼"                              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ PII 마스킹 (security/pii_masker.rs)                         │
│                                                             │
│ 입력: "password=MyP@ss123 연결 안 돼"                        │
│ 출력: "password=[SECRET_1] 연결 안 돼"                       │
│                                                             │
│ 매핑 테이블 (메모리):                                        │
│   [SECRET_1] → "MyP@ss123"                                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ LLM 전송 (Azure OpenAI)                                     │
│                                                             │
│ 전송 내용: "password=[SECRET_1] 연결 안 돼"                  │
│ → 원본 시크릿 절대 전송 안 됨                                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 로그 기록 (llm_request.*.jsonl)                             │
│                                                             │
│ 기록 내용: "password=[SECRET_1] 연결 안 돼"                  │
│ → 로그 파일에도 마스킹된 상태                                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ LLM 응답                                                    │
│                                                             │
│ "[SECRET_1] 형식이 잘못된 것 같습니다"                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ PII 복원 (security/pii_masker.rs)                           │
│                                                             │
│ 입력: "[SECRET_1] 형식이 잘못된 것 같습니다"                  │
│ 출력: "MyP@ss123 형식이 잘못된 것 같습니다"                   │
│                                                             │
│ → 사용자에게는 복원된 내용 표시                               │
│ → 복원은 메모리에서만, 로그에 미기록                          │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 상세 데이터 흐름

### 2.1 각 단계별 데이터 상태

```
┌─────────────────────────────────────────────────────────────┐
│ 1. 사용자 입력 (터미널)                                      │
│    "password=MyP@ss123으로 DB 연결해봐"                      │
│    → 원본 값 보임 (사용자가 입력했으니까)                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. 마스킹 (로컬 메모리)                                      │
│    "password=[SECRET_1]으로 DB 연결해봐"                     │
│    매핑 테이블: [SECRET_1] = MyP@ss123                       │
│    → 매핑은 메모리에만 존재                                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. LLM 전송 (네트워크)                                       │
│    전송 내용: "password=[SECRET_1]으로 DB 연결해봐"          │
│    → LLM은 [SECRET_1]만 봄, 실제 값 모름                     │
│    → 로그에도 마스킹된 상태로 기록                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. LLM 응답                                                  │
│    "mysql -u admin -p[SECRET_1] 실행해볼게요"                │
│    → LLM은 토큰을 그대로 사용 (값 모름)                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. 도구 실행 전 복원 (로컬)                                  │
│    실제 실행: "mysql -u admin -pMyP@ss123"                   │
│    → 실행은 로컬 PC에서, 네트워크 안 탐                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 6. 터미널 출력                                               │
│    → 사용자에게는 복원된 원본 값 표시                         │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 각 위치별 데이터 노출

| 위치 | 원본 값 | 설명 |
|------|---------|------|
| 터미널 (입력) | ✅ 보임 | 사용자가 입력 |
| 로컬 메모리 | ✅ 존재 | 매핑 테이블 |
| **네트워크 전송** | ❌ 안 감 | `[SECRET_1]`만 전송 |
| **LLM 서버** | ❌ 모름 | 토큰만 처리 |
| **로그 파일** | ❌ 안 남음 | 마스킹 상태 기록 |
| 도구 실행 | ✅ 복원 | 실행은 로컬 |
| 터미널 (출력) | ✅ 보임 | 사용자에게 표시 |

### 2.3 LLM 실행 권한

```
LLM: "mysql -p[SECRET_1] 실행할게요"
              │
              ▼
        goose가 복원
        (LLM이 직접 실행 ❌)
              │
              ▼
        로컬 PC에서 실행
        (네트워크 안 탐)
```

**핵심**: LLM은 토큰만 알고, 실행은 로컬 goose가 함. 실제 값은 PC 안에서만 존재.

---

## 3. UI 표시

### 3.1 마스킹 발생 시 알림

사용자가 오해하지 않도록 마스킹 상태를 명확히 표시:

```
┌─────────────────────────────────────────────────────────────┐
│ You: password=MyP@ss123으로 DB 연결해봐                      │
│                                                             │
│ 🔒 PII 마스킹됨 (LLM에 원본 전송 안 함)                       │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ 탐지 항목          토큰           원본 (일부 표시)       │ │
│ ├─────────────────────────────────────────────────────────┤ │
│ │ password 값       [SECRET_1]     MyP@****23             │ │
│ │ API Key           [SECRET_2]     sk-abc...xyz           │ │
│ │ Bearer Token      [TOKEN_1]      eyJhb...               │ │
│ └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 3.1.1 탐지 항목 종류

| 탐지 종류 | 토큰 형식 | 예시 |
|----------|----------|------|
| 비밀번호 | `[SECRET_N]` | `password=xxx`, `pwd:xxx` |
| API 키 | `[SECRET_N]` | `API_KEY=xxx`, `api-key:xxx` |
| Bearer 토큰 | `[TOKEN_N]` | `Bearer eyJhb...` |
| 개인키/인증서 | `[CERT_N]` | `-----BEGIN PRIVATE KEY-----` |
| DB 연결문자열 | `[CRED_N]` | `Server=...;Password=xxx` |
| 내부 토큰 | `[TOKEN_N]` | `auth_token=xxx` |

### 3.1.2 원본 표시 옵션

```yaml
# config.yaml
pii_masking:
  ui:
    # 원본 값 일부 표시 방식
    show_partial_original: true    # 일부만 표시 (MyP@****23)
    partial_visible_chars: 4       # 앞뒤 몇 글자 표시
    # 또는 완전 숨김
    show_partial_original: false   # [HIDDEN] 으로 표시
```

### 3.1.3 간략 모드 vs 상세 모드

```
[간략 모드 - 기본]
🔒 PII: 3개 마스킹됨 (password, API Key, Bearer Token)

[상세 모드 - verbose]
🔒 PII 마스킹됨 (LLM에 원본 전송 안 함)
   • password 값    → [SECRET_1] (MyP@****23)
   • API Key        → [SECRET_2] (sk-ab****yz)
   • Bearer Token   → [TOKEN_1]  (eyJh****)
```

### 3.2 전송 상태 표시

```
┌─────────────────────────────────────────────────────────────┐
│ 📤 Sending to LLM...                                        │
│    마스킹: password=[SECRET_1]으로 DB 연결해봐               │
│    (원본 값은 전송되지 않습니다)                              │
└─────────────────────────────────────────────────────────────┘
```

### 3.3 복원 표시

```
┌─────────────────────────────────────────────────────────────┐
│ Goose: mysql 연결 테스트 해볼게요                            │
│                                                             │
│ 🔓 실행 시 원본 값으로 복원됨                                 │
│    • [SECRET_1] → ******* (마스킹 표시)                      │
└─────────────────────────────────────────────────────────────┘
```

### 3.4 설정 옵션

```yaml
# config.yaml
pii_masking:
  enabled: true
  ui:
    show_mask_notification: true   # 마스킹 알림 표시
    show_masked_preview: true      # 마스킹된 내용 미리보기
    show_restore_notification: true # 복원 알림 표시
    mask_in_terminal: false        # 터미널 출력도 마스킹 (선택)
```

### 3.5 상태바 (Ratatui 적용 시)

```
┌─────────────────────────────────────────────────────────────┐
│ goose session                                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ ... 대화 내용 ...                                           │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ 🔒 PII: 2 masked │ 📤 Tokens: 1.2k │ ⏱️ Session: 5m        │
└─────────────────────────────────────────────────────────────┘
```

---

## 4. 탐지 패턴

### 4.1 시크릿 패턴

| 패턴명 | 정규식 | 예시 |
|--------|--------|------|
| `password_assignment` | `(password\|passwd\|pwd)\s*[=:]\s*\S+` | `password=MyP@ss123` |
| `api_key` | `(api[_-]?key\|apikey\|secret[_-]?key)\s*[=:]\s*\S+` | `API_KEY=sk-xxx` |
| `bearer_token` | `Bearer\s+[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_]+` | `Bearer eyJhbG...` |
| `connection_string` | `(Server\|Data Source)=.*?(Password\|Pwd)=[^;]+` | `Server=...;Password=xxx` |

### 2.2 인증서/키 패턴

| 패턴명 | 정규식 | 예시 |
|--------|--------|------|
| `private_key` | `-----BEGIN\s+(RSA\s+)?PRIVATE KEY-----[\s\S]*?-----END` | PEM 키 |
| `ssh_private` | `-----BEGIN OPENSSH PRIVATE KEY-----[\s\S]*?-----END` | SSH 키 |
| `certificate` | `-----BEGIN CERTIFICATE-----[\s\S]*?-----END` | 인증서 |

### 2.3 클라우드 자격증명

| 패턴명 | 정규식 | 예시 |
|--------|--------|------|
| `azure_key` | `[A-Za-z0-9+/]{43}=` | Azure Storage Key |
| `aws_access_key` | `AKIA[0-9A-Z]{16}` | AWS Access Key |
| `gcp_service_account` | `"private_key":\s*"-----BEGIN` | GCP SA JSON |

### 2.4 내부망 특화

| 패턴명 | 정규식 | 예시 |
|--------|--------|------|
| `internal_token` | `(token\|auth)\s*[=:]\s*[A-Za-z0-9\-_]{20,}` | 내부 토큰 |
| `db_connection` | `(jdbc\|mongodb\|redis)://[^@]+@` | DB 연결 문자열 |

---

## 3. 구현 설계

### 3.1 파일 구조

```
crates/goose/src/security/
├── mod.rs                    # 기존
├── patterns.rs               # 기존 (위협 패턴)
├── scanner.rs                # 기존 (프롬프트 인젝션)
├── pii_masker.rs             # 🆕 PII 마스킹
└── pii_patterns.rs           # 🆕 PII 패턴 정의
```

### 3.2 핵심 구조체

```rust
// pii_patterns.rs

#[derive(Debug, Clone)]
pub struct PiiPattern {
    pub name: &'static str,
    pub pattern: &'static str,
    pub mask_type: MaskType,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MaskType {
    Secret,         // [SECRET_N]
    Credential,     // [CRED_N]
    Certificate,    // [CERT_N]
    Token,          // [TOKEN_N]
}

pub const PII_PATTERNS: &[PiiPattern] = &[
    PiiPattern {
        name: "password_assignment",
        pattern: r"(?i)(password|passwd|pwd)\s*[=:]\s*\S+",
        mask_type: MaskType::Secret,
        description: "Password assignment",
    },
    // ... 추가 패턴
];
```

```rust
// pii_masker.rs

use std::collections::HashMap;
use regex::Regex;

pub struct PiiMasker {
    patterns: Vec<CompiledPattern>,
    mappings: HashMap<String, String>,  // [SECRET_1] -> 원본
    counter: HashMap<MaskType, usize>,   // 타입별 카운터
}

struct CompiledPattern {
    name: String,
    regex: Regex,
    mask_type: MaskType,
}

impl PiiMasker {
    pub fn new() -> Self { ... }

    /// 입력 텍스트에서 PII를 마스킹
    pub fn mask(&mut self, input: &str) -> MaskResult {
        // 1. 패턴 매칭
        // 2. 마스킹 토큰 생성 ([SECRET_1], [SECRET_2], ...)
        // 3. 매핑 테이블 저장
        // 4. 마스킹된 텍스트 반환
    }

    /// 마스킹된 텍스트를 원본으로 복원
    pub fn unmask(&self, output: &str) -> String {
        // 매핑 테이블 참조하여 복원
    }

    /// 세션 종료 시 매핑 테이블 초기화
    pub fn clear(&mut self) {
        self.mappings.clear();
        self.counter.clear();
    }
}

pub struct MaskResult {
    pub masked_text: String,
    pub masked_count: usize,
    pub masked_items: Vec<MaskedItem>,
}

pub struct MaskedItem {
    pub token: String,      // [SECRET_1]
    pub mask_type: MaskType,
    pub pattern_name: String,
    // 원본은 저장 안 함 (보안)
}
```

### 3.3 통합 위치

```rust
// providers/base.rs 또는 agents/agent.rs

impl Provider {
    async fn complete(...) -> Result<...> {
        // 1. 마스킹
        let mut masker = PiiMasker::new();
        let masked_messages = messages.iter()
            .map(|m| masker.mask_message(m))
            .collect();

        // 2. LLM 호출 (마스킹된 상태)
        let response = self.stream(..., &masked_messages, ...).await?;

        // 3. 응답 복원
        let unmasked_response = masker.unmask_message(&response);

        Ok(unmasked_response)
    }
}
```

---

## 4. 설정

### 4.1 config.yaml

```yaml
# PII 마스킹 설정
security:
  pii_masking:
    enabled: true
    log_masked_items: true    # 마스킹 발생 로그 (토큰만, 원본 미포함)
    patterns:
      - password_assignment
      - api_key
      - bearer_token
      - private_key
      # 비활성화할 패턴
      # - internal_token
```

### 4.2 환경변수

```bash
# 전체 활성화/비활성화
GOOSE_PII_MASKING_ENABLED=true

# 디버그 모드 (마스킹 발생 시 경고 출력)
GOOSE_PII_MASKING_DEBUG=true
```

---

## 5. 로깅

### 5.1 마스킹 발생 로그

```
[INFO] PII masked: 2 items (SECRET: 1, TOKEN: 1)
[DEBUG] Masked patterns: password_assignment, bearer_token
```

### 5.2 로그에 남지 않는 것

- 원본 값
- 매핑 테이블 내용

---

## 6. 보안 고려사항

### 6.1 매핑 테이블 보호

- 메모리에만 존재
- 세션 종료 시 자동 삭제
- 디스크에 저장 안 함
- 로그에 출력 안 함

### 6.2 패턴 우회 방지

- 대소문자 무시 (`(?i)`)
- 공백 유연 처리
- 다양한 구분자 지원 (`=`, `:`, `=>`)

### 6.3 오탐 최소화

- 명확한 패턴만 탐지
- 일반 텍스트와 구분 (키워드 + 값 조합)
- 필요시 화이트리스트 지원

---

## 7. 테스트 케이스

### 7.1 기본 마스킹

```rust
#[test]
fn test_password_masking() {
    let mut masker = PiiMasker::new();
    let result = masker.mask("password=MyP@ss123 연결 안 돼");
    assert_eq!(result.masked_text, "password=[SECRET_1] 연결 안 돼");
    assert_eq!(result.masked_count, 1);
}
```

### 7.2 복원

```rust
#[test]
fn test_unmask() {
    let mut masker = PiiMasker::new();
    masker.mask("password=MyP@ss123");
    let restored = masker.unmask("[SECRET_1] 형식이 잘못됐네요");
    assert_eq!(restored, "MyP@ss123 형식이 잘못됐네요");
}
```

### 7.3 다중 마스킹

```rust
#[test]
fn test_multiple_masking() {
    let mut masker = PiiMasker::new();
    let result = masker.mask("user=admin password=secret123 token=abc123def");
    // password와 token 마스킹
    assert!(result.masked_text.contains("[SECRET_1]"));
    assert!(result.masked_text.contains("[TOKEN_1]"));
    assert_eq!(result.masked_count, 2);
}
```

---

## 8. 구현 우선순위

### Phase 1: 기본 구현

- [ ] `pii_patterns.rs` - 패턴 정의
- [ ] `pii_masker.rs` - 마스킹/복원 로직
- [ ] 단위 테스트

### Phase 2: 통합

- [ ] Provider 레이어 통합
- [ ] 설정 파일 지원
- [ ] 로깅 추가

### Phase 3: 고도화

- [ ] 스트리밍 응답 복원
- [ ] 커스텀 패턴 지원
- [ ] 화이트리스트 지원

---

## 9. 참고

### 9.1 유사 솔루션

| 솔루션 | 특징 |
|--------|------|
| Microsoft Presidio | Python, PII 탐지/익명화 |
| LLM Guard | Python, 입출력 스캐너 |
| LiteLLM + Presidio | 프록시 레벨 마스킹 |

### 9.2 관련 문서

- `003-security-guidelines.md` - 보안 가이드라인
- `security/patterns.rs` - 기존 위협 패턴 참조

---

## 10. 구현 현황 (2026-03-09)

### 10.1 완료된 작업

| 항목 | 파일 | 상태 |
|------|------|------|
| 패턴 정의 | `security/pii_patterns.rs` | ✅ 완료 |
| 마스킹 로직 | `security/pii_masker.rs` | ✅ 완료 |
| 단위 테스트 | `pii_masker.rs` (16개) | ✅ 통과 |
| Agent 필드 추가 | `agents/agent.rs` | ✅ 완료 |
| 마스킹 메서드 | `mask_pii()`, `unmask_pii()` | ✅ 완료 |
| 설정 플래그 | `PII_MASKING_ENABLED` | ✅ 완료 |

### 10.2 추가 완료 작업 (2026-03-09)

| 항목 | 파일 | 상태 |
|------|------|------|
| 메시지 흐름 통합 | `agents/agent.rs` | ✅ 완료 |
| 사용자 메시지 마스킹 | `reply()` 함수 | ✅ 완료 |
| LLM 응답 언마스킹 | `reply_internal()` 함수 | ✅ 완료 |
| 도구 결과 언마스킹 | 주요 yield 포인트 | ✅ 완료 |

#### 연결된 코드 위치

```rust
// reply() 함수 - 사용자 메시지 마스킹
let masked_user_message = self.mask_message(&user_message).await;
session_manager.add_message(&session_config.id, &masked_user_message).await?;

// reply_internal() 함수 - LLM 응답 언마스킹
let unmasked_response = self.unmask_message(&filtered_response).await;
yield AgentEvent::Message(unmasked_response);
```

### 10.3 남은 작업

| 항목 | 설명 | 우선순위 | 상태 |
|------|------|---------|------|
| 도구 파라미터 언마스킹 | `call_tool()` 인자에서 unmask | 중간 | 🔲 TODO |
| UI 알림 | 마스킹 발생 시 사용자 알림 | 중간 | ✅ 완료 (상태바 + 시스템 메시지) |
| 설정 UI | 활성화/비활성화 설정 | 낮음 | ✅ 완료 (F7 설정 패널) |
| 화이트리스트 | 특정 패턴 제외 설정 | 낮음 | ✅ 완료 (F7 PII 탭) |
| 카테고리 비활성화 | Secret/Token/Credential/Certificate 개별 on/off | 낮음 | ✅ 완료 (F7 PII 탭) |

### 10.4 활성화 방법

```bash
# 환경변수로 활성화
export PII_MASKING_ENABLED=true
goose session start
```

또는 config에서:
```yaml
# ~/.config/goose/config.yaml
PII_MASKING_ENABLED: true
```

### 10.5 마스킹된 내용 확인 방법

#### 1. 로그 확인
```bash
# 마스킹 발생 시 로그에 기록됨
tail -f ~/.local/share/goose/logs/goose.log | grep "PII"
```

출력 예시:
```
[INFO] PII 마스킹: 2 개 항목 마스킹됨
[DEBUG] 마스킹된 항목 token=[SECRET_1] pattern=password_assignment preview=My****23
```

#### 2. OpenTelemetry 메트릭
```
goose.pii_masked = 2  # 마스킹된 항목 수
```

#### 3. 디버그 모드
```bash
export GOOSE_LOG=debug
export PII_MASKING_ENABLED=true
goose session start
```

### 10.6 코드 구조

```
crates/goose/src/
├── security/
│   ├── mod.rs              # 모듈 등록
│   ├── pii_masker.rs       # ✅ 마스킹/복원 로직
│   └── pii_patterns.rs     # ✅ 패턴 정의 (15개)
└── agents/
    └── agent.rs            # ✅ PiiMasker 필드 + 메서드
```

### 10.7 주요 API

```rust
// Agent 메서드
impl Agent {
    /// 텍스트 마스킹 (LLM 전송 전)
    pub async fn mask_pii(&self, text: &str) -> String;

    /// 텍스트 복원 (LLM 응답 후)
    pub async fn unmask_pii(&self, text: &str) -> String;

    /// Message 마스킹 (reply() 함수에서 사용)
    pub async fn mask_message(&self, message: &Message) -> Message;

    /// Message 복원 (reply_internal() 함수에서 사용)
    pub async fn unmask_message(&self, message: &Message) -> Message;

    /// 마스킹된 항목 수
    pub async fn pii_masked_count(&self) -> usize;

    /// 세션 종료 시 초기화
    pub async fn clear_pii_mappings(&self);
}
```

### 10.8 버그 수정 이력

| 날짜 | 문제 | 해결 |
|------|------|------|
| 2026-03-09 | 무한 루프로 메모리 폭발 | 매치를 먼저 찾고 역순 대체 |
| 2026-03-09 | 토큰이 다시 매칭됨 | 토큰 포함 여부 체크 추가 |

### 10.9 화이트리스트 및 설정 패널 (2026-03-20)

| 항목 | 파일 | 상태 |
|------|------|------|
| PiiMasker 화이트리스트 | `security/pii_masker.rs` | ✅ 완료 |
| PiiMasker 카테고리 비활성화 | `security/pii_masker.rs` | ✅ 완료 |
| Agent 런타임 메서드 | `agents/agent.rs` | ✅ 완료 |
| F7 설정 패널 UI | `tui/config_panel.rs` | ✅ 완료 |
| Config 영속화 | `tui_session.rs` | ✅ 완료 |

#### 주요 API 추가

```rust
impl Agent {
    pub async fn set_pii_enabled(&self, enabled: bool);
    pub async fn set_pii_whitelist(&self, values: Vec<String>);
    pub async fn set_pii_disabled_types(&self, types: HashSet<MaskType>);
    pub async fn get_pii_whitelist(&self) -> Vec<String>;
    pub async fn get_pii_disabled_types(&self) -> Vec<MaskType>;
}
```

#### Config 키

```yaml
PII_MASKING_ENABLED: true
PII_WHITELIST_VALUES:
  - test_password
  - dummy_key
PII_DISABLED_TYPES:
  - Credential
```

#### F7 설정 패널 탭 구성

| 탭 | 필드 | 동작 |
|----|------|------|
| General | Provider/Model | 표시 + 환경변수 힌트 |
| General | Mode | Space로 순환 토글 (Auto/Approve/SmartApprove/Chat) |
| PII | PII on/off + 4개 카테고리 | 토글 |
| PII | 화이트리스트 | A:추가, D:삭제 |
| 고급 | Max Tokens/Turns | +/-로 조정 |
| 고급 | 감사 로깅 | 토글 |
