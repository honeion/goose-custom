<div align="center">

# Goose Custom

_내부망 전용 CLI AI Agent_

Forked from [block/goose](https://github.com/block/goose) (Apache License 2.0, Copyright 2024 Block, Inc.)

</div>

---

## 소개

Goose Custom은 Block의 오픈소스 AI Agent인 [goose](https://github.com/block/goose)를 내부망 환경에 맞게 커스터마이징한 포크입니다.

Azure OpenAI(GPT-4o) 전용으로 최적화되었으며, Ratatui 기반 TUI, PII 마스킹, 프롬프트 인젝션 탐지, 감사 로그, DevOps 도구 지원 등을 추가했습니다.

> **사용자 가이드**: [docs/USER_GUIDE.md](docs/USER_GUIDE.md)

---

## 원본 대비 변경사항

### 인프라

| 항목 | 원본 Goose | Goose Custom |
|------|-----------|-------------|
| LLM | 12+ Provider | **Azure OpenAI 전용** |
| 로컬 추론 | llama-cpp-2 | **제거** |
| 플랫폼 | macOS 중심 | **Windows + Linux** |
| 한글 | 미지원 | **EUC-KR/CP949 자동 감지** |

### UI

| 항목 | 원본 | 커스텀 |
|------|------|--------|
| 인터페이스 | console/bat 텍스트 | **Ratatui TUI** (TEA 패턴) |
| 테마 | bat 테마 | **Catppuccin Dark/Light** (F4) |
| 입력 | rustyline | **tui-textarea + Vim 키바인딩** |
| 마크다운 | bat 렌더링 | **자체 파서** (h1~h4, bold, italic, code) |
| 코드 강조 | syntect | **tree-sitter** (Rust/Python/JS/Go/Bash/SQL) |
| 스크롤 | 터미널 네이티브 | **OffscreenBuffer 독립 스크롤** |
| 텍스트 복사 | 터미널 드래그 | **마우스 드래그 → 클립보드 자동 복사** |

### 패널 시스템 (전부 신규)

| 키 | 패널 | 기능 |
|----|------|------|
| F1 | 도움말 | 키바인딩 안내 |
| F2 | 마우스 캡처 | 휠 스크롤 + 드래그 선택 토글 |
| F3 | 도구 출력 | 도구 실행 로그 (독립 스크롤) |
| F4 | 테마 | Dark / Light 전환 |
| F5 | Hints 편집 | .goosehints Global/Project/Local 편집 |
| F6 | 감사 로그 | 요약/타임라인/토큰/PII/보안 + HTML 리포트 |
| F7 | 설정 | Mode 토글, PII 화이트리스트, Max Tokens/Turns |

### 도구 시스템

| 항목 | 원본 | 커스텀 |
|------|------|--------|
| 파일 도구 | text_editor (통합) | **Read / Edit / Write 분리** |
| 파일 검색 | shell find/grep | **Glob / Grep 전용 도구** |
| PDF | 미지원 | **텍스트 추출** (lopdf) |
| DOCX | computercontroller만 | **Read 도구에서 직접 읽기** (docx-rs) |
| XLSX | computercontroller만 | **Read 도구에서 직접 읽기** (umya-spreadsheet) |
| 인코딩 | UTF-8만 | **자동 감지** (EUC-KR, CP949, Shift_JIS) |
| Edit 미리보기 | 없음 | **Diff Preview** |

### 서브에이전트

| 항목 | 원본 | 커스텀 |
|------|------|--------|
| 도구 제한 | MD로 권고만 | **allowed_tools 강제** |
| 오버라이드 | 사용자 우선 | **빌트인 우선** |

| 스킬 | 허용 도구 | 용도 |
|------|----------|------|
| explore | Glob, Grep, Read | 읽기 전용 탐색 |
| research | Glob, Grep, Read, WebFetch | 심층 분석 |
| coder | Read, Edit, Write, Undo | 코드 수정 |
| bash | Bash | 명령 실행 전용 |
| general | 전체 | 복합 작업 |

### 보안

| 항목 | 원본 | 커스텀 |
|------|------|--------|
| PII 마스킹 | 없음 | **15개 패턴**, 화이트리스트, 카테고리 on/off |
| 프롬프트 인젝션 | ML 엔드포인트 의존 | **14개 패턴 (EN/KO)**, ML 없이 동작 |
| 명령어 위협 | 기본 패턴 | **30+개 패턴** |
| 감사 로그 | llm_request.jsonl | **JSONL + F6 뷰어 + HTML 리포트** |

### DevOps / 기타

| 항목 | 원본 | 커스텀 |
|------|------|--------|
| 인프라 명령 | 미지원 | **kubectl, az, helm, docker** |
| 한국어 의도 | 미지원 | **"네임스페이스 조회" → kubectl** |
| .goosehints | 단일 파일 | **Global/Project/Local 3계층 + F5 편집** |
| 브라우저 | 외부 MCP | **내장 Extension** (chromiumoxide CDP) |
| 스크린샷 | macOS Peekaboo | **xcap** (Windows/Linux/macOS) |

---

## 빠른 시작

### 1. 설정

```bash
goose configure
# Provider: azure_openai
# Model: gpt-4o
```

### 2. 환경변수

```bash
export AZURE_OPENAI_API_KEY="your-key"
export AZURE_OPENAI_ENDPOINT="https://your-resource.openai.azure.com/"
export AZURE_OPENAI_DEPLOYMENT_NAME="your-deployment"
```

### 3. 빌드 & 실행

```bash
cargo build --release --package goose-cli
./target/release/goose session --tui
```

---

## 프로젝트 구조

```
crates/
├── goose/                          # 코어 라이브러리
│   ├── src/agents/                 # Agent, 서브에이전트, 프롬프트 관리
│   │   └── builtin_skills/         # explore, research, coder, bash, general
│   ├── src/security/               # 보안 모듈
│   │   ├── pii_masker.rs           # PII 마스킹/복원 + 화이트리스트
│   │   ├── pii_patterns.rs         # PII 탐지 패턴 15개
│   │   ├── patterns.rs             # 명령어 위협 30+ / 프롬프트 인젝션 14개
│   │   └── scanner.rs             # 통합 스캐너 (패턴 + ML fallback)
│   ├── src/audit/                  # 감사 로그 (JSONL, 분석, 리포트)
│   ├── src/hints/                  # .goosehints 3계층 로드
│   └── src/config/                 # Config, GooseMode, 권한 관리
│
├── goose-cli/                      # CLI + TUI
│   └── src/session/
│       ├── tui/                    # Ratatui TUI 모듈
│       │   ├── app.rs              # 상태 (Model)
│       │   ├── events.rs           # 이벤트 (Update) + 키/마우스 처리
│       │   ├── render.rs           # 렌더링 (View) + 마크다운 + 선택 하이라이트
│       │   ├── theme.rs            # Catppuccin Dark/Light
│       │   ├── markdown.rs         # 마크다운 파서 + 구문 강조
│       │   ├── hints_panel.rs      # F5 Hints 편집
│       │   ├── audit_panel.rs      # F6 감사 로그 뷰어
│       │   ├── config_panel.rs     # F7 설정 패널
│       │   ├── offscreen_buffer.rs # 독립 스크롤백
│       │   └── animation.rs        # 스피너
│       └── tui_session.rs          # Agent ↔ TUI 통합, 스트리밍
│
├── goose-mcp/                      # MCP 도구
│   ├── src/developer/              # Developer Extension
│   │   ├── read.rs                 # Read (PDF/DOCX/XLSX/이미지/인코딩 감지)
│   │   ├── edit.rs                 # Edit (Diff 미리보기)
│   │   ├── write.rs                # Write
│   │   └── analyze/                # 코드 분석 (tree-sitter)
│   ├── src/browser/                # 브라우저 Extension (chromiumoxide)
│   └── src/computercontroller/     # OS 자동화 (xcap, docx, xlsx)
│
├── goose-server/                   # Desktop 백엔드
└── goose-bench/                    # 벤치마크
```

---

## 슬래시 명령어

| 명령어 | 기능 |
|--------|------|
| `/help` | 도움말 |
| `/clear` | 대화 삭제 |
| `/quit` | 종료 |
| `/theme` | 테마 전환 |
| `/hints` | Hints 관리 (show/reload/add/edit/path/panel) |
| `/audit` | 감사 로그 (status/path/help) |
| `/config` | 설정 패널 (F7) |

---

## 문서

| 문서 | 설명 |
|------|------|
| **[docs/USER_GUIDE.md](docs/USER_GUIDE.md)** | **사용자 가이드** |
| docs/000-overview.md | 프로젝트 개요 + Phase 진행 상태 |
| docs/001-cleanup-plan.md | 소스 정리 (삭제/유지 대상) |
| docs/002-improvement-plan.md | 개선 계획 (기능별 상세) |
| docs/003-security-guidelines.md | 보안 가이드라인 |
| docs/004-pii-masking-design.md | PII 마스킹 설계 |
| docs/005-tool-separation-plan.md | 도구 분리 (Read/Edit/Write) |
| docs/007-ratatui-ui-design.md | TUI 아키텍처 설계 |
| docs/008-ui-visual-design.md | TUI 비주얼 스펙 |
| docs/009-internal-mcp-guide.md | 내부망 MCP 개발 가이드 |
| docs/010-goosehints-design.md | .goosehints 설계 |
| docs/011-audit-log-design.md | 감사 로그 설계 |

---

## 라이선스

Apache License 2.0 — 원본 저작권: Copyright 2024 Block, Inc.
