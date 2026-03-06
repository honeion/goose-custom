//! OffscreenBuffer - 독립 스크롤백 버퍼
//!
//! Alternate Screen 모드에서 터미널 스크롤백을 사용할 수 없는 한계를 극복.
//! 각 영역이 자체 라인 히스토리를 유지하여 독립적인 스크롤 제공.

use ratatui::{
    style::Style,
    text::Line,
};

/// 독립 스크롤 영역을 위한 오프스크린 버퍼
#[derive(Debug)]
pub struct OffscreenBuffer {
    /// 버퍼에 저장된 라인들
    lines: Vec<Line<'static>>,
    /// 최대 라인 수 (히스토리 제한)
    max_lines: usize,
    /// 현재 스크롤 오프셋 (위에서부터)
    scroll_offset: usize,
    /// 자동 스크롤 활성화 (새 라인 추가 시 맨 아래로)
    auto_scroll: bool,
    /// 뷰포트 높이 (마지막 렌더링 시 설정)
    viewport_height: usize,
}

impl Default for OffscreenBuffer {
    fn default() -> Self {
        Self::new(10000) // 기본 10000줄 히스토리
    }
}

impl OffscreenBuffer {
    /// 새 버퍼 생성
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: Vec::with_capacity(max_lines.min(1000)),
            max_lines,
            scroll_offset: 0,
            auto_scroll: true,
            viewport_height: 0,
        }
    }

    /// 라인 추가
    pub fn push_line(&mut self, line: Line<'static>) {
        self.lines.push(line);

        // 최대 라인 수 초과 시 오래된 것 제거
        if self.lines.len() > self.max_lines {
            let overflow = self.lines.len() - self.max_lines;
            self.lines.drain(0..overflow);
            // 스크롤 오프셋 조정
            self.scroll_offset = self.scroll_offset.saturating_sub(overflow);
        }

        // 자동 스크롤 시 맨 아래로
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    /// 여러 라인 추가
    pub fn push_lines(&mut self, lines: impl IntoIterator<Item = Line<'static>>) {
        for line in lines {
            self.push_line(line);
        }
    }

    /// 텍스트 라인 추가 (기본 스타일, 여러 줄 지원)
    pub fn push_text(&mut self, text: &str) {
        for line in text.lines() {
            self.push_line(Line::raw(line.to_string()));
        }
        // 빈 텍스트나 마지막이 개행으로 끝나면 빈 줄 추가
        if text.is_empty() || text.ends_with('\n') {
            self.push_line(Line::raw(String::new()));
        }
    }

    /// 스타일 있는 텍스트 라인 추가
    pub fn push_styled(&mut self, text: &str, style: Style) {
        self.push_line(Line::styled(text.to_string(), style));
    }

    /// 버퍼 클리어
    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
        self.auto_scroll = true;
    }

    /// 위로 스크롤
    pub fn scroll_up(&mut self, amount: usize) {
        self.auto_scroll = false; // 수동 스크롤 시 자동 스크롤 끄기
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    /// 아래로 스크롤
    pub fn scroll_down(&mut self, amount: usize) {
        self.auto_scroll = false;
        let max_offset = self.max_scroll_offset();
        self.scroll_offset = (self.scroll_offset + amount).min(max_offset);
    }

    /// 맨 위로 스크롤
    pub fn scroll_to_top(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = 0;
    }

    /// 맨 아래로 스크롤 (자동 스크롤 켜기)
    pub fn scroll_to_bottom(&mut self) {
        self.auto_scroll = true;
        self.scroll_offset = self.max_scroll_offset();
    }

    /// 최대 스크롤 오프셋
    fn max_scroll_offset(&self) -> usize {
        self.lines.len().saturating_sub(self.viewport_height)
    }

    /// 뷰포트 높이 설정
    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height;
        // 오프셋이 범위 벗어나면 조정
        let max = self.max_scroll_offset();
        if self.scroll_offset > max {
            self.scroll_offset = max;
        }
    }

    /// 현재 뷰포트에 보이는 라인들
    pub fn visible_lines(&self) -> &[Line<'static>] {
        let start = if self.auto_scroll {
            self.max_scroll_offset()
        } else {
            self.scroll_offset
        };
        let end = (start + self.viewport_height).min(self.lines.len());

        if start < self.lines.len() {
            &self.lines[start..end]
        } else {
            &[]
        }
    }

    /// 전체 라인 수
    pub fn total_lines(&self) -> usize {
        self.lines.len()
    }

    /// 현재 스크롤 오프셋
    pub fn scroll_offset(&self) -> usize {
        if self.auto_scroll {
            self.max_scroll_offset()
        } else {
            self.scroll_offset
        }
    }

    /// 스크롤 퍼센트 (0.0 ~ 1.0)
    pub fn scroll_percent(&self) -> f64 {
        let max = self.max_scroll_offset();
        if max == 0 {
            1.0 // 모두 보이면 100%
        } else {
            self.scroll_offset() as f64 / max as f64
        }
    }

    /// 자동 스크롤 활성화 여부
    pub fn is_auto_scroll(&self) -> bool {
        self.auto_scroll
    }

    /// 스크롤 가능한지 (컨텐츠가 뷰포트보다 큰지)
    pub fn can_scroll(&self) -> bool {
        self.lines.len() > self.viewport_height
    }
}

/// 포커스 가능한 패널 식별자
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelId {
    /// 메인 대화창
    #[default]
    Conversation,
    /// 도구 출력 패널
    ToolOutput,
}

/// 다중 패널 관리자
#[derive(Debug)]
pub struct PanelManager {
    /// 대화창 버퍼
    pub conversation: OffscreenBuffer,
    /// 도구 출력 버퍼
    pub tool_output: OffscreenBuffer,
    /// 현재 포커스된 패널
    pub focused: PanelId,
    /// 도구 출력 패널 표시 여부
    pub show_tool_output: bool,
}

impl Default for PanelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PanelManager {
    pub fn new() -> Self {
        Self {
            conversation: OffscreenBuffer::new(10000),
            tool_output: OffscreenBuffer::new(5000),
            focused: PanelId::Conversation,
            show_tool_output: false,
        }
    }

    /// 포커스 전환
    pub fn set_focus(&mut self, panel: PanelId) {
        self.focused = panel;
    }

    /// 포커스된 버퍼 가져오기 (불변)
    pub fn focused_buffer(&self) -> &OffscreenBuffer {
        match self.focused {
            PanelId::Conversation => &self.conversation,
            PanelId::ToolOutput => &self.tool_output,
        }
    }

    /// 포커스된 버퍼 가져오기 (가변)
    pub fn focused_buffer_mut(&mut self) -> &mut OffscreenBuffer {
        match self.focused {
            PanelId::Conversation => &mut self.conversation,
            PanelId::ToolOutput => &mut self.tool_output,
        }
    }

    /// 포커스된 패널 위로 스크롤
    pub fn scroll_up(&mut self, amount: usize) {
        self.focused_buffer_mut().scroll_up(amount);
    }

    /// 포커스된 패널 아래로 스크롤
    pub fn scroll_down(&mut self, amount: usize) {
        self.focused_buffer_mut().scroll_down(amount);
    }

    /// 포커스된 패널 맨 위로
    pub fn scroll_to_top(&mut self) {
        self.focused_buffer_mut().scroll_to_top();
    }

    /// 포커스된 패널 맨 아래로
    pub fn scroll_to_bottom(&mut self) {
        self.focused_buffer_mut().scroll_to_bottom();
    }

    /// 도구 출력 패널 토글 (포커스는 대화창 유지)
    pub fn toggle_tool_output(&mut self) {
        self.show_tool_output = !self.show_tool_output;
        // 패널 닫으면 대화창으로 포커스 이동
        if !self.show_tool_output {
            self.focused = PanelId::Conversation;
        }
    }

    /// 포커스 토글 (Tab 키용)
    pub fn toggle_focus(&mut self) {
        if self.show_tool_output {
            self.focused = match self.focused {
                PanelId::Conversation => PanelId::ToolOutput,
                PanelId::ToolOutput => PanelId::Conversation,
            };
        }
    }

    /// 좌표로 패널 판별 (마우스 클릭 시)
    pub fn panel_at(&self, _x: u16, _y: u16, _conv_area: ratatui::layout::Rect, _tool_area: Option<ratatui::layout::Rect>) -> Option<PanelId> {
        // TODO: 영역 비교하여 패널 반환
        // 현재는 단순화
        Some(PanelId::Conversation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_basic() {
        let mut buf = OffscreenBuffer::new(100);
        buf.set_viewport_height(10);

        buf.push_text("Line 1");
        buf.push_text("Line 2");

        assert_eq!(buf.total_lines(), 2);
        assert!(!buf.can_scroll());
    }

    #[test]
    fn test_buffer_scroll() {
        let mut buf = OffscreenBuffer::new(100);
        buf.set_viewport_height(5);

        for i in 0..20 {
            buf.push_text(&format!("Line {}", i));
        }

        assert_eq!(buf.total_lines(), 20);
        assert!(buf.can_scroll());
        assert!(buf.is_auto_scroll()); // 기본은 자동 스크롤

        buf.scroll_up(5);
        assert!(!buf.is_auto_scroll()); // 수동 스크롤 시 꺼짐

        buf.scroll_to_bottom();
        assert!(buf.is_auto_scroll()); // 맨 아래로 가면 다시 켜짐
    }

    #[test]
    fn test_buffer_max_lines() {
        let mut buf = OffscreenBuffer::new(10);
        buf.set_viewport_height(5);

        for i in 0..20 {
            buf.push_text(&format!("Line {}", i));
        }

        assert_eq!(buf.total_lines(), 10); // 최대 10줄만 유지
    }

    #[test]
    fn test_panel_manager() {
        let mut pm = PanelManager::new();

        pm.conversation.push_text("Hello");
        pm.tool_output.push_text("$ ls");

        assert_eq!(pm.focused, PanelId::Conversation);

        pm.set_focus(PanelId::ToolOutput);
        assert_eq!(pm.focused, PanelId::ToolOutput);
    }
}
