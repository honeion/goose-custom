//! TUI 앱 상태 (Model)
//!
//! TEA (Elm Architecture) 패턴의 Model 역할
//! Phase 5: Ratatui UI 고도화

use std::time::Instant;

use chrono::{DateTime, Local};
use rmcp::model::Role;
use tui_textarea::TextArea;

use super::animation::SpinnerFrames;
use super::markdown::{DiffStyles, is_diff_preview, parse_diff};
use super::offscreen_buffer::{PanelId, PanelManager};
use super::theme::Theme;

/// 입력 모드 (Vim 스타일)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    /// 일반 모드 - 네비게이션
    Normal,
    /// 입력 모드 - 텍스트 입력
    #[default]
    Insert,
    /// 명령 모드 - 슬래시 명령어
    Command,
}

impl InputMode {
    pub fn label(&self) -> &'static str {
        match self {
            InputMode::Normal => "NORMAL",
            InputMode::Insert => "INSERT",
            InputMode::Command => "COMMAND",
        }
    }
}

/// 메시지 역할
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl From<Role> for MessageRole {
    fn from(role: Role) -> Self {
        match role {
            Role::User => MessageRole::User,
            Role::Assistant => MessageRole::Assistant,
        }
    }
}

/// 채팅 메시지
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Local>,
    pub is_streaming: bool,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            timestamp: Local::now(),
            is_streaming: false,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: Local::now(),
            is_streaming: false,
        }
    }

    pub fn assistant_streaming() -> Self {
        Self {
            role: MessageRole::Assistant,
            content: String::new(),
            timestamp: Local::now(),
            is_streaming: true,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            timestamp: Local::now(),
            is_streaming: false,
        }
    }

    /// 스트리밍 메시지에 텍스트 추가
    pub fn append(&mut self, text: &str) {
        self.content.push_str(text);
    }

    /// 스트리밍 완료
    pub fn finish_streaming(&mut self) {
        self.is_streaming = false;
    }
}

/// 도구 실행 상태
#[derive(Debug, Clone)]
pub enum ToolStatus {
    /// 대기 중 (Thinking)
    Thinking,
    /// 실행 중
    Running {
        name: String,
        progress: Option<f64>,
        started_at: Instant,
    },
    /// 완료
    Completed {
        name: String,
        duration_ms: u64,
    },
    /// 에러
    Error {
        name: String,
        message: String,
    },
    /// 없음
    None,
}

impl Default for ToolStatus {
    fn default() -> Self {
        Self::None
    }
}

/// 스크롤 상태
#[derive(Debug, Clone)]
pub struct ScrollState {
    pub offset: u16,
    pub total_lines: u16,
    pub viewport_height: u16,
    pub auto_scroll: bool, // 자동 스크롤 활성화 여부
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: 0,
            total_lines: 0,
            viewport_height: 0,
            auto_scroll: true,
        }
    }
}

impl ScrollState {
    pub fn scroll_up(&mut self, amount: u16) {
        self.auto_scroll = false; // 위로 스크롤하면 자동 스크롤 끄기
        self.offset = self.offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.offset = self.offset.saturating_add(amount);
        // offset이 충분히 크면 auto_scroll이 알아서 맨 아래 표시
    }

    pub fn scroll_to_bottom(&mut self) {
        self.auto_scroll = true; // 맨 아래로 가면 자동 스크롤 켜기
    }

    pub fn scroll_to_top(&mut self) {
        self.offset = 0;
        self.auto_scroll = false;
    }

    /// 스크롤 퍼센트 (0.0 ~ 1.0)
    pub fn scroll_percent(&self) -> f64 {
        if self.total_lines <= self.viewport_height {
            return 0.0;
        }
        let max_offset = self.total_lines.saturating_sub(self.viewport_height);
        if max_offset == 0 {
            return 0.0;
        }
        self.offset as f64 / max_offset as f64
    }
}

/// 토큰 사용량 (추정)
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub max: u32,
}

impl TokenUsage {
    pub fn new(max: u32) -> Self {
        Self { input_tokens: 0, output_tokens: 0, max }
    }

    pub fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    pub fn percent(&self) -> f64 {
        if self.max == 0 {
            return 0.0;
        }
        (self.total() as f64 / self.max as f64).min(1.0)
    }

    pub fn is_warning(&self) -> bool {
        self.percent() > 0.7
    }

    pub fn is_critical(&self) -> bool {
        self.percent() > 0.9
    }

    /// 텍스트 길이 기반 토큰 추정 (대략 4글자 = 1토큰)
    pub fn estimate_tokens(text: &str) -> u32 {
        (text.chars().count() / 4).max(1) as u32
    }

    /// 입력 토큰 추가
    pub fn add_input(&mut self, text: &str) {
        self.input_tokens += Self::estimate_tokens(text);
    }

    /// 출력 토큰 추가
    pub fn add_output(&mut self, text: &str) {
        self.output_tokens += Self::estimate_tokens(text);
    }

    /// 포맷된 문자열 (예: "1.2k/128k" 또는 "1.2k")
    pub fn display(&self) -> String {
        let total = self.total();
        if total == 0 {
            "-".to_string()
        } else if self.max > 0 {
            format!("{:.1}k/{:.0}k", total as f64 / 1000.0, self.max as f64 / 1000.0)
        } else {
            format!("{:.1}k", total as f64 / 1000.0)
        }
    }
}

/// TUI 앱 상태 (Model)
pub struct TuiApp<'a> {
    // 세션 정보
    pub session_id: String,
    pub model_name: String,
    pub is_connected: bool,

    // 메시지
    pub messages: Vec<ChatMessage>,

    // 입력
    pub input: TextArea<'a>,
    pub input_mode: InputMode,

    // 명령어 히스토리
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub history_temp: String, // 히스토리 탐색 중 임시 저장

    // 상태
    pub tool_status: ToolStatus,
    pub scroll_state: ScrollState,  // 레거시 (하위 호환성)
    pub token_usage: TokenUsage,

    // 패널 관리 (OffscreenBuffer 기반)
    pub panels: PanelManager,

    // UI
    pub theme: Theme,
    pub theme_name: super::theme::ThemeName,
    pub should_quit: bool,
    pub show_help: bool,
    pub debug_mode: bool,
    pub mouse_capture: bool, // 마우스 캡처 상태 (F2로 토글)

    // 애니메이션
    pub spinner: SpinnerFrames,
    pub last_tick: Instant,
}

impl<'a> TuiApp<'a> {
    pub fn new(session_id: String, model_name: String) -> Self {
        let mut input = TextArea::default();
        input.set_cursor_line_style(ratatui::style::Style::default());
        input.set_placeholder_text("여기에 질문을 입력하세요...");

        Self {
            session_id,
            model_name,
            is_connected: true,

            messages: Vec::new(),

            input,
            input_mode: InputMode::Insert,

            input_history: Vec::new(),
            history_index: None,
            history_temp: String::new(),

            tool_status: ToolStatus::None,
            scroll_state: ScrollState::default(),
            token_usage: TokenUsage::new(128000), // 기본 128k 컨텍스트

            panels: PanelManager::new(),

            theme: Theme::default(),
            theme_name: super::theme::ThemeName::default(),
            should_quit: false,
            show_help: false,
            debug_mode: false,
            mouse_capture: true, // 기본: 마우스 캡처 ON (휠 스크롤 가능)

            spinner: SpinnerFrames::new(),
            last_tick: Instant::now(),
        }
    }

    /// 현재 포커스된 패널 ID
    pub fn focused_panel(&self) -> PanelId {
        self.panels.focused
    }

    /// 패널 포커스 설정
    pub fn set_panel_focus(&mut self, panel: PanelId) {
        self.panels.set_focus(panel);
    }

    /// 도구 출력 패널 토글
    pub fn toggle_tool_panel(&mut self) {
        self.panels.toggle_tool_output();
    }

    /// 도구 출력에 라인 추가
    pub fn push_tool_output(&mut self, line: ratatui::text::Line<'static>) {
        self.panels.tool_output.push_line(line);
    }

    /// 도구 출력에 텍스트 추가 (diff 자동 감지 및 하이라이팅)
    pub fn push_tool_text(&mut self, text: &str) {
        if is_diff_preview(text) {
            // diff 프리뷰인 경우 스타일 적용
            let styles = DiffStyles::default();
            let lines = parse_diff(text, &styles);
            self.panels.tool_output.push_lines(lines);
        } else {
            self.panels.tool_output.push_text(text);
        }
    }

    /// 마우스 캡처 토글
    pub fn toggle_mouse_capture(&mut self) {
        self.mouse_capture = !self.mouse_capture;
    }

    /// 테마 토글
    pub fn toggle_theme(&mut self) {
        self.theme_name = self.theme_name.next();
        self.theme = Theme::from_name(self.theme_name);
    }

    /// 사용자 메시지 추가
    pub fn add_user_message(&mut self, content: String) {
        self.token_usage.add_input(&content);
        self.messages.push(ChatMessage::user(content));
        self.scroll_state.scroll_to_bottom();
    }

    /// 어시스턴트 스트리밍 시작
    pub fn start_assistant_streaming(&mut self) {
        self.messages.push(ChatMessage::assistant_streaming());
        self.tool_status = ToolStatus::Thinking;
    }

    /// 스트리밍 텍스트 추가
    pub fn append_streaming_text(&mut self, text: &str) {
        if let Some(msg) = self.messages.last_mut() {
            if msg.is_streaming {
                msg.append(text);
                self.scroll_state.scroll_to_bottom();
            }
        }
    }

    /// 스트리밍 완료
    pub fn finish_streaming(&mut self) {
        if let Some(msg) = self.messages.last_mut() {
            // 출력 토큰 추정
            self.token_usage.add_output(&msg.content);
            msg.finish_streaming();
        }
        self.tool_status = ToolStatus::None;
    }

    /// 시스템 메시지 추가
    pub fn add_system_message(&mut self, content: String) {
        self.messages.push(ChatMessage::system(content));
    }

    /// 도구 실행 시작
    pub fn start_tool(&mut self, name: String) {
        self.tool_status = ToolStatus::Running {
            name,
            progress: None,
            started_at: Instant::now(),
        };
    }

    /// 도구 진행률 업데이트
    pub fn update_tool_progress(&mut self, progress: f64) {
        if let ToolStatus::Running {
            progress: ref mut p,
            ..
        } = self.tool_status
        {
            *p = Some(progress);
        }
    }

    /// 도구 완료
    pub fn finish_tool(&mut self) {
        if let ToolStatus::Running {
            name, started_at, ..
        } = &self.tool_status
        {
            self.tool_status = ToolStatus::Completed {
                name: name.clone(),
                duration_ms: started_at.elapsed().as_millis() as u64,
            };
        }
    }

    /// 도구 에러
    pub fn error_tool(&mut self, message: String) {
        if let ToolStatus::Running { name, .. } = &self.tool_status {
            self.tool_status = ToolStatus::Error {
                name: name.clone(),
                message,
            };
        }
    }

    /// 입력 내용 가져오기 및 초기화
    pub fn take_input(&mut self) -> String {
        let lines: Vec<String> = self.input.lines().iter().map(|s| s.to_string()).collect();
        let content = lines.join("\n");

        // 히스토리에 추가 (빈 문자열 제외, 중복 제외)
        if !content.trim().is_empty() {
            if self.input_history.last() != Some(&content) {
                self.input_history.push(content.clone());
            }
        }
        self.history_index = None;
        self.history_temp.clear();

        self.input = TextArea::default();
        self.input.set_placeholder_text("여기에 질문을 입력하세요...");
        content
    }

    /// 히스토리 위로 (이전 명령어)
    pub fn history_prev(&mut self) {
        if self.input_history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // 현재 입력 임시 저장
                let lines: Vec<String> = self.input.lines().iter().map(|s| s.to_string()).collect();
                self.history_temp = lines.join("\n");
                // 마지막 히스토리로 이동
                let idx = self.input_history.len() - 1;
                self.history_index = Some(idx);
                self.set_input_text(&self.input_history[idx].clone());
            }
            Some(idx) if idx > 0 => {
                // 이전 히스토리로 이동
                let new_idx = idx - 1;
                self.history_index = Some(new_idx);
                self.set_input_text(&self.input_history[new_idx].clone());
            }
            _ => {} // 맨 처음이면 아무것도 안 함
        }
    }

    /// 히스토리 아래로 (다음 명령어)
    pub fn history_next(&mut self) {
        match self.history_index {
            Some(idx) => {
                if idx + 1 < self.input_history.len() {
                    // 다음 히스토리로 이동
                    let new_idx = idx + 1;
                    self.history_index = Some(new_idx);
                    self.set_input_text(&self.input_history[new_idx].clone());
                } else {
                    // 히스토리 끝 -> 임시 저장된 입력으로 복원
                    self.history_index = None;
                    let temp = self.history_temp.clone();
                    self.set_input_text(&temp);
                }
            }
            None => {} // 이미 현재 입력 상태
        }
    }

    /// 입력창 텍스트 설정
    fn set_input_text(&mut self, text: &str) {
        self.input = TextArea::default();
        self.input.set_placeholder_text("여기에 질문을 입력하세요...");
        for line in text.lines() {
            self.input.insert_str(line);
            self.input.insert_newline();
        }
        // 마지막 줄바꿈 제거
        if text.lines().count() > 0 {
            self.input.delete_char();
        }
    }

    /// 스피너 프레임 업데이트 (자동 타이밍)
    pub fn tick(&mut self) {
        // SpinnerFrames가 자체적으로 타이밍 처리
        self.last_tick = Instant::now();
    }

    /// 현재 스피너 문자
    pub fn spinner_char(&mut self) -> &'static str {
        self.spinner.current_frame()
    }

    /// 스크롤 상태 업데이트 (렌더링 전 호출)
    pub fn update_scroll_metrics(&mut self, total_lines: u16, viewport_height: u16) {
        self.scroll_state.total_lines = total_lines;
        self.scroll_state.viewport_height = viewport_height;
    }

    /// 메시지 총 라인 수 계산 (렌더링과 동일한 로직)
    pub fn calculate_total_lines(&self, width: u16) -> u16 {
        let mut total = 0u16;
        let content_width = (width as usize).saturating_sub(4).max(1);

        for msg in &self.messages {
            // 헤더 1줄
            total += 1;

            // 내용 (줄바꿈 + 텍스트 래핑) - textwrap_simple과 동일한 로직
            for line in msg.content.lines() {
                if line.is_empty() {
                    total += 1;
                } else {
                    // 단어 단위로 래핑했을 때 예상 줄 수
                    let words: Vec<&str> = line.split_whitespace().collect();
                    if words.is_empty() {
                        total += 1;
                    } else {
                        let mut current_len = 0usize;
                        let mut line_count = 1u16;
                        for word in words {
                            if current_len == 0 {
                                current_len = word.len();
                            } else if current_len + 1 + word.len() <= content_width {
                                current_len += 1 + word.len();
                            } else {
                                line_count += 1;
                                current_len = word.len();
                            }
                        }
                        total += line_count;
                    }
                }
            }

            // 스트리밍 커서
            if msg.is_streaming {
                total += 1;
            }
            // 여백
            total += 1;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let app = TuiApp::new("test-session".to_string(), "gpt-4o".to_string());
        assert_eq!(app.session_id, "test-session");
        assert_eq!(app.model_name, "gpt-4o");
        assert!(app.messages.is_empty());
    }

    #[test]
    fn test_message_streaming() {
        let mut app = TuiApp::new("test".to_string(), "gpt-4o".to_string());
        app.start_assistant_streaming();

        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].is_streaming);

        app.append_streaming_text("Hello, ");
        app.append_streaming_text("World!");

        assert_eq!(app.messages[0].content, "Hello, World!");

        app.finish_streaming();
        assert!(!app.messages[0].is_streaming);
    }

    #[test]
    fn test_scroll_state() {
        let mut scroll = ScrollState {
            offset: 10,
            total_lines: 100,
            viewport_height: 20,
        };

        scroll.scroll_up(5);
        assert_eq!(scroll.offset, 5);

        scroll.scroll_down(10);
        assert_eq!(scroll.offset, 15);

        scroll.scroll_to_bottom();
        assert_eq!(scroll.offset, 80); // 100 - 20

        scroll.scroll_to_top();
        assert_eq!(scroll.offset, 0);
    }
}
