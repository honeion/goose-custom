//! TUI 세션 통합
//!
//! Phase 5.2: 에이전트 연동
//! CliSession과 TUI를 연결하는 브릿지

use std::time::Duration;

use anyhow::Result;
use crossterm::event;
use futures::StreamExt;
use goose::agents::AgentEvent;
use goose::audit::{AuditConfig, AuditLogger};
use goose::config::Config;
use goose::conversation::message::{ActionRequiredData, Message, MessageContent};
use goose::hints::{
    get_hints_metadata, format_hints_summary,
    GOOSE_HINTS_FILENAME, GOOSE_HINTS_LOCAL_FILENAME, AGENTS_MD_FILENAME,
};
use goose::permission::permission_confirmation::PrincipalType;
use goose::permission::{Permission, PermissionConfirmation};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{stdout, Stdout};
use crossterm::{
    execute,
    event::{EnableMouseCapture, DisableMouseCapture, EnableBracketedPaste, DisableBracketedPaste},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tokio_util::sync::CancellationToken;

use super::tui::{
    app::{InputMode, TuiApp},
    events::{event_to_action, Action, UpdateResult},
};
use crate::CliSession;

/// TUI 세션 실행
pub async fn run_tui_session(session: &mut CliSession) -> Result<()> {
    // 터미널 초기화
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_tui_loop(&mut terminal, session).await;

    // 터미널 정리 (에러 발생해도 반드시 실행)
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste);
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
    let provider = std::env::var("GOOSE_PROVIDER")
        .or_else(|_| Config::global().get_param::<String>("GOOSE_PROVIDER"))
        .unwrap_or_else(|_| "openai".to_string());
    let model = std::env::var("GOOSE_MODEL")
        .or_else(|_| Config::global().get_param::<String>("GOOSE_MODEL"))
        .unwrap_or_else(|_| "gpt-4o".to_string());
    let model_name = format!("{} {}", provider, model);
    let mut app = TuiApp::new(session.session_id.clone(), model_name);

    // 감사 로거 초기화
    let audit_enabled = Config::global()
        .get_param::<bool>("AUDIT_LOG_ENABLED")
        .unwrap_or(true);
    let audit_config = AuditConfig {
        enabled: audit_enabled,
        retention_days: Config::global()
            .get_param::<u32>("AUDIT_RETENTION_DAYS")
            .unwrap_or(30),
        ..AuditConfig::default()
    };

    if let Err(e) = AuditLogger::init(audit_config) {
        tracing::warn!("감사 로거 초기화 실패: {}", e);
    } else {
        // 세션 시작 이벤트 기록
        if let Some(logger) = AuditLogger::global() {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            logger.log_session_start(&session.session_id, &cwd);
        }
    }

    // PII 마스킹 상태 설정 (기본: 활성화)
    app.pii_masking_enabled = Config::global()
        .get_param::<bool>("PII_MASKING_ENABLED")
        .unwrap_or(true);

    // 설정 패널 초기값 로드
    app.config_panel.provider_name = provider.clone();
    app.config_panel.model_name = model.clone();
    app.config_panel.goose_mode = Config::global()
        .get_goose_mode()
        .unwrap_or(goose::config::GooseMode::Auto);
    app.config_panel.pii_enabled = app.pii_masking_enabled;
    app.config_panel.audit_enabled = audit_enabled;
    app.config_panel.max_tokens = app.token_usage.max;
    app.config_panel.max_turns = Config::global()
        .get_param::<u32>("GOOSE_MAX_TURNS")
        .unwrap_or(1000);
    // API Version은 Azure 전용, 다른 프로바이더는 N/A
    app.config_panel.api_version = if provider.contains("azure") {
        Config::global()
            .get_param::<String>("AZURE_OPENAI_API_VERSION")
            .or_else(|_| std::env::var("AZURE_OPENAI_API_VERSION"))
            .unwrap_or_else(|_| "2024-10-21".to_string())
    } else {
        "N/A".to_string()
    };

    // PII 화이트리스트 로드
    if let Ok(whitelist) = Config::global().get_param::<Vec<String>>("PII_WHITELIST_VALUES") {
        app.config_panel.pii_whitelist = whitelist.clone();
        session.agent.set_pii_whitelist(whitelist).await;
    }

    // PII 비활성화 타입 로드
    if let Ok(disabled_strs) = Config::global().get_param::<Vec<String>>("PII_DISABLED_TYPES") {
        let mut disabled_types = std::collections::HashSet::new();
        for s in &disabled_strs {
            match s.as_str() {
                "Secret" => { disabled_types.insert(goose::security::pii_patterns::MaskType::Secret); app.config_panel.pii_secret_enabled = false; }
                "Token" => { disabled_types.insert(goose::security::pii_patterns::MaskType::Token); app.config_panel.pii_token_enabled = false; }
                "Credential" => { disabled_types.insert(goose::security::pii_patterns::MaskType::Credential); app.config_panel.pii_credential_enabled = false; }
                "Certificate" => { disabled_types.insert(goose::security::pii_patterns::MaskType::Certificate); app.config_panel.pii_certificate_enabled = false; }
                _ => {}
            }
        }
        if !disabled_types.is_empty() {
            session.agent.set_pii_disabled_types(disabled_types).await;
        }
    }

    // 환영 메시지
    let welcome_msg = if app.pii_masking_enabled {
        "Goose Custom TUI 세션이 시작되었습니다. 🔒 민감정보 보호 활성화".to_string()
    } else {
        "Goose Custom TUI 세션이 시작되었습니다.".to_string()
    };
    app.add_system_message(welcome_msg);

    // Hints 요약 표시
    let hints_filenames = vec![
        GOOSE_HINTS_FILENAME.to_string(),
        GOOSE_HINTS_LOCAL_FILENAME.to_string(),
        AGENTS_MD_FILENAME.to_string(),
    ];
    let cwd = std::env::current_dir().unwrap_or_default();
    let hints_metadata = get_hints_metadata(&cwd, &hints_filenames);
    if !hints_metadata.is_empty() {
        let hints_summary = format_hints_summary(&hints_metadata);
        app.add_system_message(hints_summary);
    }

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

    let mut needs_redraw = true;

    loop {
        // 마우스 캡처 OFF + 스트리밍 안 할 때는 불필요한 리드로우 스킵
        // (터미널 네이티브 텍스트 선택이 리셋되지 않도록)
        let is_streaming = app.messages.last().map_or(false, |m| m.is_streaming);
        if needs_redraw || app.mouse_capture || is_streaming {
            let area = terminal.get_frame().area();
            let total_lines = app.calculate_total_lines(area.width);
            let viewport_height = area.height.saturating_sub(6);
            app.update_scroll_metrics(total_lines, viewport_height);

            terminal.draw(|frame| {
                app.render(frame);
            })?;
            needs_redraw = false;
        }

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
                    needs_redraw = true;
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

                            // intent 감지 + 컨텍스트 자동 수집
                            let augmented_content = detect_and_augment_intent(&content).await;

                            // 에이전트에 메시지 전송 및 응답 처리
                            process_agent_message(
                                session,
                                &augmented_content,
                                &mut app,
                                terminal,
                            ).await?;
                        }
                    }
                    _ => {}
                }
            }

            // 설정 패널 변경사항 처리 (패널 내부 키는 None 반환하므로 if-let 밖에서 처리)
            let config_changes = app.config_panel.take_pending_changes();
            for change in config_changes {
                apply_config_change(&change, session, &mut app).await;
            }

            needs_redraw = true;
        }

        // 틱 업데이트
        app.update(Action::Tick);
    }

    // 세션 종료 이벤트 기록
    if let Some(logger) = AuditLogger::global() {
        logger.log_session_end(&session.session_id);
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
    let stream = session
        .agent
        .reply(user_message, session_config, Some(cancel_token.clone()))
        .await?;

    // PII 마스킹 카운트 업데이트 및 알림 (마스킹은 reply() 내부에서 발생)
    if app.pii_masking_enabled {
        let prev_count = app.pii_masked_count;
        let new_count = session.agent.pii_masked_count().await;
        if new_count > prev_count {
            let masked_this_time = new_count - prev_count;
            app.add_system_message(format!(
                "🔒 민감정보 {}개가 마스킹되었습니다. AI에게는 [SECRET_N] 형태로 전달됩니다.",
                masked_this_time
            ));
        }
        app.pii_masked_count = new_count;
        // 화면 갱신
        terminal.draw(|frame| app.render(frame))?;
    }

    // 스트림 처리 - 이벤트와 스트림을 번갈아 처리
    let mut stream = std::pin::pin!(stream);
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
                    // ESC during streaming = interrupt (stop generation)
                    Action::SwitchMode(InputMode::Normal) => {
                        cancel_token.cancel();
                        app.finish_streaming();
                        app.add_system_message("⚠️ 응답 생성이 중단되었습니다.".to_string());
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

        // 4. 스트림에서 데이터 가져오기 (biased select로 UI 우선)
        let stream_result = tokio::select! {
            biased;
            // 50ms 후 타임아웃 (UI 반응성)
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                // 타임아웃 - yield 후 다시 시도
                tokio::task::yield_now().await;
                continue;
            }
            // 스트림 이벤트
            result = stream.next() => result,
        };

        match stream_result {
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
                        MessageContent::ToolRequest(req) => {
                            let tool_desc = req.to_readable_string();
                            app.push_tool_text(&format!("▶ {}", tool_desc));
                            let tool_name = tool_desc.split('(').next().unwrap_or("tool").to_string();

                            // delegate(서브에이전트) 호출 시 대화창에 진행 상태 표시
                            if tool_name == "delegate" {
                                let source = tool_desc
                                    .split("source:")
                                    .nth(1)
                                    .and_then(|s| s.split(',').next())
                                    .map(|s| s.trim().trim_matches('"').trim_matches('\''))
                                    .unwrap_or("subagent");
                                if let Some(last_msg) = app.messages.last_mut() {
                                    if last_msg.is_streaming {
                                        last_msg.content.push_str(&format!("\n🔄 `{}` 서브에이전트 분석 중...\n", source));
                                    }
                                }
                            }

                            app.start_tool(tool_name);
                        }
                        MessageContent::ToolResponse(res) => {
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
                        MessageContent::ActionRequired(action) => {
                            // 도구 승인 요청 처리 (TUI에서는 자동 승인)
                            if let ActionRequiredData::ToolConfirmation { id, tool_name, .. } = &action.data {
                                tracing::info!("TUI: 도구 승인 요청 - {} (자동 승인)", tool_name);
                                app.push_tool_text(&format!("🔓 {} 승인됨", tool_name));

                                // 자동 승인 전송
                                session.agent.handle_confirmation(
                                    id.clone(),
                                    PermissionConfirmation {
                                        principal_type: PrincipalType::Tool,
                                        permission: Permission::AllowOnce,
                                    },
                                ).await;
                            }
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
                // 스트림 종료 - 박스 문자 제거 + 언마스킹
                if let Some(last_msg) = app.messages.last_mut() {
                    if last_msg.is_streaming {
                        // 1. 박스 문자 제거
                        let sanitized = sanitize_box_chars(&last_msg.content);
                        if sanitized != last_msg.content {
                            last_msg.content = sanitized;
                        }
                        // 2. PII 언마스킹
                        let unmasked_content = session.agent.unmask_pii(&last_msg.content).await;
                        if unmasked_content != last_msg.content {
                            tracing::debug!("TUI: PII 언마스킹 적용됨");
                            last_msg.content = unmasked_content.clone();
                        }
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

/// LLM 응답에서 Unicode 박스 문자 제거
/// GPT-4o가 ┌─────/└─────/│ 로 감싸는 습관 교정
fn sanitize_box_chars(text: &str) -> String {
    let mut result = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // ┌ 또는 └ 로 시작하는 줄은 전부 제거 (테두리 줄)
        if trimmed.starts_with('┌') || trimmed.starts_with('└') {
            continue;
        }
        // │ 로 시작하는 줄에서 │ 접두사 제거
        if trimmed.starts_with('│') {
            let stripped = trimmed.trim_start_matches('│').trim_start();
            // │ 뒤에 내용이 있으면 내용만, 없으면 빈 줄
            if !stripped.is_empty() {
                result.push_str(stripped);
            }
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    if result.ends_with('\n') {
        result.pop();
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
        _ if cmd.starts_with("/hints") => {
            handle_hints_command(cmd, app);
        }
        _ if cmd.starts_with("/audit") => {
            handle_audit_command(cmd, app);
        }
        "/config" | "/settings" => {
            app.config_panel.open();
        }
        _ => {
            app.add_system_message(format!("알 수 없는 명령어: {}\n사용 가능: /help /clear /quit /t(theme) /hints /audit /config", cmd));
        }
    }
}

/// /audit 명령어 처리
fn handle_audit_command(cmd: &str, app: &mut TuiApp) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let subcmd = parts.get(1).map(|s| *s).unwrap_or("status");

    match subcmd {
        "status" | "s" => {
            // 현재 세션 감사 상태 표시
            if let Some(logger) = AuditLogger::global() {
                if let Some(stats) = logger.get_session_stats(&app.session_id) {
                    let mut lines = vec![
                        "📊 감사 로그 상태".to_string(),
                        "".to_string(),
                        format!("세션 ID: {}", app.session_id),
                        format!("토큰 사용: {} (입력) / {} (출력)", stats.total_tokens.input, stats.total_tokens.output),
                        format!("도구 호출: {}", stats.tool_calls),
                        format!("PII 마스킹: {} 건", stats.pii_masked_count),
                        format!("보안 이벤트: {} 건", stats.security_events),
                    ];
                    if stats.start_time.is_some() {
                        lines.push(format!("실행 시간: {:?}", stats.start_time.unwrap().elapsed()));
                    }
                    app.add_system_message(lines.join("\n"));
                } else {
                    app.add_system_message("❌ 세션 통계를 찾을 수 없습니다.".to_string());
                }
            } else {
                app.add_system_message("❌ 감사 로거가 초기화되지 않았습니다.".to_string());
            }
        }
        "path" | "p" => {
            // 감사 로그 파일 경로 표시
            let log_dir = goose::config::paths::Paths::in_state_dir("logs").join("audit");
            let today = chrono::Local::now().format("%Y-%m-%d");
            let current_file = log_dir.join(format!("audit.{}.jsonl", today));

            let mut lines = vec![
                "📁 감사 로그 경로".to_string(),
                "".to_string(),
                format!("로그 디렉토리: {}", log_dir.display()),
                format!("현재 파일: audit.{}.jsonl", today),
            ];

            if current_file.exists() {
                if let Ok(metadata) = std::fs::metadata(&current_file) {
                    lines.push(format!("파일 크기: {} bytes", metadata.len()));
                }
            } else {
                lines.push("(파일 아직 없음)".to_string());
            }

            app.add_system_message(lines.join("\n"));
        }
        "help" | "?" => {
            let help = vec![
                "📝 감사 로그 명령어",
                "",
                "  /audit [status]  - 현재 세션 감사 상태",
                "  /audit path      - 로그 파일 경로",
                "  /audit help      - 이 도움말",
                "",
                "감사 로그는 모든 사용자 입력, PII 마스킹,",
                "API 요청/응답, 도구 실행을 기록합니다.",
                "",
                "⚠️ 원본 PII 값은 절대 로그에 기록되지 않습니다.",
            ];
            app.add_system_message(help.join("\n"));
        }
        _ => {
            app.add_system_message(format!(
                "❓ 알 수 없는 audit 서브명령어: {}\n\n사용법:\n  /audit [status]  - 상태 표시\n  /audit path      - 경로 정보\n  /audit help      - 도움말",
                subcmd
            ));
        }
    }
}

/// /hints 명령어 처리
fn handle_hints_command(cmd: &str, app: &mut TuiApp) {
    use goose::config::paths::Paths;

    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let subcmd = parts.get(1).map(|s| *s).unwrap_or("show");

    let hints_filenames = vec![
        GOOSE_HINTS_FILENAME.to_string(),
        GOOSE_HINTS_LOCAL_FILENAME.to_string(),
        AGENTS_MD_FILENAME.to_string(),
    ];
    let cwd = std::env::current_dir().unwrap_or_default();

    match subcmd {
        "show" | "list" => {
            // 현재 로드된 hints 표시
            let metadata = get_hints_metadata(&cwd, &hints_filenames);
            if metadata.is_empty() {
                app.add_system_message("📋 로드된 Hints가 없습니다.\n\n생성하려면:\n  /hints add global  - 글로벌 hints\n  /hints add project - 프로젝트 hints\n  /hints add local   - 로컬 hints".to_string());
            } else {
                let mut lines = vec!["📋 로드된 Hints:".to_string(), "".to_string()];
                for hint in &metadata {
                    lines.push(format!(
                        "  {} {} - {} ({}줄)",
                        hint.layer.icon(),
                        hint.layer.label(),
                        hint.file_path.display(),
                        hint.line_count
                    ));
                }
                lines.push("".to_string());
                lines.push("명령어: /hints edit <layer> | /hints add <layer> | /hints reload".to_string());
                app.add_system_message(lines.join("\n"));
            }
        }
        "reload" => {
            // hints 다시 로드
            let metadata = get_hints_metadata(&cwd, &hints_filenames);
            let summary = format_hints_summary(&metadata);
            if summary.is_empty() {
                app.add_system_message("🔄 Hints 리로드 완료 (로드된 파일 없음)".to_string());
            } else {
                app.add_system_message(format!("🔄 Hints 리로드 완료\n{}", summary));
            }
        }
        "add" => {
            let layer = parts.get(2).map(|s| *s).unwrap_or("project");
            let (path, layer_name) = match layer {
                "global" | "g" => {
                    let path = Paths::in_config_dir(GOOSE_HINTS_FILENAME);
                    (path, "Global")
                }
                "local" | "l" => {
                    let path = cwd.join(GOOSE_HINTS_LOCAL_FILENAME);
                    (path, "Local")
                }
                _ => {
                    let path = cwd.join(GOOSE_HINTS_FILENAME);
                    (path, "Project")
                }
            };

            if path.exists() {
                app.add_system_message(format!(
                    "⚠️ {} hints 파일이 이미 존재합니다:\n  {}\n\n편집하려면: /hints edit {}",
                    layer_name, path.display(), layer
                ));
            } else {
                // 기본 템플릿으로 파일 생성
                let template = format!(
                    "# {} Hints for Goose\n\n## Project Context\n\n[프로젝트 설명]\n\n## Instructions\n\n[AI에게 주는 지시사항]\n",
                    layer_name
                );
                match std::fs::write(&path, &template) {
                    Ok(_) => {
                        app.add_system_message(format!(
                            "✅ {} hints 파일 생성됨:\n  {}\n\n편집하려면: /hints edit {}",
                            layer_name, path.display(), layer
                        ));
                    }
                    Err(e) => {
                        app.add_system_message(format!("❌ 파일 생성 실패: {}", e));
                    }
                }
            }
        }
        "edit" => {
            let layer = parts.get(2).map(|s| *s).unwrap_or("project");
            let path = match layer {
                "global" | "g" => Paths::in_config_dir(GOOSE_HINTS_FILENAME),
                "local" | "l" => cwd.join(GOOSE_HINTS_LOCAL_FILENAME),
                _ => cwd.join(GOOSE_HINTS_FILENAME),
            };

            if !path.exists() {
                app.add_system_message(format!(
                    "⚠️ 파일이 존재하지 않습니다: {}\n\n생성하려면: /hints add {}",
                    path.display(), layer
                ));
            } else {
                // 에디터로 파일 열기
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
                    if cfg!(windows) { "notepad".to_string() } else { "vi".to_string() }
                });
                app.add_system_message(format!(
                    "📝 에디터로 여는 중: {}\n  {}\n\n(외부 에디터에서 편집 후 /hints reload 실행)",
                    editor, path.display()
                ));
                // 백그라운드에서 에디터 실행 (blocking하지 않음)
                let _ = std::process::Command::new(&editor)
                    .arg(&path)
                    .spawn();
            }
        }
        "path" => {
            // hints 경로 정보 표시
            let mut lines = vec!["📂 Hints 경로 정보:".to_string(), "".to_string()];
            lines.push(format!("  🌐 Global: {}", Paths::in_config_dir(GOOSE_HINTS_FILENAME).display()));
            lines.push(format!("  📁 Project: {}", cwd.join(GOOSE_HINTS_FILENAME).display()));
            lines.push(format!("  👤 Local: {}", cwd.join(GOOSE_HINTS_LOCAL_FILENAME).display()));
            app.add_system_message(lines.join("\n"));
        }
        "panel" | "p" => {
            // TUI 편집 패널 열기 (F5 대안)
            app.hints_panel.open(&cwd);
        }
        _ => {
            app.add_system_message(format!(
                "❓ 알 수 없는 hints 서브명령어: {}\n\n사용법:\n  /hints [show]     - 로드된 hints 표시\n  /hints reload     - hints 다시 로드\n  /hints add <g|p|l> - hints 파일 생성\n  /hints edit <g|p|l> - hints 편집\n  /hints path       - 경로 정보\n  /hints panel      - 편집 패널 열기 (F5)",
                subcmd
            ));
        }
    }
}

/// 설정 변경사항 에이전트에 적용
async fn apply_config_change(
    change: &super::tui::config_panel::ConfigChange,
    session: &mut CliSession,
    app: &mut TuiApp<'_>,
) {
    use super::tui::config_panel::ConfigChange;

    match change {
        ConfigChange::ModeChanged(mode) => {
            let mode_label = format!("{:?}", mode);
            app.add_system_message(format!("⚙️ 실행 모드 변경: {}", mode_label));
        }
        ConfigChange::PiiToggled(enabled) => {
            session.agent.set_pii_enabled(*enabled).await;
            app.pii_masking_enabled = *enabled;
            app.add_system_message(format!(
                "🔒 PII 마스킹 {}",
                if *enabled { "활성화됨" } else { "비활성화됨" }
            ));
        }
        ConfigChange::PiiWhitelistUpdated(values) => {
            session.agent.set_pii_whitelist(values.clone()).await;
            app.add_system_message(format!("📋 PII 화이트리스트 업데이트 ({} 항목)", values.len()));
        }
        ConfigChange::PiiDisabledTypesUpdated(types) => {
            session.agent.set_pii_disabled_types(types.clone()).await;
            if types.is_empty() {
                app.add_system_message("🔒 모든 PII 카테고리 활성화됨".to_string());
            } else {
                let names: Vec<String> = types.iter().map(|t| format!("{:?}", t)).collect();
                app.add_system_message(format!("🔒 PII 카테고리 비활성화: {}", names.join(", ")));
            }
        }
        ConfigChange::MaxTokensChanged(max) => {
            app.token_usage.max = *max;
        }
        ConfigChange::MaxTurnsChanged(turns) => {
            session.max_turns = Some(*turns);
            app.add_system_message(format!("🔄 Max Turns 변경: {}", turns));
        }
        ConfigChange::AuditToggled(enabled) => {
            app.add_system_message(format!(
                "📊 감사 로깅 {}",
                if *enabled { "활성화됨" } else { "비활성화됨" }
            ));
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

                    // 서브에이전트 진행 상태를 대화창에도 표시
                    if let Some(last_msg) = app.messages.last_mut() {
                        if last_msg.is_streaming {
                            if tool_name.contains("read") {
                                let path = tool_call.get("path")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let short_path = path.rsplit('/').next()
                                    .or_else(|| path.rsplit('\\').next())
                                    .unwrap_or(path);
                                last_msg.content.push_str(&format!("  📖 reading `{}`...\n", short_path));
                            } else if tool_name.contains("glob") {
                                last_msg.content.push_str("  🔍 scanning files...\n");
                            } else if tool_name.contains("grep") {
                                last_msg.content.push_str("  🔎 searching code...\n");
                            }
                        }
                    }
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
/// 사용자 메시지에서 intent 감지 + 프로젝트 컨텍스트 자동 수집
async fn detect_and_augment_intent(content: &str) -> String {
    // "분석" intent 감지
    let analyze_keywords = ["분석", "analyze", "설명해", "파악", "이해", "살펴", "inspect"];
    let is_analyze = analyze_keywords.iter().any(|kw| content.contains(kw));

    if !is_analyze {
        return content.to_string();
    }

    // 경로 추출 (Windows/Unix)
    let path = extract_path_from_message(content);
    let project_path = match path {
        Some(p) if std::path::Path::new(&p).is_dir() => p,
        _ => {
            // 경로 없으면 현재 디렉토리
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        }
    };

    if project_path.is_empty() {
        return content.to_string();
    }

    tracing::info!("Intent 감지: 프로젝트 분석 ({})", project_path);

    // 컨텍스트 자동 수집
    let mut context_parts: Vec<String> = Vec::new();
    let base = std::path::Path::new(&project_path);

    // 1. 프로젝트 트리 (1 depth)
    if let Ok(entries) = std::fs::read_dir(base) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && name != ".env.example" {
                continue;
            }
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if !["node_modules", "target", "__pycache__", ".git", "venv", ".venv", "dist", "build"].contains(&name.as_str()) {
                    dirs.push(format!("  {}/", name));
                }
            } else {
                files.push(format!("  {}", name));
            }
        }
        dirs.sort();
        files.sort();
        context_parts.push(format!("## 프로젝트 트리\n```\n{}\n{}\n```", dirs.join("\n"), files.join("\n")));
    }

    // 2. 핵심 파일 자동 읽기
    let doc_files = ["README.md", "CLAUDE.md", "readme.md"];
    let dep_files = ["requirements.txt", "pyproject.toml", "package.json", "Cargo.toml", "go.mod", "pom.xml"];
    let entry_files = ["main.py", "app.py", "manage.py", "main.rs", "lib.rs", "index.ts", "app.ts", "main.go", "Main.java"];
    let config_files = ["config.py", "settings.py", "config.ts", "config.rs", ".env.example"];

    let mut read_count = 0;
    let max_reads = 8;
    let max_lines_per_file = 150;

    // 문서 파일
    for name in &doc_files {
        if read_count >= max_reads { break; }
        let file_path = base.join(name);
        if file_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let truncated: String = content.lines().take(max_lines_per_file).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (자동 읽기)\n```\n{}\n```", name, truncated));
                read_count += 1;
            }
        }
    }

    // 의존성 파일
    for name in &dep_files {
        if read_count >= max_reads { break; }
        let file_path = base.join(name);
        if file_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let truncated: String = content.lines().take(80).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (의존성)\n```\n{}\n```", name, truncated));
                read_count += 1;
            }
        }
    }

    // 엔트리포인트 — 재귀 검색 (최대 2 depth)
    for name in &entry_files {
        if read_count >= max_reads { break; }
        if let Some(found) = find_file_recursive(base, name, 3) {
            if let Ok(content) = std::fs::read_to_string(&found) {
                let rel_path = found.strip_prefix(base).unwrap_or(&found);
                let truncated: String = content.lines().take(max_lines_per_file).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (엔트리포인트)\n```\n{}\n```", rel_path.display(), truncated));
                read_count += 1;
            }
        }
    }

    // 설정 파일 — 재귀 검색
    for name in &config_files {
        if read_count >= max_reads { break; }
        if let Some(found) = find_file_recursive(base, name, 3) {
            if let Ok(content) = std::fs::read_to_string(&found) {
                let rel_path = found.strip_prefix(base).unwrap_or(&found);
                let truncated: String = content.lines().take(max_lines_per_file).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (설정)\n```\n{}\n```", rel_path.display(), truncated));
                read_count += 1;
            }
        }
    }

    // 3. Git 상태
    if let Ok(output) = std::process::Command::new("git")
        .args(["log", "--oneline", "-5"])
        .current_dir(base)
        .output()
    {
        if output.status.success() {
            let log = String::from_utf8_lossy(&output.stdout);
            context_parts.push(format!("## Git 최근 커밋\n```\n{}\n```", log.trim()));
        }
    }

    if let Ok(output) = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(base)
        .output()
    {
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout);
            context_parts.push(format!("현재 브랜치: `{}`", branch.trim()));
        }
    }

    if context_parts.is_empty() {
        return content.to_string();
    }

    // 원본 메시지 + 수집된 컨텍스트를 합쳐서 LLM에 전달
    format!(
        "{}\n\n---\n\n# 자동 수집된 프로젝트 정보 ({}개 파일 읽음)\n\n아래 정보를 바탕으로 프로젝트를 분석해주세요.\n요약 위주로 설명하고, 핵심 아키텍처와 데이터 흐름을 파악해주세요.\n\n{}\n",
        content,
        read_count,
        context_parts.join("\n\n")
    )
}

/// 메시지에서 경로 추출
fn extract_path_from_message(content: &str) -> Option<String> {
    // Windows 경로 (C:\... 또는 C:/...)
    if let Some(caps) = regex::Regex::new(r"([A-Za-z]:[/\\][^\s]+)")
        .ok()
        .and_then(|re| re.find(content))
    {
        return Some(caps.as_str().to_string());
    }
    // Unix 경로
    if let Some(caps) = regex::Regex::new(r"(/[^\s]+)")
        .ok()
        .and_then(|re| re.find(content))
    {
        let path = caps.as_str();
        if path.len() > 2 {
            return Some(path.to_string());
        }
    }
    None
}

/// 파일 재귀 검색 (최대 depth)
fn find_file_recursive(base: &std::path::Path, name: &str, max_depth: usize) -> Option<std::path::PathBuf> {
    find_file_recursive_inner(base, name, 0, max_depth)
}

fn find_file_recursive_inner(dir: &std::path::Path, name: &str, depth: usize, max_depth: usize) -> Option<std::path::PathBuf> {
    if depth > max_depth { return None; }

    let direct = dir.join(name);
    if direct.exists() {
        return Some(direct);
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if ["node_modules", "target", "__pycache__", ".git", "venv", ".venv", "dist", "build"].contains(&dir_name.as_str()) {
                    continue;
                }
                if let Some(found) = find_file_recursive_inner(&entry.path(), name, depth + 1, max_depth) {
                    return Some(found);
                }
            }
        }
    }
    None
}

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
