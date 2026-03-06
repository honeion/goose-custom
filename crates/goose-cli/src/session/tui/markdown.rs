//! 마크다운 렌더링
//!
//! 터미널에서 마크다운 텍스트를 스타일 적용된 Span으로 변환

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// 마크다운 파서 결과
#[derive(Debug, Clone)]
pub enum MdElement {
    /// 일반 텍스트
    Text(String),
    /// 볼드 (**text**)
    Bold(String),
    /// 이탤릭 (*text* or _text_)
    Italic(String),
    /// 인라인 코드 (`code`)
    InlineCode(String),
    /// 링크 [text](url)
    Link { text: String, url: String },
    /// 헤더 (# ~ ###)
    Header { level: u8, text: String },
    /// 코드 블록 시작 (```lang)
    CodeBlockStart(String),
    /// 코드 블록 끝 (```)
    CodeBlockEnd,
    /// 리스트 아이템 (- item)
    ListItem(String),
    /// 번호 리스트 (1. item)
    NumberedItem { num: u32, text: String },
    /// 인용 (> text)
    Quote(String),
    /// 수평선 (---)
    HorizontalRule,
}

/// 마크다운 스타일 설정
#[derive(Debug, Clone)]
pub struct MdStyles {
    pub text: Style,
    pub bold: Style,
    pub italic: Style,
    pub code: Style,
    pub code_block: Style,
    pub link: Style,
    pub header1: Style,
    pub header2: Style,
    pub header3: Style,
    pub list_marker: Style,
    pub quote: Style,
}

impl Default for MdStyles {
    fn default() -> Self {
        Self {
            text: Style::default(),
            bold: Style::default().add_modifier(Modifier::BOLD),
            italic: Style::default().add_modifier(Modifier::ITALIC),
            code: Style::default().fg(Color::Yellow).bg(Color::Rgb(40, 40, 40)),
            code_block: Style::default().fg(Color::White).bg(Color::Rgb(30, 30, 30)),
            link: Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
            header1: Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            header2: Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
            header3: Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            list_marker: Style::default().fg(Color::Yellow),
            quote: Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC),
        }
    }
}

/// 한 줄의 마크다운을 파싱하여 Span 목록으로 변환
pub fn parse_line(line: &str, styles: &MdStyles) -> Vec<Span<'static>> {
    let trimmed = line.trim_start();

    // 헤더 체크
    if trimmed.starts_with("### ") {
        return vec![
            Span::styled("### ", styles.header3),
            Span::styled(trimmed[4..].to_string(), styles.header3),
        ];
    }
    if trimmed.starts_with("## ") {
        return vec![
            Span::styled("## ", styles.header2),
            Span::styled(trimmed[3..].to_string(), styles.header2),
        ];
    }
    if trimmed.starts_with("# ") {
        return vec![
            Span::styled("# ", styles.header1),
            Span::styled(trimmed[2..].to_string(), styles.header1),
        ];
    }

    // 수평선 체크
    if trimmed == "---" || trimmed == "***" || trimmed == "___" {
        return vec![Span::styled("─".repeat(40), styles.text)];
    }

    // 인용 체크
    if trimmed.starts_with("> ") {
        return vec![
            Span::styled("│ ", styles.quote),
            Span::styled(trimmed[2..].to_string(), styles.quote),
        ];
    }

    // 리스트 아이템 체크
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let indent = line.len() - trimmed.len();
        let mut spans = vec![Span::raw(" ".repeat(indent))];
        spans.push(Span::styled("• ", styles.list_marker));
        spans.extend(parse_inline(&trimmed[2..], styles));
        return spans;
    }

    // 번호 리스트 체크
    if let Some(pos) = trimmed.find(". ") {
        if pos > 0 && pos <= 3 && trimmed[..pos].chars().all(|c| c.is_ascii_digit()) {
            let indent = line.len() - trimmed.len();
            let num = &trimmed[..pos + 2];
            let mut spans = vec![Span::raw(" ".repeat(indent))];
            spans.push(Span::styled(num.to_string(), styles.list_marker));
            spans.extend(parse_inline(&trimmed[pos + 2..], styles));
            return spans;
        }
    }

    // 일반 텍스트 (인라인 스타일 파싱)
    let indent = line.len() - trimmed.len();
    let mut spans = vec![Span::raw(" ".repeat(indent))];
    spans.extend(parse_inline(trimmed, styles));
    spans
}

/// 인라인 마크다운 파싱 (볼드, 이탤릭, 코드, 링크)
fn parse_inline(text: &str, styles: &MdStyles) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current = String::new();

    while let Some(c) = chars.next() {
        match c {
            // 볼드 또는 이탤릭
            '*' | '_' => {
                // 현재까지 누적된 텍스트 추가
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), styles.text));
                    current.clear();
                }

                let is_double = chars.peek() == Some(&c);
                if is_double {
                    chars.next(); // 두 번째 * 또는 _ 소비
                    // 볼드 찾기
                    let mut bold_text = String::new();
                    while let Some(ch) = chars.next() {
                        if ch == c && chars.peek() == Some(&c) {
                            chars.next();
                            break;
                        }
                        bold_text.push(ch);
                    }
                    if !bold_text.is_empty() {
                        spans.push(Span::styled(bold_text, styles.bold));
                    }
                } else {
                    // 이탤릭 찾기
                    let mut italic_text = String::new();
                    while let Some(ch) = chars.next() {
                        if ch == c {
                            break;
                        }
                        italic_text.push(ch);
                    }
                    if !italic_text.is_empty() {
                        spans.push(Span::styled(italic_text, styles.italic));
                    }
                }
            }
            // 인라인 코드
            '`' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), styles.text));
                    current.clear();
                }

                let mut code_text = String::new();
                while let Some(ch) = chars.next() {
                    if ch == '`' {
                        break;
                    }
                    code_text.push(ch);
                }
                if !code_text.is_empty() {
                    spans.push(Span::styled(format!(" {} ", code_text), styles.code));
                }
            }
            // 링크 시작
            '[' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), styles.text));
                    current.clear();
                }

                let mut link_text = String::new();
                let mut found_close = false;
                while let Some(ch) = chars.next() {
                    if ch == ']' {
                        found_close = true;
                        break;
                    }
                    link_text.push(ch);
                }

                // (url) 부분 파싱
                if found_close && chars.peek() == Some(&'(') {
                    chars.next(); // '(' 소비
                    let mut url = String::new();
                    while let Some(ch) = chars.next() {
                        if ch == ')' {
                            break;
                        }
                        url.push(ch);
                    }
                    spans.push(Span::styled(link_text, styles.link));
                } else {
                    // 링크가 아니면 그대로 출력
                    spans.push(Span::styled(format!("[{}]", link_text), styles.text));
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    // 남은 텍스트 추가
    if !current.is_empty() {
        spans.push(Span::styled(current, styles.text));
    }

    spans
}

/// 코드 블록인지 확인
pub fn is_code_block_delimiter(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("```") {
        let lang = trimmed[3..].trim().to_string();
        Some(lang)
    } else {
        None
    }
}

/// 언어별 키워드 정의
fn get_language_keywords(lang: &str) -> (&'static [&'static str], &'static [&'static str], &'static [&'static str]) {
    // (keywords, types, constants)
    match lang.to_lowercase().as_str() {
        "rust" | "rs" => (
            &["fn", "let", "mut", "const", "pub", "mod", "use", "impl", "trait", "struct", "enum",
              "match", "if", "else", "for", "while", "loop", "return", "break", "continue",
              "async", "await", "move", "ref", "where", "as", "in", "self", "Self", "super", "crate"],
            &["i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
              "f32", "f64", "bool", "char", "str", "String", "Vec", "Option", "Result", "Box"],
            &["true", "false", "None", "Some", "Ok", "Err"],
        ),
        "python" | "py" => (
            &["def", "class", "if", "elif", "else", "for", "while", "try", "except", "finally",
              "with", "as", "import", "from", "return", "yield", "raise", "pass", "break",
              "continue", "lambda", "and", "or", "not", "in", "is", "async", "await", "global", "nonlocal"],
            &["int", "float", "str", "bool", "list", "dict", "tuple", "set", "None", "self", "cls"],
            &["True", "False", "None"],
        ),
        "javascript" | "js" | "typescript" | "ts" => (
            &["function", "const", "let", "var", "if", "else", "for", "while", "do", "switch",
              "case", "break", "continue", "return", "try", "catch", "finally", "throw", "new",
              "class", "extends", "import", "export", "from", "default", "async", "await", "yield",
              "typeof", "instanceof", "this", "super"],
            &["string", "number", "boolean", "object", "Array", "Object", "Promise", "void", "any", "never"],
            &["true", "false", "null", "undefined", "NaN", "Infinity"],
        ),
        "go" | "golang" => (
            &["func", "var", "const", "type", "struct", "interface", "map", "chan", "if", "else",
              "for", "range", "switch", "case", "default", "break", "continue", "return", "go",
              "defer", "select", "package", "import"],
            &["int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32", "uint64",
              "float32", "float64", "complex64", "complex128", "byte", "rune", "string", "bool", "error"],
            &["true", "false", "nil", "iota"],
        ),
        "bash" | "sh" | "shell" | "zsh" => (
            &["if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
              "function", "return", "exit", "break", "continue", "export", "local", "readonly",
              "source", "alias", "unalias", "set", "unset", "shift", "trap"],
            &["echo", "printf", "read", "cd", "pwd", "ls", "cp", "mv", "rm", "mkdir", "rmdir",
              "cat", "grep", "sed", "awk", "find", "xargs", "sort", "uniq", "head", "tail"],
            &["true", "false"],
        ),
        "sql" => (
            &["SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "ON", "AND",
              "OR", "NOT", "IN", "LIKE", "BETWEEN", "IS", "NULL", "ORDER", "BY", "GROUP", "HAVING",
              "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "ALTER", "DROP",
              "TABLE", "INDEX", "VIEW", "TRIGGER", "FUNCTION", "PROCEDURE", "AS", "DISTINCT", "LIMIT"],
            &["INT", "INTEGER", "BIGINT", "SMALLINT", "FLOAT", "DOUBLE", "DECIMAL", "VARCHAR", "CHAR",
              "TEXT", "BOOLEAN", "DATE", "TIME", "TIMESTAMP", "BLOB"],
            &["TRUE", "FALSE", "NULL"],
        ),
        _ => (&[], &[], &[]),
    }
}

/// 코드 줄에 구문 강조 적용
pub fn highlight_code_line(line: &str, lang: &str) -> Vec<Span<'static>> {
    let (keywords, types, constants) = get_language_keywords(lang);

    if keywords.is_empty() {
        // 언어를 모르면 기본 스타일로
        return vec![Span::styled(line.to_string(), Style::default().fg(Color::White))];
    }

    let keyword_style = Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD);
    let type_style = Style::default().fg(Color::Cyan);
    let constant_style = Style::default().fg(Color::Yellow);
    let string_style = Style::default().fg(Color::Green);
    let comment_style = Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC);
    let number_style = Style::default().fg(Color::LightYellow);
    let default_style = Style::default().fg(Color::White);

    // 주석 체크 (간단한 형태)
    let comment_start = if lang == "python" || lang == "py" || lang == "bash" || lang == "sh" || lang == "shell" {
        "#"
    } else if lang == "sql" {
        "--"
    } else {
        "//"
    };

    if let Some(comment_pos) = line.find(comment_start) {
        // 주석 전 부분 처리
        let before_comment = &line[..comment_pos];
        let comment_part = &line[comment_pos..];

        let mut spans = highlight_code_segment(before_comment, keywords, types, constants,
            keyword_style, type_style, constant_style, string_style, number_style, default_style);
        spans.push(Span::styled(comment_part.to_string(), comment_style));
        return spans;
    }

    highlight_code_segment(line, keywords, types, constants,
        keyword_style, type_style, constant_style, string_style, number_style, default_style)
}

/// 코드 세그먼트에 하이라이트 적용
fn highlight_code_segment(
    segment: &str,
    keywords: &[&str],
    types: &[&str],
    constants: &[&str],
    keyword_style: Style,
    type_style: Style,
    constant_style: Style,
    string_style: Style,
    number_style: Style,
    default_style: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut chars = segment.chars().peekable();
    let mut current = String::new();
    let mut in_string = false;
    let mut string_char = '"';

    while let Some(c) = chars.next() {
        // 문자열 처리
        if (c == '"' || c == '\'' || c == '`') && !in_string {
            // 문자열 시작
            if !current.is_empty() {
                spans.extend(tokenize_and_style(&current, keywords, types, constants,
                    keyword_style, type_style, constant_style, number_style, default_style));
                current.clear();
            }
            in_string = true;
            string_char = c;
            current.push(c);
        } else if in_string {
            current.push(c);
            if c == string_char {
                // 문자열 종료
                in_string = false;
                spans.push(Span::styled(current.clone(), string_style));
                current.clear();
            }
        } else if c.is_alphanumeric() || c == '_' {
            current.push(c);
        } else {
            // 단어 종료
            if !current.is_empty() {
                spans.extend(tokenize_and_style(&current, keywords, types, constants,
                    keyword_style, type_style, constant_style, number_style, default_style));
                current.clear();
            }
            spans.push(Span::styled(c.to_string(), default_style));
        }
    }

    // 남은 토큰 처리
    if !current.is_empty() {
        if in_string {
            spans.push(Span::styled(current, string_style));
        } else {
            spans.extend(tokenize_and_style(&current, keywords, types, constants,
                keyword_style, type_style, constant_style, number_style, default_style));
        }
    }

    spans
}

/// 단어를 토큰화하고 스타일 적용
fn tokenize_and_style(
    word: &str,
    keywords: &[&str],
    types: &[&str],
    constants: &[&str],
    keyword_style: Style,
    type_style: Style,
    constant_style: Style,
    number_style: Style,
    default_style: Style,
) -> Vec<Span<'static>> {
    // 숫자 체크
    if word.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return vec![Span::styled(word.to_string(), number_style)];
    }

    // 키워드, 타입, 상수 체크
    let style = if keywords.contains(&word) {
        keyword_style
    } else if types.contains(&word) {
        type_style
    } else if constants.contains(&word) {
        constant_style
    } else {
        default_style
    };

    vec![Span::styled(word.to_string(), style)]
}

/// 전체 텍스트를 마크다운으로 파싱하여 Line 목록 반환
pub fn parse_markdown(text: &str, styles: &MdStyles) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines: Vec<String> = Vec::new();

    for line in text.lines() {
        if let Some(lang) = is_code_block_delimiter(line) {
            if in_code_block {
                // 코드 블록 종료 - 구문 강조 적용하여 렌더링
                in_code_block = false;
                for code_line in &code_lines {
                    let mut highlighted = vec![Span::styled("  │ ", styles.code)];
                    highlighted.extend(highlight_code_line(code_line, &code_lang));
                    lines.push(Line::from(highlighted));
                }
                // 하단 테두리
                lines.push(Line::from(vec![
                    Span::styled("  └─────", styles.code),
                ]));
                code_lines.clear();
            } else {
                // 코드 블록 시작
                in_code_block = true;
                code_lang = lang;
                // 언어 표시
                if !code_lang.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  ┌─ {} ─", code_lang), styles.code),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("  ┌─────", styles.code),
                    ]));
                }
            }
        } else if in_code_block {
            code_lines.push(line.to_string());
        } else {
            lines.push(Line::from(parse_line(line, styles)));
        }
    }

    // 미완료 코드 블록 처리
    if in_code_block {
        for code_line in &code_lines {
            let mut highlighted = vec![Span::styled("  │ ", styles.code)];
            highlighted.extend(highlight_code_line(code_line, &code_lang));
            lines.push(Line::from(highlighted));
        }
    }

    lines
}

// ============================================================================
// Diff 하이라이팅
// ============================================================================

/// Diff 스타일 설정
#[derive(Debug, Clone)]
pub struct DiffStyles {
    pub added: Style,      // + 줄 (추가)
    pub removed: Style,    // - 줄 (삭제)
    pub context: Style,    // 공백 줄 (컨텍스트)
    pub hunk: Style,       // @@ 줄 (헝크 헤더)
    pub header: Style,     // 파일 헤더
}

impl Default for DiffStyles {
    fn default() -> Self {
        Self {
            added: Style::default().fg(Color::Green),
            removed: Style::default().fg(Color::Red),
            context: Style::default().fg(Color::DarkGray),
            hunk: Style::default().fg(Color::Cyan),
            header: Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        }
    }
}

/// Diff 텍스트를 파싱하여 스타일 적용된 Line 목록 반환
///
/// 지원 형식:
/// - `+line` → 추가 (초록색)
/// - `-line` → 삭제 (빨간색)
/// - ` line` → 컨텍스트 (회색)
/// - `@@...@@` → 헝크 헤더 (시안)
/// - `📝`, `📄` → 프리뷰 헤더
pub fn parse_diff(text: &str, styles: &DiffStyles) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_diff_block = false;

    for line in text.lines() {
        // diff 블록 감지
        if line.starts_with("```diff") {
            in_diff_block = true;
            continue;
        }
        if line == "```" && in_diff_block {
            in_diff_block = false;
            continue;
        }

        let styled_line = if in_diff_block {
            // diff 블록 내부
            style_diff_line(line, styles)
        } else if line.starts_with("📝") || line.starts_with("📄") {
            // 프리뷰 헤더
            Line::styled(line.to_string(), styles.header)
        } else if line.starts_with("File:") || line.starts_with("Matches:") || line.starts_with("Current:") {
            // 메타 정보
            Line::styled(line.to_string(), Style::default().fg(Color::Cyan))
        } else if line.starts_with("⚠️") {
            // 경고
            Line::styled(line.to_string(), Style::default().fg(Color::Yellow))
        } else {
            // 일반 텍스트
            Line::raw(line.to_string())
        };

        lines.push(styled_line);
    }

    lines
}

/// 단일 diff 줄에 스타일 적용
fn style_diff_line(line: &str, styles: &DiffStyles) -> Line<'static> {
    if line.starts_with('+') {
        Line::styled(line.to_string(), styles.added)
    } else if line.starts_with('-') {
        Line::styled(line.to_string(), styles.removed)
    } else if line.starts_with("@@") {
        Line::styled(line.to_string(), styles.hunk)
    } else if line.starts_with(' ') {
        Line::styled(line.to_string(), styles.context)
    } else if line == "..." {
        Line::styled(line.to_string(), Style::default().fg(Color::DarkGray))
    } else {
        Line::raw(line.to_string())
    }
}

/// 텍스트가 diff 프리뷰인지 확인
pub fn is_diff_preview(text: &str) -> bool {
    text.starts_with("📝 Edit Preview") ||
    text.starts_with("📄 Write Preview") ||
    text.contains("```diff")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header() {
        let styles = MdStyles::default();
        let spans = parse_line("# Hello", &styles);
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn test_parse_list() {
        let styles = MdStyles::default();
        let spans = parse_line("- Item 1", &styles);
        assert!(spans.len() >= 2);
    }

    #[test]
    fn test_parse_bold() {
        let styles = MdStyles::default();
        let spans = parse_inline("Hello **world**", &styles);
        assert!(spans.len() >= 2);
    }

    #[test]
    fn test_parse_code() {
        let styles = MdStyles::default();
        let spans = parse_inline("Use `code` here", &styles);
        assert!(spans.len() >= 3);
    }
}
