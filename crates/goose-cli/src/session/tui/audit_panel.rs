//! 감사 로그 뷰어 패널
//!
//! F6으로 토글되는 감사 로그 조회 패널
//! Phase 4: 감사 로그 시스템

use chrono::{Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use goose::audit::event::{AuditEvent, AuditEventData, AuditEventType, SecuritySeverity};
use goose::config::paths::Paths;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
    Frame,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// 뷰 모드
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AuditView {
    Summary,
    Timeline,
    Tokens,
    Pii,
    Security,
}

impl AuditView {
    fn label(&self) -> &'static str {
        match self {
            AuditView::Summary => "요약",
            AuditView::Timeline => "타임라인",
            AuditView::Tokens => "토큰",
            AuditView::Pii => "PII",
            AuditView::Security => "보안",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            AuditView::Summary => "📊",
            AuditView::Timeline => "📋",
            AuditView::Tokens => "🔢",
            AuditView::Pii => "🔒",
            AuditView::Security => "🛡️",
        }
    }
}

/// 감사 로그 뷰어 패널
pub struct AuditPanel {
    /// 패널 표시 여부
    pub visible: bool,
    /// 현재 선택된 뷰
    pub active_view: AuditView,
    /// 일수 필터
    pub days: u32,
    /// 현재 세션 ID (타임라인 뷰용)
    pub current_session_id: Option<String>,
    /// 이벤트 캐시
    events: Vec<AuditEvent>,
    /// 리스트 상태
    list_state: ListState,
    /// 상태 메시지
    status_message: Option<String>,
    /// 로딩 중
    loading: bool,
}

impl AuditPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            active_view: AuditView::Summary,
            days: 7,
            current_session_id: None,
            events: Vec::new(),
            list_state: ListState::default(),
            status_message: None,
            loading: false,
        }
    }

    /// 패널 열기
    pub fn open(&mut self) {
        self.visible = true;
        self.load_events();
    }

    /// 패널 닫기
    pub fn close(&mut self) {
        self.visible = false;
        self.status_message = None;
    }

    /// 토글
    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    /// 이벤트 로드
    fn load_events(&mut self) {
        self.loading = true;
        self.events.clear();

        let log_dir = Paths::in_state_dir("logs").join("audit");
        if !log_dir.exists() {
            self.status_message = Some("감사 로그 없음".to_string());
            self.loading = false;
            return;
        }

        // 날짜 필터링
        let cutoff = Local::now().date_naive() - chrono::Duration::days(self.days as i64);

        // 파일 목록
        let files: Vec<PathBuf> = match std::fs::read_dir(&log_dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("audit.") && n.ends_with(".jsonl"))
                        .unwrap_or(false)
                })
                .filter(|p| {
                    let date_str = p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .and_then(|n| n.strip_prefix("audit."))
                        .and_then(|n| n.strip_suffix(".jsonl"))
                        .unwrap_or("");
                    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                        .map(|d| d >= cutoff)
                        .unwrap_or(false)
                })
                .collect(),
            Err(_) => {
                self.status_message = Some("로그 디렉토리 읽기 실패".to_string());
                self.loading = false;
                return;
            }
        };

        // 이벤트 로드
        for file_path in files {
            if let Ok(file) = File::open(&file_path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    if let Ok(event) = serde_json::from_str::<AuditEvent>(&line) {
                        self.events.push(event);
                    }
                }
            }
        }

        // 시간순 정렬
        self.events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        self.status_message = Some(format!("{}일간 {} 이벤트", self.days, self.events.len()));
        self.loading = false;

        // 리스트 초기화
        if !self.events.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    /// 새로고침
    pub fn refresh(&mut self) {
        self.load_events();
    }

    /// 다음 뷰로 이동
    pub fn next_view(&mut self) {
        self.active_view = match self.active_view {
            AuditView::Summary => AuditView::Timeline,
            AuditView::Timeline => AuditView::Tokens,
            AuditView::Tokens => AuditView::Pii,
            AuditView::Pii => AuditView::Security,
            AuditView::Security => AuditView::Summary,
        };
    }

    /// 이전 뷰로 이동
    pub fn prev_view(&mut self) {
        self.active_view = match self.active_view {
            AuditView::Summary => AuditView::Security,
            AuditView::Timeline => AuditView::Summary,
            AuditView::Tokens => AuditView::Timeline,
            AuditView::Pii => AuditView::Tokens,
            AuditView::Security => AuditView::Pii,
        };
    }

    /// 리스트 위로
    pub fn scroll_up(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i > 0 {
                    i - 1
                } else {
                    0
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// 리스트 아래로
    pub fn scroll_down(&mut self, max: usize) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i < max.saturating_sub(1) {
                    i + 1
                } else {
                    i
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// 키 입력 처리
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Esc: 닫기
        if key.code == KeyCode::Esc {
            self.close();
            return true;
        }

        // Ctrl+R: 새로고침
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R'))
        {
            self.refresh();
            return true;
        }

        // Tab: 다음 뷰
        if key.code == KeyCode::Tab && !key.modifiers.contains(KeyModifiers::SHIFT) {
            self.next_view();
            return true;
        }

        // Shift+Tab: 이전 뷰
        if key.code == KeyCode::BackTab
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            self.prev_view();
            return true;
        }

        // 화살표: 스크롤
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_up();
                return true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.get_current_item_count();
                self.scroll_down(max);
                return true;
            }
            KeyCode::PageUp => {
                for _ in 0..10 {
                    self.scroll_up();
                }
                return true;
            }
            KeyCode::PageDown => {
                let max = self.get_current_item_count();
                for _ in 0..10 {
                    self.scroll_down(max);
                }
                return true;
            }
            _ => {}
        }

        // +/-: 일수 조정
        match key.code {
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.days = (self.days + 7).min(90);
                self.refresh();
                return true;
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                self.days = (self.days.saturating_sub(7)).max(1);
                self.refresh();
                return true;
            }
            _ => {}
        }

        false
    }

    /// 현재 뷰의 아이템 수
    fn get_current_item_count(&self) -> usize {
        match self.active_view {
            AuditView::Summary => 10,
            AuditView::Timeline => self.events.len(),
            AuditView::Tokens => 30,
            AuditView::Pii => self
                .events
                .iter()
                .filter(|e| matches!(e.data, AuditEventData::PiiMasked(_)))
                .count(),
            AuditView::Security => self
                .events
                .iter()
                .filter(|e| matches!(e.data, AuditEventData::SecurityEvent(_)))
                .count(),
        }
    }

    /// 렌더링
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // 배경 클리어
        frame.render_widget(Clear, area);

        // 패널 블록
        let block = Block::default()
            .title(" 감사 로그 [F6] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // 내부 레이아웃: 탭 + 컨텐츠 + 상태바
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // 탭
                Constraint::Min(5),    // 컨텐츠
                Constraint::Length(1), // 상태바
            ])
            .split(inner);

        // 탭 렌더링
        self.render_tabs(frame, chunks[0]);

        // 컨텐츠 렌더링
        self.render_content(frame, chunks[1]);

        // 상태바 렌더링
        self.render_status_bar(frame, chunks[2]);
    }

    /// 탭 렌더링
    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let views = [
            AuditView::Summary,
            AuditView::Timeline,
            AuditView::Tokens,
            AuditView::Pii,
            AuditView::Security,
        ];

        let titles: Vec<Line> = views
            .iter()
            .map(|view| Line::from(format!(" {} {} ", view.icon(), view.label())))
            .collect();

        let selected = views.iter().position(|v| *v == self.active_view).unwrap_or(0);

        let tabs = Tabs::new(titles)
            .select(selected)
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("|");

        frame.render_widget(tabs, area);
    }

    /// 컨텐츠 렌더링
    fn render_content(&mut self, frame: &mut Frame, area: Rect) {
        match self.active_view {
            AuditView::Summary => self.render_summary(frame, area),
            AuditView::Timeline => self.render_timeline(frame, area),
            AuditView::Tokens => self.render_tokens(frame, area),
            AuditView::Pii => self.render_pii(frame, area),
            AuditView::Security => self.render_security(frame, area),
        }
    }

    /// 요약 뷰
    fn render_summary(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        // 통계 계산
        let total = self.events.len();
        let sessions = self
            .events
            .iter()
            .filter(|e| matches!(e.data, AuditEventData::SessionStart(_)))
            .count();

        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        let mut tool_calls = 0usize;
        let mut pii_masked = 0usize;
        let mut security_events = 0usize;

        let mut event_counts: HashMap<String, usize> = HashMap::new();

        for event in &self.events {
            *event_counts
                .entry(event.event_type.to_string())
                .or_insert(0) += 1;

            match &event.data {
                AuditEventData::ApiResponse(data) => {
                    input_tokens += data.usage.input;
                    output_tokens += data.usage.output;
                }
                AuditEventData::ToolExecution(_) => {
                    tool_calls += 1;
                }
                AuditEventData::PiiMasked(data) => {
                    pii_masked += data.masked_count;
                }
                AuditEventData::SecurityEvent(_) => {
                    security_events += 1;
                }
                _ => {}
            }
        }

        lines.push(Line::from(vec![
            Span::styled(
                format!("📊 감사 로그 요약 (최근 {}일)", self.days),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::raw("  세션 수: "),
            Span::styled(
                format!("{}", sessions),
                Style::default().fg(Color::Yellow),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  총 이벤트: "),
            Span::styled(format!("{}", total), Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::from(""));

        lines.push(Line::from(vec![Span::styled(
            "  토큰 사용량:",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![
            Span::raw("    입력: "),
            Span::styled(
                format_tokens(input_tokens),
                Style::default().fg(Color::Green),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("    출력: "),
            Span::styled(
                format_tokens(output_tokens),
                Style::default().fg(Color::Green),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("    합계: "),
            Span::styled(
                format_tokens(input_tokens + output_tokens),
                Style::default().fg(Color::Cyan),
            ),
        ]));
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::raw("  도구 호출: "),
            Span::styled(
                format!("{}", tool_calls),
                Style::default().fg(Color::Blue),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  PII 마스킹: "),
            Span::styled(
                format!("{} 건", pii_masked),
                Style::default().fg(Color::Magenta),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  보안 이벤트: "),
            Span::styled(
                format!("{} 건", security_events),
                if security_events > 0 {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
        ]));

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray)),
        );
        frame.render_widget(paragraph, area);
    }

    /// 타임라인 뷰
    fn render_timeline(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .events
            .iter()
            .map(|event| {
                let time = event.timestamp.with_timezone(&Local).format("%m-%d %H:%M:%S");
                let icon = get_event_icon(&event.event_type);
                let summary = get_event_summary(event);

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", time),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(format!("{} ", icon)),
                    Span::raw(summary),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" 이벤트 타임라인 ({}) ", self.events.len()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// 토큰 뷰
    fn render_tokens(&self, frame: &mut Frame, area: Rect) {
        // 일별 토큰 집계
        let mut daily: HashMap<String, (u64, u64)> = HashMap::new();

        for event in &self.events {
            if let AuditEventData::ApiResponse(data) = &event.data {
                let date = event
                    .timestamp
                    .with_timezone(&Local)
                    .format("%Y-%m-%d")
                    .to_string();
                let entry = daily.entry(date).or_insert((0, 0));
                entry.0 += data.usage.input;
                entry.1 += data.usage.output;
            }
        }

        let mut dates: Vec<_> = daily.keys().cloned().collect();
        dates.sort_by(|a, b| b.cmp(a));

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("  날짜         ", Style::default().fg(Color::Gray)),
            Span::styled("   입력    ", Style::default().fg(Color::Gray)),
            Span::styled("   출력    ", Style::default().fg(Color::Gray)),
            Span::styled("    합계", Style::default().fg(Color::Gray)),
        ]));
        lines.push(Line::from(
            "  ─────────────────────────────────────────────",
        ));

        for date in dates {
            if let Some((input, output)) = daily.get(&date) {
                lines.push(Line::from(vec![
                    Span::raw(format!("  {}   ", date)),
                    Span::styled(
                        format!("{:>8}  ", format_tokens(*input)),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(
                        format!("{:>8}  ", format_tokens(*output)),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        format!("{:>8}", format_tokens(input + output)),
                        Style::default().fg(Color::Cyan),
                    ),
                ]));
            }
        }

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .title(" 일별 토큰 사용량 ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray)),
        );
        frame.render_widget(paragraph, area);
    }

    /// PII 뷰
    fn render_pii(&mut self, frame: &mut Frame, area: Rect) {
        let pii_events: Vec<&AuditEvent> = self
            .events
            .iter()
            .filter(|e| matches!(e.data, AuditEventData::PiiMasked(_)))
            .collect();

        let items: Vec<ListItem> = pii_events
            .iter()
            .map(|event| {
                let time = event.timestamp.with_timezone(&Local).format("%m-%d %H:%M");
                if let AuditEventData::PiiMasked(data) = &event.data {
                    let types: Vec<String> = data.items.iter().map(|i| i.pii_type.clone()).collect();
                    let types_str = if types.len() > 3 {
                        format!("{} 외 {}개", types[..3].join(", "), types.len() - 3)
                    } else {
                        types.join(", ")
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("[{}] ", time),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled("🔒 ", Style::default()),
                        Span::styled(
                            format!("{} 건", data.masked_count),
                            Style::default().fg(Color::Magenta),
                        ),
                        Span::raw(format!(" - {}", types_str)),
                    ]))
                } else {
                    ListItem::new(Line::from(""))
                }
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" PII 마스킹 이력 ({}) ", pii_events.len()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// 보안 뷰
    fn render_security(&mut self, frame: &mut Frame, area: Rect) {
        let security_events: Vec<&AuditEvent> = self
            .events
            .iter()
            .filter(|e| matches!(e.data, AuditEventData::SecurityEvent(_)))
            .collect();

        if security_events.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  ✅ 보안 이벤트가 없습니다.",
                    Style::default().fg(Color::Green),
                )]),
            ];
            let paragraph = Paragraph::new(lines).block(
                Block::default()
                    .title(" 보안 이벤트 ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray)),
            );
            frame.render_widget(paragraph, area);
            return;
        }

        let items: Vec<ListItem> = security_events
            .iter()
            .map(|event| {
                let time = event.timestamp.with_timezone(&Local).format("%m-%d %H:%M");
                if let AuditEventData::SecurityEvent(data) = &event.data {
                    let (icon, color) = match data.severity {
                        SecuritySeverity::Info => ("ℹ️", Color::Blue),
                        SecuritySeverity::Warning => ("⚠️", Color::Yellow),
                        SecuritySeverity::Error => ("❌", Color::Red),
                        SecuritySeverity::Critical => ("🚨", Color::LightRed),
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("[{}] ", time),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::raw(format!("{} ", icon)),
                        Span::styled(&data.event_name, Style::default().fg(color)),
                        Span::raw(" - "),
                        Span::styled(
                            data.action_taken.clone().unwrap_or_default(),
                            Style::default().fg(Color::Gray),
                        ),
                    ]))
                } else {
                    ListItem::new(Line::from(""))
                }
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" 보안 이벤트 ({}) ", security_events.len()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// 상태바 렌더링
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let mut spans = vec![
            Span::styled(" Ctrl+R", Style::default().fg(Color::Green)),
            Span::raw(":새로고침  "),
            Span::styled("Tab", Style::default().fg(Color::Green)),
            Span::raw(":탭전환  "),
            Span::styled("↑↓", Style::default().fg(Color::Green)),
            Span::raw(":스크롤  "),
            Span::styled("+/-", Style::default().fg(Color::Green)),
            Span::raw(":일수  "),
            Span::styled("Esc", Style::default().fg(Color::Green)),
            Span::raw(":닫기"),
        ];

        if let Some(msg) = &self.status_message {
            spans.push(Span::raw("  │  "));
            spans.push(Span::styled(msg, Style::default().fg(Color::Cyan)));
        }

        let status = Paragraph::new(Line::from(spans));
        frame.render_widget(status, area);
    }
}

impl Default for AuditPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Helper functions
// ============================================================

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

fn get_event_icon(event_type: &AuditEventType) -> &'static str {
    match event_type {
        AuditEventType::SessionStart => "🚀",
        AuditEventType::SessionEnd => "🏁",
        AuditEventType::UserInput => "📝",
        AuditEventType::PiiMasked => "🔒",
        AuditEventType::PiiUnmasked => "🔓",
        AuditEventType::ApiRequest => "📤",
        AuditEventType::ApiResponse => "📥",
        AuditEventType::ToolExecution => "🔧",
        AuditEventType::HookExecution => "🪝",
        AuditEventType::SecurityEvent => "🛡️",
    }
}

fn get_event_summary(event: &AuditEvent) -> String {
    match &event.data {
        AuditEventData::SessionStart(data) => {
            format!("세션 시작 ({})", truncate(&data.working_directory, 30))
        }
        AuditEventData::SessionEnd(data) => {
            format!(
                "세션 종료 ({}초, {} tokens)",
                data.duration_secs,
                data.total_tokens.input + data.total_tokens.output
            )
        }
        AuditEventData::UserInput(data) => {
            format!("입력: \"{}\"", truncate(&data.content_masked.replace('\n', " "), 40))
        }
        AuditEventData::PiiMasked(data) => {
            format!("PII 마스킹 {} 건", data.masked_count)
        }
        AuditEventData::PiiUnmasked(data) => {
            format!("PII 언마스킹 {} 건", data.tokens.len())
        }
        AuditEventData::ApiRequest(data) => {
            format!("→ {} ({})", data.model, data.provider)
        }
        AuditEventData::ApiResponse(data) => {
            format!(
                "← {} tokens ({}ms)",
                data.usage.input + data.usage.output,
                data.latency_ms
            )
        }
        AuditEventData::ToolExecution(data) => {
            format!("도구: {} ({:?})", data.tool_name, data.result_status)
        }
        AuditEventData::HookExecution(data) => {
            format!(
                "Hook: {} ({})",
                data.hook_name,
                if data.success { "성공" } else { "실패" }
            )
        }
        AuditEventData::SecurityEvent(data) => {
            format!("{} - {:?}", data.event_name, data.severity)
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_len - 3).collect::<String>())
    }
}
