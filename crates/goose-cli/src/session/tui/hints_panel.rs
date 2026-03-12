//! Hints 편집 패널
//!
//! F5로 토글되는 힌트 파일 편집 패널
//! Phase 5: TUI 고도화

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use goose::config::paths::Paths;
use goose::hints::{HintLayer, GOOSE_HINTS_FILENAME, GOOSE_HINTS_LOCAL_FILENAME};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
    Frame,
};
use tui_textarea::TextArea;

/// Hints 편집 패널
pub struct HintsPanel<'a> {
    /// 패널 표시 여부
    pub visible: bool,
    /// 현재 선택된 탭
    pub active_tab: HintLayer,
    /// 각 레이어별 텍스트 에디터
    editors: HashMap<HintLayer, TextArea<'a>>,
    /// 수정된 레이어 추적
    modified: HashSet<HintLayer>,
    /// 파일 경로 캐시
    paths: HashMap<HintLayer, PathBuf>,
    /// 현재 작업 디렉토리
    cwd: PathBuf,
    /// 상태 메시지
    status_message: Option<(String, bool)>, // (message, is_error)
}

impl<'a> HintsPanel<'a> {
    pub fn new() -> Self {
        Self {
            visible: false,
            active_tab: HintLayer::Project,
            editors: HashMap::new(),
            modified: HashSet::new(),
            paths: HashMap::new(),
            cwd: PathBuf::new(),
            status_message: None,
        }
    }

    /// 패널 열기 및 파일 로드
    pub fn open(&mut self, cwd: &Path) {
        self.cwd = cwd.to_path_buf();
        self.visible = true;
        self.load_all();
    }

    /// 패널 닫기
    pub fn close(&mut self) {
        self.visible = false;
        self.status_message = None;
    }

    /// 토글
    pub fn toggle(&mut self, cwd: &Path) {
        if self.visible {
            self.close();
        } else {
            self.open(cwd);
        }
    }

    /// 모든 레이어 파일 로드
    fn load_all(&mut self) {
        self.editors.clear();
        self.modified.clear();
        self.paths.clear();

        for layer in [HintLayer::Global, HintLayer::Project, HintLayer::Local] {
            let path = self.get_path(layer);
            self.paths.insert(layer, path.clone());

            let content = if path.exists() {
                fs::read_to_string(&path).unwrap_or_default()
            } else {
                String::new()
            };

            let mut editor = TextArea::new(content.lines().map(String::from).collect());
            editor.set_cursor_line_style(Style::default().bg(Color::DarkGray));
            editor.set_line_number_style(Style::default().fg(Color::DarkGray));
            self.editors.insert(layer, editor);
        }

        self.status_message = Some(("로드됨".to_string(), false));
    }

    /// 레이어별 파일 경로
    fn get_path(&self, layer: HintLayer) -> PathBuf {
        match layer {
            HintLayer::Global => Paths::in_config_dir(GOOSE_HINTS_FILENAME),
            HintLayer::Project => self.cwd.join(GOOSE_HINTS_FILENAME),
            HintLayer::Local => self.cwd.join(GOOSE_HINTS_LOCAL_FILENAME),
        }
    }

    /// 현재 탭 저장
    pub fn save_current(&mut self) -> Result<(), String> {
        let layer = self.active_tab;
        let editor = self.editors.get(&layer).ok_or("에디터 없음")?;
        let path = self.paths.get(&layer).ok_or("경로 없음")?;

        // 디렉토리 생성 (필요시)
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }

        let content: String = editor.lines().join("\n");
        fs::write(path, &content).map_err(|e| e.to_string())?;

        self.modified.remove(&layer);
        self.status_message = Some((format!("{} 저장됨", layer.label()), false));
        Ok(())
    }

    /// 현재 탭 리로드
    pub fn reload_current(&mut self) {
        let layer = self.active_tab;
        let path = self.get_path(layer);

        let content = if path.exists() {
            fs::read_to_string(&path).unwrap_or_default()
        } else {
            String::new()
        };

        let mut editor = TextArea::new(content.lines().map(String::from).collect());
        editor.set_cursor_line_style(Style::default().bg(Color::DarkGray));
        editor.set_line_number_style(Style::default().fg(Color::DarkGray));
        self.editors.insert(layer, editor);
        self.modified.remove(&layer);
        self.status_message = Some((format!("{} 리로드됨", layer.label()), false));
    }

    /// 다음 탭으로 이동
    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            HintLayer::Global => HintLayer::Project,
            HintLayer::Project => HintLayer::Local,
            HintLayer::Local => HintLayer::Global,
        };
    }

    /// 이전 탭으로 이동
    pub fn prev_tab(&mut self) {
        self.active_tab = match self.active_tab {
            HintLayer::Global => HintLayer::Local,
            HintLayer::Project => HintLayer::Global,
            HintLayer::Local => HintLayer::Project,
        };
    }

    /// 수정 여부 확인
    pub fn is_modified(&self) -> bool {
        !self.modified.is_empty()
    }

    /// 키 입력 처리
    /// 반환: true = 이벤트 소비됨, false = 상위로 전파
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Esc: 닫기
        if key.code == KeyCode::Esc {
            self.close();
            return true;
        }

        // Ctrl+S: 저장
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S'))
        {
            match self.save_current() {
                Ok(_) => {}
                Err(e) => {
                    self.status_message = Some((format!("저장 실패: {}", e), true));
                }
            }
            return true;
        }

        // Ctrl+R: 리로드
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && (key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R'))
        {
            self.reload_current();
            return true;
        }

        // Tab: 다음 탭
        if key.code == KeyCode::Tab && !key.modifiers.contains(KeyModifiers::SHIFT) {
            self.next_tab();
            return true;
        }

        // Shift+Tab: 이전 탭
        if key.code == KeyCode::BackTab
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            self.prev_tab();
            return true;
        }

        // 에디터에 키 전달
        if let Some(editor) = self.editors.get_mut(&self.active_tab) {
            // tui-textarea에 키 이벤트 전달
            let input = tui_textarea::Input::from(key);
            if editor.input(input) {
                self.modified.insert(self.active_tab);
            }
            return true;
        }

        false
    }

    /// 렌더링
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // 배경 클리어
        frame.render_widget(Clear, area);

        // 패널 레이아웃
        let block = Block::default()
            .title(" Hints 편집 [F5] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // 내부 레이아웃: 탭 + 에디터 + 상태바
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // 탭
                Constraint::Min(5),    // 에디터
                Constraint::Length(1), // 상태바
            ])
            .split(inner);

        // 탭 렌더링
        self.render_tabs(frame, chunks[0]);

        // 에디터 렌더링
        self.render_editor(frame, chunks[1]);

        // 상태바 렌더링
        self.render_status_bar(frame, chunks[2]);
    }

    /// 탭 렌더링
    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles: Vec<Line> = [HintLayer::Global, HintLayer::Project, HintLayer::Local]
            .iter()
            .map(|layer| {
                let icon = layer.icon();
                let label = layer.label();
                let modified = if self.modified.contains(layer) {
                    " *"
                } else {
                    ""
                };
                Line::from(format!(" {} {}{} ", icon, label, modified))
            })
            .collect();

        let selected = match self.active_tab {
            HintLayer::Global => 0,
            HintLayer::Project => 1,
            HintLayer::Local => 2,
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

    /// 에디터 렌더링
    fn render_editor(&self, frame: &mut Frame, area: Rect) {
        if let Some(editor) = self.editors.get(&self.active_tab) {
            // 에디터 블록
            let path = self
                .paths
                .get(&self.active_tab)
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            let exists = self
                .paths
                .get(&self.active_tab)
                .map(|p| p.exists())
                .unwrap_or(false);

            let title = if exists {
                format!(" {} ", path)
            } else {
                format!(" {} (신규) ", path)
            };

            let block = Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray));

            let inner = block.inner(area);
            frame.render_widget(block, area);

            // tui-textarea 렌더링
            frame.render_widget(editor, inner);
        }
    }

    /// 상태바 렌더링
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let mut spans = vec![
            Span::styled(" Ctrl+S", Style::default().fg(Color::Green)),
            Span::raw(":저장  "),
            Span::styled("Ctrl+R", Style::default().fg(Color::Green)),
            Span::raw(":리로드  "),
            Span::styled("Tab", Style::default().fg(Color::Green)),
            Span::raw(":탭전환  "),
            Span::styled("Esc", Style::default().fg(Color::Green)),
            Span::raw(":닫기"),
        ];

        // 상태 메시지 추가
        if let Some((msg, is_error)) = &self.status_message {
            spans.push(Span::raw("  │  "));
            let style = if *is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Cyan)
            };
            spans.push(Span::styled(msg, style));
        }

        let status = Paragraph::new(Line::from(spans));
        frame.render_widget(status, area);
    }
}

impl Default for HintsPanel<'_> {
    fn default() -> Self {
        Self::new()
    }
}
