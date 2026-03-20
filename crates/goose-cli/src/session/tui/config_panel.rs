//! 설정 패널
//!
//! F7으로 토글되는 런타임 설정 패널
//! PII 마스킹 화이트리스트, 카테고리 비활성화 등 즉시 적용

use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use goose::config::GooseMode;
use goose::security::pii_patterns::MaskType;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
    Frame,
};

/// 설정 탭
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ConfigTab {
    General,
    Pii,
    Advanced,
}

impl ConfigTab {
    fn label(&self) -> &'static str {
        match self {
            ConfigTab::General => "General",
            ConfigTab::Pii => "PII 마스킹",
            ConfigTab::Advanced => "고급",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            ConfigTab::General => "⚙️",
            ConfigTab::Pii => "🔒",
            ConfigTab::Advanced => "🔧",
        }
    }
}

/// 설정 변경 이벤트 (tui_session에서 처리)
#[derive(Debug, Clone)]
pub enum ConfigChange {
    ModeChanged(GooseMode),
    PiiToggled(bool),
    PiiWhitelistUpdated(Vec<String>),
    PiiDisabledTypesUpdated(HashSet<MaskType>),
    MaxTokensChanged(u32),
    MaxTurnsChanged(u32),
    AuditToggled(bool),
}

/// 화이트리스트 편집 모드
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhitelistEditMode {
    /// 목록 탐색
    Browse,
    /// 새 항목 입력 중
    Adding,
}

/// 설정 패널
pub struct ConfigPanel {
    /// 패널 표시 여부
    pub visible: bool,
    /// 현재 선택된 탭
    pub active_tab: ConfigTab,
    /// 현재 포커스된 필드 인덱스
    pub focused_field: usize,
    /// 상태 메시지
    pub status_message: Option<(String, bool)>,
    /// 대기 중인 설정 변경
    pub pending_changes: Vec<ConfigChange>,

    // === General 탭 값 ===
    pub provider_name: String,
    pub model_name: String,
    pub goose_mode: GooseMode,

    // === PII 탭 값 ===
    pub pii_enabled: bool,
    pub pii_secret_enabled: bool,
    pub pii_token_enabled: bool,
    pub pii_credential_enabled: bool,
    pub pii_certificate_enabled: bool,
    pub pii_whitelist: Vec<String>,
    /// 화이트리스트에서 선택된 인덱스
    whitelist_selected: usize,
    /// 화이트리스트 편집 모드
    whitelist_mode: WhitelistEditMode,
    /// 새 화이트리스트 입력 버퍼
    whitelist_input: String,

    // === Advanced 탭 값 ===
    pub max_tokens: u32,
    pub max_turns: u32,
    pub api_version: String,
    pub audit_enabled: bool,
}

impl ConfigPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            active_tab: ConfigTab::General,
            focused_field: 0,
            status_message: None,
            pending_changes: Vec::new(),

            provider_name: String::new(),
            model_name: String::new(),
            goose_mode: GooseMode::Auto,

            pii_enabled: true,
            pii_secret_enabled: true,
            pii_token_enabled: true,
            pii_credential_enabled: true,
            pii_certificate_enabled: true,
            pii_whitelist: Vec::new(),
            whitelist_selected: 0,
            whitelist_mode: WhitelistEditMode::Browse,
            whitelist_input: String::new(),

            max_tokens: 128000,
            max_turns: 1000,
            api_version: String::new(),
            audit_enabled: true,
        }
    }

    /// 패널 열기
    pub fn open(&mut self) {
        self.visible = true;
        self.focused_field = 0;
        self.status_message = None;
    }

    /// 패널 닫기
    pub fn close(&mut self) {
        self.visible = false;
        self.status_message = None;
        self.whitelist_mode = WhitelistEditMode::Browse;
        self.whitelist_input.clear();
    }

    /// 토글
    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    /// 현재 탭의 필드 수
    fn field_count(&self) -> usize {
        match self.active_tab {
            ConfigTab::General => 3,   // provider, model, mode
            ConfigTab::Pii => 6,       // on/off, 4 categories, whitelist
            ConfigTab::Advanced => 4,  // max_tokens, max_turns, api_version, audit
        }
    }

    /// 다음 탭
    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ConfigTab::General => ConfigTab::Pii,
            ConfigTab::Pii => ConfigTab::Advanced,
            ConfigTab::Advanced => ConfigTab::General,
        };
        self.focused_field = 0;
    }

    /// 이전 탭
    pub fn prev_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ConfigTab::General => ConfigTab::Advanced,
            ConfigTab::Pii => ConfigTab::General,
            ConfigTab::Advanced => ConfigTab::Pii,
        };
        self.focused_field = 0;
    }

    /// GooseMode 순환 (Auto → Approve → SmartApprove → Chat → Auto)
    fn cycle_mode(&mut self) {
        self.goose_mode = match self.goose_mode {
            GooseMode::Auto => GooseMode::Approve,
            GooseMode::Approve => GooseMode::SmartApprove,
            GooseMode::SmartApprove => GooseMode::Chat,
            GooseMode::Chat => GooseMode::Auto,
        };
    }

    /// GooseMode 라벨
    fn mode_label(mode: GooseMode) -> &'static str {
        match mode {
            GooseMode::Auto => "Auto",
            GooseMode::Approve => "Approve",
            GooseMode::SmartApprove => "SmartApprove",
            GooseMode::Chat => "Chat",
        }
    }

    /// GooseMode 설명
    fn mode_description(mode: GooseMode) -> &'static str {
        match mode {
            GooseMode::Auto => "도구 자동 실행",
            GooseMode::Approve => "매 실행 승인 필요",
            GooseMode::SmartApprove => "위험 작업만 승인",
            GooseMode::Chat => "대화만 (도구 사용 안 함)",
        }
    }

    /// 비활성화된 타입 HashSet 생성
    fn build_disabled_types(&self) -> HashSet<MaskType> {
        let mut types = HashSet::new();
        if !self.pii_secret_enabled {
            types.insert(MaskType::Secret);
        }
        if !self.pii_token_enabled {
            types.insert(MaskType::Token);
        }
        if !self.pii_credential_enabled {
            types.insert(MaskType::Credential);
        }
        if !self.pii_certificate_enabled {
            types.insert(MaskType::Certificate);
        }
        types
    }

    /// 저장 (config.yaml + 런타임 변경 이벤트 생성)
    pub fn save(&mut self) {
        // 변경 이벤트 생성
        self.pending_changes.push(ConfigChange::ModeChanged(self.goose_mode));
        self.pending_changes.push(ConfigChange::PiiToggled(self.pii_enabled));
        self.pending_changes.push(ConfigChange::PiiWhitelistUpdated(self.pii_whitelist.clone()));
        self.pending_changes.push(ConfigChange::PiiDisabledTypesUpdated(self.build_disabled_types()));
        self.pending_changes.push(ConfigChange::MaxTokensChanged(self.max_tokens));
        self.pending_changes.push(ConfigChange::MaxTurnsChanged(self.max_turns));
        self.pending_changes.push(ConfigChange::AuditToggled(self.audit_enabled));

        // config.yaml 영속화
        if let Err(e) = self.save_to_config() {
            self.status_message = Some((format!("저장 실패: {}", e), true));
        } else {
            self.status_message = Some(("설정 저장됨 ✓".to_string(), false));
        }
    }

    /// config.yaml에 저장
    fn save_to_config(&self) -> Result<(), String> {
        use goose::config::Config;

        let config = Config::global();

        config
            .set_goose_mode(self.goose_mode)
            .map_err(|e| e.to_string())?;

        config
            .set_param("PII_MASKING_ENABLED", self.pii_enabled)
            .map_err(|e| e.to_string())?;

        config
            .set_param("PII_WHITELIST_VALUES", self.pii_whitelist.clone())
            .map_err(|e| e.to_string())?;

        let disabled: Vec<String> = self
            .build_disabled_types()
            .iter()
            .map(|t| format!("{:?}", t))
            .collect();
        config
            .set_param("PII_DISABLED_TYPES", disabled)
            .map_err(|e| e.to_string())?;

        config
            .set_param("AUDIT_LOG_ENABLED", self.audit_enabled)
            .map_err(|e| e.to_string())?;

        config
            .set_param("GOOSE_MAX_TURNS", self.max_turns as i64)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// 대기 중인 변경사항 가져오기
    pub fn take_pending_changes(&mut self) -> Vec<ConfigChange> {
        std::mem::take(&mut self.pending_changes)
    }

    /// 키 입력 처리
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // 화이트리스트 입력 모드
        if self.whitelist_mode == WhitelistEditMode::Adding {
            return self.handle_whitelist_input_key(key);
        }

        // Esc: 닫기
        if key.code == KeyCode::Esc {
            self.close();
            return true;
        }

        // Ctrl+S: 저장
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S'))
        {
            self.save();
            return true;
        }

        // Tab/Shift+Tab: 탭 전환
        if key.code == KeyCode::Tab && !key.modifiers.contains(KeyModifiers::SHIFT) {
            self.next_tab();
            return true;
        }
        if key.code == KeyCode::BackTab
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            self.prev_tab();
            return true;
        }

        // 위/아래: 필드 이동
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.focused_field > 0 {
                    self.focused_field -= 1;
                }
                return true;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.field_count().saturating_sub(1);
                if self.focused_field < max {
                    self.focused_field += 1;
                }
                return true;
            }
            _ => {}
        }

        // Space/Enter: 토글 또는 편집
        if key.code == KeyCode::Enter || key.code == KeyCode::Char(' ') {
            self.toggle_current_field();
            return true;
        }

        // PII 탭 전용: A = 화이트리스트 추가, D = 삭제
        if self.active_tab == ConfigTab::Pii {
            match key.code {
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    if self.focused_field == 5 {
                        self.whitelist_mode = WhitelistEditMode::Adding;
                        self.whitelist_input.clear();
                        return true;
                    }
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    if self.focused_field == 5 && !self.pii_whitelist.is_empty() {
                        let idx = self.whitelist_selected.min(self.pii_whitelist.len().saturating_sub(1));
                        self.pii_whitelist.remove(idx);
                        if self.whitelist_selected >= self.pii_whitelist.len() && self.whitelist_selected > 0 {
                            self.whitelist_selected -= 1;
                        }
                        self.status_message = Some(("항목 삭제됨".to_string(), false));
                        return true;
                    }
                }
                KeyCode::Left => {
                    if self.focused_field == 5 && self.whitelist_selected > 0 {
                        self.whitelist_selected -= 1;
                        return true;
                    }
                }
                KeyCode::Right => {
                    if self.focused_field == 5 && self.whitelist_selected + 1 < self.pii_whitelist.len() {
                        self.whitelist_selected += 1;
                        return true;
                    }
                }
                _ => {}
            }
        }

        // Advanced 탭: +/- 로 숫자 값 조정
        if self.active_tab == ConfigTab::Advanced {
            match key.code {
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    match self.focused_field {
                        0 => self.max_tokens = (self.max_tokens + 1000).min(1_000_000),
                        1 => self.max_turns = (self.max_turns + 10).min(10_000),
                        _ => {}
                    }
                    return true;
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    match self.focused_field {
                        0 => self.max_tokens = self.max_tokens.saturating_sub(1000).max(1000),
                        1 => self.max_turns = self.max_turns.saturating_sub(10).max(1),
                        _ => {}
                    }
                    return true;
                }
                _ => {}
            }
        }

        false
    }

    /// 화이트리스트 입력 모드 키 처리
    fn handle_whitelist_input_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.whitelist_mode = WhitelistEditMode::Browse;
                self.whitelist_input.clear();
            }
            KeyCode::Enter => {
                let value = self.whitelist_input.trim().to_string();
                if !value.is_empty() && !self.pii_whitelist.contains(&value) {
                    self.pii_whitelist.push(value);
                    self.whitelist_selected = self.pii_whitelist.len().saturating_sub(1);
                    self.status_message = Some(("항목 추가됨".to_string(), false));
                }
                self.whitelist_mode = WhitelistEditMode::Browse;
                self.whitelist_input.clear();
            }
            KeyCode::Char(c) => {
                self.whitelist_input.push(c);
            }
            KeyCode::Backspace => {
                self.whitelist_input.pop();
            }
            _ => {}
        }
        true
    }

    /// 현재 필드 토글
    fn toggle_current_field(&mut self) {
        match self.active_tab {
            ConfigTab::General => {
                match self.focused_field {
                    // 0, 1 = Provider/Model (표시만)
                    2 => self.cycle_mode(), // Mode 순환
                    _ => {}
                }
            }
            ConfigTab::Pii => {
                match self.focused_field {
                    0 => self.pii_enabled = !self.pii_enabled,
                    1 => self.pii_secret_enabled = !self.pii_secret_enabled,
                    2 => self.pii_token_enabled = !self.pii_token_enabled,
                    3 => self.pii_credential_enabled = !self.pii_credential_enabled,
                    4 => self.pii_certificate_enabled = !self.pii_certificate_enabled,
                    5 => {
                        self.whitelist_mode = WhitelistEditMode::Adding;
                        self.whitelist_input.clear();
                    }
                    _ => {}
                }
            }
            ConfigTab::Advanced => {
                match self.focused_field {
                    // 0 = max_tokens (+/- 로 조정)
                    // 1 = max_turns (+/- 로 조정)
                    // 2 = api_version (표시만)
                    3 => self.audit_enabled = !self.audit_enabled,
                    _ => {}
                }
            }
        }
    }

    /// 렌더링
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        frame.render_widget(Clear, area);

        let block = Block::default()
            .title(" 설정 [F7] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(inner);

        self.render_tabs(frame, chunks[0]);

        match self.active_tab {
            ConfigTab::General => self.render_general(frame, chunks[1]),
            ConfigTab::Pii => self.render_pii(frame, chunks[1]),
            ConfigTab::Advanced => self.render_advanced(frame, chunks[1]),
        }

        self.render_status_bar(frame, chunks[2]);
    }

    /// 탭 렌더링
    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let tabs_list = [ConfigTab::General, ConfigTab::Pii, ConfigTab::Advanced];
        let titles: Vec<Line> = tabs_list
            .iter()
            .map(|tab| Line::from(format!(" {} {} ", tab.icon(), tab.label())))
            .collect();

        let selected = match self.active_tab {
            ConfigTab::General => 0,
            ConfigTab::Pii => 1,
            ConfigTab::Advanced => 2,
        };

        let tabs = Tabs::new(titles)
            .select(selected)
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("|");

        frame.render_widget(tabs, area);
    }

    /// General 탭 렌더링
    fn render_general(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        // Provider (표시만 + env var 힌트)
        let prov_focused = self.focused_field == 0;
        lines.push(self.render_field_line(
            prov_focused,
            "Provider",
            &self.provider_name,
            Some("GOOSE_PROVIDER"),
        ));
        lines.push(Line::from(""));

        // Model (표시만 + env var 힌트)
        let model_focused = self.focused_field == 1;
        lines.push(self.render_field_line(
            model_focused,
            "Model",
            &self.model_name,
            Some("GOOSE_MODEL"),
        ));
        lines.push(Line::from(""));

        // Mode (토글 가능!)
        let mode_focused = self.focused_field == 2;
        let prefix = if mode_focused { " ▶ " } else { "   " };
        let label_style = if mode_focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let mode_label = Self::mode_label(self.goose_mode);
        let mode_color = match self.goose_mode {
            GooseMode::Auto => Color::Green,
            GooseMode::Approve => Color::Yellow,
            GooseMode::SmartApprove => Color::Cyan,
            GooseMode::Chat => Color::Magenta,
        };
        lines.push(Line::from(vec![
            Span::raw(prefix),
            Span::styled(format!("{:<15}", "Mode"), label_style),
            Span::styled(format!("[{}]", mode_label), Style::default().fg(mode_color).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("  {}", Self::mode_description(self.goose_mode)),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        if mode_focused {
            lines.push(Line::styled(
                "                  Space/Enter: 모드 전환",
                Style::default().fg(Color::DarkGray),
            ));
        }
        lines.push(Line::from(""));

        lines.push(Line::from(""));
        lines.push(Line::styled(
            "   Provider/Model 변경: 환경변수 설정 후 재시작",
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(Line::styled(
            "   Mode 변경: Space로 즉시 전환 → Ctrl+S로 저장",
            Style::default().fg(Color::DarkGray),
        ));

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray)),
        );
        frame.render_widget(paragraph, area);
    }

    /// 읽기 전용 필드 렌더링 (env var 힌트 포함)
    fn render_field_line(&self, focused: bool, label: &str, value: &str, env_var: Option<&str>) -> Line<'static> {
        let prefix = if focused { " ▶ " } else { "   " };
        let label_style = if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let hint = if let Some(var) = env_var {
            format!("  ${}", var)
        } else {
            String::new()
        };

        Line::from(vec![
            Span::styled(prefix.to_string(), Style::default()),
            Span::styled(format!("{:<15}", label), label_style),
            Span::styled(value.to_string(), Style::default().fg(Color::Cyan)),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ])
    }

    /// PII 탭 렌더링
    fn render_pii(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        let toggle_fields: Vec<(&str, bool)> = vec![
            ("PII 마스킹", self.pii_enabled),
            ("  Secret 카테고리", self.pii_secret_enabled),
            ("  Token 카테고리", self.pii_token_enabled),
            ("  Credential 카테고리", self.pii_credential_enabled),
            ("  Certificate 카테고리", self.pii_certificate_enabled),
        ];

        for (i, (label, enabled)) in toggle_fields.iter().enumerate() {
            let focused = i == self.focused_field;
            let prefix = if focused { " ▶ " } else { "   " };
            let label_style = if focused {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let toggle = if *enabled { "[ON] " } else { "[OFF]" };
            let toggle_style = if *enabled {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };

            lines.push(Line::from(vec![
                Span::raw(prefix),
                Span::styled(format!("{:<22}", label), label_style),
                Span::styled(toggle, toggle_style),
            ]));
        }

        // 화이트리스트 (필드 인덱스 5)
        lines.push(Line::from(""));
        let wl_focused = self.focused_field == 5;
        let wl_prefix = if wl_focused { " ▶ " } else { "   " };
        let wl_label_style = if wl_focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(vec![
            Span::raw(wl_prefix),
            Span::styled("화이트리스트 값", wl_label_style),
            Span::styled(
                format!("  ({} 항목)", self.pii_whitelist.len()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        if self.pii_whitelist.is_empty() {
            lines.push(Line::styled(
                "     (비어있음 - A로 추가)",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            for (i, val) in self.pii_whitelist.iter().enumerate() {
                let selected = wl_focused && i == self.whitelist_selected;
                let marker = if selected { "  ● " } else { "    " };
                let style = if selected {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                lines.push(Line::from(vec![
                    Span::raw(marker),
                    Span::styled(val.to_string(), style),
                ]));
            }
        }

        if self.whitelist_mode == WhitelistEditMode::Adding {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("     새 값: ", Style::default().fg(Color::Yellow)),
                Span::styled(self.whitelist_input.clone(), Style::default().fg(Color::White)),
                Span::styled("▌", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            ]));
            lines.push(Line::styled(
                "     Enter: 추가 | Esc: 취소",
                Style::default().fg(Color::DarkGray),
            ));
        }

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray)),
        );
        frame.render_widget(paragraph, area);
    }

    /// Advanced 탭 렌더링
    fn render_advanced(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        // 0: Max Tokens (+/-)
        let mt_focused = self.focused_field == 0;
        lines.push(self.render_number_field(mt_focused, "Max Tokens", self.max_tokens, "+/-: 1000 단위"));
        lines.push(Line::from(""));

        // 1: Max Turns (+/-)
        let turns_focused = self.focused_field == 1;
        lines.push(self.render_number_field(turns_focused, "Max Turns", self.max_turns, "+/-: 10 단위"));
        lines.push(Line::from(""));

        // 2: API Version (표시만 + env var 힌트)
        let av_focused = self.focused_field == 2;
        lines.push(self.render_field_line(av_focused, "API Version", &self.api_version, Some("AZURE_OPENAI_API_VERSION")));
        lines.push(Line::from(""));

        // 3: Audit 토글
        let au_focused = self.focused_field == 3;
        let au_prefix = if au_focused { " ▶ " } else { "   " };
        let au_style = if au_focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let au_toggle = if self.audit_enabled { "[ON] " } else { "[OFF]" };
        let au_toggle_style = if self.audit_enabled {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Red)
        };
        lines.push(Line::from(vec![
            Span::raw(au_prefix),
            Span::styled(format!("{:<22}", "감사 로깅"), au_style),
            Span::styled(au_toggle, au_toggle_style),
        ]));

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray)),
        );
        frame.render_widget(paragraph, area);
    }

    /// 숫자 필드 렌더링 (+/- 조정 가능)
    fn render_number_field(&self, focused: bool, label: &str, value: u32, hint: &str) -> Line<'static> {
        let prefix = if focused { " ▶ " } else { "   " };
        let label_style = if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        Line::from(vec![
            Span::styled(prefix.to_string(), Style::default()),
            Span::styled(format!("{:<22}", label), label_style),
            Span::styled(format!("{}", value), Style::default().fg(Color::Cyan)),
            Span::styled(format!("  ({})", hint), Style::default().fg(Color::DarkGray)),
        ])
    }

    /// 상태바 렌더링
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let mut spans = if self.whitelist_mode == WhitelistEditMode::Adding {
            vec![
                Span::styled(" Enter", Style::default().fg(Color::Green)),
                Span::raw(":추가  "),
                Span::styled("Esc", Style::default().fg(Color::Green)),
                Span::raw(":취소"),
            ]
        } else {
            vec![
                Span::styled(" Ctrl+S", Style::default().fg(Color::Green)),
                Span::raw(":저장  "),
                Span::styled("Tab", Style::default().fg(Color::Green)),
                Span::raw(":탭전환  "),
                Span::styled("↑↓", Style::default().fg(Color::Green)),
                Span::raw(":이동  "),
                Span::styled("Space", Style::default().fg(Color::Green)),
                Span::raw(":토글  "),
                Span::styled("A", Style::default().fg(Color::Green)),
                Span::raw(":추가  "),
                Span::styled("D", Style::default().fg(Color::Green)),
                Span::raw(":삭제  "),
                Span::styled("Esc", Style::default().fg(Color::Green)),
                Span::raw(":닫기"),
            ]
        };

        if let Some((msg, is_error)) = &self.status_message {
            spans.push(Span::raw("  │  "));
            let style = if *is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Cyan)
            };
            spans.push(Span::styled(msg.to_string(), style));
        }

        let status = Paragraph::new(Line::from(spans));
        frame.render_widget(status, area);
    }
}

impl Default for ConfigPanel {
    fn default() -> Self {
        Self::new()
    }
}
