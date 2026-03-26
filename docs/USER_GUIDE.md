# Goose Custom 사용자 가이드

## 1. 설치 및 실행

### 최초 설정

```bash
# 1. Provider 설정
goose configure
# Provider: azure_openai
# Model: gpt-4o

# 2. 빌드
cargo build --release --package goose-cli

# 3. 실행
./target/release/goose session --tui
```

### 환경변수

```bash
# 필수
AZURE_OPENAI_API_KEY=your-key
AZURE_OPENAI_ENDPOINT=https://your-resource.openai.azure.com/
AZURE_OPENAI_DEPLOYMENT_NAME=your-deployment

# 선택
AZURE_OPENAI_API_VERSION=2024-05-01-preview   # 기본값 2024-10-21
GOOSE_MAX_TOKENS=4096                          # 모델별 최대 토큰
```

---

## 2. 기본 사용법

### 대화

TUI 실행 후 하단 입력창에 질문을 입력하고 Enter.

```
이 프로젝트 구조 알려줘
src/ 폴더에서 TODO 찾아줘
이 파일 읽어줘: C:\path\to\file.xlsx
```

### 멀티라인 입력

Shift+Enter로 줄바꿈. 또는 여러 줄 텍스트를 Ctrl+V로 붙여넣기 (자동 감지).

### 입력 히스토리

↑/↓ 키로 이전 입력 탐색 (마우스 캡처 ON 상태에서).

---

## 3. 키바인딩

### 기능키

| 키 | 기능 | 설명 |
|----|------|------|
| F1 | 도움말 | 키바인딩 안내 팝업 |
| F2 | 마우스 캡처 | ON: 휠 스크롤 + 드래그 선택 / OFF: 터미널 모드 |
| F3 | 도구 출력 | 도구 실행 로그 패널 (독립 스크롤) |
| F4 | 테마 | Dark ↔ Light 전환 |
| F5 | Hints | .goosehints 편집 패널 |
| F6 | 감사 로그 | 세션 통계, 타임라인, PII 이력, 보안 이벤트 |
| F7 | 설정 | Mode, PII, Max Tokens 등 런타임 설정 |

### Vim 모드

| 키 | Insert 모드 | Normal 모드 |
|----|------------|------------|
| Esc | → Normal | - |
| i, a | - | → Insert |
| j/k | - | 스크롤 |
| g/G | - | 맨 위/아래 |
| q | - | 종료 |
| Ctrl+C | 종료 | 종료 |

### 스크롤

| 키 | 동작 |
|----|------|
| Ctrl+↑/↓ | 1줄 스크롤 |
| PageUp/Down | 10줄 스크롤 |
| 마우스 휠 | 3줄 스크롤 (F2 ON 시) |
| Tab | 패널 포커스 전환 |

---

## 4. 텍스트 선택 & 복사

마우스 캡처 ON (기본) 상태에서:

1. 대화 영역에서 **마우스 클릭 + 드래그**
2. 선택 영역이 **파란색 하이라이트**로 표시
3. 마우스 놓으면 **클립보드에 자동 복사**
4. Ctrl+V로 다른 곳에 붙여넣기

---

## 5. 슬래시 명령어

입력창에 `/` 로 시작하는 명령어 입력:

| 명령어 | 기능 |
|--------|------|
| `/help` | 도움말 표시 |
| `/clear` | 대화 기록 삭제 |
| `/quit` | 세션 종료 |
| `/theme` | 테마 전환 |
| `/config` | 설정 패널 열기 (= F7) |
| `/hints show` | 로드된 hints 목록 |
| `/hints reload` | hints 다시 로드 |
| `/hints add project` | 프로젝트 hints 생성 |
| `/hints edit project` | 프로젝트 hints 편집 |
| `/hints panel` | F5 편집 패널 열기 |
| `/audit status` | 감사 로그 상태 |
| `/audit path` | 로그 파일 경로 |

---

## 6. 설정 패널 (F7)

F7 또는 `/config`으로 열기. Ctrl+S로 저장 (config.yaml + 런타임 즉시 적용).

### General 탭

| 필드 | 동작 |
|------|------|
| Provider | 표시 (변경: `goose configure`) |
| Model | 표시 (변경: `goose configure`) |
| Mode | **Space로 순환**: Auto → Approve → SmartApprove → Chat |

**Mode 설명:**
- **Auto**: 도구 자동 실행
- **Approve**: 매 실행마다 승인 필요
- **SmartApprove**: 위험 작업만 승인
- **Chat**: 대화만 (도구 미사용)

### PII 마스킹 탭

| 필드 | 동작 |
|------|------|
| PII 마스킹 | on/off 토글 |
| Secret 카테고리 | on/off (비밀번호, API 키) |
| Token 카테고리 | on/off (Bearer, JWT) |
| Credential 카테고리 | on/off (DB 연결문자열) |
| Certificate 카테고리 | on/off (인증서, 개인키) |
| 화이트리스트 | A: 추가, D: 삭제, ←→: 항목 선택 |

**화이트리스트**: 테스트용 값 등 마스킹에서 제외할 문자열 등록.

### 고급 탭

| 필드 | 동작 |
|------|------|
| Max Tokens | +/-: 1000 단위 조정 |
| Max Turns | +/-: 10 단위 조정 |
| API Version | 표시 (Azure 전용) |
| 감사 로깅 | on/off 토글 |

---

## 7. PII 마스킹

민감 정보가 LLM에 전송되지 않도록 자동 마스킹합니다.

### 동작 흐름

```
사용자 입력: "password=MyP@ss123 연결 안 돼"
     ↓ 마스킹
LLM 전송:   "password=[SECRET_1] 연결 안 돼"
     ↓ LLM 응답
LLM 응답:   "[SECRET_1] 형식이 잘못된 것 같습니다"
     ↓ 복원
사용자 표시: "MyP@ss123 형식이 잘못된 것 같습니다"
```

### 탐지 대상

| 토큰 | 탐지 대상 | 예시 |
|------|----------|------|
| `[SECRET_N]` | 비밀번호, API 키 | `password=xxx`, `API_KEY=xxx` |
| `[TOKEN_N]` | Bearer, JWT | `Bearer eyJhb...` |
| `[CRED_N]` | DB 연결문자열 | `Server=...;Password=xxx` |
| `[CERT_N]` | 인증서, 개인키 | `-----BEGIN PRIVATE KEY-----` |

### 상태 확인

- 상태바: `🔒PII:N` (세션 내 마스킹 건수)
- 시스템 메시지: "🔒 민감정보 N개가 마스킹되었습니다"

---

## 8. .goosehints

AI에게 프로젝트 컨텍스트를 전달하는 지침 파일입니다.

### 3계층 구조

```
~/.config/goose/.goosehints       # Global  (모든 프로젝트)
{project}/.goosehints             # Project (git 추적)
{project}/.goosehints.local       # Local   (gitignore, 개인용)
```

### 작성 예시

```markdown
# Project Context

이 프로젝트는 Rust 기반 CLI 도구입니다.

## Instructions

- 한국어로 응답하세요
- 코드 블록에 언어 태그를 붙이세요
- 파일 수정 전 반드시 읽기부터 하세요
```

### 관리

- F5: TUI 편집 패널
- `/hints show`: 로드된 파일 확인
- `/hints reload`: 변경사항 다시 로드
- `/hints add local`: 로컬 hints 생성

---

## 9. 감사 로그 (F6)

### 뷰어 탭

| 탭 | 내용 |
|----|------|
| 요약 | 세션 수, 토큰, 도구 호출, PII, 보안 이벤트 |
| 타임라인 | 이벤트 시간순 목록 |
| 토큰 | 일별 토큰 사용량 (입력/출력) |
| PII | PII 마스킹 이력 |
| 보안 | 보안 이벤트 (탐지/차단) |

### 키바인딩

| 키 | 동작 |
|----|------|
| Tab | 탭 전환 |
| ↑↓ | 스크롤 |
| +/- | 일수 필터 조정 |
| R | HTML 리포트 생성 (브라우저 열림) |
| Ctrl+R | 새로고침 |
| Esc | 닫기 |

### 로그 경로

```
~/.local/state/goose/logs/audit/
├── audit.2026-03-26.jsonl
├── audit.2026-03-25.jsonl
└── reports/
    └── audit-report-20260326-143000.html
```

---

## 10. DevOps 지원

kubectl, az, helm, docker 명령어를 한국어로 요청할 수 있습니다.

```
"aiworker-app 네임스페이스 파드 조회해줘"
→ kubectl get pods -n aiworker-app

"db-agent 로그 보여줘"
→ kubectl logs -f deployment/workflow-db-agent -n aiworker-app

"이미지 목록 확인"
→ az acr repository list --name agenticaidevacr45141
```

---

## 11. 파일 읽기 지원

Read 도구가 확장자에 따라 자동으로 적절한 방식으로 읽습니다.

| 확장자 | 처리 |
|--------|------|
| `.txt`, `.rs`, `.py` 등 | 텍스트 (인코딩 자동 감지) |
| `.pdf` | 텍스트 추출 (최대 20페이지) |
| `.docx` | 텍스트 + 구조 추출 |
| `.xlsx`, `.xls` | 시트별 마크다운 테이블 (최대 100행) |
| `.png`, `.jpg` 등 | 메타데이터 + base64 (비전 분석) |

---

## 12. 보안

### 프롬프트 인젝션 탐지

LLM 조작 시도를 패턴으로 탐지합니다 (영어/한국어 14개 패턴):
- 지시 무시 시도 ("Ignore previous instructions")
- DAN/탈옥 시도
- 시스템 프롬프트 추출 시도
- 간접 인젝션 ([SYSTEM] 마커)

### 명령어 위협 탐지

위험한 쉘 명령을 30+개 패턴으로 탐지:
- 파일 시스템 파괴 (rm -rf, dd, format)
- 원격 코드 실행 (curl|bash, PowerShell download)
- 데이터 유출 (SSH 키, 비밀번호 파일)
- 권한 상승 (sudo NOPASSWD, SUID)
- 리버스 쉘, 네트워크 스캐닝

---

## 13. 문제 해결

### 자주 묻는 질문

**Q: PII 마스킹을 끄고 싶어요**
A: F7 → PII 탭 → PII 마스킹 OFF → Ctrl+S

**Q: 특정 값이 마스킹되지 않게 하고 싶어요**
A: F7 → PII 탭 → 화이트리스트에 A로 추가 → Ctrl+S

**Q: 모델을 바꾸고 싶어요**
A: `goose configure`로 변경 후 재시작

**Q: F7 설정이 저장 안 돼요**
A: Ctrl+S를 반드시 눌러야 config.yaml에 저장됩니다

**Q: 텍스트 복사가 안 돼요**
A: 마우스 캡처 ON (기본) 상태에서 드래그하세요. 놓으면 자동 복사됩니다.

### 로그 확인

```bash
# 앱 로그
tail -f ~/.local/state/goose/logs/goose.log

# 감사 로그
cat ~/.local/state/goose/logs/audit/audit.$(date +%Y-%m-%d).jsonl | jq .
```
