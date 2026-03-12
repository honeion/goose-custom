---
title: Goose Custom - 프로젝트 개요
status: active
created: 2026-02-26 14:30:00
updated: 2026-03-12
author: claude
---

# Goose Custom - 프로젝트 개요

## 1. 프로젝트 목표

Goose 오픈소스를 포크하여 **내부망 전용 CLI Agent**로 커스터마이징

### 1.1 핵심 목표

| 목표 | 설명 |
|------|------|
| 내부망 최적화 | 외부 서비스 제거, 내부 인프라 연동 |
| Azure OpenAI 전용 | GPT-4o 기반, 내부 엔드포인트 |
| Windows 지원 | PowerShell 환경, 한글 인코딩 |
| 개선된 UX | 세션 관리, 구조화된 도구, 브라우저 자동화 |

### 1.2 원본 정보

- **원본**: https://github.com/block/goose
- **라이선스**: Apache License 2.0
- **저작권**: Copyright 2024 Block, Inc.

---

## 2. 환경 제약사항

| 제약 | 설명 |
|------|------|
| 네트워크 | 내부망만 접근 가능, 외부 API 호출 불가 |
| LLM | Azure OpenAI (GPT-4o) 만 사용 가능 |
| 플랫폼 | Windows 주력, Linux 서버 |
| 인증 | 내부 인증 시스템 (SSO, NTLM) |
| 배포 | 내부 Git 서버, 내부 패키지 저장소 |
| DevOps | Azure DevOps (GitHub 아님) |

---

## 3. 개발 Phase

### Phase 1: 소스 정리 + 기본 기능 ✅ 완료

**목표**: 불필요한 코드 제거, Azure OpenAI 안정화

| 항목 | 상태 | 문서 |
|------|------|------|
| llama-cpp-2 제거 | ✅ 완료 | - |
| Azure content_filter 수정 | ✅ 완료 | - |
| max_tokens 제한 | ✅ 완료 | - |
| Root 폴더 정리 (991 파일) | ✅ 완료 | 001-cleanup-plan.md 단계 1 |
| Provider 정리 (70+ 파일) | ✅ 완료 | 001-cleanup-plan.md 단계 2 |
| MCP Extension 정리 | ✅ 완료 | 외부 MCP 연동 유지, 009-internal-mcp-guide.md 작성 |
| Warning 정리 | ✅ 완료 | - |

### Phase 2: 핵심 개선 ✅ 완료

**목표**: UX 개선, 내부 도구 통합

| 항목 | 상태 | 문서 |
|------|------|------|
| Shell 환경 감지 + 프롬프트 | ✅ 완료 | 002-improvement-plan.md §2 |
| 세션 관리 UX | ✅ 완료 | 002-improvement-plan.md §3 |
| 한글 인코딩 | ✅ 완료 | 002-improvement-plan.md §4 |
| 출력 가독성 | ✅ 완료 | 002-improvement-plan.md §5 |
| 시스템 프롬프트 관리 | ✅ 완료 | 002-improvement-plan.md §9 |
| 계층적 프롬프트 (.goosehints) | 🔲 예정 | 002-improvement-plan.md §9.12 |

### Phase 3: 도구 시스템 확장 ✅ 완료

**목표**: Claude Code 스타일 도구 시스템 구현

| 항목 | 상태 | 문서 |
|------|------|------|
| Glob/Grep 도구 | ✅ 완료 | 002-improvement-plan.md §10.2 |
| TODO 개선 (구조화) | ✅ 완료 | 002-improvement-plan.md §10.1.1 |
| 서브에이전트 타입 시스템 | 🔶 분석완료 | CUSTOMIZATION_TODO.md §서브에이전트 |
| allowed_tools (도구 필터) | ✅ 완료 | - |
| WebFetch (내부망) | ✅ 완료 | 002-improvement-plan.md §10.2.2 |
| **text_editor 분리** | ✅ 완료 | **005-tool-separation-plan.md** |
| **Edit/Write 미리보기** | ✅ 완료 | 005-tool-separation-plan.md Phase 2 |
| **인코딩 자동 감지** | ✅ 완료 | 005-tool-separation-plan.md Phase 3 |
| **PDF/이미지 읽기** | ✅ 완료 | 005-tool-separation-plan.md Phase 4 |
| **NotebookEdit** | ✅ 완료 | 005-tool-separation-plan.md Phase 4 |
| **AskUserQuestion** | ✅ 완료 | 구조화된 사용자 질문 도구 |
| **서브에이전트 타입 시스템** | ✅ 완료 | builtin_skills 스킬 파일 |

#### 서브에이전트 구현 완료 (2026-03-03)

**구현된 서브에이전트 스킬:**

| 스킬 | allowed_tools | 용도 |
|------|--------------|------|
| explore | Glob, Grep, Read | 코드베이스 탐색 (읽기 전용) |
| research | Glob, Grep, Read, WebFetch | 심층 분석 + 웹 검색 |
| coder | Glob, Grep, Read, Edit, Write, Undo | 코드 수정 |
| bash | Bash | 쉘 명령 실행 |
| general | 전체 | 복합 작업 |

- 위치: `crates/goose/src/agents/builtin_skills/skills/*.md`

### Phase 4: 브라우저 자동화 ✅ 기본 완료

**목표**: 내부망 웹 애플리케이션 자동화

| 항목 | 상태 | 문서 |
|------|------|------|
| browser extension (builtin) | ✅ 완료 | browser_ext.rs |
| chromiumoxide 통합 | ✅ 완료 | CDP 기반 제어 |
| Edge/Chrome 자동 탐색 | ✅ 완료 | browser_launch에서 자동 탐색 |
| 내부 SSO 인증 | 🔲 예정 | 002-improvement-plan.md §10.4.6 |

#### 브라우저 도구 (2026-03-04)

| 도구 | 설명 |
|------|------|
| browser_launch | Chrome/Edge 브라우저 실행 |
| browser_navigate | URL 이동 (탭 재사용) |
| browser_click | CSS selector로 클릭 |
| browser_input | 텍스트 입력 |
| browser_screenshot | 스크린샷 캡처 |
| browser_read_page | 페이지 HTML 읽기 |
| browser_find | selector로 요소 찾기 |
| browser_close | 브라우저 종료 |

### Phase 5: UI 고도화 ✅ 완료

**목표**: Ratatui 기반 화려한 TUI

| 항목 | 상태 | 문서 |
|------|------|------|
| **아키텍처 설계** | ✅ 완료 | **007-ratatui-ui-design.md** |
| **비주얼 디자인** | ✅ 완료 | **008-ui-visual-design.md** |
| TEA 패턴 구현 | ✅ 완료 | app.rs, events.rs, render.rs |
| Vim 키바인딩 | ✅ 완료 | Normal/Insert/Command 모드 |
| OffscreenBuffer | ✅ 완료 | 독립 스크롤 영역 |
| 마크다운 렌더링 | ✅ 완료 | bold, italic, code, headers |
| 코드 구문 강조 | ✅ 완료 | Rust, Python, JS, Go, Bash, SQL |
| Diff Preview | ✅ 완료 | +/- 색상 하이라이팅 |
| 테마 전환 (F4) | ✅ 완료 | Dark/Light |
| **도구 승인 자동화** | ✅ 완료 | TUI 모드에서 도구 호출 자동 승인 |
| **붙여넣기 감지** | ✅ 완료 | 빠른 입력 감지로 여러줄 붙여넣기 지원 |
| **스크롤/히스토리 분리** | ✅ 완료 | F2 토글로 ↑↓ 동작 전환 |
| 탭 바 (다중 세션) | ⏭️ 보류 | 별도 창 실행으로 대체 가능, 우선순위 낮음 |
| TachyonFX 애니메이션 | ❌ 비활성화 | 깜빡거림 이슈로 사용 불가 |

#### 구현된 키바인딩 (2026-03-06)

| 키 | 기능 |
|----|------|
| F1 | 도움말 토글 |
| F2 | 마우스 캡처 토글 |
| F3 | 도구 출력 패널 토글 |
| F4 | 테마 전환 (Dark/Light) |
| Tab | 패널 포커스 전환 |
| Esc | Normal 모드 |
| i/a | Insert 모드 |

#### 구현 파일 (2026-03-06)

```
crates/goose-cli/src/session/tui/
├── app.rs              # TuiApp 상태
├── events.rs           # 이벤트 핸들링
├── render.rs           # 렌더링 로직
├── theme.rs            # Catppuccin 테마
├── markdown.rs         # 마크다운 + 구문강조 + Diff
├── animation.rs        # SpinnerFrames
├── offscreen_buffer.rs # 독립 스크롤백
└── tui_session.rs      # 에이전트 통합
```

### Phase 6: 보안 강화 ✅ 완료

**목표**: 내부망 환경 보안 완성

| 항목 | 상태 | 문서 |
|------|------|------|
| 보안 가이드라인 | ✅ 완료 | 003-security-guidelines.md |
| PII 마스킹 설계 | ✅ 완료 | 004-pii-masking-design.md |
| PII 마스킹 UI 설계 | ✅ 완료 | 004-pii-masking-design.md §3 |
| **PII 패턴 정의** | ✅ 완료 | security/pii_patterns.rs (15개 패턴) |
| **PII 마스킹 로직** | ✅ 완료 | security/pii_masker.rs |
| **단위 테스트** | ✅ 완료 | 16개 테스트 통과 |
| **Agent 통합** | ✅ 완료 | agents/agent.rs (메서드 추가) |
| **메시지 흐름 통합** | ✅ 완료 | reply() 함수 연동 |
| **기본값 활성화** | ✅ 완료 | PII 마스킹 기본 ON |
| 감사 로그 분석 | 🔲 예정 | - |

### Phase 7: DevOps 지원 ✅ 완료

**목표**: 인프라 관리 명령어 지원

| 항목 | 상태 | 설명 |
|------|------|------|
| **kubectl 지원** | ✅ 완료 | Kubernetes 클러스터 관리 |
| **az CLI 지원** | ✅ 완료 | Azure 리소스 관리 |
| **helm 지원** | ✅ 완료 | Kubernetes 패키지 관리 |
| **docker 지원** | ✅ 완료 | 컨테이너 관리 |
| **한국어 의도 파악** | ✅ 완료 | "네임스페이스", "파드" → kubectl |
| **Fuzzy Matching** | ✅ 완료 | 유사 이름 검색 + 확인 요청 |

#### DevOps 명령어 예시

```
사용자: "aiworker-app 네임스페이스 조회해줘"
goose: kubectl get all -n aiworker-app 실행

사용자: "db-agent 상세 정보"
goose: grep으로 workflow-db-agent 찾고 describe 실행
```

#### PII 마스킹 핵심

```
입력 → 마스킹 → LLM 전송 → 응답 → 복원 → 출력
         ↓
    🔒 PII: 2개 마스킹됨 (password, API Key)
       • password → [SECRET_1] (MyP@****23)
       • API Key  → [SECRET_2] (sk-ab****yz)
```

- **네트워크**: 원본 미전송, 토큰만 전송
- **로그**: 마스킹된 상태로 기록
- **UI**: 마스킹 항목 명확히 표시

---

## 4. 문서 구조

```
docs/
├── 000-overview.md             # 이 문서: 프로젝트 개요 + Phase
├── 001-cleanup-plan.md         # 소스 정리 계획 (삭제/유지 대상)
├── 002-improvement-plan.md     # 개선 계획 (새 기능, UX 개선)
├── 003-security-guidelines.md  # 보안 가이드라인 + 데이터 안전성
├── 004-pii-masking-design.md   # PII 마스킹 설계 (TODO)
├── 005-tool-separation-plan.md # 도구 분리 계획 (text_editor → Read/Edit/Write)
├── 006-vision-analysis-guide.md # 비전 분석 가이드 (이미지/PDF)
├── 007-ratatui-ui-design.md    # Phase 5: Ratatui 아키텍처 설계
├── 008-ui-visual-design.md     # Phase 5: 비주얼 디자인 스펙
├── 009-internal-mcp-guide.md   # 내부망 MCP 서버 개발 가이드
└── CUSTOMIZATION_TODO.md       # 완료된 작업 + 빠른 참조
```

---

## 5. 우선순위 요약

### 즉시 (Phase 1-2)

1. **Shell 환경 감지** - PowerShell 환경에서 올바른 명령어 사용
2. **한글 인코딩** - PowerShell 출력 깨짐 해결
3. **세션 관리** - 프로젝트별 그룹핑, 재개 UX
4. **시스템 프롬프트** - 응답 품질 향상

### 중기 (Phase 3)

5. **Glob/Grep 도구** ✅ - 플랫폼 독립적 파일 검색
6. **TODO 개선** ✅ - 구조화된 태스크 관리
7. **서브에이전트 타입** - 효율적인 작업 위임
8. **WebFetch** ✅ - 내부망 URL 접근
9. **도구 분리 (text_editor → Read/Edit/Write)** - allowed_tools 세밀한 제어

### 장기 (Phase 4-5)

10. **브라우저 자동화** - ServiceNow, Confluence 등 자동화
11. **Ratatui UI** - 화려한 TUI (선택)

---

## 6. 비활성화 대상

```yaml
# config.yaml
extensions:
  apps:
    enabled: false        # Desktop 전용, CLI 불필요
  code_execution:
    enabled: false        # 보안 위험, 실험적
```

---

## 7. 의존성

### 필수

| 의존성 | 용도 |
|--------|------|
| Azure OpenAI | LLM 백엔드 |
| Edge/Chrome | 브라우저 자동화 (Phase 4) |

### 선택

| 의존성 | 용도 |
|--------|------|
| 내부 검색 엔진 | WebSearch 연동 |
| 내부 Git | 레포지토리 검색 |

---

## 8. 빠른 시작

### 환경 설정

```powershell
# 필수 환경변수
$env:GOOSE_MAX_TOKENS = "4096"
$env:AZURE_OPENAI_API_KEY = "your-key"
$env:AZURE_OPENAI_ENDPOINT = "https://your-resource.openai.azure.com/"
$env:AZURE_OPENAI_DEPLOYMENT_NAME = "your-deployment"
```

### 빌드

```powershell
# llama-cpp 없이 빌드 (default features 제거됨)
cargo build --release
```

### 실행

```powershell
./target/release/goose.exe
```

---

## 9. 참고 문서

| 문서 | 설명 |
|------|------|
| `ai_agent_tools_implementation.md` | Claude Code 스타일 도구 시스템 |
| `claude_code_subagents.md` | 서브에이전트 타입 설계 |
| `code_search_tool_implementation.md` | Glob/Grep 구현 가이드 |
| `007-ratatui-ui-design.md` | Ratatui TUI 설계 (Phase 5) |
| `009-internal-mcp-guide.md` | 내부망 MCP 서버 개발 가이드 |

### 외부 참조

| 프로젝트 | 설명 |
|----------|------|
| [Tenere](https://github.com/pythops/tenere) | LLM TUI 참조 (Vim 키바인딩) |
| [Oatmeal](https://github.com/dustinblackman/oatmeal) | LLM 채팅 참조 (채널 기반) |
| [Ratatui](https://ratatui.rs/) | TUI 프레임워크 공식 문서 |
