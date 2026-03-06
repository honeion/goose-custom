//! 이벤트 핸들링
//!
//! TEA (Elm Architecture) 패턴의 Update 역할
//! Phase 5: Ratatui UI 고도화

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};

use super::app::{InputMode, TuiApp};
use super::offscreen_buffer::PanelId;

/// 앱 액션 (Update 함수에 전달)
#[derive(Debug, Clone)]
pub enum Action {
    /// 종료
    Quit,
    /// 메시지 전송
    Submit,
    /// 스크롤
    ScrollUp(u16),
    ScrollDown(u16),
    ScrollToTop,
    ScrollToBottom,
    /// 모드 전환
    SwitchMode(InputMode),
    /// 도움말 토글
    ToggleHelp,
    /// 테마 토글 (추후)
    ToggleTheme,
    /// 마우스 캡처 토글 (F2)
    ToggleMouseCapture,
    /// 틱 (애니메이션)
    Tick,
    /// 에이전트 이벤트 (스트리밍 등)
    AgentStreamStart,
    AgentStreamChunk(String),
    AgentStreamEnd,
    AgentToolStart(String),
    AgentToolProgress(f64),
    AgentToolEnd,
    AgentToolError(String),
    /// 토큰 사용량 업데이트
    UpdateTokens(u32, u32),
    /// 리사이즈
    Resize(u16, u16),
    /// 패널 포커스 전환
    FocusPanel(PanelId),
    /// 도구 출력 패널 토글 (F3)
    ToggleToolPanel,
    /// 마우스 클릭 (패널 포커스용)
    MouseClick(u16, u16),
    /// 패널 포커스 토글 (Tab)
    TogglePanelFocus,
    /// 히스토리 이전 (↑)
    HistoryPrev,
    /// 히스토리 다음 (↓)
    HistoryNext,
}

/// 업데이트 결과
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateResult {
    /// 계속 실행
    Continue,
    /// 종료
    Quit,
    /// 메시지 전송 필요
    SendMessage(bool),
}

impl<'a> TuiApp<'a> {
    /// 액션 처리 (Update 함수)
    pub fn update(&mut self, action: Action) -> UpdateResult {
        match action {
            Action::Quit => {
                self.should_quit = true;
                UpdateResult::Quit
            }

            Action::Submit => {
                let content = self.take_input();
                if !content.trim().is_empty() {
                    self.add_user_message(content);
                    return UpdateResult::SendMessage(true);
                }
                UpdateResult::Continue
            }

            Action::ScrollUp(amount) => {
                // 포커스된 패널만 스크롤
                self.panels.scroll_up(amount as usize);
                // 레거시 호환: 대화창 포커스일 때만
                if self.panels.focused == super::offscreen_buffer::PanelId::Conversation {
                    self.scroll_state.scroll_up(amount);
                }
                UpdateResult::Continue
            }

            Action::ScrollDown(amount) => {
                self.panels.scroll_down(amount as usize);
                if self.panels.focused == super::offscreen_buffer::PanelId::Conversation {
                    self.scroll_state.scroll_down(amount);
                }
                UpdateResult::Continue
            }

            Action::ScrollToTop => {
                self.panels.scroll_to_top();
                if self.panels.focused == super::offscreen_buffer::PanelId::Conversation {
                    self.scroll_state.scroll_to_top();
                }
                UpdateResult::Continue
            }

            Action::ScrollToBottom => {
                self.panels.scroll_to_bottom();
                if self.panels.focused == super::offscreen_buffer::PanelId::Conversation {
                    self.scroll_state.scroll_to_bottom();
                }
                UpdateResult::Continue
            }

            Action::FocusPanel(panel) => {
                self.panels.set_focus(panel);
                UpdateResult::Continue
            }

            Action::ToggleToolPanel => {
                self.panels.toggle_tool_output();
                UpdateResult::Continue
            }

            Action::TogglePanelFocus => {
                self.panels.toggle_focus();
                UpdateResult::Continue
            }

            Action::HistoryPrev => {
                self.history_prev();
                UpdateResult::Continue
            }

            Action::HistoryNext => {
                self.history_next();
                UpdateResult::Continue
            }

            Action::MouseClick(_x, _y) => {
                // TODO: 좌표로 패널 판별하여 포커스 전환
                UpdateResult::Continue
            }

            Action::SwitchMode(mode) => {
                self.input_mode = mode;
                UpdateResult::Continue
            }

            Action::ToggleHelp => {
                self.show_help = !self.show_help;
                UpdateResult::Continue
            }

            Action::ToggleTheme => {
                self.toggle_theme();
                UpdateResult::Continue
            }

            Action::ToggleMouseCapture => {
                self.toggle_mouse_capture();
                UpdateResult::Continue
            }

            Action::Tick => {
                self.tick();
                UpdateResult::Continue
            }

            Action::AgentStreamStart => {
                self.start_assistant_streaming();
                UpdateResult::Continue
            }

            Action::AgentStreamChunk(text) => {
                self.append_streaming_text(&text);
                UpdateResult::Continue
            }

            Action::AgentStreamEnd => {
                self.finish_streaming();
                UpdateResult::Continue
            }

            Action::AgentToolStart(name) => {
                self.start_tool(name);
                UpdateResult::Continue
            }

            Action::AgentToolProgress(progress) => {
                self.update_tool_progress(progress);
                UpdateResult::Continue
            }

            Action::AgentToolEnd => {
                self.finish_tool();
                UpdateResult::Continue
            }

            Action::AgentToolError(message) => {
                self.error_tool(message);
                UpdateResult::Continue
            }

            Action::UpdateTokens(_current, max) => {
                // 토큰 업데이트는 add_input/add_output으로 처리
                // max 값만 업데이트
                self.token_usage.max = max;
                UpdateResult::Continue
            }

            Action::Resize(_, _) => {
                // 리사이즈는 렌더링 시 자동 처리
                UpdateResult::Continue
            }
        }
    }

    /// 키 이벤트를 액션으로 변환
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Option<Action> {
        // Windows에서 Press만 처리
        #[cfg(target_os = "windows")]
        if key.kind != KeyEventKind::Press {
            return None;
        }

        // 도움말이 열려있을 때
        if self.show_help {
            return match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                    Some(Action::ToggleHelp)
                }
                _ => None,
            };
        }

        match self.input_mode {
            InputMode::Insert => self.handle_insert_mode_key(key),
            InputMode::Normal => self.handle_normal_mode_key(key),
            InputMode::Command => self.handle_command_mode_key(key),
        }
    }

    /// Insert 모드 키 처리
    fn handle_insert_mode_key(&mut self, key: KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            // 종료
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),

            // 전송
            (KeyCode::Enter, KeyModifiers::NONE) => Some(Action::Submit),

            // 새 줄 (Shift+Enter)
            (KeyCode::Enter, KeyModifiers::SHIFT) => {
                self.input.insert_newline();
                None
            }

            // Normal 모드로 전환
            (KeyCode::Esc, _) => Some(Action::SwitchMode(InputMode::Normal)),

            // 히스토리 탐색 (↑/↓)
            (KeyCode::Up, KeyModifiers::NONE) => Some(Action::HistoryPrev),
            (KeyCode::Down, KeyModifiers::NONE) => Some(Action::HistoryNext),

            // 스크롤 (Ctrl+화살표, PageUp/Down)
            (KeyCode::Up, KeyModifiers::CONTROL) => Some(Action::ScrollUp(1)),
            (KeyCode::Down, KeyModifiers::CONTROL) => Some(Action::ScrollDown(1)),
            (KeyCode::PageUp, _) => Some(Action::ScrollUp(10)),
            (KeyCode::PageDown, _) => Some(Action::ScrollDown(10)),

            // 마우스 캡처 토글
            (KeyCode::F(2), _) => Some(Action::ToggleMouseCapture),

            // 도구 출력 패널 토글
            (KeyCode::F(3), _) => Some(Action::ToggleToolPanel),

            // 패널 포커스 전환 (Tab)
            (KeyCode::Tab, _) => Some(Action::TogglePanelFocus),

            // 도움말
            (KeyCode::F(1), _) => Some(Action::ToggleHelp),

            // 테마 토글
            (KeyCode::F(4), _) => Some(Action::ToggleTheme),

            // 기본 입력은 TextArea가 처리
            _ => {
                self.input.input(key);
                None
            }
        }
    }

    /// Normal 모드 키 처리 (Vim 스타일)
    fn handle_normal_mode_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            // 종료
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Quit)
            }

            // Insert 모드
            KeyCode::Char('i') => Some(Action::SwitchMode(InputMode::Insert)),
            KeyCode::Char('a') => {
                // Append mode: 커서 오른쪽으로 이동 후 Insert
                self.input.move_cursor(tui_textarea::CursorMove::Forward);
                Some(Action::SwitchMode(InputMode::Insert))
            }

            // 스크롤
            KeyCode::Char('j') | KeyCode::Down => Some(Action::ScrollDown(1)),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::ScrollUp(1)),
            KeyCode::Char('g') => Some(Action::ScrollToTop),
            KeyCode::Char('G') => Some(Action::ScrollToBottom),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::ScrollDown(10))
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::ScrollUp(10))
            }

            // Command 모드
            KeyCode::Char('/') | KeyCode::Char(':') => {
                Some(Action::SwitchMode(InputMode::Command))
            }

            // 기능키
            KeyCode::F(1) => Some(Action::ToggleHelp),
            KeyCode::F(2) => Some(Action::ToggleMouseCapture),
            KeyCode::F(3) => Some(Action::ToggleToolPanel),
            KeyCode::F(4) => Some(Action::ToggleTheme),

            _ => None,
        }
    }

    /// Command 모드 키 처리
    fn handle_command_mode_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            // 취소
            KeyCode::Esc => Some(Action::SwitchMode(InputMode::Normal)),

            // TODO: 슬래시 명령어 처리
            KeyCode::Enter => {
                // 명령어 실행 후 Normal 모드로
                Some(Action::SwitchMode(InputMode::Normal))
            }

            // 입력
            _ => {
                self.input.input(key);
                None
            }
        }
    }
}

/// crossterm 이벤트를 Action으로 변환
pub fn event_to_action(event: Event, app: &mut TuiApp) -> Option<Action> {
    match event {
        Event::Key(key) => app.handle_key_event(key),
        Event::Resize(w, h) => Some(Action::Resize(w, h)),
        Event::FocusGained | Event::FocusLost => None,
        Event::Mouse(mouse) => {
            // 마우스 캡처 OFF일 때는 마우스 이벤트 무시
            if !app.mouse_capture {
                return None;
            }
            match mouse.kind {
                MouseEventKind::ScrollUp => Some(Action::ScrollUp(3)),
                MouseEventKind::ScrollDown => Some(Action::ScrollDown(3)),
                // 왼쪽 클릭으로 패널 포커스 전환
                MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                    Some(Action::MouseClick(mouse.column, mouse.row))
                }
                _ => None, // 드래그 등은 무시 (Shift+드래그로 텍스트 선택)
            }
        }
        Event::Paste(_) => None, // TODO: 붙여넣기 지원
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_quit() {
        let mut app = TuiApp::new("test".to_string(), "gpt-4o".to_string());
        let result = app.update(Action::Quit);
        assert_eq!(result, UpdateResult::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_update_scroll() {
        let mut app = TuiApp::new("test".to_string(), "gpt-4o".to_string());
        app.scroll_state.total_lines = 100;
        app.scroll_state.viewport_height = 20;

        app.update(Action::ScrollDown(5));
        assert_eq!(app.scroll_state.offset, 5);

        app.update(Action::ScrollUp(3));
        assert_eq!(app.scroll_state.offset, 2);
    }

    #[test]
    fn test_mode_switch() {
        let mut app = TuiApp::new("test".to_string(), "gpt-4o".to_string());
        assert_eq!(app.input_mode, InputMode::Insert);

        app.update(Action::SwitchMode(InputMode::Normal));
        assert_eq!(app.input_mode, InputMode::Normal);

        app.update(Action::SwitchMode(InputMode::Command));
        assert_eq!(app.input_mode, InputMode::Command);
    }
}
