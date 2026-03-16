//! 감사 로그 CLI 명령어
//!
//! `goose audit` 명령어로 감사 로그를 조회하고 분석합니다.

use anyhow::Result;
use chrono::{Local, NaiveDate};
use goose::audit::event::{AuditEvent, AuditEventType};
use goose::config::paths::Paths;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// 감사 로그 요약 표시
pub async fn handle_audit_summary(days: u32) -> Result<()> {
    let log_dir = get_audit_log_dir();

    if !log_dir.exists() {
        println!("📁 감사 로그 디렉토리가 없습니다: {}", log_dir.display());
        return Ok(());
    }

    let files = list_audit_files(&log_dir, days)?;

    if files.is_empty() {
        println!("📋 최근 {}일간 감사 로그가 없습니다.", days);
        return Ok(());
    }

    let mut total_events = 0usize;
    let mut event_counts: HashMap<String, usize> = HashMap::new();
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;
    let mut tool_calls = 0usize;
    let mut pii_masked = 0usize;
    let mut security_events = 0usize;
    let mut sessions = 0usize;

    for file_path in &files {
        let events = read_audit_file(file_path)?;
        total_events += events.len();

        for event in events {
            *event_counts.entry(event.event_type.to_string()).or_insert(0) += 1;

            match event.data {
                goose::audit::event::AuditEventData::SessionStart(_) => {
                    sessions += 1;
                }
                goose::audit::event::AuditEventData::ApiResponse(data) => {
                    total_input_tokens += data.usage.input;
                    total_output_tokens += data.usage.output;
                }
                goose::audit::event::AuditEventData::ToolExecution(_) => {
                    tool_calls += 1;
                }
                goose::audit::event::AuditEventData::PiiMasked(data) => {
                    pii_masked += data.masked_count;
                }
                goose::audit::event::AuditEventData::SecurityEvent(_) => {
                    security_events += 1;
                }
                _ => {}
            }
        }
    }

    println!("📊 감사 로그 요약 (최근 {}일)", days);
    println!();
    println!("  세션 수: {}", sessions);
    println!("  총 이벤트: {}", total_events);
    println!();
    println!("  토큰 사용량:");
    println!("    입력: {}", format_tokens(total_input_tokens));
    println!("    출력: {}", format_tokens(total_output_tokens));
    println!("    합계: {}", format_tokens(total_input_tokens + total_output_tokens));
    println!();
    println!("  도구 호출: {}", tool_calls);
    println!("  PII 마스킹: {} 건", pii_masked);
    println!("  보안 이벤트: {} 건", security_events);
    println!();
    println!("  이벤트 타입별:");
    let mut sorted_counts: Vec<_> = event_counts.iter().collect();
    sorted_counts.sort_by(|a, b| b.1.cmp(a.1));
    for (event_type, count) in sorted_counts {
        println!("    {}: {}", event_type, count);
    }

    Ok(())
}

/// 특정 세션의 타임라인 표시
pub async fn handle_audit_session(session_id: &str) -> Result<()> {
    let log_dir = get_audit_log_dir();

    if !log_dir.exists() {
        println!("📁 감사 로그 디렉토리가 없습니다.");
        return Ok(());
    }

    let files = list_audit_files(&log_dir, 30)?;
    let mut session_events = Vec::new();

    for file_path in &files {
        let events = read_audit_file(file_path)?;
        for event in events {
            if event.session_id == session_id || event.session_id.starts_with(session_id) {
                session_events.push(event);
            }
        }
    }

    if session_events.is_empty() {
        println!("❌ 세션 ID '{}'를 찾을 수 없습니다.", session_id);
        return Ok(());
    }

    session_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    println!("📋 세션 타임라인: {}", session_id);
    println!();

    for event in session_events {
        let time = event.timestamp.with_timezone(&Local).format("%H:%M:%S");
        let icon = get_event_icon(&event.event_type);
        let summary = get_event_summary(&event);
        println!("  [{}] {} {}", time, icon, summary);
    }

    Ok(())
}

/// 토큰 사용량 일별 표시
pub async fn handle_audit_tokens(days: u32) -> Result<()> {
    let log_dir = get_audit_log_dir();

    if !log_dir.exists() {
        println!("📁 감사 로그 디렉토리가 없습니다.");
        return Ok(());
    }

    let files = list_audit_files(&log_dir, days)?;
    let mut daily_usage: HashMap<String, (u64, u64)> = HashMap::new();

    for file_path in &files {
        let date = extract_date_from_filename(file_path);
        let events = read_audit_file(file_path)?;

        for event in events {
            if let goose::audit::event::AuditEventData::ApiResponse(data) = event.data {
                let entry = daily_usage.entry(date.clone()).or_insert((0, 0));
                entry.0 += data.usage.input;
                entry.1 += data.usage.output;
            }
        }
    }

    if daily_usage.is_empty() {
        println!("📋 토큰 사용 기록이 없습니다.");
        return Ok(());
    }

    println!("📊 일별 토큰 사용량 (최근 {}일)", days);
    println!();
    println!("  날짜           입력        출력        합계");
    println!("  ─────────────────────────────────────────────");

    let mut dates: Vec<_> = daily_usage.keys().cloned().collect();
    dates.sort_by(|a, b| b.cmp(a));

    for date in dates {
        if let Some((input, output)) = daily_usage.get(&date) {
            let total = input + output;
            println!(
                "  {}   {:>8}    {:>8}    {:>8}",
                date,
                format_tokens(*input),
                format_tokens(*output),
                format_tokens(total)
            );
        }
    }

    Ok(())
}

/// PII 마스킹 이력 표시
pub async fn handle_audit_pii(days: u32) -> Result<()> {
    let log_dir = get_audit_log_dir();

    if !log_dir.exists() {
        println!("📁 감사 로그 디렉토리가 없습니다.");
        return Ok(());
    }

    let files = list_audit_files(&log_dir, days)?;
    let mut pii_by_type: HashMap<String, usize> = HashMap::new();
    let mut total_masked = 0usize;

    for file_path in &files {
        let events = read_audit_file(file_path)?;

        for event in events {
            if let goose::audit::event::AuditEventData::PiiMasked(data) = event.data {
                total_masked += data.masked_count;
                for item in data.items {
                    *pii_by_type.entry(item.pii_type).or_insert(0) += 1;
                }
            }
        }
    }

    println!("🔒 PII 마스킹 이력 (최근 {}일)", days);
    println!();
    println!("  총 마스킹: {} 건", total_masked);
    println!();

    if !pii_by_type.is_empty() {
        println!("  타입별:");
        let mut sorted: Vec<_> = pii_by_type.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (pii_type, count) in sorted {
            println!("    {}: {}", pii_type, count);
        }
    }

    println!();
    println!("  ⚠️ 원본 PII 값은 로그에 기록되지 않습니다.");

    Ok(())
}

/// 보안 이벤트 표시
pub async fn handle_audit_security(days: u32) -> Result<()> {
    let log_dir = get_audit_log_dir();

    if !log_dir.exists() {
        println!("📁 감사 로그 디렉토리가 없습니다.");
        return Ok(());
    }

    let files = list_audit_files(&log_dir, days)?;
    let mut security_events = Vec::new();

    for file_path in &files {
        let events = read_audit_file(file_path)?;

        for event in events {
            if let goose::audit::event::AuditEventData::SecurityEvent(data) = event.data {
                security_events.push((event.timestamp, event.session_id, data));
            }
        }
    }

    println!("🛡️ 보안 이벤트 (최근 {}일)", days);
    println!();

    if security_events.is_empty() {
        println!("  보안 이벤트가 없습니다. ✅");
        return Ok(());
    }

    security_events.sort_by(|a, b| b.0.cmp(&a.0));

    for (timestamp, session_id, data) in security_events {
        let time = timestamp.with_timezone(&Local).format("%Y-%m-%d %H:%M");
        let severity_icon = match data.severity {
            goose::audit::event::SecuritySeverity::Info => "ℹ️",
            goose::audit::event::SecuritySeverity::Warning => "⚠️",
            goose::audit::event::SecuritySeverity::Error => "❌",
            goose::audit::event::SecuritySeverity::Critical => "🚨",
        };
        println!(
            "  [{}] {} {} - {} ({})",
            time,
            severity_icon,
            data.event_name,
            data.action_taken.unwrap_or_default(),
            session_id
        );
    }

    Ok(())
}

/// 감사 로그 경로 표시
pub async fn handle_audit_path() -> Result<()> {
    let log_dir = get_audit_log_dir();
    let today = Local::now().format("%Y-%m-%d");

    println!("📁 감사 로그 경로");
    println!();
    println!("  디렉토리: {}", log_dir.display());
    println!("  현재 파일: audit.{}.jsonl", today);

    if log_dir.exists() {
        let files = list_audit_files(&log_dir, 30)?;
        println!();
        println!("  최근 파일 ({}):", files.len());
        for (i, file) in files.iter().take(5).enumerate() {
            if let Some(filename) = file.file_name() {
                let size = std::fs::metadata(file)
                    .map(|m| format!("{} bytes", m.len()))
                    .unwrap_or_default();
                println!("    {}. {} ({})", i + 1, filename.to_string_lossy(), size);
            }
        }
    } else {
        println!();
        println!("  (디렉토리 없음)");
    }

    Ok(())
}

// ============================================================
// Helper functions
// ============================================================

fn get_audit_log_dir() -> PathBuf {
    Paths::in_state_dir("logs").join("audit")
}

fn list_audit_files(log_dir: &PathBuf, days: u32) -> Result<Vec<PathBuf>> {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(days as i64);

    let mut files: Vec<PathBuf> = std::fs::read_dir(log_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("audit.") && n.ends_with(".jsonl"))
                .unwrap_or(false)
        })
        .filter(|p| {
            let date_str = extract_date_from_filename(p);
            NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                .map(|d| d >= cutoff)
                .unwrap_or(false)
        })
        .collect();

    files.sort_by(|a, b| b.cmp(a));
    Ok(files)
}

fn extract_date_from_filename(path: &PathBuf) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("audit."))
        .and_then(|n| n.strip_suffix(".jsonl"))
        .unwrap_or("unknown")
        .to_string()
}

fn read_audit_file(path: &PathBuf) -> Result<Vec<AuditEvent>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        if let Ok(line) = line {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(&line) {
                events.push(event);
            }
        }
    }

    Ok(events)
}

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
        goose::audit::event::AuditEventData::SessionStart(data) => {
            format!("세션 시작 ({})", data.working_directory)
        }
        goose::audit::event::AuditEventData::SessionEnd(data) => {
            format!(
                "세션 종료 ({}초, {} tokens)",
                data.duration_secs,
                data.total_tokens.input + data.total_tokens.output
            )
        }
        goose::audit::event::AuditEventData::UserInput(data) => {
            let preview = if data.content_masked.len() > 50 {
                format!("{}...", &data.content_masked[..50])
            } else {
                data.content_masked.clone()
            };
            format!("입력: \"{}\"", preview.replace('\n', " "))
        }
        goose::audit::event::AuditEventData::PiiMasked(data) => {
            format!("PII 마스킹 {} 건", data.masked_count)
        }
        goose::audit::event::AuditEventData::PiiUnmasked(data) => {
            format!("PII 언마스킹 {} 건", data.tokens.len())
        }
        goose::audit::event::AuditEventData::ApiRequest(data) => {
            format!("→ {} ({})", data.model, data.provider)
        }
        goose::audit::event::AuditEventData::ApiResponse(data) => {
            format!(
                "← {} tokens ({}ms)",
                data.usage.input + data.usage.output,
                data.latency_ms
            )
        }
        goose::audit::event::AuditEventData::ToolExecution(data) => {
            format!("도구: {} ({:?})", data.tool_name, data.result_status)
        }
        goose::audit::event::AuditEventData::HookExecution(data) => {
            format!("Hook: {} ({})", data.hook_name, if data.success { "성공" } else { "실패" })
        }
        goose::audit::event::AuditEventData::SecurityEvent(data) => {
            format!("{} - {:?}", data.event_name, data.severity)
        }
    }
}
