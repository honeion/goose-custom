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

// ============================================================
// 자동 컨텍스트 수집 파이프라인 — 데이터 구조
// ============================================================

/// 사용자 메시지에서 감지된 Intent 유형
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserIntent {
    Analyze,  // 프로젝트 분석
    Debug,    // 에러/디버그
    Modify,   // 코드 수정
    Deploy,   // 배포/빌드
    Data,     // 데이터 분석/처리
}

impl UserIntent {
    /// 시스템 컨텍스트 키 접미사
    fn context_key(&self) -> &'static str {
        match self {
            UserIntent::Analyze => "analyze",
            UserIntent::Debug => "debug",
            UserIntent::Modify => "modify",
            UserIntent::Deploy => "deploy",
            UserIntent::Data => "data",
        }
    }

    /// 진행 표시용 한국어 라벨
    fn label(&self) -> &'static str {
        match self {
            UserIntent::Analyze => "프로젝트 분석",
            UserIntent::Debug => "디버그 컨텍스트",
            UserIntent::Modify => "수정 대상 분석",
            UserIntent::Deploy => "배포 환경 분석",
            UserIntent::Data => "데이터 분석",
        }
    }

    /// Intent별 TASK 지시문
    fn task_instruction(&self) -> &'static str {
        match self {
            UserIntent::Analyze => "위에 수집된 실제 코드 내용만을 기반으로 분석해주세요. 절대로 코드를 추측하거나 지어내지 마세요. 수집된 파일에 없는 함수나 클래스를 언급하지 마세요. 각 파일을 인용할 때 [파일 N] 번호를 사용해주세요. 이 프로젝트가 무엇을 하는 시스템인지, 핵심 아키텍처, 데이터 흐름, 주요 모듈의 역할을 한국어로 요약해주세요.",
            UserIntent::Debug => "위에 수집된 실제 컨텍스트만을 기반으로 문제의 원인을 분석해주세요. 존재하지 않는 코드나 에러를 지어내지 마세요. 각 파일을 인용할 때 [파일 N] 번호를 사용해주세요. 코드상으로 원인을 특정하기 어려우면, 수집된 환경 설정(docker-compose, k8s manifest 등)을 참고하여 kubectl logs 등 런타임 로그 확인을 시도해주세요.",
            UserIntent::Modify => "위에 수집된 실제 파일 내용만을 기반으로 분석해주세요. 수집되지 않은 코드를 추측하지 마세요. 각 파일을 인용할 때 [파일 N] 번호를 사용해주세요. 파일의 구조와 import 관계를 파악하고, 수정 방안을 제시해주세요.",
            UserIntent::Deploy => "위에 수집된 실제 인프라 설정 파일만을 기반으로 분석해주세요. 수집되지 않은 설정 파일의 내용을 지어내지 마세요. 각 파일을 인용할 때 [파일 N] 번호를 사용해주세요. Dockerfile, CI/CD, k8s manifest를 기반으로 현재 배포 파이프라인을 설명해주세요.",
            UserIntent::Data => "위에 수집된 데이터 파일의 스키마와 샘플을 기반으로 분석해주세요. 데이터 구조, 컬럼 의미, 활용 가능한 분석 방향을 한국어로 설명해주세요. 관련 스크립트가 있으면 함께 참고하세요.",
        }
    }
}

/// Intent 감지 — 우선순위: Debug > Modify > Deploy > Analyze
fn detect_intent(content: &str) -> Option<UserIntent> {
    let lower = content.to_lowercase();

    let debug_keywords = [
        "에러", "오류", "안됨", "debug", "왜", "안돼", "실패",
        "fail", "error", "bug", "crash", "문제", "traceback", "stack",
        "로그", "log", "exception", "panic",
    ];
    let modify_keywords = [
        "고쳐", "바꿔", "수정", "추가", "변경",
        "fix", "change", "add", "update", "modify", "리팩",
    ];
    let deploy_keywords = [
        "배포", "deploy", "릴리즈", "release", "빌드", "build",
        "도커", "docker", "k8s", "kubernetes", "ci", "cd", "파이프라인",
    ];
    let analyze_keywords = [
        "분석", "analyze", "파악", "이해", "살펴", "inspect",
        "설명해", "구조", "아키텍처", "architecture",
    ];

    let data_keywords = [
        "데이터", "csv", "엑셀", "excel", "xlsx", "json",
        "통계", "차트", "리포트", "report", "대시보드", "dashboard",
    ];

    if debug_keywords.iter().any(|kw| lower.contains(kw)) {
        Some(UserIntent::Debug)
    } else if data_keywords.iter().any(|kw| lower.contains(kw)) {
        Some(UserIntent::Data)
    } else if modify_keywords.iter().any(|kw| lower.contains(kw)) {
        Some(UserIntent::Modify)
    } else if deploy_keywords.iter().any(|kw| lower.contains(kw)) {
        Some(UserIntent::Deploy)
    } else if analyze_keywords.iter().any(|kw| lower.contains(kw)) {
        Some(UserIntent::Analyze)
    } else {
        None
    }
}

/// 컨텍스트 생명주기 추적용 상태
struct ContextState {
    last_intent: Option<UserIntent>,
    last_path: Option<String>,
    last_context_key: Option<String>,
}

impl ContextState {
    fn new() -> Self {
        Self {
            last_intent: None,
            last_path: None,
            last_context_key: None,
        }
    }
}

/// 멀티서비스 감지 결과
#[allow(dead_code)]
enum ProjectMode {
    SingleService,
    MultiService {
        service_count: usize,
        services: Vec<ServiceInfo>,
    },
}

/// 멀티서비스 내 개별 서비스 정보
struct ServiceInfo {
    name: String,
    entry_path: std::path::PathBuf,
    has_dockerfile: bool,
}

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

    // 환영 메시지 + Hints 요약 통합 표시
    let hints_filenames = vec![
        GOOSE_HINTS_FILENAME.to_string(),
        GOOSE_HINTS_LOCAL_FILENAME.to_string(),
        AGENTS_MD_FILENAME.to_string(),
    ];
    let cwd = std::env::current_dir().unwrap_or_default();
    let hints_metadata = get_hints_metadata(&cwd, &hints_filenames);

    let mut startup_msg = if app.pii_masking_enabled {
        "Goose Custom TUI 세션이 시작되었습니다. 🔒 민감정보 보호 활성화".to_string()
    } else {
        "Goose Custom TUI 세션이 시작되었습니다.".to_string()
    };
    if !hints_metadata.is_empty() {
        startup_msg.push_str(&format!("\n{}", format_hints_summary(&hints_metadata)));
    }
    app.add_system_message(startup_msg);

    // 컨텍스트 생명주기 추적
    let mut context_state = ContextState::new();

    // Plan 모드 상태
    let mut plan_mode_previous_filter: Option<Option<Vec<String>>> = None; // Some = Plan 모드 중

    // 세션 메모리 파일 확인 (알림만, 시스템 컨텍스트 주입 안 함)
    let memory_dir = std::path::Path::new(".goose/sessions");
    let memory_file = memory_dir.join("memory.md");
    if memory_file.exists() {
        if let Ok(memory_content) = std::fs::read_to_string(&memory_file) {
            if !memory_content.trim().is_empty() {
                app.add_system_message("📝 세션 메모리 존재: .goose/sessions/memory.md (필요 시 read로 확인)".to_string());
            }
        }
    } else {
        let _ = std::fs::create_dir_all(memory_dir);
    }

    // 프로젝트 문서 확인 (알림만, 시스템 컨텍스트 주입 안 함)
    let cwd = std::env::current_dir().unwrap_or_default();
    let docs_dir = cwd.join("docs");
    if docs_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&docs_dir) {
            let doc_count = entries.flatten()
                .filter(|e| e.path().is_file() && e.file_name().to_string_lossy().ends_with(".md"))
                .count();
            if doc_count > 0 {
                app.add_system_message(format!("📚 프로젝트 문서 {}개 확인됨 (docs/)", doc_count));
            }
        }
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
                                // /plan — Plan 모드 (agent 접근 필요)
                                if content.trim() == "/plan" {
                                    if let Some(msg) = app.messages.last() {
                                        if msg.role == super::tui::MessageRole::User { app.messages.pop(); }
                                    }
                                    if plan_mode_previous_filter.is_some() {
                                        // Plan 모드 종료
                                        let prev = plan_mode_previous_filter.take().unwrap();
                                        session.agent.exit_plan_mode(prev).await;
                                        app.plan_mode = false;
                                        app.add_system_message("📋 Plan 모드 종료 — 도구 제한 해제, 실행 모드로 복귀".to_string());
                                    } else {
                                        // Plan 모드 진입
                                        let prev = session.agent.enter_plan_mode().await;
                                        plan_mode_previous_filter = Some(prev);
                                        app.plan_mode = true;
                                        app.add_system_message("📋 Plan 모드 진입 — 읽기 전용 (read/glob/grep만 가능)\n탐색 후 계획을 세우세요. /plan으로 종료합니다.".to_string());
                                    }
                                    terminal.draw(|frame| app.render(frame))?;
                                    continue;
                                }

                                // /sessions — 세션 목록 표시
                                if content.trim() == "/sessions" {
                                    if let Some(msg) = app.messages.last() {
                                        if msg.role == super::tui::MessageRole::User { app.messages.pop(); }
                                    }
                                    let sm = goose::session::SessionManager::instance();
                                    if let Ok(sessions) = sm.list_sessions().await {
                                        let mut lines = vec!["📋 세션 목록 (최근 10개):".to_string(), "".to_string()];
                                        for (i, s) in sessions.iter().take(10).enumerate() {
                                            let current = if s.id == session.session_id { " ◀ 현재" } else { "" };
                                            let name = if s.name.is_empty() { &s.id[..8] } else { &s.name };
                                            let msgs = s.message_count;
                                            let time = s.updated_at.format("%m-%d %H:%M");
                                            let dir = s.working_dir.file_name()
                                                .and_then(|n| n.to_str()).unwrap_or("?");
                                            lines.push(format!("{}. {} ({} msgs, {}) [{}]{}", i+1, name, msgs, time, dir, current));
                                        }
                                        lines.push("".to_string());
                                        lines.push("/resume <번호|ID> 로 세션 전환".to_string());
                                        app.add_system_message(lines.join("\n"));
                                    } else {
                                        app.add_system_message("❌ 세션 목록을 불러올 수 없습니다.".to_string());
                                    }
                                    terminal.draw(|frame| app.render(frame))?;
                                    continue;
                                }

                                // /resume — 세션 전환
                                if content.trim() == "/resume" || content.trim().starts_with("/resume ") {
                                    if let Some(msg) = app.messages.last() {
                                        if msg.role == super::tui::MessageRole::User { app.messages.pop(); }
                                    }
                                    let sm = goose::session::SessionManager::instance();
                                    let target_id = if content.trim() == "/resume" {
                                        // 가장 최근 세션
                                        sm.list_sessions().await.ok()
                                            .and_then(|sessions| sessions.into_iter()
                                                .find(|s| s.id != session.session_id)
                                                .map(|s| s.id))
                                    } else {
                                        let arg = content.trim().strip_prefix("/resume ").unwrap_or("").trim();
                                        // 번호 또는 ID
                                        if let Ok(num) = arg.parse::<usize>() {
                                            sm.list_sessions().await.ok()
                                                .and_then(|sessions| sessions.get(num.saturating_sub(1)).map(|s| s.id.clone()))
                                        } else {
                                            Some(arg.to_string())
                                        }
                                    };

                                    if let Some(id) = target_id {
                                        match sm.get_session(&id, true).await {
                                            Ok(target_session) => {
                                                // 세션 전환: 대화 히스토리 로드
                                                session.session_id = id.clone();
                                                if let Some(conv) = target_session.conversation {
                                                    session.messages = conv;
                                                }
                                                // TUI 메시지 재구성
                                                app.messages.clear();
                                                for msg in session.messages.messages() {
                                                    match msg.role {
                                                        rmcp::model::Role::User => {
                                                            app.messages.push(super::tui::ChatMessage::user(msg.as_concat_text()));
                                                        }
                                                        rmcp::model::Role::Assistant => {
                                                            app.messages.push(super::tui::ChatMessage::assistant(msg.as_concat_text()));
                                                        }
                                                    }
                                                }
                                                let name = if target_session.name.is_empty() { &id[..8.min(id.len())] } else { &target_session.name };
                                                app.add_system_message(format!("✅ 세션 전환: {} ({} 메시지)", name, target_session.message_count));
                                                app.session_id = id;
                                                // 컨텍스트 상태 초기화
                                                context_state = ContextState::new();
                                            }
                                            Err(e) => {
                                                app.add_system_message(format!("❌ 세션을 찾을 수 없습니다: {}", e));
                                            }
                                        }
                                    } else {
                                        app.add_system_message("❌ 전환할 세션이 없습니다. /sessions로 목록을 확인하세요.".to_string());
                                    }
                                    terminal.draw(|frame| app.render(frame))?;
                                    continue;
                                }

                                handle_slash_command(&content, &mut app);
                                terminal.draw(|frame| app.render(frame))?;
                                // /quit, /exit 처리
                                if app.should_quit {
                                    break;
                                }
                                continue;
                            }

                            // === 통합 컨텍스트 파이프라인 ===
                            // 1. Intent 감지
                            let current_intent = detect_intent(&content);
                            let explicit_path = extract_path_from_message(&content);
                            // 경로가 명시되지 않으면 이전 경로 유지 (후속 질문 지원)
                            let current_path = match &explicit_path {
                                Some(p) => p.clone(),
                                None => context_state.last_path.clone()
                                    .unwrap_or_else(|| std::env::current_dir()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_default()),
                            };

                            // 2. 생명주기 판단
                            let should_collect = match (&current_intent, &context_state.last_intent) {
                                (Some(intent), Some(last_intent)) => {
                                    if intent == last_intent && current_path == context_state.last_path.as_deref().unwrap_or("") {
                                        false // 같은 intent + 같은 path → 기존 유지 (후속 질문)
                                    } else if explicit_path.is_none() && context_state.last_context_key.is_some() {
                                        false // 경로 미지정 + 기존 컨텍스트 있음 → 후속 질문으로 간주
                                    } else {
                                        // 다른 intent + 새 경로 → 기존 제거
                                        if let Some(old_key) = &context_state.last_context_key {
                                            session.agent.remove_system_context(old_key).await;
                                        }
                                        true
                                    }
                                }
                                (Some(_), None) => true, // 첫 intent
                                (None, Some(_)) => {
                                    if explicit_path.is_some() {
                                        // 새 경로 명시 + intent 없음 → 주제 변경, 기존 제거
                                        if let Some(old_key) = &context_state.last_context_key {
                                            session.agent.remove_system_context(old_key).await;
                                        }
                                        context_state.last_intent = None;
                                        context_state.last_path = None;
                                        context_state.last_context_key = None;
                                    }
                                    // 경로 없으면 후속 질문 → 기존 유지
                                    false
                                }
                                (None, None) => false,
                            };

                            // 3. 수집 + 주입
                            if should_collect {
                                let intent = current_intent.unwrap(); // safe: should_collect=true → Some
                                if let Some(context) = collect_context_with_progress(
                                    &content, intent, &mut app, terminal,
                                ).await? {
                                    let context_key = format!("project_{}_context", intent.context_key());
                                    session.agent.add_system_context_with_ttl(context_key.clone(), context, 8).await;
                                    context_state.last_intent = Some(intent);
                                    context_state.last_path = Some(current_path);
                                    context_state.last_context_key = Some(context_key);
                                }
                            }

                            // 4. 에이전트에 원본 메시지 전송
                            process_agent_message(
                                session,
                                &content,
                                &mut app,
                                terminal,
                            ).await?;

                            // 5. 컨텍스트 TTL 감소
                            session.agent.tick_context_ttls().await;
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
                            // 도구 이름과 경로를 구조적으로 추출
                            let (tool_name, tool_path) = match &req.tool_call {
                                Ok(tc) => {
                                    let name = tc.name.to_string();
                                    let path = tc.arguments.as_ref()
                                        .and_then(|args| args.get("path"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    (name, path)
                                }
                                Err(_) => ("unknown".to_string(), "".to_string()),
                            };

                            // 도구 패널에 상세 표시
                            let tool_summary = if tool_path.is_empty() {
                                tool_name.clone()
                            } else {
                                // 경로에서 파일명만 추출
                                let file_name = std::path::Path::new(&tool_path)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or(&tool_path);
                                format!("{} → {}", tool_name, file_name)
                            };
                            app.push_tool_text(&format!("▶ {}", tool_summary));

                            // Assistant 스트리밍 영역에 도구 사용 한 줄 표시 (인용 아닌 일반 텍스트)
                            if let Some(last_msg) = app.messages.last_mut() {
                                if last_msg.is_streaming {
                                    // 도구 이름에서 extension prefix 제거 (developer__read → read)
                                    let short_tool = tool_name.split("__").last().unwrap_or(&tool_name);
                                    if tool_path.is_empty() {
                                        last_msg.content.push_str(&format!("  ▶ {}\n", short_tool));
                                    } else {
                                        let short_path = std::path::Path::new(&tool_path)
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or(&tool_path);
                                        last_msg.content.push_str(&format!("  ▶ {} ← {}\n", short_tool, short_path));
                                    }
                                }
                            }

                            // delegate(서브에이전트) 호출 시
                            if tool_name.contains("delegate") {
                                if let Ok(tc) = &req.tool_call {
                                    let source = tc.arguments.as_ref()
                                        .and_then(|args| args.get("source"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("subagent");
                                    if let Some(last_msg) = app.messages.last_mut() {
                                        if last_msg.is_streaming {
                                            last_msg.content.push_str(&format!("\n🔄 `{}` 서브에이전트 분석 중...\n", source));
                                        }
                                    }
                                }
                            }

                            // 세션 메모리: write/edit/shell 도구 호출 기록
                            let write_tools = ["write", "edit", "text_editor", "shell", "notebook_edit"];
                            if write_tools.iter().any(|t| tool_name.to_lowercase().contains(t)) {
                                let memory_path = std::path::Path::new(".goose/sessions/modified_files.md");
                                if let Some(parent) = memory_path.parent() {
                                    let _ = std::fs::create_dir_all(parent);
                                }
                                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M");
                                let target = if tool_path.is_empty() { &tool_name } else { &tool_path };
                                let entry = format!("- [{}] {} → {}\n", timestamp, tool_name, target);
                                let _ = std::fs::OpenOptions::new()
                                    .create(true).append(true)
                                    .open(memory_path)
                                    .and_then(|mut f| std::io::Write::write_all(&mut f, entry.as_bytes()));
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

                            // (세션 메모리 기록은 ToolRequest에서 처리)
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
            app.add_system_message(format!("알 수 없는 명령어: {}\n사용 가능: /help /clear /quit /t /hints /audit /config /plan /sessions /resume", cmd));
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
///
/// 통합 컨텍스트 수집 오케스트레이터
/// Intent를 받아 적절한 수집 함수로 디스패치 + 프리-LLM 평가
async fn collect_context_with_progress(
    content: &str,
    intent: UserIntent,
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<Option<String>> {
    // 경로 추출
    let path = extract_path_from_message(content);
    let project_path = match path {
        Some(p) if std::path::Path::new(&p).is_dir() => p,
        _ => std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    };

    if project_path.is_empty() {
        return Ok(None);
    }

    let base = std::path::Path::new(&project_path);
    let short_name = base.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");

    // 진행 표시 — 시스템 메시지 + 도구 상태바 병행
    app.add_system_message(make_progress_bar(0, 7, &format!("{} {} 준비", short_name, intent.label())));
    app.start_tool(format!("{} {} 준비", short_name, intent.label()));
    terminal.draw(|frame| app.render(frame))?;
    let progress_idx = app.messages.len() - 1;

    // Intent별 수집 디스패치
    let (context_parts, read_count) = match intent {
        UserIntent::Analyze => collect_analyze_context(base, content, "unknown", app, terminal)?,
        UserIntent::Debug => collect_debug_context(base, content, app, terminal)?,
        UserIntent::Modify => collect_modify_context(base, content, app, terminal)?,
        UserIntent::Deploy => collect_deploy_context(base, content, app, terminal)?,
        UserIntent::Data => collect_data_context(base, content, app, terminal)?,
    };

    // 프리-LLM 평가: 수집 파일이 너무 적으면 Analyze로 폴백 (1회)
    let (final_parts, final_count) = if read_count < 5 && intent != UserIntent::Analyze {
        update_progress(app, terminal, progress_idx, &make_progress_bar(5, 7, "수집 부족 — 프로젝트 분석으로 보충"))?;
        let (mut extra_parts, extra_count) = collect_analyze_context(base, content, "unknown", app, terminal)?;
        let mut combined = context_parts;
        combined.append(&mut extra_parts);
        (combined, read_count + extra_count)
    } else {
        (context_parts, read_count)
    };

    // 완료 표시
    app.finish_tool();
    update_progress(app, terminal, progress_idx,
        &format!("◉ ━━━━━━━━━━━━━━━━━━━━ [100%] — {}개 파일 수집 완료, AI 분석 중...", final_count))?;

    if final_parts.is_empty() {
        return Ok(None);
    }

    // 컨텍스트 문자열 조립 — 각 파일에 번호 매기기
    let numbered_parts: Vec<String> = final_parts.iter().enumerate()
        .map(|(i, part)| format!("[파일 {}/{}]\n{}", i + 1, final_parts.len(), part))
        .collect();
    let context = format!(
        "[AUTO-CONTEXT: {} — {} ({}개 파일)]\n\n{}\n\n[TASK: {}]\n[END AUTO-CONTEXT]",
        intent.label(),
        short_name,
        final_parts.len(),
        numbered_parts.join("\n\n"),
        intent.task_instruction(),
    );

    Ok(Some(context))
}

/// 진행 메시지 업데이트 + 리드로우
fn update_progress(
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    msg_idx: usize,
    text: &str,
) -> Result<()> {
    if msg_idx < app.messages.len() {
        app.messages[msg_idx].content = text.to_string();
    }
    terminal.draw(|frame| app.render(frame))?;
    Ok(())
}

/// 도구 상태바 + 시스템 메시지 동시 업데이트
fn tool_step(
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    step: usize,
    total: usize,
    step_name: &str,
) -> Result<()> {
    let progress = step as f64 / total.max(1) as f64;
    app.update_tool_step(&format!("{}/{} {}", step, total, step_name), progress);
    // 시스템 메시지도 업데이트 (마지막 시스템 메시지가 프로그레스 바이면)
    let msg_count = app.messages.len();
    if msg_count > 0 {
        let last_idx = msg_count - 1;
        if let Some(msg) = app.messages.get(last_idx) {
            if msg.role == super::tui::MessageRole::System && msg.content.contains("━") {
                // 프로그레스 바 시스템 메시지 업데이트
                let bar_text = make_progress_bar(step, total, step_name);
                app.messages[last_idx].content = bar_text;
            }
        }
    }
    terminal.draw(|frame| app.render(frame))?;
    Ok(())
}

/// 프로그레스 바 생성
fn make_progress_bar(current: usize, total: usize, step_name: &str) -> String {
    let pct = if total > 0 { current * 100 / total } else { 0 };
    let filled = current * 20 / total.max(1);
    let empty = 20 - filled;
    let bar = format!("{}{}", "━".repeat(filled), "─".repeat(empty));
    let spinner = match current % 4 {
        0 => "◐",
        1 => "◓",
        2 => "◑",
        _ => "◒",
    };
    format!("{} {} [{}] {}/{} — {}", spinner, bar, pct, current, total, step_name)
}

// ============================================================
// Intent별 수집 함수
// ============================================================

/// Analyze intent: 프로젝트 구조 분석 (멀티서비스 감지 포함)
fn collect_analyze_context(
    base: &std::path::Path,
    content: &str,
    detected_lang: &str,
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(Vec<String>, usize)> {
    let mut context_parts: Vec<String> = Vec::new();
    let mut read_count = 0;
    let max_lines_per_file = 50;
    let total_steps = 7;
    let mut step = 0;

    // === Step 1: 프로젝트 트리 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "디렉토리 구조 스캔")?;
    if let Ok(entries) = std::fs::read_dir(base) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if !SKIP_DIRS.contains(&name.as_str()) {
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

    // === Step 2: 문서 파일 ===
    step += 1;
    for name in &["README.md", "CLAUDE.md", "readme.md"] {
        if read_count >= 10 { break; }
        let file_path = base.join(name);
        if file_path.exists() {
            tool_step(app, terminal, step, total_steps, &format!("{}", name))?;
            if let Ok(file_content) = std::fs::read_to_string(&file_path) {
                let truncated: String = file_content.lines().take(max_lines_per_file).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (문서)\n```\n{}\n```", name, truncated));
                read_count += 1;
            }
        }
    }

    // === Step 3: 의존성 파일 + 언어 감지 ===
    step += 1;
    let dep_lang_map: &[(&str, &str)] = &[
        ("requirements.txt", "python"), ("pyproject.toml", "python"), ("setup.py", "python"),
        ("package.json", "node"), ("tsconfig.json", "node"),
        ("Cargo.toml", "rust"),
        ("go.mod", "go"),
        ("pom.xml", "java"), ("build.gradle", "java"), ("build.gradle.kts", "kotlin"),
        ("*.csproj", "dotnet"), ("*.sln", "dotnet"),
        ("Gemfile", "ruby"),
    ];
    let mut lang = detected_lang.to_string();
    for (dep_file, dep_lang) in dep_lang_map {
        if read_count >= 10 { break; }
        if dep_file.starts_with('*') {
            let ext = dep_file.trim_start_matches('*');
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(ext) {
                        lang = dep_lang.to_string();
                        tool_step(app, terminal, step, total_steps, &format!("{}", name))?;
                        if let Ok(c) = std::fs::read_to_string(entry.path()) {
                            let t: String = c.lines().take(50).collect::<Vec<_>>().join("\n");
                            context_parts.push(format!("## {} (의존성/{})\n```\n{}\n```", name, dep_lang, t));
                            read_count += 1;
                        }
                        break;
                    }
                }
            }
        } else {
            let file_path = base.join(dep_file);
            if file_path.exists() {
                lang = dep_lang.to_string();
                tool_step(app, terminal, step, total_steps, &format!("{}", dep_file))?;
                if let Ok(c) = std::fs::read_to_string(&file_path) {
                    let t: String = c.lines().take(50).collect::<Vec<_>>().join("\n");
                    context_parts.push(format!("## {} (의존성/{})\n```\n{}\n```", dep_file, dep_lang, t));
                    read_count += 1;
                }
            }
        }
    }

    // === Step 4: 엔트리포인트 + 멀티서비스 감지 ===
    step += 1;
    let entry_names: Vec<&str> = match lang.as_str() {
        "python" => vec!["main.py", "app.py", "manage.py", "wsgi.py", "__main__.py"],
        "node" => vec!["index.ts", "index.js", "app.ts", "app.js", "server.ts", "server.js", "main.tsx", "App.tsx", "App.vue"],
        "rust" => vec!["main.rs", "lib.rs"],
        "go" => vec!["main.go"],
        "java" => vec!["Application.java", "Main.java", "App.java"],
        "kotlin" => vec!["Application.kt", "Main.kt"],
        "dotnet" => vec!["Program.cs", "Startup.cs"],
        "ruby" => vec!["config.ru", "app.rb"],
        _ => vec!["main.py", "app.py", "main.rs", "index.ts", "main.go", "Program.cs", "Application.java"],
    };
    let mut found_entries = Vec::new();
    for name in &entry_names {
        find_all_files_recursive(base, name, 4, &mut found_entries);
    }
    found_entries.sort();
    found_entries.dedup();

    // 멀티서비스 감지: 1단계 하위 디렉토리 기준 그룹핑
    let mut service_dirs: std::collections::HashMap<String, Vec<std::path::PathBuf>> = std::collections::HashMap::new();
    for entry in &found_entries {
        if let Ok(rel) = entry.strip_prefix(base) {
            if let Some(first_component) = rel.components().next() {
                let dir_name = first_component.as_os_str().to_string_lossy().to_string();
                service_dirs.entry(dir_name).or_default().push(entry.clone());
            }
        }
    }

    let is_multi_service = service_dirs.len() >= 3;

    if is_multi_service {
        // 멀티서비스 모드: 서비스 맵 생성
        tool_step(app, terminal, step, total_steps, "멀티서비스 감지")?;

        // 사용자가 특정 서비스를 언급했는지 확인 (퍼지 매칭: 언더스코어/하이픈 무시)
        let normalize = |s: &str| -> String {
            s.to_lowercase().replace('_', "").replace('-', "")
        };
        let content_normalized = normalize(content);
        let target_service: Option<String> = service_dirs.keys()
            .find(|dir_name| content_normalized.contains(&normalize(dir_name)))
            .cloned();

        if let Some(ref target) = target_service {
            // 특정 서비스 딥 분석
            context_parts.push(format!("## 멀티서비스 프로젝트 ({}개 서비스 중 `{}` 상세 분석)", service_dirs.len(), target));
            let service_base = base.join(target);
            let max_reads = 10;
            // 해당 서비스 내 엔트리포인트만 깊이 읽기
            for entry_path in found_entries.iter().filter(|p| p.starts_with(&service_base)) {
                if read_count >= max_reads { break; }
                let rel_path = entry_path.strip_prefix(base).unwrap_or(entry_path);
                tool_step(app, terminal, step, total_steps, &format!("{}", rel_path.display()))?;
                if let Ok(fc) = std::fs::read_to_string(entry_path) {
                    let truncated: String = fc.lines().take(max_lines_per_file).collect::<Vec<_>>().join("\n");
                    context_parts.push(format!("## {} (엔트리포인트)\n```\n{}\n```", rel_path.display(), truncated));
                    read_count += 1;
                }
            }
        } else {
            // 전체 서비스 개요: 각 서비스 main.py 첫 5줄 + Dockerfile 유무
            let mut service_map = Vec::new();
            let mut sorted_dirs: Vec<_> = service_dirs.keys().cloned().collect();
            sorted_dirs.sort();
            for dir_name in &sorted_dirs {
                if read_count >= 15 { break; }
                let entries = &service_dirs[dir_name];
                let has_dockerfile = base.join(dir_name).join("Dockerfile").exists();
                let df_mark = if has_dockerfile { " [Dockerfile ✓]" } else { "" };

                if let Some(first_entry) = entries.first() {
                    if let Ok(fc) = std::fs::read_to_string(first_entry) {
                        let preview: String = fc.lines().take(5).collect::<Vec<_>>().join("\n");
                        let rel = first_entry.strip_prefix(base).unwrap_or(first_entry);
                        service_map.push(format!("### {}/{}{}\n```\n{}\n```", dir_name, rel.file_name().unwrap_or_default().to_string_lossy(), df_mark, preview));
                        read_count += 1;
                    }
                }
            }
            context_parts.push(format!(
                "## 멀티서비스 프로젝트 — {}개 서비스 구성\n\n특정 서비스를 상세 분석하려면 서비스명을 지정해주세요.\n\n{}",
                service_dirs.len(), service_map.join("\n\n")
            ));
        }
    } else {
        // 단일서비스: 기존 깊이 분석
        for found in &found_entries {
            if read_count >= 10 { break; }
            let rel_path = found.strip_prefix(base).unwrap_or(found);
            tool_step(app, terminal, step, total_steps, &format!("{}", rel_path.display()))?;
            if let Ok(file_content) = std::fs::read_to_string(found) {
                let truncated: String = file_content.lines().take(max_lines_per_file).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (엔트리포인트)\n```\n{}\n```", rel_path.display(), truncated));
                read_count += 1;
            }
        }
    }

    // === Step 5: 설정 파일 ===
    step += 1;
    let config_names: Vec<&str> = match lang.as_str() {
        "python" => vec!["config.py", "settings.py", ".env.example", "docker-compose.yml"],
        "node" => vec!["vite.config.ts", "next.config.js", "webpack.config.js", ".env.example", "docker-compose.yml"],
        "rust" => vec!["config.rs", ".env.example", "docker-compose.yml"],
        "go" => vec!["config.go", "config.yaml", ".env.example", "docker-compose.yml"],
        "java" | "kotlin" => vec!["application.yml", "application.properties", "docker-compose.yml"],
        "dotnet" => vec!["appsettings.json", "appsettings.Development.json", "docker-compose.yml"],
        _ => vec!["config.py", "config.ts", "config.rs", ".env.example", "docker-compose.yml"],
    };
    let max_reads_limit = if is_multi_service { 15 } else { 10 };
    for name in &config_names {
        if read_count >= max_reads_limit { break; }
        if let Some(found) = find_file_recursive(base, name, 3) {
            let rel_path = found.strip_prefix(base).unwrap_or(&found);
            tool_step(app, terminal, step, total_steps, &format!("{}", rel_path.display()))?;
            if let Ok(file_content) = std::fs::read_to_string(&found) {
                let truncated: String = file_content.lines().take(max_lines_per_file).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (설정)\n```\n{}\n```", rel_path.display(), truncated));
                read_count += 1;
            }
        }
    }

    // === Step 6: 핵심 코드 디렉토리 (단일서비스만) ===
    step += 1;
    if !is_multi_service {
        let code_patterns: Vec<(&str, &str)> = match lang.as_str() {
            "python" => vec![
                ("routers", "라우터"), ("router", "라우터"), ("api", "API"),
                ("services", "서비스"), ("service", "서비스"),
                ("models", "모델"), ("schemas", "스키마"),
                ("orchestrator", "오케스트레이터"), ("handlers", "핸들러"),
                ("core", "코어"), ("middleware", "미들웨어"),
            ],
            "node" => vec![
                ("routes", "라우터"), ("pages", "페이지"), ("views", "뷰"),
                ("components", "컴포넌트"), ("services", "서비스"),
                ("models", "모델"), ("lib", "라이브러리"),
                ("hooks", "훅"), ("store", "상태관리"), ("middleware", "미들웨어"),
            ],
            "java" | "kotlin" => vec![
                ("controller", "컨트롤러"), ("controllers", "컨트롤러"),
                ("service", "서비스"), ("services", "서비스"),
                ("repository", "레포지토리"), ("model", "모델"), ("entity", "엔티티"),
                ("config", "설정"), ("dto", "DTO"),
            ],
            "go" => vec![
                ("handler", "핸들러"), ("handlers", "핸들러"),
                ("service", "서비스"), ("services", "서비스"),
                ("model", "모델"), ("router", "라우터"),
                ("middleware", "미들웨어"), ("cmd", "커맨드"),
            ],
            "dotnet" => vec![
                ("Controllers", "컨트롤러"), ("Services", "서비스"),
                ("Models", "모델"), ("Data", "데이터"),
                ("Middleware", "미들웨어"), ("Hubs", "허브"),
            ],
            "rust" => vec![
                ("routes", "라우터"), ("handlers", "핸들러"),
                ("services", "서비스"), ("models", "모델"),
            ],
            _ => vec![
                ("routes", "라우터"), ("services", "서비스"),
                ("models", "모델"), ("handlers", "핸들러"),
                ("controllers", "컨트롤러"), ("core", "코어"),
            ],
        };
        for (dir_name, label) in &code_patterns {
            if read_count >= max_reads_limit { break; }
            if let Some(dir_path) = find_dir_recursive(base, dir_name, 3) {
                if let Some(biggest) = find_biggest_file(&dir_path) {
                    let rel_path = biggest.strip_prefix(base).unwrap_or(&biggest);
                    tool_step(app, terminal, step, total_steps, &format!("{}", rel_path.display()))?;
                    if let Ok(file_content) = std::fs::read_to_string(&biggest) {
                        let truncated: String = file_content.lines().take(max_lines_per_file).collect::<Vec<_>>().join("\n");
                        context_parts.push(format!("## {} — {} (핵심 코드)\n```\n{}\n```", rel_path.display(), label, truncated));
                        read_count += 1;
                    }
                }
            }
        }
    } else {
        tool_step(app, terminal, step, total_steps, "멀티서비스 — 핵심코드 스킵")?;
    }

    // === Step 7: Git 상태 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "Git 상태 확인")?;
    collect_git_status(base, &mut context_parts);

    Ok((context_parts, read_count))
}

/// Debug intent: 에러 원인 추적용 컨텍스트
fn collect_debug_context(
    base: &std::path::Path,
    content: &str,
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(Vec<String>, usize)> {
    let mut context_parts: Vec<String> = Vec::new();
    let mut read_count = 0;
    let max_reads = 12;
    let max_lines = 50;
    let total_steps = 6;
    let mut step = 0;

    // === Step 1: 프로젝트 트리 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "프로젝트 구조 스캔")?;
    collect_project_tree(base, &mut context_parts);

    // === Step 2: 사용자가 언급한 파일 읽기 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "관련 파일 탐색")?;
    if let Some(mentioned_path) = extract_path_from_message(content) {
        let p = std::path::Path::new(&mentioned_path);
        if p.is_file() {
            if let Ok(fc) = std::fs::read_to_string(p) {
                let rel = p.strip_prefix(base).unwrap_or(p);
                let truncated: String = fc.lines().take(max_lines).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (사용자 지정 파일)\n```\n{}\n```", rel.display(), truncated));
                read_count += 1;
            }
        }
    }

    // === Step 3: 로그 파일 탐색 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "로그 파일 탐색")?;
    let log_patterns = ["*.log", "*.err"];
    for pattern in &log_patterns {
        if read_count >= max_reads { break; }
        let ext = pattern.trim_start_matches('*');
        // logs/ 디렉토리 확인
        let logs_dir = base.join("logs");
        if logs_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&logs_dir) {
                for entry in entries.flatten() {
                    if read_count >= max_reads { break; }
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(ext) {
                        if let Ok(fc) = std::fs::read_to_string(entry.path()) {
                            // 로그는 끝부분이 중요 — 마지막 50줄
                            let lines: Vec<&str> = fc.lines().collect();
                            let start = lines.len().saturating_sub(max_lines);
                            let tail: String = lines[start..].join("\n");
                            context_parts.push(format!("## logs/{} (최근 로그)\n```\n{}\n```", name, tail));
                            read_count += 1;
                        }
                    }
                }
            }
        }
        // 루트의 로그 파일
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                if read_count >= max_reads { break; }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(ext) && entry.path().is_file() {
                    if let Ok(fc) = std::fs::read_to_string(entry.path()) {
                        let lines: Vec<&str> = fc.lines().collect();
                        let start = lines.len().saturating_sub(max_lines);
                        let tail: String = lines[start..].join("\n");
                        context_parts.push(format!("## {} (로그)\n```\n{}\n```", name, tail));
                        read_count += 1;
                    }
                }
            }
        }
    }

    // === Step 4: 환경 설정 (LLM이 런타임 로그 확인할 때 필요) ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "환경 설정 수집")?;
    // Docker/compose 접두사 매칭 + .env.example
    let docker_prefixes = ["Dockerfile", "docker-compose"];
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            if read_count >= max_reads { break; }
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_file() && docker_prefixes.iter().any(|p| name.starts_with(p)) {
                if let Ok(fc) = std::fs::read_to_string(entry.path()) {
                    let truncated: String = fc.lines().take(40).collect::<Vec<_>>().join("\n");
                    context_parts.push(format!("## {} (환경설정)\n```\n{}\n```", name, truncated));
                    read_count += 1;
                }
            }
        }
    }
    if read_count < max_reads {
        let env_example = base.join(".env.example");
        if env_example.exists() {
            if let Ok(fc) = std::fs::read_to_string(&env_example) {
                let truncated: String = fc.lines().take(30).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## .env.example (환경변수)\n```\n{}\n```", truncated));
                read_count += 1;
            }
        }
    }
    // k8s manifest 탐색
    for dir_name in &["k8s", "deploy", "manifests", "kubernetes", "helm"] {
        if read_count >= max_reads { break; }
        if let Some(dir_path) = find_dir_recursive(base, dir_name, 2) {
            if let Some(biggest) = find_biggest_file_any(&dir_path, &["yml", "yaml", "json"]) {
                let rel = biggest.strip_prefix(base).unwrap_or(&biggest);
                if let Ok(fc) = std::fs::read_to_string(&biggest) {
                    let truncated: String = fc.lines().take(40).collect::<Vec<_>>().join("\n");
                    context_parts.push(format!("## {} (k8s/배포)\n```\n{}\n```", rel.display(), truncated));
                    read_count += 1;
                }
            }
        }
    }

    // === Step 5: Git diff (unstaged) + 최근 커밋 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "Git 변경사항 확인")?;
    if let Ok(output) = std::process::Command::new("git")
        .args(["diff", "--stat"])
        .current_dir(base).output()
    {
        if output.status.success() {
            let diff = String::from_utf8_lossy(&output.stdout);
            if !diff.trim().is_empty() {
                context_parts.push(format!("## Git diff (미커밋 변경)\n```\n{}\n```", diff.trim()));
            }
        }
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["log", "--oneline", "-3"])
        .current_dir(base).output()
    {
        if output.status.success() {
            let log = String::from_utf8_lossy(&output.stdout);
            context_parts.push(format!("## Git 최근 커밋\n```\n{}\n```", log.trim()));
        }
    }

    // === Step 6: 의존성 파일 (에러 원인 파악용) ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "의존성 확인")?;
    let dep_files = ["requirements.txt", "pyproject.toml", "package.json", "Cargo.toml", "go.mod"];
    for name in &dep_files {
        if read_count >= max_reads { break; }
        let file_path = base.join(name);
        if file_path.exists() {
            if let Ok(fc) = std::fs::read_to_string(&file_path) {
                let truncated: String = fc.lines().take(30).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (의존성)\n```\n{}\n```", name, truncated));
                read_count += 1;
            }
            break; // 의존성 파일 하나만
        }
    }

    Ok((context_parts, read_count))
}

/// Modify intent: 수정 대상 파일 + 관련 파일 수집
fn collect_modify_context(
    base: &std::path::Path,
    content: &str,
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(Vec<String>, usize)> {
    let mut context_parts: Vec<String> = Vec::new();
    let mut read_count = 0;
    let max_reads = 12;
    let max_lines = 150;
    let total_steps = 6;
    let mut step = 0;

    // === Step 1: 프로젝트 트리 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "프로젝트 구조 스캔")?;
    collect_project_tree(base, &mut context_parts);

    // === Step 2: 사용자가 지정한 파일 읽기 (150줄) ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "대상 파일 읽기")?;
    let mut target_file: Option<std::path::PathBuf> = None;
    if let Some(mentioned_path) = extract_path_from_message(content) {
        let p = std::path::Path::new(&mentioned_path);
        if p.is_file() {
            if let Ok(fc) = std::fs::read_to_string(p) {
                let rel = p.strip_prefix(base).unwrap_or(p);
                let truncated: String = fc.lines().take(max_lines).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (수정 대상)\n```\n{}\n```", rel.display(), truncated));
                read_count += 1;
                target_file = Some(p.to_path_buf());
            }
        }
    }

    // === Step 3: import 관계 파악 (2depth) ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "import 관계 탐색 (2depth)")?;
    if let Some(ref tf) = target_file {
        if let Ok(fc) = std::fs::read_to_string(tf) {
            let imports_depth1 = extract_imports(&fc, tf);
            // depth 1
            for imp_path in &imports_depth1 {
                if read_count >= max_reads { break; }
                let candidate = if imp_path.is_absolute() { imp_path.clone() } else { base.join(imp_path) };
                if candidate.is_file() {
                    if let Ok(imp_content) = std::fs::read_to_string(&candidate) {
                        let rel = candidate.strip_prefix(base).unwrap_or(&candidate);
                        let truncated: String = imp_content.lines().take(60).collect::<Vec<_>>().join("\n");
                        context_parts.push(format!("## {} (import depth-1)\n```\n{}\n```", rel.display(), truncated));
                        read_count += 1;
                        // depth 2: 이 파일의 import도 추적
                        let imports_depth2 = extract_imports(&imp_content, &candidate);
                        for imp2 in imports_depth2.iter().take(2) {
                            if read_count >= max_reads { break; }
                            let c2 = if imp2.is_absolute() { imp2.clone() } else { base.join(imp2) };
                            if c2.is_file() {
                                if let Ok(c2_content) = std::fs::read_to_string(&c2) {
                                    let r2 = c2.strip_prefix(base).unwrap_or(&c2);
                                    let t2: String = c2_content.lines().take(30).collect::<Vec<_>>().join("\n");
                                    context_parts.push(format!("## {} (import depth-2)\n```\n{}\n```", r2.display(), t2));
                                    read_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // === Step 4: 같은 디렉토리 관련 파일 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "관련 파일 탐색")?;
    if let Some(ref tf) = target_file {
        if let Some(parent) = tf.parent() {
            let stem = tf.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    if read_count >= max_reads { break; }
                    let name = entry.file_name().to_string_lossy().to_string();
                    // 같은 접두사 또는 관련 패턴 (예: user.py ↔ user_service.py)
                    if entry.path() != *tf && name.contains(stem) && entry.path().is_file() {
                        if let Ok(fc) = std::fs::read_to_string(entry.path()) {
                            let rel = entry.path().strip_prefix(base).unwrap_or(&entry.path()).to_path_buf();
                            let truncated: String = fc.lines().take(40).collect::<Vec<_>>().join("\n");
                            context_parts.push(format!("## {} (관련 파일)\n```\n{}\n```", rel.display(), truncated));
                            read_count += 1;
                        }
                    }
                }
            }
        }
    }

    // === Step 5: 테스트 파일 탐색 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "테스트 파일 탐색")?;
    if let Some(ref tf) = target_file {
        if let Some(test_file) = find_test_file(base, tf) {
            if read_count < max_reads {
                if let Ok(fc) = std::fs::read_to_string(&test_file) {
                    let rel = test_file.strip_prefix(base).unwrap_or(&test_file);
                    let truncated: String = fc.lines().take(60).collect::<Vec<_>>().join("\n");
                    context_parts.push(format!("## {} (테스트)\n```\n{}\n```", rel.display(), truncated));
                    read_count += 1;
                }
            }
        }
    }

    // === Step 6: Git diff ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "Git 변경사항 확인")?;
    if let Some(ref tf) = target_file {
        let rel = tf.strip_prefix(base).unwrap_or(tf);
        if let Ok(output) = std::process::Command::new("git")
            .args(["diff", &rel.to_string_lossy()])
            .current_dir(base).output()
        {
            if output.status.success() {
                let diff = String::from_utf8_lossy(&output.stdout);
                if !diff.trim().is_empty() {
                    context_parts.push(format!("## Git diff ({})\n```\n{}\n```", rel.display(), diff.trim()));
                }
            }
        }
    } else {
        collect_git_status(base, &mut context_parts);
    }

    Ok((context_parts, read_count))
}

/// Deploy intent: 배포/인프라 설정 수집
fn collect_deploy_context(
    base: &std::path::Path,
    _content: &str,
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(Vec<String>, usize)> {
    let mut context_parts: Vec<String> = Vec::new();
    let mut read_count = 0;
    let max_reads = 10;
    let max_lines = 40;
    let total_steps = 5;
    let mut step = 0;

    // === Step 1: 프로젝트 트리 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "프로젝트 구조 스캔")?;
    collect_project_tree(base, &mut context_parts);

    // === Step 2: Dockerfile / docker-compose (접두사 매칭) ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "Docker 설정 탐색")?;
    let docker_prefixes = ["Dockerfile", "docker-compose"];
    if let Ok(entries) = std::fs::read_dir(base) {
        let mut docker_files: Vec<_> = entries.flatten()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                e.path().is_file() && docker_prefixes.iter().any(|p| name.starts_with(p))
            })
            .collect();
        docker_files.sort_by_key(|e| e.file_name());
        for entry in docker_files {
            if read_count >= max_reads { break; }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(fc) = std::fs::read_to_string(entry.path()) {
                let truncated: String = fc.lines().take(max_lines).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (Docker)\n```\n{}\n```", name, truncated));
                read_count += 1;
            }
        }
    }

    // === Step 3: CI/CD 설정 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "CI/CD 설정 탐색")?;
    // GitHub Actions
    let ci_dirs = [".github/workflows", ".gitlab", "pipelines"];
    for ci_dir in &ci_dirs {
        let dir_path = base.join(ci_dir);
        if dir_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&dir_path) {
                for entry in entries.flatten() {
                    if read_count >= max_reads { break; }
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".yml") || name.ends_with(".yaml") {
                        if let Ok(fc) = std::fs::read_to_string(entry.path()) {
                            let truncated: String = fc.lines().take(max_lines).collect::<Vec<_>>().join("\n");
                            context_parts.push(format!("## {}/{} (CI/CD)\n```\n{}\n```", ci_dir, name, truncated));
                            read_count += 1;
                        }
                    }
                }
            }
        }
    }
    // Jenkinsfile, Makefile
    for name in &["Jenkinsfile", "Makefile", "azure-pipelines.yml"] {
        if read_count >= max_reads { break; }
        let fp = base.join(name);
        if fp.exists() {
            if let Ok(fc) = std::fs::read_to_string(&fp) {
                let truncated: String = fc.lines().take(max_lines).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (CI/CD)\n```\n{}\n```", name, truncated));
                read_count += 1;
            }
        }
    }

    // === Step 4: k8s / Helm / Terraform ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "k8s/IaC 설정 탐색")?;
    let infra_dirs = ["k8s", "deploy", "manifests", "kubernetes", "helm", "charts", "terraform", "infra"];
    for dir_name in &infra_dirs {
        if read_count >= max_reads { break; }
        if let Some(dir_path) = find_dir_recursive(base, dir_name, 2) {
            if let Ok(entries) = std::fs::read_dir(&dir_path) {
                for entry in entries.flatten() {
                    if read_count >= max_reads { break; }
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".yml") || name.ends_with(".yaml") || name.ends_with(".tf") || name.ends_with(".json") {
                        if let Ok(fc) = std::fs::read_to_string(entry.path()) {
                            let truncated: String = fc.lines().take(max_lines).collect::<Vec<_>>().join("\n");
                            context_parts.push(format!("## {}/{} (인프라)\n```\n{}\n```", dir_name, name, truncated));
                            read_count += 1;
                        }
                    }
                }
            }
        }
    }

    // === Step 5: 빌드 설정 + Git ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "빌드 설정 확인")?;
    let build_files = ["pyproject.toml", "package.json", "Cargo.toml", "go.mod", "pom.xml", "build.gradle"];
    for name in &build_files {
        if read_count >= max_reads { break; }
        let fp = base.join(name);
        if fp.exists() {
            if let Ok(fc) = std::fs::read_to_string(&fp) {
                let truncated: String = fc.lines().take(30).collect::<Vec<_>>().join("\n");
                context_parts.push(format!("## {} (빌드 설정)\n```\n{}\n```", name, truncated));
                read_count += 1;
            }
            break; // 하나만
        }
    }
    collect_git_status(base, &mut context_parts);

    Ok((context_parts, read_count))
}

// ============================================================
// 공통 헬퍼 (수집 함수에서 공유)
// ============================================================

/// Data intent: 데이터 파일 감지 + 스키마/샘플 수집
fn collect_data_context(
    base: &std::path::Path,
    _content: &str,
    app: &mut TuiApp<'_>,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(Vec<String>, usize)> {
    let mut context_parts: Vec<String> = Vec::new();
    let mut read_count = 0;
    let max_reads = 10;
    let total_steps = 4;
    let mut step = 0;

    // === Step 1: 프로젝트 트리 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "프로젝트 구조 스캔")?;
    collect_project_tree(base, &mut context_parts);

    // === Step 2: 데이터 파일 탐색 (CSV, JSON, XLSX, TSV) ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "데이터 파일 탐색")?;
    let data_exts = ["csv", "json", "xlsx", "xls", "tsv", "parquet"];
    let mut data_files: Vec<std::path::PathBuf> = Vec::new();
    // 루트 + 1depth 탐색
    fn scan_data_files(dir: &std::path::Path, exts: &[&str], results: &mut Vec<std::path::PathBuf>, depth: usize) {
        if depth > 2 { return; }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                if entry.path().is_file() {
                    if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                        if exts.contains(&ext) {
                            results.push(entry.path());
                        }
                    }
                } else if entry.path().is_dir() && depth < 2 {
                    let skip = ["node_modules", "target", "__pycache__", ".git", "venv", ".venv"];
                    if !skip.contains(&name.as_str()) {
                        scan_data_files(&entry.path(), exts, results, depth + 1);
                    }
                }
            }
        }
    }
    scan_data_files(base, &data_exts, &mut data_files, 0);
    data_files.sort();

    // 파일 목록 + 크기
    if !data_files.is_empty() {
        let file_list: Vec<String> = data_files.iter().map(|p| {
            let rel = p.strip_prefix(base).unwrap_or(p);
            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            let size_str = if size > 1_000_000 { format!("{:.1}MB", size as f64 / 1_000_000.0) }
                else if size > 1_000 { format!("{:.1}KB", size as f64 / 1_000.0) }
                else { format!("{}B", size) };
            format!("- {} ({})", rel.display(), size_str)
        }).collect();
        context_parts.push(format!("## 데이터 파일 목록 ({}개)\n{}", data_files.len(), file_list.join("\n")));
    }

    // === Step 3: CSV/TSV 헤더 + 첫 5행, JSON 구조 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "스키마/샘플 읽기")?;
    for file in &data_files {
        if read_count >= max_reads { break; }
        let rel = file.strip_prefix(base).unwrap_or(file);
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "csv" | "tsv" => {
                if let Ok(fc) = std::fs::read_to_string(file) {
                    let sample: String = fc.lines().take(6).collect::<Vec<_>>().join("\n");
                    context_parts.push(format!("## {} (헤더+샘플)\n```\n{}\n```", rel.display(), sample));
                    read_count += 1;
                }
            }
            "json" => {
                if let Ok(fc) = std::fs::read_to_string(file) {
                    // JSON 첫 30줄 (구조 파악)
                    let sample: String = fc.lines().take(30).collect::<Vec<_>>().join("\n");
                    context_parts.push(format!("## {} (JSON 구조)\n```json\n{}\n```", rel.display(), sample));
                    read_count += 1;
                }
            }
            "xlsx" | "xls" => {
                // 바이너리라 내용 못 읽음 — 파일 존재만 기록
                context_parts.push(format!("## {} (Excel — read 도구로 내용 확인 필요)", rel.display()));
            }
            _ => {}
        }
    }

    // === Step 4: 관련 스크립트 탐색 ===
    step += 1;
    tool_step(app, terminal, step, total_steps, "분석 스크립트 탐색")?;
    let script_patterns = ["analyze", "analysis", "report", "chart", "plot", "etl", "transform", "pipeline"];
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            if read_count >= max_reads { break; }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let is_script = name.ends_with(".py") || name.ends_with(".r") || name.ends_with(".ipynb");
            if is_script && script_patterns.iter().any(|p| name.contains(p)) {
                if let Ok(fc) = std::fs::read_to_string(entry.path()) {
                    let rel = entry.path().strip_prefix(base).unwrap_or(&entry.path()).to_path_buf();
                    let truncated: String = fc.lines().take(40).collect::<Vec<_>>().join("\n");
                    context_parts.push(format!("## {} (분석 스크립트)\n```\n{}\n```", rel.display(), truncated));
                    read_count += 1;
                }
            }
        }
    }
    collect_git_status(base, &mut context_parts);

    Ok((context_parts, read_count))
}

/// 스킵할 디렉토리 목록
const SKIP_DIRS: &[&str] = &[
    "node_modules", "target", "__pycache__", ".git", ".claude", ".goose",
    "venv", ".venv", "dist", "build", "scripts", "tests", "docs",
    "patches", "static", "data", ".github", ".vscode",
];

/// 프로젝트 트리 (1 depth) 수집
fn collect_project_tree(base: &std::path::Path, context_parts: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(base) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if !SKIP_DIRS.contains(&name.as_str()) {
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
}

/// Git 상태 수집 (최근 5커밋 + 현재 브랜치)
fn collect_git_status(base: &std::path::Path, context_parts: &mut Vec<String>) {
    if let Ok(output) = std::process::Command::new("git")
        .args(["log", "--oneline", "-5"])
        .current_dir(base).output()
    {
        if output.status.success() {
            let log = String::from_utf8_lossy(&output.stdout);
            if !log.trim().is_empty() {
                context_parts.push(format!("## Git 최근 커밋\n```\n{}\n```", log.trim()));
            }
        }
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(base).output()
    {
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout);
            if !branch.trim().is_empty() {
                context_parts.push(format!("현재 브랜치: `{}`", branch.trim()));
            }
        }
    }
}

/// 소스 파일에서 import 경로 추출 (Python/Node/Rust)
fn extract_imports(content: &str, file_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut imports = Vec::new();
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let parent = file_path.parent().unwrap_or(std::path::Path::new("."));

    for line in content.lines() {
        let trimmed = line.trim();
        match ext {
            "py" => {
                // from xxx.yyy import zzz → xxx/yyy.py
                if trimmed.starts_with("from ") {
                    if let Some(module) = trimmed.strip_prefix("from ").and_then(|s| s.split_whitespace().next()) {
                        if !module.starts_with('.') && !module.contains("stdlib") {
                            let path = parent.join(module.replace('.', "/")).with_extension("py");
                            imports.push(path);
                        }
                    }
                }
            }
            "ts" | "js" | "tsx" | "jsx" => {
                // import ... from './xxx' → xxx.ts
                if trimmed.contains("from '") || trimmed.contains("from \"") {
                    let parts: Vec<&str> = trimmed.split("from ").collect();
                    if parts.len() > 1 {
                        let module = parts[1].trim().trim_matches(|c| c == '\'' || c == '"' || c == ';');
                        if module.starts_with('.') {
                            let path = parent.join(module);
                            // .ts, .tsx, .js 순서로 시도
                            for try_ext in &["ts", "tsx", "js", "jsx"] {
                                let candidate = path.with_extension(try_ext);
                                if candidate.exists() {
                                    imports.push(candidate);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    imports.truncate(5); // 최대 5개만
    imports
}

/// 대상 파일에 대응하는 테스트 파일 찾기
fn find_test_file(base: &std::path::Path, file_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let stem = file_path.file_stem()?.to_str()?;
    let ext = file_path.extension()?.to_str()?;

    let test_names: Vec<String> = match ext {
        "py" => vec![format!("test_{}.py", stem), format!("{}_test.py", stem)],
        "ts" | "tsx" => vec![format!("{}.test.ts", stem), format!("{}.test.tsx", stem), format!("{}.spec.ts", stem)],
        "js" | "jsx" => vec![format!("{}.test.js", stem), format!("{}.test.jsx", stem), format!("{}.spec.js", stem)],
        "rs" => vec![format!("{}_test.rs", stem)],
        "go" => vec![format!("{}_test.go", stem)],
        "java" | "kt" => vec![format!("{}Test.java", stem), format!("{}Test.kt", stem)],
        _ => return None,
    };

    // 같은 디렉토리에서 먼저 찾기
    if let Some(parent) = file_path.parent() {
        for name in &test_names {
            let candidate = parent.join(name);
            if candidate.exists() { return Some(candidate); }
        }
    }

    // tests/ 디렉토리에서 찾기
    for name in &test_names {
        if let Some(found) = find_file_recursive(base, name, 3) {
            return Some(found);
        }
    }

    None
}

/// 지정한 확장자 중 가장 큰 파일 찾기 (k8s manifest 등)
fn find_biggest_file_any(dir: &std::path::Path, exts: &[&str]) -> Option<std::path::PathBuf> {
    let mut biggest: Option<(std::path::PathBuf, u64)> = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !exts.contains(&ext) { continue; }
            if let Ok(meta) = std::fs::metadata(&path) {
                let size = meta.len();
                if biggest.as_ref().map_or(true, |(_, s)| size > *s) {
                    biggest = Some((path, size));
                }
            }
        }
    }
    biggest.map(|(p, _)| p)
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

/// 디렉토리 재귀 검색
fn find_dir_recursive(base: &std::path::Path, name: &str, max_depth: usize) -> Option<std::path::PathBuf> {
    find_dir_recursive_inner(base, name, 0, max_depth)
}

fn find_dir_recursive_inner(dir: &std::path::Path, name: &str, depth: usize, max_depth: usize) -> Option<std::path::PathBuf> {
    if depth > max_depth { return None; }
    let skip = ["node_modules", "target", "__pycache__", ".git", "venv", ".venv", "dist", "build", "scripts", "tests", "docs", "patches", "static", "data"];

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if skip.contains(&dir_name.as_str()) { continue; }
                if dir_name == name {
                    return Some(entry.path());
                }
                if let Some(found) = find_dir_recursive_inner(&entry.path(), name, depth + 1, max_depth) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// 디렉토리에서 가장 큰 .py/.rs/.ts/.js/.go/.java 파일 찾기
fn find_biggest_file(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let code_exts = ["py", "rs", "ts", "js", "go", "java", "kt", "rb"];
    let mut biggest: Option<(std::path::PathBuf, u64)> = None;

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !code_exts.contains(&ext) { continue; }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("__") || name.starts_with("test_") { continue; }
            if let Ok(meta) = std::fs::metadata(&path) {
                let size = meta.len();
                if biggest.as_ref().map_or(true, |(_, s)| size > *s) {
                    biggest = Some((path, size));
                }
            }
        }
    }
    biggest.map(|(p, _)| p)
}

/// 파일 이름으로 모든 매치를 재귀 탐색
fn find_all_files_recursive(dir: &std::path::Path, name: &str, max_depth: usize, results: &mut Vec<std::path::PathBuf>) {
    find_all_files_recursive_inner(dir, name, 0, max_depth, results);
}

fn find_all_files_recursive_inner(dir: &std::path::Path, name: &str, depth: usize, max_depth: usize, results: &mut Vec<std::path::PathBuf>) {
    if depth > max_depth { return; }
    let skip = ["node_modules", "target", "__pycache__", ".git", "venv", ".venv", "dist", "build", "scripts", "tests", "docs", "patches", "static", "data"];

    let direct = dir.join(name);
    if direct.exists() {
        results.push(direct);
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if skip.contains(&dir_name.as_str()) || dir_name.starts_with('.') { continue; }
                find_all_files_recursive_inner(&entry.path(), name, depth + 1, max_depth, results);
            }
        }
    }
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
                if ["node_modules", "target", "__pycache__", ".git", "venv", ".venv", "dist", "build", "scripts", "tests", "docs", "patches", "static", "data"].contains(&dir_name.as_str()) {
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
