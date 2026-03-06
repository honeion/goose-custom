//! TUI 실행 루프
//!
//! Phase 5: Ratatui UI 고도화
//! crossterm 터미널 초기화 및 이벤트 루프

use std::io::{stdout, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use super::{
    app::TuiApp,
    events::{event_to_action, Action, UpdateResult},
};

/// TUI 러너
pub struct TuiRunner {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TuiRunner {
    /// 터미널 초기화
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    /// 터미널 정리
    pub fn cleanup(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    /// 메인 루프 실행 (데모 모드 - 에이전트 연결 없이)
    pub async fn run_demo(&mut self, model_name: String) -> Result<()> {
        let mut app = TuiApp::new("demo-session".to_string(), model_name);

        // 환영 메시지
        app.add_system_message("Goose Custom TUI에 오신 것을 환영합니다! (Phase 5 데모)".to_string());

        loop {
            // 렌더링
            self.terminal.draw(|frame| {
                app.render(frame);
            })?;

            // 이벤트 폴링 (50ms 타임아웃)
            if event::poll(Duration::from_millis(50))? {
                let event = event::read()?;

                if let Some(action) = event_to_action(event, &mut app) {
                    match app.update(action) {
                        UpdateResult::Quit => break,
                        UpdateResult::SendMessage(true) => {
                            // 데모 모드: 에코 응답
                            let last_msg = app.messages.last()
                                .map(|m| m.content.clone())
                                .unwrap_or_default();

                            app.start_assistant_streaming();
                            app.append_streaming_text(&format!("(데모 에코) {}", last_msg));
                            app.finish_streaming();
                        }
                        _ => {}
                    }
                }
            }

            // 틱 업데이트 (스피너 등)
            app.update(Action::Tick);
        }

        Ok(())
    }

    /// 메인 루프 실행 (에이전트 연결)
    pub async fn run_with_agent(
        &mut self,
        model_name: String,
        session_id: String,
        mut agent_rx: mpsc::Receiver<AgentMessage>,
        agent_tx: mpsc::Sender<String>,
    ) -> Result<()> {
        let mut app = TuiApp::new(session_id, model_name);

        loop {
            // 렌더링
            self.terminal.draw(|frame| {
                app.render(frame);
            })?;

            // 이벤트 처리
            tokio::select! {
                // 터미널 이벤트
                event_result = poll_terminal_event() => {
                    if let Some(event) = event_result? {
                        if let Some(action) = event_to_action(event, &mut app) {
                            match app.update(action) {
                                UpdateResult::Quit => break,
                                UpdateResult::SendMessage(true) => {
                                    // 마지막 메시지 전송
                                    if let Some(msg) = app.messages.last() {
                                        agent_tx.send(msg.content.clone()).await?;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // 에이전트 이벤트
                agent_event = agent_rx.recv() => {
                    if let Some(event) = agent_event {
                        match event {
                            AgentMessage::StreamStart => {
                                app.update(Action::AgentStreamStart);
                            }
                            AgentMessage::StreamChunk(text) => {
                                app.update(Action::AgentStreamChunk(text));
                            }
                            AgentMessage::StreamEnd => {
                                app.update(Action::AgentStreamEnd);
                            }
                            AgentMessage::ToolStart(name) => {
                                app.update(Action::AgentToolStart(name));
                            }
                            AgentMessage::ToolProgress(progress) => {
                                app.update(Action::AgentToolProgress(progress));
                            }
                            AgentMessage::ToolEnd => {
                                app.update(Action::AgentToolEnd);
                            }
                            AgentMessage::ToolError(msg) => {
                                app.update(Action::AgentToolError(msg));
                            }
                            AgentMessage::TokenUpdate(current, max) => {
                                app.update(Action::UpdateTokens(current, max));
                            }
                            AgentMessage::Disconnected => {
                                app.is_connected = false;
                            }
                        }
                    }
                }

                // 틱 (50ms)
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    app.update(Action::Tick);
                }
            }
        }

        Ok(())
    }
}

impl Drop for TuiRunner {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

/// 에이전트에서 TUI로 보내는 메시지
#[derive(Debug, Clone)]
pub enum AgentMessage {
    StreamStart,
    StreamChunk(String),
    StreamEnd,
    ToolStart(String),
    ToolProgress(f64),
    ToolEnd,
    ToolError(String),
    TokenUpdate(u32, u32),
    Disconnected,
}

/// 터미널 이벤트 비동기 폴링
async fn poll_terminal_event() -> Result<Option<Event>> {
    // crossterm의 event::poll은 blocking이므로 tokio::task::spawn_blocking 사용
    let result = tokio::task::spawn_blocking(|| -> std::io::Result<Option<Event>> {
        if event::poll(Duration::from_millis(10))? {
            Ok(Some(event::read()?))
        } else {
            Ok(None)
        }
    })
    .await??;

    Ok(result)
}

/// TUI 데모 실행 (테스트용)
pub async fn run_tui_demo(model_name: &str) -> Result<()> {
    let mut runner = TuiRunner::new()?;
    let result = runner.run_demo(model_name.to_string()).await;
    runner.cleanup()?;
    result
}
