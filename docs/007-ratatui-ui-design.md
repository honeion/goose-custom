---
title: Phase 5 - Ratatui UI 고도화 설계 (아키텍처)
status: mostly-done
created: 2026-03-04
updated: 2026-03-06
author: claude
---

# Phase 5: Ratatui UI 고도화 설계

> **관련 문서:**
> - `007-ratatui-ui-design.md` - 아키텍처 설계 (이 문서)
> - `008-ui-visual-design.md` - **비주얼 디자인 스펙** (컬러, 레이아웃, 애니메이션)

## 현재 구현 상태

| Phase | 상태 | 설명 |
|-------|------|------|
| 5.1 기반 구조 | ✅ 완료 | app.rs, events.rs, render.rs, tui_session.rs |
| 5.2 핵심 위젯 | ✅ 완료 | Tokens 표시, 메시지 구분선 스타일 |
| 5.3 스트리밍 통합 | ✅ 완료 | 마크다운 렌더링, OffscreenBuffer |
| 5.4 고급 기능 | ✅ 완료 | 구문 강조, 테마 전환 (F4), Diff Preview |
| 5.5 폴리싱 | ⚠️ 부분 | 애니메이션 모듈 (TachyonFX 미적용) |

### 남은 작업

| 작업 | 설명 | 우선순위 |
|------|------|---------|
| 탭 바 | 다중 세션 전환 UI | 중 |
| TachyonFX | 메시지 등장 애니메이션 | 낮음 |

### 구현된 파일

```
crates/goose-cli/src/session/
├── tui_session.rs          # TUI-에이전트 통합, 스트리밍 루프
└── tui/
    ├── mod.rs              # 모듈 re-export
    ├── app.rs              # TuiApp 상태, ChatMessage, ToolStatus
    ├── events.rs           # Action enum, 키 이벤트 처리
    ├── render.rs           # 렌더링 로직
    ├── theme.rs            # Catppuccin Mocha/Light 테마
    ├── markdown.rs         # 마크다운 파싱, 구문 강조, Diff 하이라이팅
    ├── animation.rs        # SpinnerFrames, TypingAnimation, ProgressBar
    ├── offscreen_buffer.rs # 독립 스크롤백 버퍼, PanelManager
    └── runner.rs           # (미사용)
```

### 해결된 이슈

- ✅ Windows KeyEventKind::Press 필터링
- ✅ 스트리밍 중 스크롤 가능 (이벤트 루프 리팩토링)
- ✅ 한글 텍스트 래핑 (display_width 계산)
- ✅ F2 마우스 캡처 토글 (휠 스크롤 vs 텍스트 선택)
- ✅ OffscreenBuffer로 독립 스크롤 영역 구현
- ✅ 도구 출력 패널 분리 (F3 토글)
- ✅ 마우스 휠 → 포커스된 패널만 스크롤
- ✅ 테마 전환 (F4)
- ✅ 마크다운 렌더링 (bold, italic, code, headers)
- ✅ 코드 블록 구문 강조 (Rust, Python, JS, Go, Bash, SQL)
- ✅ Diff Preview 하이라이팅

### 남은 이슈

- ⚠️ **탭 바**: 다중 세션 전환 UI (미구현)
- ⚠️ **TachyonFX**: 메시지 등장 애니메이션 (미적용)

---

## 1. 개요

### 1.1 목표

현재 goose-cli의 console/bat 기반 출력을 **Ratatui 기반 화려한 TUI**로 전환

### 1.2 현재 상태 분석

| 파일 | LOC | 역할 |
|------|-----|------|
| output.rs | 1,569 | 렌더링 로직, 테마, 프로그레스 |
| streaming_buffer.rs | 596 | 안전한 마크다운 스트리밍 |
| input.rs | 709 | 사용자 입력, 슬래시 명령어 |
| mod.rs | 1,000+ | 세션 오케스트레이션 |

### 1.3 현재 의존성

```toml
# UI/출력 렌더링
console = "0.16.1"       # 터미널 스타일링, 색상
bat = "0.26.1"           # 구문 강조 (markdown)
comfy_table = "7.2.2"    # 마크다운 테이블
indicatif = "0.18.1"     # 프로그레스 바, 스피너
cliclack = "0.3.5"       # 인터랙티브 프롬프트 (thinking)
anstream = "0.6.18"      # ANSI 스트림

# 입력
rustyline = "15.0.0"     # 라인 에디터 + 히스토리
```

---

## 2. 참조 프로젝트 분석

### 2.1 Tenere (pythops/tenere)

**특징:**
- Elm Architecture (TEA) 패턴
- Vim 스타일 키바인딩 (Normal/Insert/Visual 모드)
- tui-textarea 사용
- 히스토리 아카이브 파일

**주요 패턴:**
```rust
// TEA 패턴
struct App {
    messages: Vec<ChatMessage>,
    current_response: String,  // 스트리밍 버퍼
    input_buffer: TextArea,
    mode: Mode,
    is_loading: bool,
}

enum Mode { Normal, Insert, Visual }

// 이벤트 루프
tokio::select! {
    Some(event) = event_stream.next() => handle_event(event),
    Some(chunk) = response_channel.recv() => app.current_response.push_str(&chunk),
    _ = render_interval.tick() => terminal.draw(|f| render_ui(f, &app))?,
}
```

### 2.2 Oatmeal (dustinblackman/oatmeal)

**특징:**
- Trait 기반 백엔드 아키텍처
- tokio::mpsc 채널로 스트리밍
- 채팅 버블 렌더링
- Discord 스타일 슬래시 명령어

**주요 패턴:**
```rust
// Trait 기반 확장성
trait Backend: Send + Sync {
    async fn stream_response(&self, messages: &[Message]) -> ResponseStream;
}

// 채널 기반 스트리밍
let (tx, rx) = mpsc::channel(100);
tokio::spawn(async move {
    while let Some(token) = response_stream.next().await {
        tx.send(token).await.ok();
    }
});
```

### 2.3 핵심 교훈

| 항목 | 패턴 |
|------|------|
| 아키텍처 | TEA (Model → Update → View) |
| 스트리밍 | tokio::mpsc 채널 |
| 렌더링 | Immediate-mode (매 프레임 전체 재그림) |
| 입력 | tui-textarea + Vim 키바인딩 |
| 이벤트 | tokio::select! 멀티플렉싱 |

---

## 3. Ratatui 핵심 개념

### 3.1 Immediate-Mode 렌더링

```
매 프레임마다:
1. 현재 상태(Model)로부터 전체 UI 재구성
2. 이전 버퍼와 현재 버퍼 diff 계산
3. 변경된 셀만 터미널에 전송 (최적화)
```

**장점:**
- 상태 동기화 문제 없음
- 스트리밍 중에도 부드러운 업데이트
- 코드 단순화 (위젯 라이프사이클 없음)

### 3.2 주요 위젯

| 위젯 | 용도 | goose-cli 적용 |
|------|------|---------------|
| Paragraph | 텍스트 블록, 워드랩 | 메시지 렌더링 |
| List | 스크롤 가능한 목록 | 대화 히스토리 |
| Block | 테두리, 제목 | 채팅 버블 |
| Gauge | 진행률 바 | 토큰 사용량 |
| Scrollbar | 스크롤 인디케이터 | 긴 대화 |
| Table | 구조화된 데이터 | 도구 출력 |

### 3.3 레이아웃 구조

```
┌─────────────────────────────────────────────┐
│ 헤더: 세션 정보, 모델명                        │ Length(2)
├─────────────────────────────────────────────┤
│                                             │
│ 대화 영역 (스크롤 가능)                        │ Min(10)
│ ┌─ User ─────────────────────────────────┐  │
│ │ 질문 내용...                            │  │
│ └────────────────────────────────────────┘  │
│ ┌─ Assistant ────────────────────────────┐  │
│ │ 응답 내용... (스트리밍)                   │  │
│ └────────────────────────────────────────┘  │
│                                             │
├─────────────────────────────────────────────┤
│ 도구 상태: [Bash] 실행 중... ████░░░░░░       │ Length(1)
├─────────────────────────────────────────────┤
│ > 입력 영역 (멀티라인)                        │ Length(3)
└─────────────────────────────────────────────┘
│ [INSERT] | Tokens: 1234/8192 | gpt-4o       │ Length(1)
└─────────────────────────────────────────────┘
```

### 3.4 Alternate Screen 한계와 해결책

**문제점:**
Ratatui는 `EnterAlternateScreen`을 사용하여 별도 화면에서 렌더링.
이 모드에서는 **터미널 네이티브 스크롤백이 비활성화**됨.

```
일반 터미널:
┌─────────────────┐
│ 이전 출력들...   │ ← 스크롤백 버퍼 (터미널이 관리)
│ ...             │
├─────────────────┤
│ 현재 화면       │ ← 뷰포트
└─────────────────┘

Alternate Screen:
┌─────────────────┐
│                 │
│ TUI 전용 버퍼   │ ← 스크롤백 없음!
│                 │
└─────────────────┘
```

**해결책: OffscreenBuffer 패턴**

각 영역이 자체 버퍼를 유지하여 독립적인 스크롤백 제공:

```rust
/// 독립 스크롤 영역을 위한 오프스크린 버퍼
pub struct OffscreenBuffer {
    lines: Vec<Line<'static>>,
    max_lines: usize,
    scroll_offset: usize,
}

impl OffscreenBuffer {
    pub fn push_line(&mut self, line: Line<'static>) {
        self.lines.push(line);
        if self.lines.len() > self.max_lines {
            self.lines.remove(0);
        }
    }

    pub fn scroll_up(&mut self, amount: usize) { ... }
    pub fn scroll_down(&mut self, amount: usize) { ... }

    pub fn visible_lines(&self, height: usize) -> &[Line] {
        let start = self.scroll_offset;
        let end = (start + height).min(self.lines.len());
        &self.lines[start..end]
    }
}
```

**참고 프로젝트:**
- [r3bl_tui](https://github.com/r3bl-org/r3bl-open-core) - OffscreenBuffer 패턴
- [tui-term](https://github.com/a-kenji/tui-term) - PTY 에뮬레이션

### 3.5 다중 윈도우 레이아웃

```
┌─────────────────────────────────────────────────────────┐
│  Tab Bar: [Session 1] [Session 2*] [+]                  │
├────────────────────┬────────────────────────────────────┤
│ 도구 출력          │  메인 대화                          │
│ (OffscreenBuffer)  │  (OffscreenBuffer)                 │
│                    │                                    │
│ $ ls -la           │  ╭─ You ─────────────────────╮     │
│ total 48           │  │ 파일 목록 보여줘          │     │
│ drwxr-xr-x  5      │  ╰───────────────────────────╯     │
│ -rw-r--r--  1      │                                    │
│ ...                │  ╭─ Assistant ───────────────╮     │
│ [독립 스크롤]      │  │ 현재 디렉토리 내용입니다:  │     │
│                    │  │ ...                       │     │
│                    │  ╰───────────────────────────╯     │
├────────────────────┴────────────────────────────────────┤
│  > 입력창                                               │
├─────────────────────────────────────────────────────────┤
│  [INSERT] │ Tokens: 2,847/8,192 │ gpt-4o │ 🖱️ON        │
└─────────────────────────────────────────────────────────┘
```

**포커스 관리:**
- 마우스 클릭으로 패널 포커스 전환
- 포커스된 패널만 휠 스크롤 수신
- 포커스 표시: 테두리 색상 변경 (활성: Blue, 비활성: Gray)

### 3.6 Windows 호환성

**중요:** Windows에서 키 이벤트 중복 발생

```rust
use crossterm::event::{KeyEvent, KeyEventKind};

fn handle_key_event(key_event: KeyEvent) {
    // Windows에서 Press/Release 모두 발생 → Press만 처리
    #[cfg(target_os = "windows")]
    if key_event.kind != KeyEventKind::Press {
        return;
    }

    // 키 처리...
}
```

---

## 4. 아키텍처 설계

### 4.1 모듈 구조

```
crates/goose-cli/src/
├── session/
│   ├── mod.rs                 # 기존 유지 (세션 로직)
│   ├── output.rs              # 기존 유지 (basic 모드 호환)
│   ├── streaming_buffer.rs    # 기존 유지 (마크다운 파싱)
│   ├── input.rs               # 기존 유지 (슬래시 명령어)
│   │
│   └── tui/                   # 신규: Ratatui 모듈
│       ├── mod.rs             # TUI 앱 진입점
│       ├── app.rs             # App 상태 (Model)
│       ├── events.rs          # 이벤트 핸들링
│       ├── render.rs          # 렌더링 (View)
│       └── widgets/           # 커스텀 위젯
│           ├── chat_bubble.rs
│           ├── tool_status.rs
│           ├── input_area.rs
│           └── status_bar.rs
```

### 4.2 핵심 타입

```rust
// app.rs
pub struct TuiApp {
    // 상태
    pub messages: Vec<ChatMessage>,
    pub streaming_buffer: MarkdownBuffer,  // 기존 재사용
    pub input: TextArea<'static>,

    // UI 상태
    pub scroll_state: ScrollState,
    pub mode: InputMode,
    pub tool_status: Vec<ToolProgress>,

    // 채널
    pub agent_rx: mpsc::Receiver<AgentEvent>,
    pub action_tx: mpsc::Sender<Action>,
}

pub enum InputMode {
    Normal,
    Insert,
    Command,  // 슬래시 명령어 입력 중
}

pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Vec<ToolCall>,
}

// events.rs
pub enum Action {
    Quit,
    Submit,
    Scroll(ScrollDirection),
    SwitchMode(InputMode),
    AgentEvent(AgentEvent),
    Tick,
}
```

### 4.3 이벤트 루프

```rust
// mod.rs
pub async fn run_tui(agent: Agent, session_id: String) -> Result<()> {
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let (action_tx, mut action_rx) = mpsc::channel(100);
    let (agent_tx, agent_rx) = mpsc::channel(100);

    let mut app = TuiApp::new(agent_rx, action_tx.clone());

    // 에이전트 이벤트 전달 태스크
    tokio::spawn(async move {
        while let Some(event) = agent_stream.next().await {
            agent_tx.send(event).await.ok();
        }
    });

    // 키보드 이벤트 태스크
    let action_tx_clone = action_tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(event)) = reader.next().await {
            if let Event::Key(key) = event {
                handle_key_event(&action_tx_clone, key).await;
            }
        }
    });

    // 틱 타이머
    let mut tick_interval = interval(Duration::from_millis(100));

    // 메인 루프
    loop {
        tokio::select! {
            Some(action) = action_rx.recv() => {
                match app.update(action).await {
                    UpdateResult::Continue => {},
                    UpdateResult::Quit => break,
                }
            }
            _ = tick_interval.tick() => {
                // 에이전트 이벤트 확인
                while let Ok(event) = app.agent_rx.try_recv() {
                    app.handle_agent_event(event);
                }

                // 렌더링
                terminal.draw(|f| app.render(f))?;
            }
        }
    }

    Ok(())
}
```

### 4.4 렌더링

```rust
// render.rs
impl TuiApp {
    pub fn render(&self, frame: &mut Frame) {
        let chunks = Layout::vertical([
            Constraint::Length(2),   // 헤더
            Constraint::Min(10),     // 대화
            Constraint::Length(1),   // 도구 상태
            Constraint::Length(3),   // 입력
            Constraint::Length(1),   // 상태바
        ])
        .split(frame.size());

        self.render_header(frame, chunks[0]);
        self.render_conversation(frame, chunks[1]);
        self.render_tool_status(frame, chunks[2]);
        self.render_input(frame, chunks[3]);
        self.render_status_bar(frame, chunks[4]);
    }

    fn render_conversation(&self, frame: &mut Frame, area: Rect) {
        let messages: Vec<ListItem> = self.messages
            .iter()
            .map(|msg| {
                let style = match msg.role {
                    MessageRole::User => Style::default().fg(Color::Cyan),
                    MessageRole::Assistant => Style::default().fg(Color::Green),
                    MessageRole::System => Style::default().fg(Color::Yellow),
                };

                let content = Paragraph::new(msg.content.as_str())
                    .wrap(Wrap { trim: false })
                    .style(style);

                ListItem::new(content)
            })
            .collect();

        let list = List::new(messages)
            .block(Block::default().borders(Borders::ALL));

        frame.render_stateful_widget(list, area, &mut self.scroll_state);
    }
}
```

---

## 5. 기존 코드 재사용

### 5.1 MarkdownBuffer

**100% 재사용 가능**

```rust
// streaming_buffer.rs는 순수 Rust 문자열 처리
// output 레이어와 독립적
pub struct MarkdownBuffer {
    buffer: String,
}

impl MarkdownBuffer {
    pub fn push(&mut self, chunk: &str) -> Option<String>;
    pub fn flush(&mut self) -> String;
}
```

### 5.2 슬래시 명령어

**재사용** (input.rs의 파싱 로직)

```rust
// 기존: input.rs
pub enum InputResult {
    Message(String),
    Exit,
    ToggleTheme,
    // ...
}

// TUI: 동일한 enum 사용, UI만 다름
fn handle_command(input: &str) -> InputResult {
    // 기존 파싱 로직 재사용
}
```

### 5.3 테마 시스템

**확장 필요**

```rust
// 기존: bat 테마
pub enum Theme { Light, Dark, Ansi }

// TUI: Ratatui 스타일로 매핑
impl Theme {
    pub fn to_ratatui_style(&self) -> ThemeStyles {
        match self {
            Theme::Dark => ThemeStyles {
                user_msg: Style::default().fg(Color::Cyan),
                assistant_msg: Style::default().fg(Color::Green),
                code_bg: Color::Rgb(40, 44, 52),
                // ...
            },
            // ...
        }
    }
}
```

### 5.4 PreviewInspector Diff 연동

**파일:** `goose/src/preview/preview_inspector.rs`

PreviewInspector가 edit/write/undo 도구 호출 전에 diff preview를 생성.
TUI에서 이 diff를 렌더링해야 함.

**Diff 출력 형식:**
```
📝 Edit Preview

File: src/main.rs
Matches: 2 (replace all)

```diff
-old_string
+new_string
```
```

**TUI 렌더링:**
```rust
fn render_diff_block(content: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut in_diff = false;

    for line in content.lines() {
        if line.starts_with("```diff") {
            in_diff = true;
            continue;
        }
        if line == "```" && in_diff {
            in_diff = false;
            continue;
        }

        let styled_line = if in_diff {
            if line.starts_with('+') {
                Line::styled(line, Style::default().fg(Color::Green))
            } else if line.starts_with('-') {
                Line::styled(line, Style::default().fg(Color::Red))
            } else {
                Line::raw(line)
            }
        } else {
            Line::raw(line)
        };
        lines.push(styled_line);
    }
    lines
}
```

---

## 6. 구현 단계 (업데이트)

### Phase 5.1: 기반 구조 ✅ 완료

| 작업 | 설명 | 상태 |
|------|------|------|
| Cargo.toml | ratatui, tui-textarea, crossterm, tachyonfx | ✅ |
| tui/mod.rs | 기본 앱 구조 | ✅ |
| tui/app.rs | App 상태 정의 | ✅ |
| tui/events.rs | 이벤트 핸들링 | ✅ |
| tui/render.rs | 기본 렌더링 | ✅ |
| tui_session.rs | 에이전트 통합 | ✅ |

### Phase 5.2: 핵심 위젯 ✅ 완료

| 작업 | 설명 | 상태 |
|------|------|------|
| 대화 영역 | Paragraph + 스크롤 | ✅ |
| 입력 영역 | tui-textarea 통합 | ✅ |
| 상태바 | 모드, Tokens, 테마 표시 | ✅ |
| 메시지 스타일 | 구분선 + 마크다운 | ✅ |

### Phase 5.3: OffscreenBuffer 아키텍처 ✅ 완료

| 작업 | 설명 | 상태 |
|------|------|------|
| OffscreenBuffer | 독립 스크롤백 버퍼 구현 | ✅ |
| 다중 윈도우 | 도구출력 + 대화창 분리 | ✅ |
| 포커스 관리 | Tab 키 패널 포커스 전환 | ✅ |
| 마우스 라우팅 | 포커스된 패널만 휠 수신 | ✅ |
| 탭 바 | 다중 세션 전환 | ❌ |

### Phase 5.4: 렌더링 고도화 ✅ 완료

| 작업 | 설명 | 상태 |
|------|------|------|
| 마크다운 렌더링 | bold, italic, 리스트, 헤더 | ✅ |
| Diff Preview | +/- 색상 하이라이팅 | ✅ |
| 구문 강조 | 키워드 기반 (Rust, Python, JS, Go, Bash, SQL) | ✅ |
| 도구 상태 바 | 스피너 + 아이콘 | ✅ |

### Phase 5.5: 폴리싱 ⚠️ 부분

| 작업 | 설명 | 상태 |
|------|------|------|
| 애니메이션 모듈 | SpinnerFrames, TypingAnimation | ✅ |
| TachyonFX | 메시지 등장 애니메이션 | ❌ |
| 테마 전환 | F4 키바인딩 (Dark/Light) | ✅ |
| 헤더 | 세션 정보, 모델명 | ✅ |
| 도움말 팝업 | F1 키바인딩 도움말 | ✅ |
| Hints 편집 패널 | F5 토글 | ✅ |
| 감사 로그 패널 | F6 토글 | ✅ |
| 설정 패널 | F7 토글 (PII 화이트리스트, 카테고리, Mode 등) | ✅ |

### 키바인딩 요약

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
| Esc | Normal 모드 전환 |
| i/a | Insert 모드 전환 |
| ↑/↓ | 입력 히스토리 탐색 |
| Ctrl+↑/↓ | 스크롤 |
| PageUp/Down | 빠른 스크롤 |

---

## 7. 의존성 (Cargo.toml)

```toml
[dependencies]
# TUI 프레임워크
ratatui = { version = "0.28", features = ["crossterm"] }
crossterm = { version = "0.28", features = ["event-stream"] }

# 입력
tui-textarea = "0.6"

# 스크롤 (주의: OffscreenBuffer 직접 구현 필요)
tui-scrollview = "0.4"  # 단순 스크롤 위젯, 독립 스크롤백 아님

# 애니메이션
tachyonfx = "0.7"

# 구문 강조 (예정)
# syntect = "5.1"
```

**⚠️ 주의:**
- `tui-scrollview`는 단순 스크롤 위젯일 뿐, **독립 스크롤백을 제공하지 않음**
- 다중 윈도우 독립 스크롤을 위해서는 `OffscreenBuffer` 직접 구현 필요

---

## 8. 마이그레이션 전략

### 8.1 병행 운영

```rust
// main.rs
enum UiMode {
    Basic,   // 기존 console/bat
    Tui,     // 새로운 Ratatui
}

async fn run_session(mode: UiMode, ...) {
    match mode {
        UiMode::Basic => session::run_basic(...).await,
        UiMode::Tui => session::tui::run_tui(...).await,
    }
}
```

### 8.2 설정

```yaml
# config.yaml
ui:
  mode: tui  # basic | tui
  theme: dark
  vim_mode: true
```

### 8.3 환경변수

```powershell
$env:GOOSE_UI_MODE = "tui"
```

---

## 9. 리스크 및 완화

| 리스크 | 완화 방안 |
|--------|----------|
| Windows 호환성 | KeyEventKind::Press 필터링 |
| 터미널 크기 | 최소 80x24 요구, 동적 레이아웃 |
| 구문 강조 성능 | 증분 파싱, 캐싱 |
| 복잡한 마크다운 | 기존 MarkdownBuffer 재사용 |
| 폴백 필요 | basic 모드 유지 |

---

## 10. 참고 자료

### 공식 문서
- [Ratatui Documentation](https://ratatui.rs/)
- [Ratatui Examples](https://ratatui.rs/examples/)
- [Async Tutorial](https://ratatui.rs/tutorials/counter-async-app/)

### 참조 프로젝트
- [Tenere](https://github.com/pythops/tenere) - TUI for LLMs
- [Oatmeal](https://github.com/dustinblackman/oatmeal) - Terminal chat
- [llm-tui](https://github.com/guilhermeprokisch/llm-tui) - LLM interface

### 위젯 라이브러리
- [tui-textarea](https://github.com/rhysd/tui-textarea)
- [throbber-widgets-tui](https://github.com/arkbig/throbber-widgets-tui)
- [tui-scrollview](https://github.com/joshka/tui-scrollview)
