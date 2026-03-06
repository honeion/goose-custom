//! 애니메이션 시스템
//!
//! 터미널 UI 애니메이션 효과 (스피너, 타이핑 커서 등)

use std::time::{Duration, Instant};

/// 스피너 애니메이션 프레임
pub struct SpinnerFrames {
    frames: Vec<&'static str>,
    current: usize,
    last_update: Instant,
    interval: Duration,
}

impl Default for SpinnerFrames {
    fn default() -> Self {
        Self::new()
    }
}

impl SpinnerFrames {
    pub fn new() -> Self {
        Self {
            frames: vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            current: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(80),
        }
    }

    /// 도트 스타일 스피너
    pub fn dots() -> Self {
        Self {
            frames: vec!["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"],
            current: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(100),
        }
    }

    /// 라인 스타일 스피너
    pub fn line() -> Self {
        Self {
            frames: vec!["-", "\\", "|", "/"],
            current: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(120),
        }
    }

    /// 현재 프레임 가져오기
    pub fn current_frame(&mut self) -> &'static str {
        if self.last_update.elapsed() >= self.interval {
            self.current = (self.current + 1) % self.frames.len();
            self.last_update = Instant::now();
        }
        self.frames[self.current]
    }

    /// 리셋
    pub fn reset(&mut self) {
        self.current = 0;
        self.last_update = Instant::now();
    }
}

/// 타이핑 애니메이션 (스트리밍용)
pub struct TypingAnimation {
    pub cursor_visible: bool,
    last_blink: Instant,
    blink_interval: Duration,
}

impl Default for TypingAnimation {
    fn default() -> Self {
        Self::new()
    }
}

impl TypingAnimation {
    pub fn new() -> Self {
        Self {
            cursor_visible: true,
            last_blink: Instant::now(),
            blink_interval: Duration::from_millis(530),
        }
    }

    /// 커서 업데이트
    pub fn update(&mut self) {
        if self.last_blink.elapsed() >= self.blink_interval {
            self.cursor_visible = !self.cursor_visible;
            self.last_blink = Instant::now();
        }
    }

    /// 커서 문자 가져오기
    pub fn cursor_char(&mut self) -> &'static str {
        self.update();
        if self.cursor_visible {
            "▌"
        } else {
            " "
        }
    }
}

/// 프로그레스 바 애니메이션
pub struct ProgressBar {
    pub progress: f32,
    pub width: u16,
    pub filled_char: char,
    pub empty_char: char,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressBar {
    pub fn new() -> Self {
        Self {
            progress: 0.0,
            width: 20,
            filled_char: '█',
            empty_char: '░',
        }
    }

    /// 프로그레스 설정 (0.0 ~ 1.0)
    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    /// 프로그레스 바 문자열 렌더링
    pub fn render(&self) -> String {
        let filled = (self.width as f32 * self.progress) as u16;
        let empty = self.width - filled;

        format!(
            "{}{}",
            self.filled_char.to_string().repeat(filled as usize),
            self.empty_char.to_string().repeat(empty as usize)
        )
    }

    /// 퍼센트 표시 포함 렌더링
    pub fn render_with_percent(&self) -> String {
        format!("{} {:3.0}%", self.render(), self.progress * 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_frames() {
        let mut spinner = SpinnerFrames::new();
        let frame1 = spinner.current_frame();
        assert!(!frame1.is_empty());
    }

    #[test]
    fn test_typing_animation() {
        let mut typing = TypingAnimation::new();
        let cursor = typing.cursor_char();
        assert!(!cursor.is_empty());
    }

    #[test]
    fn test_progress_bar() {
        let mut bar = ProgressBar::new();
        bar.set_progress(0.5);
        let rendered = bar.render();
        assert_eq!(rendered.chars().count(), bar.width as usize);
    }
}
