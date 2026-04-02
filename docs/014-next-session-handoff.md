---
title: 다음 세션 핸드오프 문서
created: 2026-04-02
author: claude
---

# Goose Custom — 다음 세션 핸드오프

## 프로젝트 개요

**Goose Custom**은 [block/goose](https://github.com/block/goose) 오픈소스 AI Agent CLI를 포크하여 내부망 전용으로 커스터마이징한 프로젝트.

- **위치**: `C:\DEV\aiworker\other\opensource\goose-custom`
- **언어**: Rust
- **LLM**: Azure OpenAI GPT-4o
- **TUI**: Ratatui 기반 터미널 UI (TEA 패턴)
- **원격**: `https://github.com/honeion/goose-custom.git` (main 브랜치)

---

## 이번 세션에서 한 작업들 (2026-04-02)

### 커밋 목록

| 커밋 | 내용 |
|------|------|
| `79e7aff44` | 통합 컨텍스트 파이프라인 — 다중 Intent, 멀티서비스, 생명주기 관리 |
| `f475dd0bc` | docker-compose 접두사 매칭 + 수집 파일 줄 수 30→50줄 확대 |
| `1925d8425` | 후속 질문 컨텍스트 유지 + 수집 파일 번호 매기기 |
| `5d32148d0` | 진행 표시 도구 상태바 전환 + 마크다운 테이블 렌더링 |
| `f91aac08c` | 세션 메모리 + 컨텍스트 우선순위 + 코드 수정 검증 + Modify 강화 |

### 핵심 아키텍처 변경

#### 1. 통합 컨텍스트 파이프라인 (핵심)

```
사용자 메시지
  ↓
detect_intent() → UserIntent (Analyze/Debug/Modify/Deploy)
  ↓
생명주기 판단 (ContextState — 이전 intent/path 비교)
  ├─ 같은 intent + 같은 path → 기존 유지 (후속 질문)
  ├─ 경로 미지정 + 기존 컨텍스트 → 후속 질문으로 간주
  ├─ 다른 intent/path → 기존 제거 + 새로 수집
  └─ intent 없음 → 기존 제거
  ↓
collect_context_with_progress() → Intent별 수집 디스패치
  ↓
agent.add_system_context() / remove_system_context()
  ↓
process_agent_message(원본 메시지만)
```

**이전 단일 "분석" intent → 4개 Intent 파이프라인:**

| Intent | 키워드 | 수집 대상 | max_reads | max_lines |
|--------|--------|----------|-----------|-----------|
| Analyze | 분석, analyze, 파악... | 트리+문서+엔트리+설정+핵심코드+Git | 10(멀티:15) | 50 |
| Debug | 에러, 오류, debug... | 소스+Git diff+환경설정+로그파일 | 12 | 50 |
| Modify | 수정, 고쳐, fix... | 대상파일(150줄)+import 2depth+관련파일+테스트 | 12 | 150 |
| Deploy | 배포, deploy, build... | Docker(접두사매칭)+CI/CD+k8s+빌드설정 | 10 | 40 |

**멀티서비스 감지**: 엔트리포인트 3+개 → 서비스맵 제공, 서비스명 퍼지 매칭 (언더스코어/하이픈 무시)

**후속 질문 지원**: 경로 미지정 시 이전 경로 유지 → "분석해봐" → "배포는?" → "로그 보여줘" 연속 가능 (재수집 안 함)

**프리-LLM 평가**: 수집 < 5개 시 Analyze 폴백 보충

#### 2. 진행 표시 전환

시스템 메시지(ℹ System) → **도구 상태바**(하단)로 전환. 대화 흐름 끊김 제거.
- `app.start_tool()` / `app.update_tool_step()` / `app.finish_tool()` 사용

#### 3. 마크다운 테이블 렌더링

`markdown.rs`에 파이프 구분 테이블 파싱 + 박스문자 렌더링 추가:
```
┌──────┬──────┐
│ 헤더 │ 헤더 │
├──────┼──────┤
│ 데이터│ 데이터│
└──────┴──────┘
```

#### 4. 세션 메모리 파일

```
.goose/sessions/memory.md  ← LLM이 작업 맥락 기록
```
- 세션 시작 시 이전 memory.md 자동 로드 → system_prompt_extras에 주입
- system.md에 기록 지시 (파악된 사실, 수정 파일, 에러/해결, 미해결)

#### 5. 컨텍스트 우선순위

system.md에 명시:
```
1. 사용자의 현재 메시지 (최우선)
2. .goosehints / CLAUDE.md (프로젝트 규칙)
3. memory.md (세션 메모리)
4. 자동 수집 컨텍스트 [AUTO-CONTEXT]
5. LLM 학습 지식 (폴백)
```

#### 6. 코드 수정 검증 지시

system.md에 추가:
- 파일 수정 후 → 구문 검증 (py_compile, cargo check, tsc 등)
- 파일 생성 후 → read로 내용 확인

#### 7. 기반 API 추가

- `prompt_manager.rs`: `remove_system_prompt_extra(key)` 추가
- `agent.rs`: `remove_system_context(key)` 추가
- `app.rs`: `update_tool_step(name, progress)` 추가

---

## GPT-4o 관련 교훈 (추가)

| 시도 | 결과 |
|------|------|
| 3단계 압축 (Snip→Summarize→Guard) | ❌ GPT-4o 요약 품질 낮음 — cliff 현상 |
| **세션 메모리 파일** | **✅ 파일로 빼놓고 필요할 때 읽기 — 모델 능력 무관** |
| TASK 지시문에 "[파일 N] 인용" 강제 | △ GPT-4o가 번호를 안 씀 — 지시 따르기 약함 |
| 할루시네이션 방지 "지어내지 마라" | ✅ 상당히 효과적 — 실제 코드 인용 비율 증가 |
| 코드 수정 후 검증 지시 | △ GPT-4o가 때때로 스킵 — 코드 강제가 더 확실할 수 있음 |

**핵심 원칙**: GPT-4o에게는 "이렇게 해라" 프롬프트보다 코드에서 강제하는 게 확실. 다만 모델 업그레이드 대응을 위해 프롬프트 지시도 병행.

---

## 참조해야 할 파일들

### 코드 (변경된 파일)

| 파일 | 역할 | 중요도 |
|------|------|--------|
| `crates/goose-cli/src/session/tui_session.rs` | 통합 파이프라인, Intent 감지, 멀티서비스, 생명주기, 세션 메모리 로드 | ★★★ |
| `crates/goose/src/prompts/system.md` | 우선순위, 세션 메모리 지시, 검증 지시 | ★★★ |
| `crates/goose/src/agents/agent.rs` | remove_system_context API | ★★ |
| `crates/goose/src/agents/prompt_manager.rs` | remove_system_prompt_extra, TTL (Phase 2) | ★★ |
| `crates/goose-cli/src/session/tui/app.rs` | update_tool_step, ToolStatus | ★★ |
| `crates/goose-cli/src/session/tui/markdown.rs` | 테이블 렌더링 | ★ |

### 설계 문서

| 파일 | 내용 |
|------|------|
| `docs/014-next-session-handoff.md` | 이 문서 |
| `docs/012-quality-improvements.md` | 품질 개선 이력 + 로드맵 |
| `C:\Users\Administrator\.claude\plans\wise-noble-falcon.md` | 고도화 전체 설계 (Phase 1~3) |

---

## 다음 세션에서 해야 할 일

### Phase 2 항목

#### 1. Plan 모드 [우선]

```rust
// agent.rs에 추가
enum AgentMode { Normal, Plan { previous_filter: Option<Vec<String>> } }

// /plan 명령:
// 1. tool_filter를 read-only 도구만으로 제한
// 2. system_prompt_extras에 Plan 지시문 주입
// 3. 상태바에 "📋 Plan" 표시
// 4. 사용자 승인 후 Normal 복귀
```

**수정 파일**: agent.rs, tui_session.rs, app.rs

#### 2. 도구 결과 Snip

10턴 이전의 도구 결과를 `[결과 생략 — N자]`로 교체. 요약이 아닌 단순 제거.

**수정 파일**: `context_mgmt/mod.rs`, `agent.rs`

#### 3. 컨텍스트 TTL

`system_prompt_extras`에 메시지 N개 후 자동 만료 기능.

**수정 파일**: `prompt_manager.rs`

#### 4. 프로젝트 문서 자동 인덱싱

세션 시작 시 `docs/*.md` 스캔 → 파일명+첫 3줄 인덱스 → 질문 시 관련 문서 자동 주입.

**수정 파일**: `tui_session.rs`

#### 5. 코딩 스킬 추가

기존 스킬 시스템 활용:
```
builtin_skills/skills/
├── code-review.md  — 코드 리뷰 체크리스트
├── refactor.md     — 리팩터링 패턴
└── test-writer.md  — 테스트 작성 가이드
```

#### 6. write/edit 후 modified_files.md 자동 기록

코드에서 write/edit 도구 실행 시 `.goose/sessions/modified_files.md`에 자동 추가.

**수정 파일**: `agent.rs` (도구 결과 후처리)

### Phase 3 항목 (중기)

- Data intent (CSV/JSON 감지 + 스키마 주입)
- 워크플로우 파이프라인 (Analyze→Plan→Execute→Evaluate)

### TUI 잔여

- 메인 에이전트 도구 호출 진행 표시 검증
- Light 테마 추가 검증

---

## Claude Code 소스 분석 결과 (참고)

`C:\DEV\personal\claude source\docs\` 6개 문서 + 소스 분석 완료.

### Goose Custom에 적용 가능한 패턴

| Claude Code 패턴 | Goose 적용 상태 |
|------------------|----------------|
| 컨텍스트 우선순위 | ✅ system.md에 명시 |
| 세션 메모리 | ✅ memory.md (하이브리드) |
| Plan 모드 (도구 제한) | Phase 2 |
| Snip 압축 (도구 결과 제거) | Phase 2 |
| 스킬 시스템 | 기존 있음, 스킬 추가 예정 |
| 멀티 에이전트 | 기존 서브에이전트 있음 |
| 프롬프트 캐싱 | ❌ GPT-4o 미지원 |
| Fork (캐시 공유) | ❌ GPT-4o 미지원 |
| 4단계 압축 | ❌ GPT-4o 요약 품질 부족 |

### 격차 핵심

- Claude Code: LLM이 강하니까 LLM이 직접 도구로 파일을 읽고 판단
- Goose: GPT-4o가 약하니까 코드에서 미리 수집해서 넣어줌 → **이 전략이 맞음**
- 모델 업그레이드 대응: 프롬프트 지시도 병행 (강한 모델에서 자연스럽게 개선)

---

## 빌드 & 테스트

```bash
cargo build --release --package goose-cli
./target/release/goose.exe session --tui

# 테스트 시나리오
1. "C:\DEV\aiworker\app\aiworker-workflow 분석해봐" → 멀티서비스
2. "db agent 상세히 봐봐" → 후속질문 (재수집 안 됨)
3. "배포는?" → 후속질문 + Deploy 폴백
4. "에러 있나?" → 후속질문 + kubectl 자율 실행
5. /plan → Plan 모드 진입 (Phase 2)
```
