//! 환경 감지 모듈
//!
//! OS, Shell, 작업 디렉토리 등 런타임 환경 정보를 수집하여
//! 시스템 프롬프트에서 사용할 수 있도록 합니다.

use serde::Serialize;
use std::path::PathBuf;

/// Shell 타입
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ShellType {
    PowerShell,         // pwsh (PowerShell 7+)
    WindowsPowerShell,  // powershell (5.1)
    Cmd,                // cmd.exe
    Bash,
    Zsh,
    Fish,
    Unknown,
}

impl ShellType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ShellType::PowerShell => "PowerShell 7+",
            ShellType::WindowsPowerShell => "Windows PowerShell",
            ShellType::Cmd => "cmd.exe",
            ShellType::Bash => "Bash",
            ShellType::Zsh => "Zsh",
            ShellType::Fish => "Fish",
            ShellType::Unknown => "Unknown",
        }
    }
}

impl std::fmt::Display for ShellType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// 런타임 환경 정보
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentInfo {
    /// OS 이름 ("windows", "linux", "macos")
    pub os: String,
    /// OS 버전 (예: "Windows 10", "Ubuntu 22.04")
    pub os_version: Option<String>,
    /// 기본 Shell
    pub default_shell: ShellType,
    /// Shell 버전 (예: "PowerShell 7.3.0")
    pub shell_version: Option<String>,
    /// 사용 가능한 Shell 목록
    pub available_shells: Vec<ShellType>,
    /// 현재 작업 디렉토리
    pub working_dir: PathBuf,
    /// 사용자 이름
    pub username: Option<String>,
}

impl Default for EnvironmentInfo {
    fn default() -> Self {
        Self::detect()
    }
}

impl EnvironmentInfo {
    /// 현재 환경 정보를 감지합니다.
    pub fn detect() -> Self {
        let os = detect_os();
        let os_version = detect_os_version();
        let default_shell = detect_default_shell();
        let shell_version = detect_shell_version(&default_shell);
        let available_shells = detect_available_shells();
        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let username = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .ok();

        Self {
            os,
            os_version,
            default_shell,
            shell_version,
            available_shells,
            working_dir,
            username,
        }
    }

    /// 시스템 프롬프트용 환경 정보 요약
    pub fn to_prompt_string(&self) -> String {
        let mut lines = vec![];

        lines.push(format!("- **OS**: {}", self.os));
        if let Some(ver) = &self.os_version {
            lines.push(format!("  - Version: {}", ver));
        }

        lines.push(format!("- **Default Shell**: {}", self.default_shell));
        if let Some(ver) = &self.shell_version {
            lines.push(format!("  - Version: {}", ver));
        }

        lines.push(format!("- **Working Directory**: {}", self.working_dir.display()));

        if let Some(user) = &self.username {
            lines.push(format!("- **User**: {}", user));
        }

        if !self.available_shells.is_empty() {
            let shells: Vec<_> = self.available_shells.iter().map(|s| s.display_name()).collect();
            lines.push(format!("- **Available Shells**: {}", shells.join(", ")));
        }

        lines.join("\n")
    }
}

fn detect_os() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        "unknown".to_string()
    }
}

fn detect_os_version() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        // Windows 버전 감지
        if let Ok(output) = std::process::Command::new("cmd")
            .args(["/c", "ver"])
            .output()
        {
            let version = String::from_utf8_lossy(&output.stdout);
            // "Microsoft Windows [Version 10.0.19045.3693]" 형태에서 추출
            if let Some(start) = version.find('[') {
                if let Some(end) = version.find(']') {
                    return Some(version[start + 1..end].to_string());
                }
            }
        }
        None
    }

    #[cfg(target_os = "linux")]
    {
        // /etc/os-release 파일에서 읽기
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    let value = line.trim_start_matches("PRETTY_NAME=").trim_matches('"');
                    return Some(value.to_string());
                }
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sw_vers")
            .args(["-productVersion"])
            .output()
        {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                return Some(format!("macOS {}", version));
            }
        }
        None
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

fn detect_default_shell() -> ShellType {
    #[cfg(target_os = "windows")]
    {
        // Windows: PowerShell 7+ 우선, 없으면 Windows PowerShell
        if which::which("pwsh").is_ok() {
            ShellType::PowerShell
        } else if which::which("powershell").is_ok() {
            ShellType::WindowsPowerShell
        } else {
            ShellType::Cmd
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Unix: SHELL 환경변수 확인
        if let Ok(shell) = std::env::var("SHELL") {
            let shell_lower = shell.to_lowercase();
            if shell_lower.contains("zsh") {
                ShellType::Zsh
            } else if shell_lower.contains("fish") {
                ShellType::Fish
            } else if shell_lower.contains("bash") {
                ShellType::Bash
            } else {
                ShellType::Unknown
            }
        } else {
            ShellType::Bash // 기본값
        }
    }
}

fn detect_shell_version(shell: &ShellType) -> Option<String> {
    match shell {
        ShellType::PowerShell | ShellType::WindowsPowerShell => {
            #[cfg(target_os = "windows")]
            {
                let exe = if matches!(shell, ShellType::PowerShell) {
                    "pwsh"
                } else {
                    "powershell"
                };
                if let Ok(output) = std::process::Command::new(exe)
                    .args(["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"])
                    .output()
                {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !version.is_empty() {
                        return Some(version);
                    }
                }
                None
            }
            #[cfg(not(target_os = "windows"))]
            None
        }
        ShellType::Bash => {
            if let Ok(output) = std::process::Command::new("bash").args(["--version"]).output() {
                let version = String::from_utf8_lossy(&output.stdout);
                // 첫 줄에서 버전 추출
                if let Some(line) = version.lines().next() {
                    return Some(line.to_string());
                }
            }
            None
        }
        ShellType::Zsh => {
            if let Ok(output) = std::process::Command::new("zsh").args(["--version"]).output() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !version.is_empty() {
                    return Some(version);
                }
            }
            None
        }
        _ => None,
    }
}

fn detect_available_shells() -> Vec<ShellType> {
    let mut shells = vec![];

    #[cfg(target_os = "windows")]
    {
        if which::which("pwsh").is_ok() {
            shells.push(ShellType::PowerShell);
        }
        if which::which("powershell").is_ok() {
            shells.push(ShellType::WindowsPowerShell);
        }
        // cmd.exe는 항상 사용 가능
        shells.push(ShellType::Cmd);

        // Git Bash 확인
        if which::which("bash").is_ok() {
            shells.push(ShellType::Bash);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if which::which("bash").is_ok() {
            shells.push(ShellType::Bash);
        }
        if which::which("zsh").is_ok() {
            shells.push(ShellType::Zsh);
        }
        if which::which("fish").is_ok() {
            shells.push(ShellType::Fish);
        }
    }

    shells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_environment() {
        let env = EnvironmentInfo::detect();

        // OS는 항상 감지되어야 함
        assert!(!env.os.is_empty());

        // 기본 Shell은 Unknown이 아니어야 함 (대부분의 경우)
        println!("Detected environment: {:?}", env);
        println!("Prompt string:\n{}", env.to_prompt_string());
    }

    #[test]
    fn test_shell_display_name() {
        assert_eq!(ShellType::PowerShell.display_name(), "PowerShell 7+");
        assert_eq!(ShellType::Bash.display_name(), "Bash");
    }
}
