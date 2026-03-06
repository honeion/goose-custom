//! 렌더링 (View)
//!
//! TEA (Elm Architecture) 패턴의 View 역할
//! Phase 5: Ratatui UI 고도화

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use super::{
    app::{ChatMessage, InputMode, MessageRole, ToolStatus, TuiApp},
    markdown::{MdStyles, parse_line, is_code_block_delimiter, highlight_code_line},
    offscreen_buffer::PanelId,
    theme::{colors, icons},
};

impl<'a> TuiApp<'a> {
    /// 전체 UI 렌더링
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // 배경색 적용 (Clear로 영역 초기화 후 배경 채우기)
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new("").style(self.theme.default),
            area,
        );

        // 입력 영역 높이 동적 계산 (최소 3, 최대 10)
        let input_lines = self.input.lines().len().max(1);
        let input_height = (input_lines + 2).min(10).max(3) as u16; // +2 for border

        // 레이아웃 분할
        let chunks = Layout::vertical([
            Constraint::Length(1),           // 헤더 (1줄)
            Constraint::Min(5),              // 대화/도구 영역
            Constraint::Length(1),           // 도구 상태 바
            Constraint::Length(input_height), // 입력 영역 (동적)
            Constraint::Length(1),           // 상태 바
        ])
        .split(area);

        self.render_header(frame, chunks[0]);

        // 도구 출력 패널 활성화 시 좌우 분할
        if self.panels.show_tool_output {
            let content_chunks = Layout::horizontal([
                Constraint::Percentage(35),  // 도구 출력
                Constraint::Percentage(65),  // 대화
            ])
            .split(chunks[1]);

            self.render_tool_output_panel(frame, content_chunks[0]);
            self.render_conversation(frame, content_chunks[1]);
        } else {
            self.render_conversation(frame, chunks[1]);
        }

        self.render_tool_status(frame, chunks[2]);
        self.render_input(frame, chunks[3]);
        self.render_status_bar(frame, chunks[4]);

        // 도움말 팝업 (최상위)
        if self.show_help {
            self.render_help_popup(frame, area);
        }
    }

    /// 도구 출력 패널 렌더링
    fn render_tool_output_panel(&mut self, frame: &mut Frame, area: Rect) {
        let is_focused = self.panels.focused == PanelId::ToolOutput;
        let border_style = if is_focused {
            self.theme.border_focused
        } else {
            self.theme.border
        };

        let block = Block::default()
            .title(" 도구 출력 [F3] ")
            .borders(Borders::ALL)
            .border_style(border_style)
            .border_set(border::ROUNDED);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // 뷰포트 높이 설정 (중요!)
        self.panels.tool_output.set_viewport_height(inner.height as usize);

        // OffscreenBuffer에서 visible_lines 가져오기
        let lines: Vec<Line> = self.panels.tool_output.visible_lines()
            .iter()
            .cloned()
            .collect();

        if lines.is_empty() {
            let empty_text = Paragraph::new(vec![
                Line::raw(""),
                Line::styled("  도구 출력이 여기에 표시됩니다.", self.theme.dimmed),
            ]);
            frame.render_widget(empty_text, inner);
        } else {
            // Word wrap 적용하여 긴 줄이 잘리지 않도록
            let paragraph = Paragraph::new(lines)
                .wrap(Wrap { trim: false });
            frame.render_widget(paragraph, inner);
        }

        // 스크롤바
        if self.panels.tool_output.can_scroll() {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_symbol("█");

            let mut scrollbar_state = ScrollbarState::new(self.panels.tool_output.total_lines())
                .position(self.panels.tool_output.scroll_offset());

            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }

    /// 헤더 렌더링 (1줄)
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let status = if self.is_connected { "●" } else { "○" };
        let status_style = if self.is_connected { self.theme.success } else { self.theme.error };

        let header = Line::from(vec![
            Span::styled(format!(" {} Goose ", icons::GOOSE), self.theme.header),
            Span::styled(status, status_style),
            Span::styled(format!(" {} ", self.model_name), self.theme.muted),
            Span::styled("│ ", self.theme.border),
            Span::styled("/help", self.theme.dimmed),
        ]);

        let paragraph = Paragraph::new(header).style(self.theme.status_bar);
        frame.render_widget(paragraph, area);
    }

    /// 대화 영역 렌더링
    fn render_conversation(&self, frame: &mut Frame, area: Rect) {
        if self.messages.is_empty() {
            // 빈 상태
            let empty_text = Paragraph::new(vec![
                Line::raw(""),
                Line::styled(
                    "    메시지가 없습니다.",
                    self.theme.dimmed,
                ),
                Line::raw(""),
                Line::styled(
                    "    아래 입력창에 질문을 입력하세요.",
                    self.theme.dimmed,
                ),
            ]);
            frame.render_widget(empty_text, area);
            return;
        }

        // 모든 메시지의 라인들을 하나로 합침
        let mut all_lines: Vec<Line> = Vec::new();
        for msg in &self.messages {
            let msg_lines = self.render_message_lines(msg, area.width);
            all_lines.extend(msg_lines);
        }

        let total_lines = all_lines.len();
        let viewport_height = area.height as usize;

        // 스크롤 오프셋 계산
        // auto_scroll이면 항상 맨 아래, 아니면 저장된 offset 사용
        let scroll_offset = if self.scroll_state.auto_scroll {
            total_lines.saturating_sub(viewport_height)
        } else {
            (self.scroll_state.offset as usize).min(total_lines.saturating_sub(viewport_height))
        };

        let visible_lines: Vec<Line> = all_lines
            .into_iter()
            .skip(scroll_offset)
            .take(viewport_height)
            .collect();

        let paragraph = Paragraph::new(visible_lines);
        frame.render_widget(paragraph, area);

        // 스크롤바
        if total_lines > viewport_height {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("█");

            let mut scrollbar_state = ScrollbarState::new(total_lines)
                .position(scroll_offset);

            frame.render_stateful_widget(
                scrollbar,
                area,
                &mut scrollbar_state,
            );
        }
    }

    /// 메시지를 라인 목록으로 변환 (구분선 + 마크다운)
    fn render_message_lines(&self, msg: &ChatMessage, width: u16) -> Vec<Line<'static>> {
        let (style, label, icon) = match msg.role {
            MessageRole::User => (&self.theme.user_message, "You", icons::GOOSE),
            MessageRole::Assistant => (&self.theme.assistant_message, "Assistant", icons::ASSISTANT),
            MessageRole::System => (&self.theme.system_message, "System", icons::INFO),
        };

        let timestamp = msg.timestamp.format("%H:%M").to_string();
        let mut lines = Vec::new();

        // 상단 구분선
        let separator = "─".repeat((width as usize).saturating_sub(2));
        lines.push(Line::styled(format!(" {}", separator), self.theme.border));

        // 헤더: 아이콘 + 이름 + 시간
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", icon), style.label.clone()),
            Span::styled(label, style.label.add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", timestamp), style.timestamp),
        ]));

        // 마크다운 스타일 설정
        let md_styles = MdStyles {
            text: style.text.clone(),
            bold: style.text.add_modifier(Modifier::BOLD),
            italic: style.text.add_modifier(Modifier::ITALIC),
            ..MdStyles::default()
        };

        // 메시지 내용 (마크다운 + 코드 블록 구문 강조)
        let content_width = (width as usize).saturating_sub(6);
        let mut in_code_block = false;
        let mut code_lang = String::new();

        for line in msg.content.lines() {
            // 코드 블록 구분자 체크
            if let Some(lang) = is_code_block_delimiter(line) {
                if in_code_block {
                    // 코드 블록 종료
                    in_code_block = false;
                    lines.push(Line::from(vec![
                        Span::raw("   "),
                        Span::styled("└─────", md_styles.code),
                    ]));
                } else {
                    // 코드 블록 시작
                    in_code_block = true;
                    code_lang = lang;
                    let header = if code_lang.is_empty() {
                        "┌─────".to_string()
                    } else {
                        format!("┌─ {} ─", code_lang)
                    };
                    lines.push(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(header, md_styles.code),
                    ]));
                }
                continue;
            }

            if in_code_block {
                // 코드 블록 내부: 구문 강조 적용
                let mut code_spans = vec![
                    Span::raw("   "),
                    Span::styled("│ ", md_styles.code),
                ];
                code_spans.extend(highlight_code_line(line, &code_lang));
                lines.push(Line::from(code_spans));
            } else {
                // 일반 텍스트: 마크다운 렌더링
                let wrapped = textwrap_simple(line, content_width);
                for wrapped_line in wrapped {
                    let md_spans = parse_line(&wrapped_line, &md_styles);
                    let mut line_spans = vec![Span::raw("   ")];
                    line_spans.extend(md_spans);
                    lines.push(Line::from(line_spans));
                }
            }
        }

        // 스트리밍 커서
        if msg.is_streaming {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled("▌", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            ]));
        }

        // 메시지 간 여백
        lines.push(Line::raw(""));

        lines
    }

    /// 도구 상태 바 렌더링
    fn render_tool_status(&mut self, frame: &mut Frame, area: Rect) {
        // 스피너 문자 미리 가져오기 (borrow 충돌 방지)
        let spinner = self.spinner_char();
        let theme = self.theme.clone();

        let content = match &self.tool_status {
            ToolStatus::None => {
                return; // 아무것도 표시 안 함
            }
            ToolStatus::Thinking => {
                Line::from(vec![
                    Span::styled(
                        format!("  {} ", icons::THINKING),
                        theme.info,
                    ),
                    Span::styled("생각 중... ", theme.info),
                    Span::styled(spinner, theme.info),
                ])
            }
            ToolStatus::Running {
                name,
                progress,
                ..
            } => {
                let progress_text = if let Some(p) = progress {
                    format!("  {}%", (p * 100.0) as u32)
                } else {
                    String::new()
                };

                Line::from(vec![
                    Span::styled(format!("  {} ", icons::TOOL), theme.info),
                    Span::styled(name, theme.info.add_modifier(Modifier::BOLD)),
                    Span::styled(" 실행 중... ", theme.info),
                    Span::styled(spinner, theme.info),
                    Span::styled(progress_text, theme.muted),
                ])
            }
            ToolStatus::Completed { name, duration_ms } => {
                Line::from(vec![
                    Span::styled(format!("  {} ", icons::SUCCESS), theme.success),
                    Span::styled(name, theme.success),
                    Span::styled(
                        format!(" 완료 ({:.1}s)", *duration_ms as f64 / 1000.0),
                        theme.muted,
                    ),
                ])
            }
            ToolStatus::Error { name, message } => {
                Line::from(vec![
                    Span::styled(format!("  {} ", icons::ERROR), theme.error),
                    Span::styled(name, theme.error),
                    Span::styled(format!(" 실패: {}", message), theme.muted),
                ])
            }
        };

        let paragraph = Paragraph::new(content);
        frame.render_widget(paragraph, area);
    }

    /// 입력 영역 렌더링
    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let border_style = if self.input_mode == InputMode::Insert {
            self.theme.border_focused
        } else {
            self.theme.border
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .border_set(border::ROUNDED)
            .title(" 입력 ");

        // TextArea 렌더링
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(&self.input, inner);
    }

    /// 상태 바 렌더링 (간소화)
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        // 모드 표시
        let mode_style = match self.input_mode {
            InputMode::Insert => self.theme.mode_insert,
            InputMode::Normal => self.theme.mode_normal,
            InputMode::Command => self.theme.mode_command,
        };
        let mode_span = Span::styled(
            format!(" {} ", self.input_mode.label()),
            mode_style,
        );

        // 토큰 사용량 (추정)
        let token_style = if self.token_usage.is_critical() {
            self.theme.error
        } else if self.token_usage.is_warning() {
            self.theme.warning
        } else {
            self.theme.muted
        };

        let token_text = format!(" {} ", self.token_usage.display());

        let status_line = Line::from(vec![
            mode_span,
            Span::styled(" │ ", self.theme.border),
            Span::styled("Tokens:", self.theme.dimmed),
            Span::styled(token_text, token_style),
            Span::styled("│ ", self.theme.border),
            Span::styled(
                if self.mouse_capture { "🖱️ON" } else { "🖱️OFF" },
                if self.mouse_capture { self.theme.success } else { self.theme.muted }
            ),
            Span::styled(" │ ", self.theme.border),
            Span::styled(
                if self.panels.show_tool_output { "📋ON" } else { "📋OFF" },
                if self.panels.show_tool_output { self.theme.success } else { self.theme.muted }
            ),
            Span::styled(" │ ", self.theme.border),
            Span::styled(
                format!("🎨{}", self.theme_name.label()),
                self.theme.info
            ),
            Span::styled(" F1:? F2:🖱️ F3:📋 F4:🎨", self.theme.dimmed),
        ]);

        let paragraph = Paragraph::new(status_line)
            .style(self.theme.status_bar);
        frame.render_widget(paragraph, area);
    }

    /// 도움말 팝업 렌더링
    fn render_help_popup(&self, frame: &mut Frame, area: Rect) {
        // 팝업 영역 계산 (중앙)
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 20.min(area.height.saturating_sub(4));
        let popup_area = centered_rect(popup_width, popup_height, area);

        // 배경 클리어
        frame.render_widget(Clear, popup_area);

        let help_text = vec![
            Line::raw(""),
            Line::styled(" 일반", self.theme.header),
            Line::styled(" ─────────────────────────────", self.theme.border),
            Line::raw(" Ctrl+C      세션 종료"),
            Line::raw(" /clear      대화 기록 삭제"),
            Line::raw(" /quit       종료"),
            Line::raw(""),
            Line::styled(" 스크롤", self.theme.header),
            Line::styled(" ─────────────────────────────", self.theme.border),
            Line::raw(" 마우스 휠   위/아래 스크롤"),
            Line::raw(" Page Up/Dn  페이지 스크롤"),
            Line::raw(" G           맨 아래로 (자동스크롤 켜기)"),
            Line::raw(""),
            Line::styled(" 입력", self.theme.header),
            Line::styled(" ─────────────────────────────", self.theme.border),
            Line::raw(" Enter       메시지 전송"),
            Line::raw(" Shift+Enter 새 줄 입력"),
            Line::raw(" ↑/↓         명령어 히스토리"),
            Line::raw(" Tab         패널 포커스 전환"),
            Line::raw(""),
            Line::styled(" 기능키", self.theme.header),
            Line::styled(" ─────────────────────────────", self.theme.border),
            Line::raw(" F1           도움말 (이 창)"),
            Line::raw(" F2           마우스 캡처 토글"),
            Line::raw(" F3           도구 출력 패널 토글"),
            Line::raw(""),
            Line::styled(" 마우스", self.theme.header),
            Line::styled(" ─────────────────────────────", self.theme.border),
            Line::raw(" 캡처 ON:  휠 스크롤, 패널 클릭 포커스"),
            Line::raw(" 캡처 OFF: 텍스트 선택/복사 가능"),
            Line::raw(""),
            Line::styled(" [Esc]로 닫기", self.theme.dimmed),
        ];

        let block = Block::default()
            .title(" 키보드 단축키 ")
            .title_style(self.theme.header)
            .borders(Borders::ALL)
            .border_style(self.theme.border_focused)
            .border_set(border::ROUNDED)
            .style(Style::default().bg(colors::BG_SECONDARY));

        let paragraph = Paragraph::new(help_text).block(block);
        frame.render_widget(paragraph, popup_area);
    }
}

/// 문자열의 표시 너비 계산 (한글/CJK = 2, ASCII = 1)
fn display_width(s: &str) -> usize {
    s.chars().map(|c| {
        if c.is_ascii() {
            1
        } else {
            2 // 한글, CJK 등 와이드 문자
        }
    }).sum()
}

/// 간단한 텍스트 래핑 (표시 너비 기준)
fn textwrap_simple(text: &str, max_width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0usize;

    for word in text.split_whitespace() {
        let word_width = display_width(word);

        if current_line.is_empty() {
            current_line = word.to_string();
            current_width = word_width;
        } else if current_width + 1 + word_width <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            lines.push(current_line);
            current_line = word.to_string();
            current_width = word_width;
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// 중앙 정렬된 영역 계산
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_textwrap_simple() {
        let result = textwrap_simple("Hello World", 20);
        assert_eq!(result, vec!["Hello World"]);

        let result = textwrap_simple("Hello World", 5);
        assert_eq!(result, vec!["Hello", "World"]);

        let result = textwrap_simple("", 10);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 50);
        let popup = centered_rect(40, 20, area);
        assert_eq!(popup.x, 30);
        assert_eq!(popup.y, 15);
        assert_eq!(popup.width, 40);
        assert_eq!(popup.height, 20);
    }
}
