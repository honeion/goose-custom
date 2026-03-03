//! Read tool - File reading functionality
//!
//! Provides read-only file access with support for:
//! - Reading file contents with line numbers
//! - Directory listing
//! - View range specification (offset/limit)
//! - Large file handling
//! - Automatic encoding detection (UTF-8, EUC-KR, CP949, Shift_JIS, etc.)
//! - PDF text extraction
//! - Image metadata reading

use base64::prelude::*;
use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use image::GenericImageView;
use indoc::formatdoc;
use lopdf::Document as PdfDocument;
use rmcp::model::{Content, ErrorCode, ErrorData, Role};
use std::{
    fs::File,
    io::Read as IoRead,
    path::{Path, PathBuf},
};

use super::lang;

// Constants
pub const LINE_READ_LIMIT: usize = 2000;
pub const MAX_FILE_SIZE: u64 = 400 * 1024; // 400KB
pub const MAX_PDF_SIZE: u64 = 10 * 1024 * 1024; // 10MB for PDFs
pub const MAX_IMAGE_SIZE: u64 = 50 * 1024 * 1024; // 50MB for images

/// Parameters for the read tool
#[derive(Debug, Clone)]
pub struct ReadParams {
    /// Absolute path to file or directory
    pub path: PathBuf,
    /// Starting line number (0-indexed). If None, starts from beginning.
    pub offset: Option<usize>,
    /// Number of lines to read. If None, reads to end (up to limit).
    pub limit: Option<usize>,
}

/// Reads a file or lists directory contents
pub async fn read(params: ReadParams) -> Result<Vec<Content>, ErrorData> {
    let path = &params.path;

    // Check if path is a directory
    if path.is_dir() {
        return list_directory_contents(path);
    }

    // Check file extension for special handling
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match extension.as_deref() {
        Some("pdf") => read_pdf(path).await,
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("bmp") | Some("webp") | Some("ico") => {
            read_image_metadata(path).await
        }
        _ => read_file(path, params.offset, params.limit).await,
    }
}

/// Reads PDF file and extracts text content
async fn read_pdf(path: &Path) -> Result<Vec<Content>, ErrorData> {
    if !path.is_file() {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("The path '{}' does not exist or is not accessible.", path.display()),
            None,
        ));
    }

    let file_size = std::fs::metadata(path)
        .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to get file metadata: {}", e), None))?
        .len();

    if file_size > MAX_PDF_SIZE {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("PDF file '{}' is too large ({:.2}MB). Maximum size is 10MB.", path.display(), file_size as f64 / 1024.0 / 1024.0),
            None,
        ));
    }

    let doc = PdfDocument::load(path).map_err(|e| {
        ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to load PDF: {}", e), None)
    })?;

    let page_count = doc.get_pages().len();
    let mut text_content = String::new();
    let mut extracted_pages = 0;

    // Extract text from each page (limit to first 20 pages for performance)
    let max_pages = std::cmp::min(page_count, 20);
    for page_num in 1..=max_pages {
        if let Ok(text) = doc.extract_text(&[page_num as u32]) {
            if !text.trim().is_empty() {
                text_content.push_str(&format!("\n--- Page {} ---\n", page_num));
                text_content.push_str(&text);
                extracted_pages += 1;
            }
        }
    }

    let output = if text_content.is_empty() {
        formatdoc! {"
            📄 PDF: {path}
            - Pages: {page_count}
            - File size: {size:.2} KB
            - Text content: (no extractable text - may be scanned/image-based PDF)
            ",
            path = path.display(),
            page_count = page_count,
            size = file_size as f64 / 1024.0,
        }
    } else {
        let truncation_note = if page_count > 20 {
            format!("\n\n... (showing first 20 of {} pages)", page_count)
        } else {
            String::new()
        };

        formatdoc! {"
            📄 PDF: {path}
            - Pages: {page_count}
            - File size: {size:.2} KB
            - Extracted text from {extracted} pages:

            {content}{truncation}
            ",
            path = path.display(),
            page_count = page_count,
            size = file_size as f64 / 1024.0,
            extracted = extracted_pages,
            content = text_content.trim(),
            truncation = truncation_note,
        }
    };

    Ok(vec![
        Content::text(output.clone()).with_audience(vec![Role::Assistant]),
        Content::text(output).with_audience(vec![Role::User]).with_priority(0.2),
    ])
}

/// Reads image file and returns metadata + base64 data for vision analysis
async fn read_image_metadata(path: &Path) -> Result<Vec<Content>, ErrorData> {
    if !path.is_file() {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("The path '{}' does not exist or is not accessible.", path.display()),
            None,
        ));
    }

    let file_size = std::fs::metadata(path)
        .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to get file metadata: {}", e), None))?
        .len();

    if file_size > MAX_IMAGE_SIZE {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Image file '{}' is too large ({:.2}MB). Maximum size is 50MB.", path.display(), file_size as f64 / 1024.0 / 1024.0),
            None,
        ));
    }

    let img = image::open(path).map_err(|e| {
        ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to open image: {}", e), None)
    })?;

    let (width, height) = img.dimensions();
    let color_type = img.color();

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "png".to_string());

    let color_info = match color_type {
        image::ColorType::L8 => "Grayscale (8-bit)",
        image::ColorType::La8 => "Grayscale + Alpha (8-bit)",
        image::ColorType::Rgb8 => "RGB (24-bit)",
        image::ColorType::Rgba8 => "RGBA (32-bit)",
        image::ColorType::L16 => "Grayscale (16-bit)",
        image::ColorType::La16 => "Grayscale + Alpha (16-bit)",
        image::ColorType::Rgb16 => "RGB (48-bit)",
        image::ColorType::Rgba16 => "RGBA (64-bit)",
        image::ColorType::Rgb32F => "RGB (32-bit float)",
        image::ColorType::Rgba32F => "RGBA (32-bit float)",
        _ => "Unknown",
    };

    // Read raw bytes for base64 encoding
    let raw_bytes = std::fs::read(path).map_err(|e| {
        ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to read image file: {}", e), None)
    })?;

    // Encode to base64
    let base64_data = BASE64_STANDARD.encode(&raw_bytes);

    // Determine MIME type
    let mime_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        _ => "image/png",
    };

    let metadata_output = formatdoc! {"
        📷 Image: {path}
        - Format: {format}
        - Dimensions: {width} x {height} pixels
        - Color type: {color}
        - File size: {size}
        ",
        path = path.display(),
        format = extension.to_uppercase(),
        width = width,
        height = height,
        color = color_info,
        size = format_file_size(file_size),
    };

    // Return metadata for Assistant + image data for vision-capable LLM
    Ok(vec![
        Content::text(metadata_output).with_audience(vec![Role::Assistant]),
        Content::image(base64_data, mime_type).with_priority(0.0),
    ])
}

/// Formats file size in human-readable format
fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} bytes", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}

/// Encodes a string back to bytes using the specified encoding
pub fn encode_with_encoding(content: &str, encoding_name: &str) -> Vec<u8> {
    // Try to find the encoding by name
    let encoding = encoding_rs::Encoding::for_label(encoding_name.as_bytes())
        .unwrap_or(encoding_rs::UTF_8);

    let (encoded, _, _) = encoding.encode(content);
    encoded.into_owned()
}

/// Detects encoding and decodes bytes to string
/// Returns (decoded_string, encoding_name)
pub fn decode_with_encoding_detection(
    bytes: &[u8],
    path: &Path,
) -> Result<(String, &'static str), ErrorData> {
    // Check for BOM first
    if let Some((encoding, bom_len)) = detect_bom(bytes) {
        let (decoded, _, had_errors) = encoding.decode(&bytes[bom_len..]);
        if !had_errors {
            return Ok((decoded.into_owned(), encoding.name()));
        }
    }

    // Try UTF-8 first (most common)
    if let Ok(content) = std::str::from_utf8(bytes) {
        return Ok((content.to_string(), "UTF-8"));
    }

    // Try common encodings in order of likelihood
    // This is more reliable than chardetng for Asian encodings
    let encodings_to_try: &[&'static Encoding] = &[
        encoding_rs::EUC_KR,       // Korean (includes CP949)
        encoding_rs::SHIFT_JIS,    // Japanese
        encoding_rs::EUC_JP,       // Japanese
        encoding_rs::GBK,          // Chinese Simplified
        encoding_rs::BIG5,         // Chinese Traditional
        encoding_rs::WINDOWS_1252, // Western European (superset of ISO-8859-1)
    ];

    for encoding in encodings_to_try {
        let (decoded, _, had_errors) = encoding.decode(bytes);
        if !had_errors {
            // Additional validation: check if result contains valid-looking text
            let decoded_str: &str = &decoded;
            if looks_like_valid_text(decoded_str) {
                return Ok((decoded.into_owned(), encoding.name()));
            }
        }
    }

    // Fallback to chardetng
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let tld_hint = get_tld_hint(path);
    let detected = detector.guess(tld_hint, true);
    let (decoded, actual_encoding, _) = detected.decode(bytes);

    Ok((decoded.into_owned(), actual_encoding.name()))
}

/// Checks if decoded text looks like valid readable text
fn looks_like_valid_text(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }

    let total_chars = text.chars().count();
    if total_chars == 0 {
        return true;
    }

    // Count "suspicious" characters (replacement char, control chars except newline/tab)
    let suspicious_count = text
        .chars()
        .filter(|c| {
            *c == '\u{FFFD}' || // Replacement character
            (*c < ' ' && *c != '\n' && *c != '\r' && *c != '\t') // Control chars
        })
        .count();

    // If more than 5% suspicious characters, probably wrong encoding
    if (suspicious_count as f64 / total_chars as f64) >= 0.05 {
        return false;
    }

    // For Korean text: check if it contains Hangul characters
    // If it contains CJK characters, check if they're in expected ranges
    let has_hangul = text.chars().any(|c| is_hangul(c));
    let has_cjk = text.chars().any(|c| is_cjk_ideograph(c));
    let has_kana = text.chars().any(|c| is_japanese_kana(c));

    // If text has CJK ideographs but no Hangul/Kana, it might be wrong encoding
    // (EUC-KR decoded as Shift_JIS or vice versa produces CJK ideographs)
    if has_cjk && !has_hangul && !has_kana {
        // Check ratio - if mostly CJK ideographs, probably wrong encoding
        let cjk_count = text.chars().filter(|c| is_cjk_ideograph(*c)).count();
        let non_ascii_count = text.chars().filter(|c| *c > '\x7F').count();
        if non_ascii_count > 0 && (cjk_count as f64 / non_ascii_count as f64) > 0.8 {
            return false;
        }
    }

    true
}

/// Checks if character is Hangul (Korean)
fn is_hangul(c: char) -> bool {
    matches!(c,
        '\u{AC00}'..='\u{D7A3}' | // Hangul Syllables
        '\u{1100}'..='\u{11FF}' | // Hangul Jamo
        '\u{3131}'..='\u{318E}' | // Hangul Compatibility Jamo
        '\u{A960}'..='\u{A97F}' | // Hangul Jamo Extended-A
        '\u{D7B0}'..='\u{D7FF}'   // Hangul Jamo Extended-B
    )
}

/// Checks if character is CJK Ideograph (Chinese/Japanese Kanji)
fn is_cjk_ideograph(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' | // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' | // CJK Unified Ideographs Extension A
        '\u{F900}'..='\u{FAFF}'   // CJK Compatibility Ideographs
    )
}

/// Checks if character is Japanese Kana
fn is_japanese_kana(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{309F}' | // Hiragana
        '\u{30A0}'..='\u{30FF}'   // Katakana
    )
}

/// Detects BOM (Byte Order Mark) and returns the encoding
fn detect_bom(bytes: &[u8]) -> Option<(&'static Encoding, usize)> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Some((encoding_rs::UTF_8, 3))
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Some((encoding_rs::UTF_16LE, 2))
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some((encoding_rs::UTF_16BE, 2))
    } else {
        None
    }
}

/// Returns TLD (top-level domain) hint for regional encoding detection
fn get_tld_hint(path: &Path) -> Option<&'static [u8]> {
    // Check for common patterns suggesting specific regions
    let path_str = path.to_string_lossy().to_lowercase();

    // Korean files -> .kr TLD
    if path_str.contains("korea") || path_str.contains("한글") || path_str.contains("euc-kr") {
        return Some(b"kr");
    }

    // Japanese files -> .jp TLD
    if path_str.contains("japan") || path_str.contains("shift_jis") || path_str.contains("sjis") {
        return Some(b"jp");
    }

    // Chinese files -> .cn TLD
    if path_str.contains("china") || path_str.contains("gbk") || path_str.contains("gb2312") {
        return Some(b"cn");
    }

    None
}

/// Reads file contents with optional range
async fn read_file(
    path: &Path,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<Vec<Content>, ErrorData> {
    if !path.is_file() {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!(
                "The path '{}' does not exist or is not accessible.",
                path.display()
            ),
            None,
        ));
    }

    let mut f = File::open(path).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to open file: {}", e),
            None,
        )
    })?;

    let file_size = f
        .metadata()
        .map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to get file metadata: {}", e),
                None,
            )
        })?
        .len();

    if file_size > MAX_FILE_SIZE {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!(
                "File '{}' is too large ({:.2}KB). Maximum size is 400KB.",
                path.display(),
                file_size as f64 / 1024.0
            ),
            None,
        ));
    }

    // Read raw bytes
    let mut raw_bytes = Vec::new();
    f.read_to_end(&mut raw_bytes).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to read file: {}", e),
            None,
        )
    })?;

    // Detect and decode encoding
    let (content, _detected_encoding) = decode_with_encoding_detection(&raw_bytes, path)?;
    // Note: detected_encoding can be used for logging or display if needed
    // tracing::debug!("File {} read with encoding: {}", path.display(), detected_encoding);

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Calculate range
    let start_idx = offset.unwrap_or(0);
    let end_idx = if let Some(lim) = limit {
        std::cmp::min(start_idx + lim, total_lines)
    } else {
        total_lines
    };

    // Check if we should recommend using range for large files
    if offset.is_none() && limit.is_none() && total_lines > LINE_READ_LIMIT {
        return Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!(
                "File '{}' is {} lines long. Use offset/limit to read in smaller chunks, or pass offset=0, limit={} to read all.",
                path.display(),
                total_lines,
                total_lines
            ),
            None,
        ));
    }

    // Validate range
    if start_idx >= total_lines && total_lines > 0 {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!(
                "Offset {} is beyond the end of the file (total lines: {})",
                start_idx, total_lines
            ),
            None,
        ));
    }

    let formatted = format_file_content(path, &lines, start_idx, end_idx, offset, limit);

    Ok(vec![
        Content::text(formatted.clone()).with_audience(vec![Role::Assistant]),
        Content::text(formatted)
            .with_audience(vec![Role::User])
            .with_priority(0.0),
    ])
}

/// Formats file content with line numbers
fn format_file_content(
    path: &Path,
    lines: &[&str],
    start_idx: usize,
    end_idx: usize,
    offset: Option<usize>,
    limit: Option<usize>,
) -> String {
    let display_content = if lines.is_empty() {
        String::new()
    } else {
        let actual_end = std::cmp::min(end_idx, lines.len());
        let selected_lines: Vec<String> = lines[start_idx..actual_end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{}: {}", start_idx + i + 1, line))
            .collect();

        selected_lines.join("\n")
    };

    let language = lang::get_language_identifier(path);

    if offset.is_some() || limit.is_some() {
        let start_display = start_idx + 1;
        let end_display = end_idx;
        formatdoc! {"
            ### {path} (lines {start}-{end})
            ```{language}
            {content}
            ```
            ",
            path=path.display(),
            start=start_display,
            end=end_display,
            language=language,
            content=display_content,
        }
    } else {
        formatdoc! {"
            ### {path}
            ```{language}
            {content}
            ```
            ",
            path=path.display(),
            language=language,
            content=display_content,
        }
    }
}

/// Lists the contents of a directory
fn list_directory_contents(path: &Path) -> Result<Vec<Content>, ErrorData> {
    const MAX_ITEMS: usize = 50;

    let entries = std::fs::read_dir(path).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to read directory: {}", e),
            None,
        )
    })?;

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut total_count = 0;

    for entry in entries {
        let entry = entry.map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to read directory entry: {}", e),
                None,
            )
        })?;

        total_count += 1;

        if dirs.len() + files.len() < MAX_ITEMS {
            let metadata = entry.metadata().map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to read metadata: {}", e),
                    None,
                )
            })?;

            let name = entry.file_name().to_string_lossy().to_string();

            if metadata.is_dir() {
                dirs.push(format!("{}/", name));
            } else {
                files.push(name);
            }
        }
    }

    dirs.sort();
    files.sort();

    let mut output = format!("'{}' is a directory. Contents:\n\n", path.display());

    if !dirs.is_empty() {
        output.push_str("Directories:\n");
        for dir in &dirs {
            output.push_str(&format!("  {}\n", dir));
        }
        output.push('\n');
    }

    if !files.is_empty() {
        output.push_str("Files:\n");
        for file in &files {
            output.push_str(&format!("  {}\n", file));
        }
    }

    if dirs.is_empty() && files.is_empty() {
        output.push_str("  (empty directory)\n");
    }

    if total_count > MAX_ITEMS {
        output.push_str(&format!(
            "\n... and {} more items (showing first {} items)\n",
            total_count - MAX_ITEMS,
            MAX_ITEMS
        ));
    }

    Ok(vec![Content::text(output)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3").unwrap();

        let params = ReadParams {
            path: file_path,
            offset: None,
            limit: None,
        };

        let result = read(params).await.unwrap();
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_read_file_with_offset_limit() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3\nline 4\nline 5").unwrap();

        let params = ReadParams {
            path: file_path,
            offset: Some(1), // Start from line 2 (0-indexed)
            limit: Some(2),  // Read 2 lines
        };

        let result = read(params).await.unwrap();
        let content = &result[0];
        if let rmcp::model::RawContent::Text(text) = &content.raw {
            assert!(text.text.contains("line 2"));
            assert!(text.text.contains("line 3"));
            assert!(!text.text.contains("line 1"));
            assert!(!text.text.contains("line 4"));
        }
    }

    #[tokio::test]
    async fn test_read_directory() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("file1.txt"), "content").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let params = ReadParams {
            path: dir.path().to_path_buf(),
            offset: None,
            limit: None,
        };

        let result = read(params).await.unwrap();
        let content = &result[0];
        if let rmcp::model::RawContent::Text(text) = &content.raw {
            assert!(text.text.contains("file1.txt"));
            assert!(text.text.contains("subdir/"));
        }
    }
}
