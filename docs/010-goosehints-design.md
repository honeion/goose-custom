---
title: Goosehints 시스템 설계
status: completed
created: 2026-03-12
updated: 2026-03-12
author: claude
---

# Goosehints 시스템 설계

## 1. 개요

CLAUDE.md처럼 프로젝트별 지침 파일을 지원하는 시스템.
세션 시작 시 로드 상태를 시각적으로 표시하고, TUI에서 편집 가능.

### 1.1 목표

| 목표 | 설명 |
|------|------|
| 계층적 힌트 | 글로벌 → 프로젝트 → 개인 순으로 병합 |
| 시각적 로드 | 세션 시작 시 어떤 힌트가 로드되었는지 표시 |
| 쉬운 편집 | /hints 명령어 + TUI 편집 패널 (F5) |
| 핫 리로드 | 편집 후 즉시 반영 |

### 1.2 현재 상태

| 항목 | 파일 | 상태 |
|------|------|------|
| 힌트 파일 로딩 | `hints/load_hints.rs` | ✅ 완료 |
| 글로벌 힌트 | `~/.config/goose/.goosehints` | ✅ 완료 |
| 프로젝트 힌트 | `.goosehints`, `.goosehints.local` | ✅ 완료 |
| @import 구문 | `hints/import_files.rs` | ✅ 완료 |
| 프롬프트 빌더 | `prompt_manager.rs` | ✅ 완료 |
| **로드 시각화** | `tui_session.rs` | ✅ 완료 |
| **/hints 명령어** | `tui_session.rs` | ✅ 완료 |
| **TUI 편집 패널** | `tui/hints_panel.rs` | ✅ 완료 |

---

## 2. 계층 구조

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: 글로벌 (회사/팀 표준)                               │
│ ~/.config/goose/prompts/global.md                           │
│ - 응답 언어 (한국어)                                        │
│ - 보안 규칙                                                 │
│ - 코드 스타일 가이드                                        │
├─────────────────────────────────────────────────────────────┤
│ Layer 2: 프로젝트별 (Git 커밋, 팀 공유)                      │
│ {project}/.goosehints                                       │
│ - 프로젝트 구조 설명                                        │
│ - 사용 기술 스택                                            │
│ - 빌드/테스트 방법                                          │
├─────────────────────────────────────────────────────────────┤
│ Layer 3: 개인별 (gitignore, 로컬만)                         │
│ {project}/.goosehints.local                                 │
│ - 개인 선호 (verbosity, 톤)                                 │
│ - 로컬 환경 경로                                            │
└─────────────────────────────────────────────────────────────┘

병합 순서: global → project → personal (나중이 우선)
```

### 2.1 파일 위치

| 레이어 | 파일 경로 | Git 추적 |
|--------|----------|----------|
| Global | `~/.config/goose/.goosehints` | N/A |
| Project | `.goosehints` | ✅ 커밋 |
| Local | `.goosehints.local` | ❌ gitignore |

### 2.2 탐색 로직

```
작업 디렉토리: C:/DEV/myproject/src/components

탐색 순서 (.goosehints 찾기):
1. C:/DEV/myproject/src/components/.goosehints
2. C:/DEV/myproject/src/.goosehints
3. C:/DEV/myproject/.goosehints  ← 보통 여기서 발견
4. (git root까지 탐색)

결과 병합:
- ~/.config/goose/.goosehints (글로벌)
- C:/DEV/myproject/.goosehints (프로젝트)
- C:/DEV/myproject/.goosehints.local (개인)
```

---

## 3. 로드 시각화 (Phase 1)

### 3.1 세션 시작 메시지

**현재:**
```
ℹ System  11:46
  Goose Custom TUI 세션이 시작되었습니다. 🔒 민감정보 보호 활성화
```

**변경 후:**
```
ℹ System  11:46
  Goose Custom TUI 세션이 시작되었습니다.

  📋 Hints 로드됨:
  ├─ 🌐 global.md (12줄)
  ├─ 📁 .goosehints (goose-custom, 25줄)
  └─ 👤 .goosehints.local (8줄)

  🔒 PII 마스킹 활성화
```

**간단 버전 (한 줄):**
```
ℹ System  11:46
  Goose TUI 시작 │ 📋 Hints: 3개 │ 🔒 PII
```

### 3.2 구현

**파일:** `crates/goose/src/hints/load_hints.rs`

```rust
/// 로드된 힌트 메타데이터
pub struct HintMetadata {
    pub layer: HintLayer,      // Global, Project, Local
    pub file_path: PathBuf,
    pub line_count: usize,
    pub project_name: Option<String>,  // 프로젝트 폴더명
}

pub enum HintLayer {
    Global,
    Project,
    Local,
}

/// 로드된 힌트 메타데이터 반환
pub fn get_hints_metadata(cwd: &Path) -> Vec<HintMetadata> {
    // 기존 load_hint_files() 로직 활용
    // 파일별 메타데이터 수집
}
```

**파일:** `crates/goose-cli/src/session/tui_session.rs`

```rust
// 시작 메시지 생성
let hints_meta = goose::hints::get_hints_metadata(&cwd);
let hints_summary = format_hints_summary(&hints_meta);

let welcome_msg = format!(
    "Goose Custom TUI 세션이 시작되었습니다.\n\n{}\n{}",
    hints_summary,
    if app.pii_masking_enabled { "🔒 PII 마스킹 활성화" } else { "" }
);
app.add_system_message(welcome_msg);
```

---

## 4. /hints 명령어 (Phase 2)

### 4.1 명령어 목록

| 명령어 | 설명 |
|--------|------|
| `/hints` | 현재 로드된 힌트 보기 |
| `/hints edit` | 프로젝트 .goosehints 편집 ($EDITOR) |
| `/hints edit global` | 글로벌 힌트 편집 |
| `/hints edit local` | 로컬 힌트 편집 |
| `/hints reload` | 힌트 다시 로드 |
| `/hints add "규칙"` | 프로젝트 힌트에 한 줄 추가 |

### 4.2 /hints 출력

```
📋 현재 로드된 Hints (3개)

┌─ 🌐 Global (~/.config/goose/.goosehints)
│  • 응답 언어: 한국어
│  • 코드 주석: 영어
│  • 이모지 사용 금지
│  (12줄)
│
├─ 📁 Project (.goosehints)
│  • 프로젝트: goose-custom (Rust CLI)
│  • 빌드: cargo build --release
│  • 테스트: cargo test
│  (25줄)
│
└─ 👤 Local (.goosehints.local)
   • verbosity: minimal
   • 개인 환경: Windows PowerShell
   (8줄)

💡 /hints edit 로 편집, /hints reload 로 다시 로드
```

### 4.3 구현

**파일:** `crates/goose-cli/src/session/input.rs`

```rust
pub enum InputResult {
    // ... 기존
    HintsCommand(HintsSubCommand),
}

pub enum HintsSubCommand {
    Show,
    Edit(HintLayer),
    Reload,
    Add(String),
}

fn parse_hints_command(input: &str) -> Option<HintsSubCommand> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts.as_slice() {
        ["/hints"] => Some(HintsSubCommand::Show),
        ["/hints", "edit"] => Some(HintsSubCommand::Edit(HintLayer::Project)),
        ["/hints", "edit", "global"] => Some(HintsSubCommand::Edit(HintLayer::Global)),
        ["/hints", "edit", "local"] => Some(HintsSubCommand::Edit(HintLayer::Local)),
        ["/hints", "reload"] => Some(HintsSubCommand::Reload),
        ["/hints", "add", rest @ ..] => Some(HintsSubCommand::Add(rest.join(" "))),
        _ => None,
    }
}
```

**파일:** `crates/goose-cli/src/session/tui/events.rs`

```rust
// /hints 명령어 처리
Action::HintsCommand(cmd) => match cmd {
    HintsSubCommand::Show => {
        let summary = get_hints_detailed_summary(&self.cwd);
        self.app.add_system_message(summary);
    }
    HintsSubCommand::Edit(layer) => {
        let path = get_hint_file_path(layer, &self.cwd);
        open_in_editor(&path)?;
    }
    HintsSubCommand::Reload => {
        reload_hints(&mut self.session)?;
        self.app.add_system_message("📋 Hints 다시 로드됨");
    }
    HintsSubCommand::Add(rule) => {
        append_to_hints(&self.cwd, &rule)?;
        self.app.add_system_message(format!("✅ 추가됨: {}", rule));
    }
}
```

---

## 5. TUI 힌트 편집 패널 (Phase 3)

### 5.1 UI 디자인

```
┌─ Hints 편집 [F5] ─────────────────────────────────────────────┐
│ [🌐 Global] [📁 Project] [👤 Local]                           │
├───────────────────────────────────────────────────────────────┤
│   1 │ # 프로젝트 설정                                         │
│   2 │                                                         │
│   3 │ ## 기본 정보                                            │
│   4 │ - 프로젝트: goose-custom                                │
│   5 │ - 언어: Rust                                            │
│   6 │                                                         │
│   7 │ ## 빌드                                                 │
│   8 │ - cargo build --release                                 │
│   9 │                                                         │
│  10 │ ## 규칙                                                 │
│  11 │ - 기존 파일 수정 선호                                   │
│  12 │ - over-engineering 금지                                 │
│     │                                                         │
├───────────────────────────────────────────────────────────────┤
│ Ctrl+S: 저장  Esc: 닫기  Tab: 탭 전환  Ctrl+R: 리로드         │
└───────────────────────────────────────────────────────────────┘
```

### 5.2 키바인딩

| 키 | 기능 |
|----|------|
| F5 | 힌트 패널 토글 |
| Tab | 탭 전환 (Global/Project/Local) |
| Ctrl+S | 저장 |
| Ctrl+R | 리로드 |
| Esc | 닫기 |

### 5.3 구현

**의존성 추가:**

```toml
# crates/goose-cli/Cargo.toml
[dependencies]
tui-textarea = "0.4"
```

**파일:** `crates/goose-cli/src/session/tui/hints_panel.rs` (신규)

```rust
use tui_textarea::TextArea;

pub struct HintsPanel<'a> {
    pub visible: bool,
    pub active_tab: HintLayer,
    pub editors: HashMap<HintLayer, TextArea<'a>>,
    pub modified: HashSet<HintLayer>,
}

impl<'a> HintsPanel<'a> {
    pub fn new() -> Self {
        Self {
            visible: false,
            active_tab: HintLayer::Project,
            editors: HashMap::new(),
            modified: HashSet::new(),
        }
    }

    pub fn load(&mut self, cwd: &Path) {
        // 각 레이어 파일 로드
        for layer in [HintLayer::Global, HintLayer::Project, HintLayer::Local] {
            let path = get_hint_file_path(layer, cwd);
            let content = fs::read_to_string(&path).unwrap_or_default();
            let mut editor = TextArea::new(content.lines().map(String::from).collect());
            editor.set_cursor_line_style(Style::default().bg(Color::DarkGray));
            self.editors.insert(layer, editor);
        }
    }

    pub fn save(&mut self, layer: HintLayer, cwd: &Path) -> Result<()> {
        let editor = self.editors.get(&layer).unwrap();
        let content = editor.lines().join("\n");
        let path = get_hint_file_path(layer, cwd);
        fs::write(&path, content)?;
        self.modified.remove(&layer);
        Ok(())
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        // 패널 렌더링
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // 키 입력 처리
    }
}
```

**파일:** `crates/goose-cli/src/session/tui/app.rs`

```rust
pub struct TuiApp<'a> {
    // ... 기존
    pub hints_panel: HintsPanel<'a>,
}
```

**파일:** `crates/goose-cli/src/session/tui/events.rs`

```rust
// F5 키 처리
KeyCode::F(5) => {
    if !self.hints_panel.visible {
        self.hints_panel.load(&self.cwd);
    }
    self.hints_panel.visible = !self.hints_panel.visible;
    return Some(Action::Render);
}
```

---

## 6. 구현 계획

### Phase 1: 로드 시각화 (0.5일)

| 작업 | 파일 | 설명 |
|------|------|------|
| 1.1 | `hints/load_hints.rs` | `HintMetadata` 구조체 + `get_hints_metadata()` |
| 1.2 | `tui_session.rs` | 시작 메시지에 hints 요약 추가 |
| 1.3 | - | 테스트 |

### Phase 2: /hints 명령어 (0.5일)

| 작업 | 파일 | 설명 |
|------|------|------|
| 2.1 | `input.rs` | `HintsSubCommand` 파싱 |
| 2.2 | `events.rs` | 명령어 처리 핸들러 |
| 2.3 | `output.rs` | 힌트 포맷팅 출력 |
| 2.4 | - | 외부 에디터 실행 ($EDITOR) |

### Phase 3: TUI 힌트 패널 (1일)

| 작업 | 파일 | 설명 |
|------|------|------|
| 3.1 | `Cargo.toml` | `tui-textarea` 의존성 |
| 3.2 | `tui/hints_panel.rs` | HintsPanel 위젯 구현 |
| 3.3 | `tui/app.rs` | 상태 추가 |
| 3.4 | `tui/events.rs` | F5 키바인딩 |
| 3.5 | `tui/render.rs` | 패널 렌더링 |
| 3.6 | - | 테스트 |

---

## 7. 파일 변경 목록

| 파일 | 변경 | Phase |
|------|------|-------|
| `hints/load_hints.rs` | `HintMetadata`, `get_hints_metadata()` | 1 |
| `tui_session.rs` | 시작 메시지 수정 | 1 |
| `input.rs` | `HintsSubCommand` 파싱 | 2 |
| `events.rs` | /hints + F5 처리 | 2, 3 |
| `tui/hints_panel.rs` | **신규** | 3 |
| `tui/app.rs` | `hints_panel` 필드 | 3 |
| `tui/render.rs` | 패널 렌더링 | 3 |
| `tui/mod.rs` | 모듈 export | 3 |
| `Cargo.toml` | `tui-textarea` | 3 |

---

## 8. 예시 파일

### 8.1 글로벌 힌트 (`~/.config/goose/.goosehints`)

```markdown
# 글로벌 설정

## 언어
- 응답: 한국어
- 코드 주석: 영어
- 커밋 메시지: 영어

## 스타일
- 간결하고 직접적으로
- 이모지 사용 금지
- over-engineering 금지

## 보안
- 하드코딩된 시크릿 금지
- .env 파일 커밋 금지
```

### 8.2 프로젝트 힌트 (`.goosehints`)

```markdown
# goose-custom 프로젝트

## 개요
Rust 기반 CLI 에이전트. Ratatui TUI 사용.

## 빌드
cargo build --release -p goose-cli

## 테스트
cargo test

## 규칙
- 기존 파일 수정 선호 (새 파일 최소화)
- Windows PowerShell 환경 고려
- 한글 인코딩 주의 (UTF-8)
```

### 8.3 개인 힌트 (`.goosehints.local`)

```markdown
# 개인 설정

## 환경
- OS: Windows 11
- Shell: PowerShell 7
- Editor: VS Code

## 선호
- verbosity: minimal
- 코드 블록에 파일명 표시
```

---

## 9. 참고

- Claude Code: `CLAUDE.md`, `CLAUDE.local.md`
- 기존 구현: `hints/load_hints.rs`, `prompt_manager.rs`
- TUI 텍스트 에디터: [tui-textarea](https://github.com/rhysd/tui-textarea)
