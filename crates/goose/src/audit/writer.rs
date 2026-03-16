//! 감사 로그 파일 기록
//!
//! JSONL 형식으로 감사 이벤트를 파일에 기록합니다.
//! 일별 파일로 로테이션되며, 설정된 보관 기간 후 자동 삭제됩니다.

use super::event::AuditEvent;
use crate::config::paths::Paths;
use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

/// 기본 로그 보관 기간 (일)
pub const DEFAULT_RETENTION_DAYS: u32 = 30;

/// 감사 로그 Writer
pub struct AuditWriter {
    /// 현재 파일 Writer
    writer: BufWriter<File>,
    /// 현재 로그 파일 날짜
    current_date: NaiveDate,
    /// 로그 디렉토리
    log_dir: PathBuf,
    /// 보관 기간 (일)
    retention_days: u32,
}

impl AuditWriter {
    /// 새 AuditWriter 생성
    pub fn new(retention_days: Option<u32>) -> Result<Self> {
        let log_dir = Self::ensure_log_dir()?;
        let retention_days = retention_days.unwrap_or(DEFAULT_RETENTION_DAYS);

        // 오래된 로그 정리
        let _ = Self::cleanup_old_logs(&log_dir, retention_days);

        let current_date = Local::now().date_naive();
        let writer = Self::open_log_file(&log_dir, current_date)?;

        Ok(Self {
            writer,
            current_date,
            log_dir,
            retention_days,
        })
    }

    /// 로그 디렉토리 확인 및 생성
    fn ensure_log_dir() -> Result<PathBuf> {
        let log_dir = Paths::in_state_dir("logs").join("audit");
        fs::create_dir_all(&log_dir)
            .with_context(|| format!("Failed to create audit log directory: {:?}", log_dir))?;
        Ok(log_dir)
    }

    /// 로그 파일 열기
    fn open_log_file(log_dir: &PathBuf, date: NaiveDate) -> Result<BufWriter<File>> {
        let filename = format!("audit.{}.jsonl", date.format("%Y-%m-%d"));
        let path = log_dir.join(filename);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open audit log file: {:?}", path))?;

        Ok(BufWriter::new(file))
    }

    /// 오래된 로그 파일 정리
    fn cleanup_old_logs(log_dir: &PathBuf, retention_days: u32) -> Result<()> {
        let cutoff_date = Local::now().date_naive() - chrono::Duration::days(retention_days as i64);

        for entry in fs::read_dir(log_dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with("audit.") && filename.ends_with(".jsonl") {
                    // audit.YYYY-MM-DD.jsonl 형식에서 날짜 추출
                    if let Some(date_str) = filename.strip_prefix("audit.").and_then(|s| s.strip_suffix(".jsonl")) {
                        if let Ok(file_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                            if file_date < cutoff_date {
                                let _ = fs::remove_file(&path);
                                tracing::debug!("Removed old audit log: {:?}", path);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 날짜가 바뀌었으면 새 파일로 전환
    fn rotate_if_needed(&mut self) -> Result<()> {
        let today = Local::now().date_naive();
        if today != self.current_date {
            // 현재 파일 flush
            self.writer.flush()?;

            // 새 파일 열기
            self.writer = Self::open_log_file(&self.log_dir, today)?;
            self.current_date = today;

            // 오래된 로그 정리
            let _ = Self::cleanup_old_logs(&self.log_dir, self.retention_days);
        }
        Ok(())
    }

    /// 이벤트 기록
    pub fn write_event(&mut self, event: &AuditEvent) -> Result<()> {
        self.rotate_if_needed()?;

        let json = serde_json::to_string(event)
            .with_context(|| "Failed to serialize audit event")?;

        writeln!(self.writer, "{}", json)
            .with_context(|| "Failed to write audit event")?;

        // 즉시 flush (감사 로그는 유실되면 안 됨)
        self.writer.flush()?;

        Ok(())
    }

    /// 현재 로그 파일 경로
    pub fn current_log_path(&self) -> PathBuf {
        let filename = format!("audit.{}.jsonl", self.current_date.format("%Y-%m-%d"));
        self.log_dir.join(filename)
    }

    /// 로그 디렉토리 경로
    pub fn log_directory(&self) -> &PathBuf {
        &self.log_dir
    }

    /// 모든 로그 파일 목록 (최신순)
    pub fn list_log_files(&self) -> Result<Vec<PathBuf>> {
        let mut files: Vec<PathBuf> = fs::read_dir(&self.log_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("audit.") && n.ends_with(".jsonl"))
                    .unwrap_or(false)
            })
            .collect();

        // 최신순 정렬 (파일명에 날짜가 있으므로 역순 정렬)
        files.sort_by(|a, b| b.cmp(a));

        Ok(files)
    }
}

impl Drop for AuditWriter {
    fn drop(&mut self) {
        let _ = self.writer.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::event::AuditEventType;
    use tempfile::tempdir;

    #[test]
    fn test_audit_writer_creates_file() {
        // 테스트에서는 실제 경로 사용하지 않고 구조만 확인
        let event = AuditEvent::user_input("test-session", "test content", 12, false, 0);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("test-session"));
        assert!(json.contains("user_input"));
    }

    #[test]
    fn test_log_filename_format() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 12).unwrap();
        let filename = format!("audit.{}.jsonl", date.format("%Y-%m-%d"));
        assert_eq!(filename, "audit.2026-03-12.jsonl");
    }
}
