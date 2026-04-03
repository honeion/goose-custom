---
title: 다음 세션 핸드오프 문서
created: 2026-04-03
author: claude
---

# Goose Custom — 다음 세션 핸드오프

## 이번 세션 작업 (2026-04-02~03) — 15개 커밋

### Phase 1: 통합 컨텍스트 파이프라인
| 커밋 | 내용 |
|------|------|
| `79e7aff44` | 통합 파이프라인 — 4개 Intent(Analyze/Debug/Modify/Deploy), 멀티서비스, 생명주기 |
| `f475dd0bc` | docker-compose 접두사 매칭 + 줄 수 50줄 |
| `1925d8425` | 후속 질문 컨텍스트 유지 + 수집 파일 번호 |

### Phase 1.5: 개발 역량 강화
| 커밋 | 내용 |
|------|------|
| `f91aac08c` | system.md에 검증 지시 + Modify intent 강화 (import 2depth, 150줄) |
| `a73ee112a` | system.md 비대화 해소 (282줄→226줄) — 분석 품질 복원 |

### Phase 2: Plan 모드 + 인프라
| 커밋 | 내용 |
|------|------|
| `407bf9fd3` | Plan 모드 (/plan 토글 — read-only 도구 제한) |
| `478a9c9d2` | Plan 모드 지시문 강화 (GPT-4o 루프 방지) |
| `0a12e084f` | 도구 결과 Snip — 10턴 이전 결과 "[결과 생략]" |
| `0f66e9a5a` | 코딩 스킬 3종 (code-review, refactor, test-writer) + 컨텍스트 TTL |

### TUI 개선
| 커밋 | 내용 |
|------|------|
| `5d32148d0` | 진행 표시 도구 상태바 전환 |
| `ce0a72277` | 프로그레스 바 시스템 메시지 복원 (도구 상태바와 병행) |
| `24ed32757` | 마크다운 테이블 렌더링 연결 |
| `63d31f03f` | 테이블 한글 정렬 (CJK display_width) |
| `810bc4d75` | 테이블 컬럼 너비 상한 + truncate |
| `bc5ef92f0` | 테이블 터미널 너비 자동 맞춤 |

### 시행착오 (중요)
| 커밋 | 내용 |
|------|------|
| `ec0f023c9` | 세션 메모리/문서 인덱스 자동 주입 **제거** — 알림만 표시 |

---

## 시행착오 기록 (GPT-4o 교훈 추가)

| 시도 | 결과 |
|------|------|
| system.md에 Context Priority + Session Memory + Code Rules 추가 (60줄) | ❌ **GPT-4o 분석 품질 급락** — 핵심 자동 수집 컨텍스트를 무시 |
| 세션 메모리 + 문서 인덱스를 system_prompt_extras에 자동 주입 | ❌ **시스템 프롬프트 비대화** — noise 증가로 핵심 놓침 |
| system.md 60줄 → 3줄로 압축 + 자동 주입 제거 | ✅ 분석 품질 복원 |
| 프로그레스 바를 도구 상태바(하단)로만 전환 | ❌ 로컬 파일 수집이 빨라서 안 보임 |
| 시스템 메시지 + 도구 상태바 병행 | ✅ 둘 다 표시 |
| Plan 모드에서 "수정/실행 불가" 지시 (일반적) | ❌ GPT-4o가 무시하고 write 반복 호출 |
| Plan 모드에서 ❌/✅ 마크 + 구체적 도구명 나열 | ✅ GPT-4o가 따름 |

**핵심 원칙**: GPT-4o의 system prompt 용량은 매우 제한적. 넣으면 넣을수록 핵심을 놓침. **최소한만 넣어야 함**.

---

## 현재 상태 — 동작하는 기능

| 기능 | 상태 | 비고 |
|------|------|------|
| 4개 Intent 파이프라인 | ✅ | Analyze/Debug/Modify/Deploy |
| 멀티서비스 감지 | ✅ | 엔트리포인트 3+개, 퍼지 매칭 |
| 후속 질문 컨텍스트 유지 | ✅ | 경로 미지정 시 이전 path 유지 |
| Plan 모드 | ✅ | /plan 토글, read-only 제한 |
| 프로그레스 바 | ✅ | 시스템 메시지 + 도구 상태바 병행 |
| 마크다운 테이블 렌더링 | ✅ | CJK 정렬 + 터미널 너비 자동 맞춤 |
| 도구 결과 Snip | ✅ | 10턴 이전 도구 결과 자동 정리 |
| 컨텍스트 TTL | ✅ | 자동 수집 컨텍스트 8턴 후 만료 |
| 코딩 스킬 3종 | ✅ | code-review, refactor, test-writer |
| 세션 메모리 | △ | 디렉토리 생성 + 알림만 (자동 주입 제거됨) |
| 문서 인덱싱 | △ | 개수 알림만 (자동 주입 제거됨) |

---

## 다음 세션에서 해야 할 일

### 1. 세션 이력 복원 (/resume)
TUI 안에서 이전 세션으로 돌아가는 기능. 현재 CLI에서 `goose session resume`은 있지만 TUI 내 명령은 없음.

### 2. 세션 메모리 재설계
system_prompt_extras 주입은 GPT-4o 품질을 떨어뜨림. 대안:
- LLM이 **직접** memory.md를 read/write 도구로 관리하도록 system.md에 지시 (이미 있음)
- 자동 주입은 하지 않음

### 3. Phase 3 항목
- Data intent (CSV/JSON 감지 + 스키마 주입)
- 워크플로우 파이프라인 (Analyze→Plan→Execute→Evaluate)

### 4. TUI 잔여
- Light 테마 검증
- 메인 에이전트 도구 호출 진행 표시 검증

---

## 핵심 파일 (변경됨)

| 파일 | 중요도 |
|------|--------|
| `crates/goose-cli/src/session/tui_session.rs` | ★★★ |
| `crates/goose/src/prompts/system.md` | ★★★ |
| `crates/goose/src/agents/agent.rs` | ★★ |
| `crates/goose/src/agents/prompt_manager.rs` | ★★ |
| `crates/goose-cli/src/session/tui/markdown.rs` | ★★ |
| `crates/goose-cli/src/session/tui/render.rs` | ★★ |
| `crates/goose-cli/src/session/tui/app.rs` | ★ |
| `crates/goose/src/context_mgmt/mod.rs` | ★ |
| `crates/goose/src/conversation/mod.rs` | ★ |

## 설계 문서

| 파일 | 내용 |
|------|------|
| `C:\Users\Administrator\.claude\plans\wise-noble-falcon.md` | 고도화 전체 설계 (Phase 1~3) |
| `C:\DEV\personal\claude source\docs\` | Claude Code 소스 분석 (6개 문서) |

## 빌드 & 테스트

```bash
cargo build --release --package goose-cli
./target/release/goose.exe session --tui

# 검증 시나리오
1. "C:\DEV\aiworker\app\aiworker-workflow 분석해봐" → 멀티서비스 + 프로그레스
2. "db agent 상세히 봐봐" → 후속질문 (재수집 안 됨)
3. "테이블로 정리해줘" → 박스문자 테이블 (터미널 안에 맞음)
4. /plan → Plan 모드 (read-only) → /plan → 종료
5. "배포는?" → 후속질문 컨텍스트 유지
```
