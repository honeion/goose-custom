# Goose-Custom 커스터마이징 상태

> 완료된 작업 + 빠른 참조
> 상세 개선 계획은 `002-improvement-plan.md` 참조

---

## 완료된 항목

### 빌드 관련
- [x] llama-cpp-2 제거 (C++ 런타임 충돌 해결)
  - `crates/goose/Cargo.toml`: `default = []`
  - `crates/goose-cli/Cargo.toml`: `default = []`
  - `crates/goose-server/Cargo.toml`: `default = []`
  - local-inference feature를 optional로 변경

### Azure OpenAI 호환성
- [x] content_filter 청크 파싱 에러 수정
  - `crates/goose/src/providers/formats/openai.rs`: Azure OpenAI의 content_filter 전용 청크 무시
- [x] max_tokens 제한 (gpt-4o-2024-05-13 = 4096)
  - 환경변수: `GOOSE_MAX_TOKENS=4096`

---

## 테스트 결과 (2026-02-26)

### 기본 기능
| 테스트 | 결과 | 문제점 |
|--------|------|--------|
| 파일 읽기 | △ | 시간 오래 걸림 |
| 파일 쓰기 | △ | 위치 확인 없이 현재 폴더에 생성 |
| 파일 수정 | △ | 내용 확인 없이 대충 채움 |
| 코드 분석 | △ | 의미있는 분석 아님, 개선 필요 |

### Shell 명령어
| 테스트 | 결과 | 문제점 |
|--------|------|--------|
| git status | ✅ | 정상 |
| cargo check | ✅ | 정상 |
| .rs 파일 개수 | ❌ | `dir /s /b` 등 cmd 문법 사용 → PowerShell에서 실패 |

### 컨텍스트
| 테스트 | 결과 | 문제점 |
|--------|------|--------|
| 멀티턴 기억 | ✅ | 이전 내용 기억함 |
| compaction | ? | 토큰이 오히려 줄어드는 현상 |

### Extension
| 테스트 | 결과 | 문제점 |
|--------|------|--------|
| todo | △ | 대충 만듦 |
| apps | ❌ | CLI에서 작동 안 함 → 비활성화 예정 |

### 기타 문제
- 한글 깨짐 지속
- 결과 중복 출력
- 도구 호출 로그 너무 장황함

---

## 수정된 파일 목록

| 파일 | 변경 내용 |
|------|----------|
| `crates/goose/Cargo.toml` | default features에서 local-inference 제거 |
| `crates/goose-cli/Cargo.toml` | default features 제거 |
| `crates/goose-server/Cargo.toml` | default features 제거 |
| `crates/goose/src/providers/formats/openai.rs` | Azure content_filter 청크 무시 |
| `crates/goose/src/providers/mod.rs` | local_inference cfg 플래그 |
| `crates/goose/src/providers/init.rs` | LocalInferenceProvider cfg 플래그 |
| `crates/goose-server/src/routes/mod.rs` | local_inference routes cfg 플래그 |
| `crates/goose-server/src/state.rs` | InferenceRuntime cfg 플래그 |
| `crates/goose-server/src/openapi.rs` | local_inference schemas 주석처리 |
| `crates/goose-cli/src/cli.rs` | LocalModels command cfg 플래그 |

---

## 환경 설정

```powershell
# 필수 환경변수
$env:GOOSE_MAX_TOKENS = "4096"
$env:AZURE_OPENAI_API_KEY = "your-key"
$env:AZURE_OPENAI_ENDPOINT = "https://your-resource.openai.azure.com/"
$env:AZURE_OPENAI_DEPLOYMENT_NAME = "your-deployment"

# 영구 설정
[Environment]::SetEnvironmentVariable("GOOSE_MAX_TOKENS", "4096", "User")
```

---

## 보안 기능

### 기존 보안 장치 (goose 원본)

| 기능 | 상태 | 위치 |
|------|------|------|
| 프롬프트 인젝션 탐지 | ✅ 있음 | `security/scanner.rs` |
| 위험 명령 패턴 탐지 | ✅ 있음 | `security/patterns.rs` |
| 권한 관리 | ✅ 있음 | `permission/` |
| 감사 로그 | ✅ 있음 | `logs/llm_request.*.jsonl` |

### 탐지되는 위협 패턴

- FileSystemDestruction: `rm -rf`, `dd`, `format`
- RemoteCodeExecution: `curl | bash`, `powershell download`
- DataExfiltration: SSH 키, 비밀번호 파일 유출
- SystemModification: crontab, systemd 수정
- NetworkAccess: netcat, reverse shell
- PrivilegeEscalation: sudo NOPASSWD

### Phase 6: 보안 강화 ✅ 완료

| 기능 | 상태 | 문서 |
|------|------|------|
| 보안 가이드라인 | ✅ 완료 | `003-security-guidelines.md` |
| PII 마스킹 설계 | ✅ 완료 | `004-pii-masking-design.md` |
| PII 마스킹 UI 설계 | ✅ 완료 | `004-pii-masking-design.md` §3 |
| **PII 마스킹 구현** | ✅ 완료 | `security/pii_masker.rs`, `pii_patterns.rs` |
| **PII 기본값 활성화** | ✅ 완료 | 기본 ON |
| 감사 로그 분석 도구 | 🔲 TODO | - |

### Phase 7: DevOps 지원 ✅ 완료 (2026-03-12)

| 기능 | 상태 | 설명 |
|------|------|------|
| kubectl 지원 | ✅ 완료 | Kubernetes 클러스터 관리 |
| az CLI 지원 | ✅ 완료 | Azure 리소스 관리 |
| helm/docker 지원 | ✅ 완료 | 패키지/컨테이너 관리 |
| 한국어 의도 파악 | ✅ 완료 | "네임스페이스" → kubectl |
| Fuzzy Matching | ✅ 완료 | 유사 이름 검색 |
| TUI 도구 승인 | ✅ 완료 | 자동 승인 처리 |

### Phase 8: .goosehints 시스템 ✅ 완료 (2026-03-12)

| 기능 | 상태 | 설명 |
|------|------|------|
| HintLayer/HintMetadata | ✅ 완료 | `hints/load_hints.rs` |
| 로드 시각화 | ✅ 완료 | 세션 시작 시 hints 목록 표시 |
| /hints 명령어 | ✅ 완료 | show/reload/add/edit/path/panel |
| F5 편집 패널 | ✅ 완료 | `tui/hints_panel.rs` |
| 도움말 업데이트 | ✅ 완료 | F1에 F4/F5 추가 |

**파일 계층:**
```
~/.config/goose/.goosehints     (🌐 Global)
{project}/.goosehints           (📁 Project, git 추적)
{project}/.goosehints.local     (👤 Local, gitignore)
```

**설계 문서:** `010-goosehints-design.md`

### Phase 9: F7 설정 패널 + PII 화이트리스트 ✅ 완료 (2026-03-20)

| 기능 | 상태 | 설명 |
|------|------|------|
| PiiMasker 화이트리스트 | ✅ 완료 | 정확한 값 매칭으로 마스킹 스킵 |
| PiiMasker 카테고리 비활성화 | ✅ 완료 | Secret/Token/Credential/Certificate 개별 on/off |
| Agent 런타임 메서드 | ✅ 완료 | set/get whitelist, disabled_types, enabled |
| F7 설정 패널 UI | ✅ 완료 | `tui/config_panel.rs` |
| Config 영속화 | ✅ 완료 | Ctrl+S → config.yaml + 런타임 즉시 적용 |
| /config 명령어 | ✅ 완료 | 슬래시 명령어로 패널 열기 |
| Ctrl+C 수정 | ✅ 완료 | 패널 열려있어도 종료 가능 (F5/F6/F7 공통) |
| GooseMode 토글 | ✅ 완료 | Auto/Approve/SmartApprove/Chat 순환 |

**설정 패널 탭:**
```
General  : Provider (표시), Model (표시), Mode (토글)
PII 마스킹: on/off, 4개 카테고리 토글, 화이트리스트 관리
고급     : Max Tokens (+/-), Max Turns (+/-), API Version (표시), 감사 로깅 (토글)
```

**키바인딩:** F7 (패널 토글), Ctrl+S (저장), Tab (탭전환), ↑↓ (이동), Space (토글), A/D (추가/삭제)

### PII 마스킹 개요

```
[데이터 흐름]
입력 → 마스킹 → LLM 전송 → 응답 → 복원 → 출력

[UI 표시]
🔒 PII 마스킹됨 (LLM에 원본 전송 안 함)
   • password 값  → [SECRET_1] (MyP@****23)
   • API Key      → [SECRET_2] (sk-ab****yz)
   • Bearer Token → [TOKEN_1]  (eyJh****)
```

### PII 토큰 종류

| 탐지 종류 | 토큰 형식 | 예시 |
|----------|----------|------|
| 비밀번호 | `[SECRET_N]` | `password=xxx` |
| API 키 | `[SECRET_N]` | `API_KEY=xxx` |
| Bearer 토큰 | `[TOKEN_N]` | `Bearer eyJhb...` |
| 개인키/인증서 | `[CERT_N]` | `-----BEGIN PRIVATE KEY-----` |
| DB 연결문자열 | `[CRED_N]` | `Server=...;Password=xxx` |

### 효과

- **네트워크**: 원본 미전송, 토큰만 전송
- **로그**: 마스킹된 상태로 기록
- **UI**: 마스킹 항목 명확히 표시
- **메모리**: 세션 종료 시 매핑 테이블 삭제

---

## 서브에이전트 시스템 (2026-02-27)

### 1. 비전

```
┌─────────────────────────────────────────────────────────────┐
│                      메인 에이전트                           │
│                    (오케스트레이터)                          │
│  - 사용자 요청 분석                                         │
│  - 서브에이전트 선택 및 병렬 디스패치                        │
│  - 결과 취합 및 최종 응답                                   │
└─────────────────────────────────────────────────────────────┘
         │              │              │              │
         ▼              ▼              ▼              ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│   Explore   │  │  Research   │  │    Coder    │  │    Bash     │
│ (병렬 실행) │  │ (병렬 실행) │  │ (순차 실행) │  │ (순차 실행) │
├─────────────┤  ├─────────────┤  ├─────────────┤  ├─────────────┤
│ Glob, Grep  │  │ Glob, Grep  │  │ Glob, Grep  │  │    Bash     │
│ Read        │  │ Read        │  │ Read, Edit  │  │             │
│             │  │ WebFetch    │  │ Write       │  │             │
├─────────────┤  ├─────────────┤  ├─────────────┤  ├─────────────┤
│ 빠른 탐색   │  │ 심층 분석   │  │ 코드 작성   │  │ 명령 실행   │
│ "어디있어?" │  │ 종합 + 계획 │  │ 수정 허용   │  │ 전용        │
└─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘
         │              │              │              │
         └──────────────┴──────────────┴──────────────┘
                                 │
                                 ▼
                       ┌─────────────────┐
                       │    General      │
                       │ (모든 도구 접근) │
                       │ 복잡한 멀티작업  │
                       └─────────────────┘
```

#### 핵심 목표

| 목표 | 설명 |
|------|------|
| 병렬 워크플로우 | 독립적인 탐색 작업을 동시 실행, 시간 단축 |
| 효율적 오케스트레이션 | 메인 에이전트가 서브에이전트 결과 취합 및 종합 |
| 도구 효율성 | 역할에 맞는 도구만 사용, 불필요한 시도 방지 |
| 보안 통제 | 위험 행동 원천 차단 (읽기 전용 에이전트) |

#### 빌트인 vs 커스텀

| 구분 | 빌트인 서브에이전트 | 커스텀 서브에이전트 |
|------|-------------------|-------------------|
| 도구 통제 | ✅ 엄격 (allowed_tools 강제) | △ MD로 권고 |
| 보안 | ✅ 수정 불가, 오버라이드 방지 | △ 패턴 탐지만 |
| 효율성 | ✅ 최적화된 지침 | △ LLM 의존 |
| 사용 예 | Explore, Research, Coder, Bash, General | 사용자 정의 워크플로우 |

### 2. 개요

서브에이전트는 특정 작업을 위임받아 실행하는 독립적인 에이전트입니다.
goose의 `summon` extension이 `delegate` 도구를 통해 서브에이전트를 실행합니다.

**goose 기존 지원 기능:**
- ✅ 비동기 실행: `r#async: true`
- ✅ 백그라운드 태스크 관리: `background_tasks`
- ✅ 결과 취합: `load(source: "task_id")`로 결과 조회
- ✅ 병렬 가이드: 시스템 프롬프트에 패턴 안내됨

```
"Research (read-only): parallelize freely - delegates explore and report back."
"Decompose → async delegates → load(taskId) for each → synthesize."
```

#### 실행 모드 구분

| 서브에이전트 | 실행 모드 | 이유 |
|-------------|----------|------|
| Explore | 🔄 백그라운드 병렬 | 읽기 전용, 독립적 |
| Research | 🔄 백그라운드 가능 | 읽기 전용 |
| Coder | ⚠️ **순차 포그라운드** | 파일 수정, 확인 필요 |
| Bash | ⚠️ **순차 포그라운드** | 명령 실행, 위험할 수 있음 |
| General | ⚠️ **순차 포그라운드** | 모든 도구 접근 가능 |

#### 상태 표시 (UI 요구사항)

```
┌────────────────────────────────────────┐
│ 🔄 백그라운드 태스크 (2/3)              │
│   ✅ explore-1: src/ 탐색 완료          │
│   🔄 explore-2: tests/ 탐색 중...       │
│   ⏳ explore-3: docs/ 대기              │
└────────────────────────────────────────┘
```

**표시 필요 정보:**
- 현재 실행 중인 태스크 수
- 각 태스크 상태 (대기/진행/완료)
- 태스크 설명 (무엇을 하고 있는지)

### 2. 서브에이전트 타입

| 타입 | 로드 위치 | 모델 지정 | 처리 함수 |
|------|----------|----------|----------|
| Skill | `.goose/skills/*.md` | ❌ | build_recipe_from_skill |
| BuiltinSkill | 코드 내장 (`include_dir!`) | ❌ | build_recipe_from_skill |
| Agent | `.goose/agents/*.md` | ✅ | build_recipe_from_agent |

**핵심:** Skill과 BuiltinSkill은 **완전히 동일**하게 처리됨. 차이는 로드 위치만.

#### 빌트인 스킬 목록

| 서브에이전트 | 도구 | 용도 | 상태 |
|-------------|------|------|------|
| Explore | Glob, Grep, Read | 빠른 탐색 - "어디 있어?" | ✅ 완료 |
| Research | Glob, Grep, Read, WebFetch | 심층 분석 + 종합 + 계획 | ✅ 완료 |
| Coder | Glob, Grep, Read, Edit, Write, Undo | 코드 작성/수정 | ✅ 완료 |
| Bash | Bash | 명령 실행 전용 | ✅ 완료 |
| General | 전체 | 복합 작업 | ✅ 완료 |
| goose_doc_guide | - | goose 문서 참조 (원본) | ✅ 기존 |

**파일 위치:** `crates/goose/src/agents/builtin_skills/skills/`

### 3. 동작 원리

```
delegate(source="explore", instructions="find auth code")
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│ 1. Source 로드                                          │
│    - .goose/skills/explore.md 또는 빌트인에서 찾기      │
│    - MD 파일 파싱 (frontmatter + body)                  │
└─────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│ 2. Recipe 빌드                                          │
│    - instructions: MD body (서브에이전트 지침)          │
│    - prompt: delegate의 instructions 파라미터          │
│    - settings: model 지정 (Agent 타입만)                │
└─────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│ 3. TaskConfig 빌드                                      │
│    - provider: 부모 세션에서 상속 (또는 오버라이드)     │
│    - extensions: 부모 세션에서 상속 (필터링 가능)       │
│    - max_turns: 환경변수 또는 기본값 (25)               │
└─────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│ 4. 서브에이전트 실행                                    │
│    - 새 Agent 인스턴스 생성                             │
│    - extensions 추가 (도구 사용 가능)                   │
│    - system_prompt = Recipe.instructions                │
│    - user_message = Recipe.prompt                       │
│    - 실행 후 결과 반환                                  │
└─────────────────────────────────────────────────────────┘
```

**MD 파일 역할:** 서브에이전트의 **지침(instructions)**만 제공
**도구:** 부모 세션의 extensions에서 **상속** (별도 정의 아님)

### 4. 로드 우선순위 (높음 → 낮음)

```
1. 로컬 레시피     .goose/recipes/
2. 로컬 스킬       .goose/skills/, .claude/skills/, .agents/skills/
3. 로컬 에이전트   .goose/agents/, .claude/agents/
4. 글로벌 레시피   ~/.config/goose/recipes/
5. 글로벌 스킬     ~/.config/goose/skills/
6. 글로벌 에이전트 ~/.config/goose/agents/
7. BuiltinSkill    코드 내장 ← 가장 낮음!
```

**문제점:** `seen` 해시셋으로 중복 체크 → 먼저 로드된 것이 우선
- 사용자가 `.goose/skills/explore.md` 생성하면 빌트인 explore 무시됨
- 시스템 핵심 에이전트 보호 안 됨

### 5. 보안 레이어

goose에는 두 가지 보안 메커니즘이 있음:

| 레이어 | 대상 | 검사 시점 | 목적 |
|--------|------|----------|------|
| **도구 제한** | 특정 서브에이전트 | 도구 호출 전 | 역할 분리 |
| **패턴 탐지** | 모든 에이전트 | 명령 실행 전 | 위험 명령 차단 |

```
도구 제한:  explore가 bash 호출 → ❌ (도구 목록에 없음)
패턴 탐지:  main agent가 bash("rm -rf /") → ❌ 차단 또는 사용자 확인
```

**패턴 탐지 (기존):** `security/patterns.rs`
- FileSystemDestruction: rm -rf, dd, format
- RemoteCodeExecution: curl | bash
- DataExfiltration: SSH 키, 비밀번호 유출
- SystemModification: crontab, systemd
- PrivilegeEscalation: sudo NOPASSWD

### 6. 도구 제어 설계

#### 문제: bash 도구의 이중성

```
explore + bash:
  bash("ls -la")       ✅ 찾기 - OK
  bash("find . -name") ✅ 찾기 - OK
  bash("rm file")      ❌ 삭제 - NO
  bash("git commit")   ❌ 수정 - NO
```

bash 자체를 허용하면 수정/삭제도 가능해짐

#### 해결: 역할별 도구 제한

```yaml
Explore:
  allowed_tools: [Glob, Grep, Read]
  # Bash 제외 → 수정/삭제 자체가 불가능

Research:
  allowed_tools: [Glob, Grep, Read, WebFetch]
  # 웹에서 데이터도 수집

Coder:
  allowed_tools: [Glob, Grep, Read, Edit, Write]
  # 코드 작성/수정 허용, Bash는 제외

Bash:
  allowed_tools: [Bash]
  # 명령 실행 전용

General:
  allowed_tools: []  # 빈 배열 = 모든 도구 허용
```

| 서브에이전트 | 허용 도구 | 금지 도구 | 역할 |
|-------------|----------|----------|------|
| Explore | Glob, Grep, Read | Edit, Write, Bash | 빠른 탐색 |
| Research | Glob, Grep, Read, WebFetch | Edit, Write, Bash | 심층 분석 + 계획 |
| Coder | Glob, Grep, Read, Edit, Write | Bash | 코드 작성/수정 |
| Bash | Bash | 나머지 전부 | 명령 실행 전용 |
| General | 전체 | - | 복잡한 멀티 작업 |

### 7. Claude Code vs Goose 비교

| 기능 | Claude Code | Goose (현재) | Goose (목표) |
|------|-------------|--------------|--------------|
| 빌트인 서브에이전트 | ✅ 코드 고정 | △ MD (오버라이드 가능) | ✅ 오버라이드 방지 |
| 도구 제한 | ✅ 있음 | ❌ 없음 | ✅ allowed_tools |
| 모델 지정 | ❌ 동일 모델 | ✅ Agent만 | ✅ 유지 |
| 우선순위 보호 | ✅ 시스템 우선 | ❌ 사용자 우선 | ✅ 빌트인 우선 |

### 8. 구현 계획

#### Phase 1: 도구 제한 (필수)

```rust
// summon.rs - SkillMetadata 수정
#[derive(Debug, Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
    #[serde(default)]
    allowed_tools: Option<Vec<String>>,  // 추가
}

// build_task_config에서 extensions 필터링 시 적용
```

#### Phase 2: 빌트인 오버라이드 방지 (필수)

```rust
// summon.rs - collect_sources 수정
// 빌트인을 먼저 로드하고 seen에 추가
for content in builtin_skills::get_all() {
    if let Some(source) = parse_skill_content(content, PathBuf::new()) {
        seen.insert(source.name.clone());  // 먼저 등록
        sources.push(Source { kind: SourceKind::BuiltinSkill, ..source });
    }
}
// 이후 외부 스킬 로드 시 seen에 있으면 스킵 (기존 로직)
```

#### Phase 3: MD 파일 업데이트

```markdown
---
name: explore
description: Fast codebase exploration
allowed_tools: ["glob", "grep", "read"]
---

Use this skill for **codebase exploration tasks**:
- Finding files by name patterns
- Searching for specific code patterns
- Understanding codebase structure

Do NOT use this skill for:
- Making code changes
- Creating new files
- Running tests or builds
```

### 9. 결정 사항

- [x] BuiltinSkill 기반으로 서브에이전트 구현
- [x] 도구 제한은 allowed_tools로 (역할별 도구 할당)
- [x] 빌트인 오버라이드 방지 필요
- [x] 서브에이전트 이름 확정: Explore, Research, Coder, Bash, General
- [x] Plan → Research 이름 변경 (goose planner와 혼동 방지)
- [x] Research에 WebFetch 추가 (웹 데이터 수집)
- [x] SkillMetadata에 allowed_tools 추가 구현
- [x] 빌트인 로드 순서 변경 구현
- [x] research.md 추가 (WebFetch 포함)
- [x] coder.md 추가
- [x] bash.md 추가
- [x] general.md 추가

---

## 브라우저 자동화 (2026-03-04)

### 개요

chromiumoxide 기반 내장 브라우저 Extension 구현 완료.

### 구현 파일

| 파일 | 설명 |
|------|------|
| `crates/goose-mcp/src/browser/mod.rs` | 모듈 정의 |
| `crates/goose-mcp/src/browser/browser_ext.rs` | 브라우저 Extension 구현 |

### 도구 목록

| 도구 | 설명 |
|------|------|
| browser_launch | Chrome/Edge 브라우저 실행 (headless/headed) |
| browser_navigate | URL 이동 (기존 탭 재사용) |
| browser_click | CSS selector로 요소 클릭 |
| browser_input | 텍스트 입력 |
| browser_screenshot | 스크린샷 캡처 (PNG) |
| browser_read_page | 페이지 HTML 읽기 |
| browser_find | selector로 요소 찾기 |
| browser_close | 브라우저 종료 |

### 설정

- **기본 활성화**: `DEFAULT_BUILTIN_EXTENSIONS`에 "browser" 포함
- **자동 승인**: `readonly_tools`에 브라우저 도구 추가
- **순차 실행**: system.md에 브라우저 도구 순차 호출 지침 추가

### 기술 스택

```toml
chromiumoxide = { version = "0.7", features = ["tokio-runtime"], default-features = false }
```

### 전역 상태 관리

```rust
static BROWSER_STATE: Lazy<Arc<Mutex<Option<Browser>>>> = ...;
static PAGE_STATE: Lazy<Arc<Mutex<Option<Page>>>> = ...;
```

도구 호출 간 브라우저/페이지 상태 유지를 위해 전역 static 사용.

### 미구현 항목

- [ ] 내부 SSO 인증 (세션 쿠키 저장/복원)
- [ ] Windows 통합 인증 (NTLM/Kerberos)
- [ ] 멀티탭 관리

---

## Phase 5: Ratatui UI 고도화 (2026-03-12) ✅ 완료

### 개요

Ratatui 기반 TUI 구현 완료.

### 구현 완료 항목

| 항목 | 상태 | 설명 |
|------|------|------|
| TEA 아키텍처 | ✅ 완료 | app.rs, events.rs, render.rs |
| Catppuccin 테마 | ✅ 완료 | Dark (Mocha) / Light |
| Vim 키바인딩 | ✅ 완료 | Normal/Insert/Command 모드 |
| OffscreenBuffer | ✅ 완료 | 독립 스크롤 영역 |
| 마크다운 렌더링 | ✅ 완료 | bold, italic, code, headers, lists |
| 코드 구문 강조 | ✅ 완료 | Rust, Python, JS, Go, Bash, SQL |
| Diff Preview | ✅ 완료 | +/- 색상 하이라이팅 |
| 도구 출력 패널 | ✅ 완료 | F3 토글, 독립 스크롤 |
| 테마 전환 | ✅ 완료 | F4 토글 |
| 도움말 팝업 | ✅ 완료 | F1 토글 |
| **도구 승인 자동화** | ✅ 완료 | TUI에서 도구 호출 자동 승인 |
| **붙여넣기 감지** | ✅ 완료 | 빠른 입력 감지로 여러줄 지원 |
| **스크롤/히스토리 분리** | ✅ 완료 | F2 토글로 ↑↓ 동작 전환 |
| **biased select!** | ✅ 완료 | UI 반응성 개선 |
| 탭 바 | ⏭️ 보류 | 별도 창 실행으로 대체 가능, 우선순위 낮음 |
| TachyonFX | ❌ 비활성화 | 터미널 깜빡거림 이슈로 사용 불가 |

### 구현 파일

```
crates/goose-cli/src/session/
├── tui_session.rs          # TUI-에이전트 통합
└── tui/
    ├── mod.rs              # 모듈 정의
    ├── app.rs              # TuiApp 상태 (651줄)
    ├── events.rs           # 이벤트 핸들링 (428줄)
    ├── render.rs           # 렌더링 로직 (580줄)
    ├── theme.rs            # 테마 시스템 (320줄)
    ├── markdown.rs         # 마크다운 + Diff (635줄)
    ├── animation.rs        # SpinnerFrames (180줄)
    ├── offscreen_buffer.rs # 독립 스크롤백 (350줄)
    └── runner.rs           # (미사용)
```

### 키바인딩

| 키 | 기능 |
|----|------|
| F1 | 도움말 토글 |
| F2 | 마우스 캡처 토글 |
| F3 | 도구 출력 패널 토글 |
| F4 | 테마 전환 (Dark/Light) |
| F5 | Hints 편집 패널 |
| F6 | 감사 로그 패널 |
| F7 | 설정 패널 |
| Tab | 패널 포커스 전환 |
| Esc | Normal 모드 |
| i/a | Insert 모드 |
| ↑/↓ | 입력 히스토리 |
| Ctrl+↑/↓ | 스크롤 |

### 문서

- 아키텍처: **007-ratatui-ui-design.md**
- 비주얼: **008-ui-visual-design.md**

---

## 문서 참조

| 문서 | 내용 |
|------|------|
| `000-overview.md` | 프로젝트 개요 + Phase 계획 |
| `001-cleanup-plan.md` | 소스 정리 (삭제/유지) |
| `002-improvement-plan.md` | 개선 계획 (새 기능) |
| `003-security-guidelines.md` | 보안 가이드라인 + 데이터 안전성 |
| `004-pii-masking-design.md` | PII 마스킹 설계 |
| `005-tool-separation-plan.md` | 도구 분리 (Read/Edit/Write) |
| `006-vision-analysis-guide.md` | 비전 분석 가이드 |
| `007-ratatui-ui-design.md` | Ratatui UI 설계 (Phase 5) |
