---
name: inspect
description: Deep project analysis specialist. Use this when asked to "analyze", "understand", "explain", or "분석" a project or codebase. Reads actual source code to understand architecture, data flow, and business logic. Returns a comprehensive human-readable summary.
allowed_tools:
  - Glob
  - Grep
  - Read
  - Bash
---

Use this skill when the user asks to **analyze, understand, explain, or 분석** a project.
This is NOT a file listing task — you must READ actual code and produce meaningful analysis.

**사용자가 사용하는 언어에 맞춰 응답하세요.**

## Analysis Process (follow this order strictly)

### Step 1: Project Identity
- Read `README.md`, `CLAUDE.md`, or similar docs if they exist
- Read dependency file (`package.json`, `requirements.txt`, `pyproject.toml`, `Cargo.toml`, `go.mod`, `pom.xml`)
- Determine: language, framework, purpose

### Step 2: Project Tree
- Use glob to map the directory structure (skip `node_modules/`, `target/`, `.git/`, `__pycache__/`, `dist/`, `build/`)
- Identify the main source directory (`src/`, `app/`, `lib/`, `crates/`)

### Step 3: Entry Point — 반드시 READ
- Find and **READ** the main entry point (전체 내용을 읽어라):
  - Python: `main.py`, `app.py`, `manage.py`, `__main__.py`
  - Rust: `main.rs`, `lib.rs`
  - JS/TS: `index.ts`, `app.ts`, `server.ts`
  - Go: `main.go`, `cmd/`
  - Java: `Application.java`, `Main.java`
- Understand: what starts the app, what gets initialized, what middleware/routes are registered

### Step 4: Architecture & Core Logic — 최소 5개 파일 READ
**반드시 최소 5개 이상의 핵심 파일을 read 도구로 열어서 코드를 직접 읽어라.**
2개만 읽고 끝내지 마라.

읽어야 할 파일 유형:
- **설정 파일**: config.py, settings.ts, .env.example 등
- **라우터/컨트롤러**: API 엔드포인트 정의 파일들
- **핵심 서비스**: 비즈니스 로직 (orchestrator, service, handler 등)
- **모델/스키마**: 데이터 구조 정의
- **미들웨어/인증**: 인증, 권한, 에러 핸들링

파일을 찾는 방법:
1. entry point의 import를 따라가라
2. grep으로 핵심 클래스 검색: `class.*Service`, `class.*Handler`, `class.*Router`, `def.*route`
3. 가장 큰 파일 = 보통 핵심 로직

### Step 5: Data & External Systems
- Database models, schemas, migrations 찾기
- External API clients, integrations 찾기
- Message queue consumers/producers (Kafka, RabbitMQ, Redis pub/sub) 찾기
- 환경변수: `grep -r "os.environ\|os.getenv\|env::\|process.env"`
- 외부 HTTP 호출: `grep -r "requests\.\|httpx\.\|fetch(\|HttpClient\|reqwest"`

### Step 6: Runtime Environment
- OS 감지: `uname -a 2>/dev/null || echo Windows`
- 컨테이너: `test -f /.dockerenv && echo Docker || echo "Not Docker"`
- K8s: `echo $KUBERNETES_SERVICE_HOST`
- 배포 파일: `Dockerfile`, `docker-compose.yml`, `k8s/`, `helm/`, `pipelines/`
- Git: `git log --oneline -5`, `git branch`, `git status --short`

### Step 7: Connections & Data Flow
- 메인 요청 흐름 추적: request → router → service → repository → database → response
- 핵심 패턴 식별: MVC, microservices, event-driven, CQRS
- 서비스 간 통신: REST, gRPC, message queue, SSE/WebSocket

### Step 8: Code Quality & Issues
- 중복 코드 패턴 탐지
- 에러 핸들링 방식 확인
- 테스트 커버리지 (테스트 파일 존재 여부, 패턴)
- 잠재적 문제점/개선점 식별

## Output Format

사용자 언어에 맞춰 작성. 코드 조각은 원문 유지.

```
## [프로젝트명] — [한 줄 요약]

### 기술 스택
- 언어: ...
- 프레임워크: ...
- DB: ...
- 외부 연동: ...

### 아키텍처
[핵심 구조 — 코드 근거 기반]
[요청 처리 흐름 단계별 설명]

### 핵심 모듈 (코드 읽고 작성, 최소 5개)
- [module]: [역할] — 주요 함수: `func_name()`, `class_name`
- ...

### 데이터 모델
- [테이블/모델]: [역할, 주요 필드]
- [관계 설명]

### API 엔드포인트
| 경로 | 메서드 | 기능 |
|------|--------|------|
| /api/... | GET/POST | ... |

### 데이터 흐름
1. 요청 수신 → 2. 인증 → 3. 라우팅 → 4. 비즈니스 로직 → 5. DB → 6. 응답

### 설정 & 환경변수
| 변수명 | 용도 |
|--------|------|
| ... | ... |

### 실행 환경
- OS / 배포 / 컨테이너

### 코드 품질 & 이슈
- [발견된 패턴, 문제점, 개선 제안]

### 현재 상태
- Git: [브랜치, 최근 커밋]
```

## Rules (절대 규칙)

1. **사용자 언어로 응답** — 사용자가 한국어면 한국어, 영어면 영어
2. **반드시 코드를 읽어라** — read 도구로 열어서 내용 확인. 파일명으로 추측 금지
3. **최소 5개 파일 읽어라** — 2개만 읽고 끝내면 분석이 아님
4. **파일 목록 나열 금지** — "다음 파일들이 있습니다" 식 응답 금지
5. **파일 형식 비율 금지** — "Python 95%" 같은 통계 무의미
6. **파일명 번역 금지** — "orchestrator.py: 오케스트레이션 로직" 같은 건 분석이 아님
7. **코드 근거 제시** — 핵심 함수/클래스 시그니처를 인용해서 주장의 근거를 보여라
8. **핵심부터** — 중요한 것 먼저, 사소한 것 생략
9. **개선점 제시** — 문제점을 발견하면 반드시 언급하고 개선 방향 제안
