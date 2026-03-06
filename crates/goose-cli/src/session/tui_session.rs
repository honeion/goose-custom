//! TUI 세션 통합
//!
//! Phase 5.2: 에이전트 연동
//! CliSession과 TUI를 연결하는 브릿지

use std::time::Duration;

use anyhow::Result;
use crossterm::event;
use futures::StreamExt;
use goose::agents::AgentEvent;
use goose::conversation::message::Message;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{stdout, Stdout};
use crossterm::{
    execute,
    event::{EnableMouseCapture, DisableMouseCapture},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tokio_util::sync::CancellationToken;

use super::tui::{
    app::TuiApp,
    events::{event_to_action, Action, UpdateResult},
};
use crate::CliSession;

/// TUI 세션 실행
pub async fn run_tui_session(session: &mut CliSession) -> Result<()> {
    // 터미널 초기화
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_tui_loop(&mut terminal, session).await;

    // 터미널 정리 (에러 발생해도 반드시 실행)
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture);
    let _ = terminal.show_cursor();

    // 에러 있으면 출력
    if let Err(ref e) = result {
        eprintln!("\n[TUI Error] {}", e);
        eprintln!("Backtrace: {:?}", e);
    }

    result
}

/// TUI 메인 루프
async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    session: &mut CliSession,
) -> Result<()> {
    let provider = std::env::var("GOOSE_PROVIDER").unwrap_or_else(|_| "openai".to_string());
    let model = std::env::var("GOOSE_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());
    let model_name = format!("{} {}", provider, model);
    let mut app = TuiApp::new(session.session_id.clone(), model_name);

    // 환영 메시지
    app.add_system_message("Goose Custom TUI 세션이 시작되었습니다.".to_string());

    // 기존 메시지 히스토리 로드
    for msg in &session.messages {
        match msg.role {
            rmcp::model::Role::User => {
                app.messages.push(super::tui::ChatMessage::user(msg.as_concat_text()));
            }
            rmcp::model::Role::Assistant => {
                app.messages.push(super::tui::ChatMessage::assistant(msg.as_concat_text()));
            }
        }
    }

    loop {
        // 스크롤 메트릭 업데이트 후 렌더링
        let area = terminal.get_frame().area();
        let total_lines = app.calculate_total_lines(area.width);
        let viewport_height = area.height.saturating_sub(6); // 헤더, 도구상태, 입력, 상태바 제외
        app.update_scroll_metrics(total_lines, viewport_height);

        terminal.draw(|frame| {
            app.render(frame);
        })?;

        // 이벤트 처리
        if event::poll(Duration::from_millis(50))? {
            let event = event::read()?;

            if let Some(action) = event_to_action(event, &mut app) {
                // 마우스 캡처 토글 특별 처리
                if matches!(action, Action::ToggleMouseCapture) {
                    app.toggle_mouse_capture();
                    if app.mouse_capture {
                        let _ = execute!(std::io::stdout(), EnableMouseCapture);
                    } else {
                        let _ = execute!(std::io::stdout(), DisableMouseCapture);
                    }
                    continue;
                }

                match app.update(action) {
                    UpdateResult::Quit => break,
                    UpdateResult::SendMessage(true) => {
                        // 마지막 사용자 메시지 가져오기
                        if let Some(user_msg) = app.messages.last() {
                            let content = user_msg.content.clone();

                            // 슬래시 명령어 처리
                            if content.starts_with('/') {
                                handle_slash_command(&content, &mut app);
                                terminal.draw(|frame| app.render(frame))?;
                                // /quit, /exit 처리
                                if app.should_quit {
                                    break;
                                }
                                continue;
                            }

                            // 에이전트에 메시지 전송 및 응답 처리
                            process_agent_message(
                                session,
                                &content,
                                &mut app,
                                terminal,
                            ).await?;
                        }
                    }
                    _ => {}
                }
            }
        }

        // 틱 업데이트
        app.update(Action::Tick);
    }

    Ok(())
}

/// 에이전트 메시지 처리
async fn process_agent_message(
    session: &mut CliSession,
    content: &str,
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<()> {
    // 사용자 메시지 생성
    let user_message = Message::user().with_text(content);
    session.messages.push(user_message.clone());

    // 스트리밍 시작
    app.start_assistant_streaming();
    app.tool_status = super::tui::ToolStatus::Thinking;

    // 렌더링 업데이트
    terminal.draw(|frame| app.render(frame))?;

    // 세션 설정
    let session_config = session.get_session_config();
    let cancel_token = CancellationToken::new();

    // 에이전트 스트림 시작
    let mut stream = session
        .agent
        .reply(user_message, session_config, Some(cancel_token.clone()))
        .await?;

    // 스트림 처리 - 이벤트와 스트림을 번갈아 처리
    loop {
        // 1. 먼저 대기중인 이벤트 모두 처리 (논블로킹)
        while event::poll(Duration::from_millis(0))? {
            let evt = event::read()?;
            if let Some(action) = event_to_action(evt, app) {
                match action {
                    Action::Quit => {
                        cancel_token.cancel();
                        app.finish_streaming();
                        return Ok(());
                    }
                    Action::ScrollUp(_) | Action::ScrollDown(_)
                    | Action::ScrollToTop | Action::ScrollToBottom => {
                        app.update(action);
                    }
                    _ => {}
                }
            }
        }

        // 2. 틱 업데이트
        app.update(Action::Tick);

        // 3. 렌더링
        terminal.draw(|frame| app.render(frame))?;

        // 4. 스트림에서 데이터 가져오기 (짧은 타임아웃)
        tokio::select! {
            result = stream.next() => {
                match result {
                    Some(Ok(AgentEvent::Message(message))) => {
                        // 텍스트 추출 및 대화창에 추가
                        let text = extract_text_content(&message);
                        if let Some(last_msg) = app.messages.last_mut() {
                            if last_msg.is_streaming && !text.is_empty() {
                                last_msg.content.push_str(&text);
                            }
                        }

                        // 도구 관련 콘텐츠 처리
                        for content in &message.content {
                            match content {
                                goose::conversation::message::MessageContent::ToolRequest(req) => {
                                    app.push_tool_text(&format!("▶ {}", req.to_readable_string()));
                                    app.start_tool(req.to_readable_string().split('(').next().unwrap_or("tool").to_string());
                                }
                                goose::conversation::message::MessageContent::ToolResponse(res) => {
                                    let output = match &res.tool_result {
                                        Ok(result) => {
                                            result.content.iter()
                                                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                                                .collect::<Vec<_>>()
                                                .join("\n")
                                        }
                                        Err(e) => format!("Error: {}", e.message),
                                    };
                                    // SUMMARY만 추출 (상세 목록 제외)
                                    let summary = extract_tool_summary(&output);
                                    app.push_tool_text(&format!("✓ {}", summary));
                                    app.finish_tool();
                                }
                                _ => {} // Text 등 다른 타입은 무시
                            }
                        }

                        app.tool_status = super::tui::ToolStatus::None;
                    }
                    Some(Ok(AgentEvent::McpNotification((ext_id, notification)))) => {
                        // MCP 알림에서 도구 정보 추출
                        handle_mcp_notification(&ext_id, &notification, app);
                    }
                    Some(Ok(AgentEvent::HistoryReplaced(updated_conversation))) => {
                        session.messages = updated_conversation;
                    }
                    Some(Ok(AgentEvent::ModelChange { model, mode: _ })) => {
                        app.model_name = model;
                    }
                    Some(Err(e)) => {
                        app.tool_status = super::tui::ToolStatus::Error {
                            name: "agent".to_string(),
                            message: e.to_string(),
                        };
                        app.finish_streaming();
                        break;
                    }
                    None => {
                        // 스트림 종료
                        if let Some(last_msg) = app.messages.last() {
                            if last_msg.is_streaming {
                                let assistant_msg = Message::assistant().with_text(&last_msg.content);
                                session.messages.push(assistant_msg);
                            }
                        }
                        app.finish_streaming();
                        app.tool_status = super::tui::ToolStatus::None;
                        break;
                    }
                }
            }
            // 짧은 대기 (이벤트 처리 기회 제공)
            _ = tokio::time::sleep(Duration::from_millis(16)) => {
                // 다음 루프에서 이벤트 처리
            }
        }
    }

    Ok(())
}

/// 메시지에서 텍스트 콘텐츠만 추출 (도구 호출 등 제외)
fn extract_text_content(message: &Message) -> String {
    use goose::conversation::message::MessageContent;

    let mut result = String::new();

    for content in &message.content {
        match content {
            MessageContent::Text(t) => {
                // 텍스트 그대로 추가 (줄바꿈 유지)
                result.push_str(&t.text);
            }
            _ => {} // 도구 호출, 이미지 등은 무시
        }
    }

    result
}

/// 슬래시 명령어 처리
fn handle_slash_command(cmd: &str, app: &mut TuiApp) {
    let cmd = cmd.trim();

    // 명령어는 메시지 목록에서 제거
    if let Some(msg) = app.messages.last() {
        if msg.role == super::tui::MessageRole::User && msg.content.starts_with('/') {
            app.messages.pop();
        }
    }

    match cmd {
        "/help" | "/?" => {
            app.show_help = true;
        }
        "/clear" => {
            app.messages.clear();
            app.add_system_message("대화 기록이 삭제되었습니다.".to_string());
        }
        "/quit" | "/exit" | "/q" => {
            app.should_quit = true;
        }
        "/t" | "/theme" => {
            app.toggle_theme();
            app.add_system_message(format!("테마 변경: {}", app.theme_name.label()));
        }
        _ => {
            app.add_system_message(format!("알 수 없는 명령어: {}\n사용 가능: /help /clear /quit /t(theme)", cmd));
        }
    }
}

/// MCP 알림 처리 (도구 출력 패널에 표시)
fn handle_mcp_notification(
    ext_id: &str,
    notification: &rmcp::model::ServerNotification,
    app: &mut TuiApp,
) {
    use rmcp::model::ServerNotification;

    match notification {
        ServerNotification::LoggingMessageNotification(log_notif) => {
            // 로깅 메시지에서 도구 정보 추출
            if let Some(obj) = log_notif.params.data.as_object() {
                // subagent tool request 처리
                if let Some(tool_call) = obj.get("tool_call").and_then(|v| v.as_object()) {
                    let tool_name = tool_call
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    app.push_tool_text(&format!("▶ {} ({})", tool_name, ext_id));
                    app.start_tool(tool_name.to_string());
                }
                // 일반 로그 메시지
                else if let Some(msg) = obj.get("message").and_then(|v| v.as_str()) {
                    app.push_tool_text(&format!("📝 {}", msg));
                }
            }
        }
        ServerNotification::ProgressNotification(prog_notif) => {
            // 진행률 표시
            let progress = prog_notif.params.progress;
            let total = prog_notif.params.total;
            let msg = prog_notif.params.message.as_deref().unwrap_or("");

            if let Some(t) = total {
                let percent = if t > 0.0 { (progress / t * 100.0) as u32 } else { 0 };
                app.push_tool_text(&format!("⏳ {}% {}", percent, msg));
                app.update_tool_progress(progress / t);
            } else {
                app.push_tool_text(&format!("⏳ {}", msg));
            }
        }
        _ => {
            // 기타 알림
            app.push_tool_text(&format!("[MCP:{}]", ext_id));
        }
    }
}

/// 도구 출력에서 SUMMARY 부분만 추출
/// 상세 파일 목록 등은 제외하고 요약만 표시
fn extract_tool_summary(output: &str) -> String {
    // 일반적인 상세 목록 시작 패턴들
    let cutoff_patterns = [
        "\nPATH [",           // developer__analyze 상세 목록
        "\n---\n",            // 구분선 이후 상세
        "\nFiles:\n",         // 파일 목록
        "\nDetails:\n",       // 상세 정보
        "\n```\n",            // 코드 블록 (긴 출력)
    ];

    let mut result = output.to_string();

    // 각 패턴에서 가장 먼저 나타나는 위치 찾기
    for pattern in cutoff_patterns {
        if let Some(pos) = result.find(pattern) {
            result = result[..pos].to_string();
        }
    }

    // 너무 길면 줄 수 제한 (최대 15줄)
    let lines: Vec<&str> = result.lines().collect();
    if lines.len() > 15 {
        let truncated: String = lines[..15].join("\n");
        format!("{}...\n(출력 생략, {} 줄 더 있음)", truncated, lines.len() - 15)
    } else {
        result
    }
}
