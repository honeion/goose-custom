//! Catppuccin Mocha 테마 정의
//!
//! Phase 5: Ratatui UI 고도화
//! 비주얼 디자인 스펙: docs/008-ui-visual-design.md

use ratatui::style::{Modifier, Style};

/// Catppuccin Mocha 컬러 팔레트
pub mod colors {
    use ratatui::style::Color;

    // Backgrounds (더 어둡게 조정)
    pub const BG_PRIMARY: Color = Color::Rgb(15, 15, 20);     // #0f0f14 - 거의 검정
    pub const BG_SECONDARY: Color = Color::Rgb(10, 10, 15);   // #0a0a0f
    pub const BG_TERTIARY: Color = Color::Rgb(5, 5, 10);      // #05050a

    // Text
    pub const TEXT_PRIMARY: Color = Color::Rgb(205, 214, 244);  // #cdd6f4
    pub const TEXT_MUTED: Color = Color::Rgb(186, 194, 222);    // #bac2de
    pub const TEXT_DIMMED: Color = Color::Rgb(166, 173, 200);   // #a6adc8

    // Surfaces
    pub const SURFACE_0: Color = Color::Rgb(49, 50, 68);      // #313244
    pub const SURFACE_1: Color = Color::Rgb(69, 71, 90);      // #45475a
    pub const SURFACE_2: Color = Color::Rgb(88, 91, 112);     // #585b70

    // Overlay
    pub const OVERLAY_0: Color = Color::Rgb(108, 112, 134);   // #6c7086

    // Accents
    pub const BLUE: Color = Color::Rgb(137, 180, 250);        // #89b4fa - User
    pub const GREEN: Color = Color::Rgb(166, 227, 161);       // #a6e3a1 - Assistant
    pub const YELLOW: Color = Color::Rgb(249, 226, 175);      // #f9e2af - System
    pub const RED: Color = Color::Rgb(243, 139, 168);         // #f38ba8 - Error
    pub const PEACH: Color = Color::Rgb(250, 179, 135);       // #fab387 - Warning
    pub const TEAL: Color = Color::Rgb(148, 226, 213);        // #94e2d5 - Code
    pub const SAPPHIRE: Color = Color::Rgb(116, 199, 236);    // #74c7ec - Link
    pub const MAUVE: Color = Color::Rgb(203, 166, 247);       // #cba6f7 - Keyword
    pub const SKY: Color = Color::Rgb(137, 220, 235);         // #89dceb - Info
}

/// 테마 스타일 정의
#[derive(Clone)]
pub struct Theme {
    // 기본 스타일
    pub default: Style,
    pub muted: Style,
    pub dimmed: Style,

    // 메시지 스타일
    pub user_message: MessageStyle,
    pub assistant_message: MessageStyle,
    pub system_message: MessageStyle,

    // 코드 스타일
    pub code_block: Style,
    pub code_border: Style,

    // 상태 스타일
    pub success: Style,
    pub warning: Style,
    pub error: Style,
    pub info: Style,

    // UI 요소
    pub border: Style,
    pub border_focused: Style,
    pub header: Style,
    pub status_bar: Style,
    pub mode_insert: Style,
    pub mode_normal: Style,
    pub mode_command: Style,

    // 프로그레스
    pub progress_filled: Style,
    pub progress_empty: Style,
}

#[derive(Clone)]
pub struct MessageStyle {
    pub border: Style,
    pub label: Style,
    pub text: Style,
    pub timestamp: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}

/// 테마 이름
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeName {
    #[default]
    CatppuccinMocha,
    Light,
}

impl ThemeName {
    pub fn next(self) -> Self {
        match self {
            ThemeName::CatppuccinMocha => ThemeName::Light,
            ThemeName::Light => ThemeName::CatppuccinMocha,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ThemeName::CatppuccinMocha => "Dark",
            ThemeName::Light => "Light",
        }
    }
}

impl Theme {
    /// 테마 이름으로 생성
    pub fn from_name(name: ThemeName) -> Self {
        match name {
            ThemeName::CatppuccinMocha => Self::catppuccin_mocha(),
            ThemeName::Light => Self::light(),
        }
    }

    /// Catppuccin Mocha 테마 생성
    pub fn catppuccin_mocha() -> Self {
        use colors::*;

        Self {
            // 기본 스타일
            default: Style::default().fg(TEXT_PRIMARY).bg(BG_PRIMARY),
            muted: Style::default().fg(TEXT_MUTED),
            dimmed: Style::default().fg(TEXT_DIMMED),

            // User 메시지 (Blue)
            user_message: MessageStyle {
                border: Style::default().fg(BLUE),
                label: Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
                text: Style::default().fg(TEXT_PRIMARY),
                timestamp: Style::default().fg(TEXT_DIMMED),
            },

            // Assistant 메시지 (Green)
            assistant_message: MessageStyle {
                border: Style::default().fg(GREEN),
                label: Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                text: Style::default().fg(TEXT_PRIMARY),
                timestamp: Style::default().fg(TEXT_DIMMED),
            },

            // System 메시지 (Yellow)
            system_message: MessageStyle {
                border: Style::default().fg(YELLOW),
                label: Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
                text: Style::default().fg(TEXT_MUTED),
                timestamp: Style::default().fg(TEXT_DIMMED),
            },

            // 코드 블록
            code_block: Style::default().fg(TEXT_PRIMARY).bg(BG_TERTIARY),
            code_border: Style::default().fg(TEAL),

            // 상태
            success: Style::default().fg(GREEN),
            warning: Style::default().fg(PEACH),
            error: Style::default().fg(RED),
            info: Style::default().fg(SKY),

            // UI 요소
            border: Style::default().fg(SURFACE_2),
            border_focused: Style::default().fg(BLUE),
            header: Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
            status_bar: Style::default().fg(TEXT_MUTED).bg(BG_SECONDARY),

            // 모드 인디케이터
            mode_insert: Style::default().fg(BG_PRIMARY).bg(GREEN).add_modifier(Modifier::BOLD),
            mode_normal: Style::default().fg(BG_PRIMARY).bg(BLUE).add_modifier(Modifier::BOLD),
            mode_command: Style::default().fg(BG_PRIMARY).bg(YELLOW).add_modifier(Modifier::BOLD),

            // 프로그레스 바
            progress_filled: Style::default().fg(GREEN),
            progress_empty: Style::default().fg(SURFACE_1),
        }
    }

    /// Light 테마 생성
    pub fn light() -> Self {
        use ratatui::style::Color;

        // Light 색상
        let bg_primary = Color::Rgb(255, 255, 255);     // 흰색
        let bg_secondary = Color::Rgb(245, 245, 245);   // 밝은 회색
        let text_primary = Color::Rgb(30, 30, 30);      // 검정에 가까운
        let text_muted = Color::Rgb(80, 80, 80);
        let text_dimmed = Color::Rgb(120, 120, 120);
        let surface_1 = Color::Rgb(220, 220, 220);
        let surface_2 = Color::Rgb(180, 180, 180);

        let blue = Color::Rgb(30, 102, 245);
        let green = Color::Rgb(22, 163, 74);
        let yellow = Color::Rgb(202, 138, 4);
        let red = Color::Rgb(220, 38, 38);
        let peach = Color::Rgb(234, 88, 12);
        let teal = Color::Rgb(13, 148, 136);
        let sky = Color::Rgb(14, 165, 233);

        Self {
            default: Style::default().fg(text_primary).bg(bg_primary),
            muted: Style::default().fg(text_muted),
            dimmed: Style::default().fg(text_dimmed),

            user_message: MessageStyle {
                border: Style::default().fg(blue),
                label: Style::default().fg(blue).add_modifier(Modifier::BOLD),
                text: Style::default().fg(text_primary),
                timestamp: Style::default().fg(text_dimmed),
            },

            assistant_message: MessageStyle {
                border: Style::default().fg(green),
                label: Style::default().fg(green).add_modifier(Modifier::BOLD),
                text: Style::default().fg(text_primary),
                timestamp: Style::default().fg(text_dimmed),
            },

            system_message: MessageStyle {
                border: Style::default().fg(yellow),
                label: Style::default().fg(yellow).add_modifier(Modifier::BOLD),
                text: Style::default().fg(text_muted),
                timestamp: Style::default().fg(text_dimmed),
            },

            code_block: Style::default().fg(text_primary).bg(bg_secondary),
            code_border: Style::default().fg(teal),

            success: Style::default().fg(green),
            warning: Style::default().fg(peach),
            error: Style::default().fg(red),
            info: Style::default().fg(sky),

            border: Style::default().fg(surface_2),
            border_focused: Style::default().fg(blue),
            header: Style::default().fg(text_primary).add_modifier(Modifier::BOLD),
            status_bar: Style::default().fg(text_muted).bg(bg_secondary),

            mode_insert: Style::default().fg(bg_primary).bg(green).add_modifier(Modifier::BOLD),
            mode_normal: Style::default().fg(bg_primary).bg(blue).add_modifier(Modifier::BOLD),
            mode_command: Style::default().fg(bg_primary).bg(yellow).add_modifier(Modifier::BOLD),

            progress_filled: Style::default().fg(green),
            progress_empty: Style::default().fg(surface_1),
        }
    }
}

/// 아이콘 상수
pub mod icons {
    pub const GOOSE: &str = "\u{1FABF}";      // 🪿
    pub const USER: &str = "\u{1F464}";       // 👤
    pub const ASSISTANT: &str = "\u{1F916}";  // 🤖
    pub const TOOL: &str = "\u{26A1}";        // ⚡
    pub const GEAR: &str = "\u{2699}";        // ⚙
    pub const SUCCESS: &str = "\u{2713}";     // ✓
    pub const ERROR: &str = "\u{2717}";       // ✗
    pub const WARNING: &str = "\u{26A0}";     // ⚠
    pub const INFO: &str = "\u{2139}";        // ℹ
    pub const CONNECTED: &str = "\u{25C9}";   // ◉
    pub const DISCONNECTED: &str = "\u{25CB}"; // ○
    pub const THINKING: &str = "\u{1F914}";   // 🤔
}

/// 테두리 문자 (Rounded)
pub mod borders {
    pub const TOP_LEFT: &str = "\u{256D}";     // ╭
    pub const TOP_RIGHT: &str = "\u{256E}";    // ╮
    pub const BOTTOM_LEFT: &str = "\u{2570}";  // ╰
    pub const BOTTOM_RIGHT: &str = "\u{256F}"; // ╯
    pub const HORIZONTAL: &str = "\u{2500}";   // ─
    pub const VERTICAL: &str = "\u{2502}";     // │
}

/// 스피너 프레임 (Braille 패턴)
pub const SPINNER_FRAMES: &[&str] = &[
    "\u{280B}", // ⠋
    "\u{2819}", // ⠙
    "\u{2839}", // ⠹
    "\u{2838}", // ⠸
    "\u{283C}", // ⠼
    "\u{2834}", // ⠴
    "\u{2826}", // ⠦
    "\u{2827}", // ⠧
    "\u{2807}", // ⠇
    "\u{280F}", // ⠏
];

/// 블록 문자 (프로그레스 바)
pub mod blocks {
    pub const FULL: &str = "\u{2588}";        // █
    pub const SEVEN_EIGHTHS: &str = "\u{2589}"; // ▉
    pub const THREE_QUARTERS: &str = "\u{258A}"; // ▊
    pub const FIVE_EIGHTHS: &str = "\u{258B}"; // ▋
    pub const HALF: &str = "\u{258C}";        // ▌
    pub const THREE_EIGHTHS: &str = "\u{258D}"; // ▍
    pub const ONE_QUARTER: &str = "\u{258E}"; // ▎
    pub const ONE_EIGHTH: &str = "\u{258F}";  // ▏
    pub const EMPTY: &str = "\u{2591}";       // ░
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_creation() {
        let theme = Theme::catppuccin_mocha();
        assert_eq!(theme.user_message.border.fg, Some(colors::BLUE));
        assert_eq!(theme.assistant_message.border.fg, Some(colors::GREEN));
    }
}
