use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::path::Path;
use chrono::Local;

#[allow(dead_code)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

pub struct Logger {
    file: Mutex<Option<std::fs::File>>,
}

static GLOBAL_LOGGER: Logger = Logger {
    file: Mutex::new(None),
};
static ACTIVE_LOG_PATH: OnceLock<String> = OnceLock::new();

pub const GREEN:   &str = "\x1b[32m";
pub const YELLOW:  &str = "\x1b[33m";
pub const RED:     &str = "\x1b[31m";
pub const CYAN:    &str = "\x1b[36m";
pub const MAGENTA: &str = "\x1b[35m";
pub const BOLD:    &str = "\x1b[1m";
pub const RESET:   &str = "\x1b[0m";

impl Logger {
    pub fn init(path: &str) {
        eprintln!("[Logger] Initializing with log path: {}", path);

        let mut candidates = vec![path.to_string()];
        let data_fallback = crate::platform::paths::fallback_data_dir()
            .join("auraseek.log")
            .to_string_lossy()
            .to_string();
        if !candidates.iter().any(|p| p == &data_fallback) {
            candidates.push(data_fallback);
        }
        if !candidates.iter().any(|p| p == "auraseek.log") {
            candidates.push("auraseek.log".to_string());
        }
        #[cfg(not(windows))]
        {
            let tmp_fallback = std::env::temp_dir()
                .join("auraseek.log")
                .to_string_lossy()
                .to_string();
            if !candidates.iter().any(|p| p == &tmp_fallback) {
                candidates.push(tmp_fallback);
            }
        }

        for candidate in candidates {
            if let Some(parent) = Path::new(&candidate).parent() {
                if let Err(e) = create_dir_all(parent) {
                    eprintln!("[Logger] Failed to create log directory {}: {}", parent.display(), e);
                    continue;
                }
            }

            match OpenOptions::new().create(true).append(true).open(&candidate) {
                Ok(file) => {
                    let mut internal = GLOBAL_LOGGER.file.lock().unwrap();
                    *internal = Some(file);
                    let _ = ACTIVE_LOG_PATH.set(candidate.clone());
                    eprintln!("[Logger] Successfully opened log file: {}", candidate);
                    return;
                }
                Err(e) => {
                    eprintln!("[Logger] Failed to open log file {}: {}", candidate, e);
                }
            }
        }

        eprintln!("[Logger] All log path candidates failed; file logging disabled");
    }

    pub fn active_log_path() -> Option<String> {
        ACTIVE_LOG_PATH.get().cloned()
    }

    fn remove_ansi(text: &str) -> String {
        let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        re.replace_all(text, "").to_string()
    }

    fn log(level: LogLevel, msg: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let (prefix, color) = match level {
            LogLevel::Info  => ("info",  GREEN),
            LogLevel::Warn  => ("warn",  YELLOW),
            LogLevel::Error => ("error", RED),
        };
        
        let console_msg = format!("{} [{}] {}", timestamp, prefix, msg);
        let colored_prefix = format!("{} {}{}[{}]{}{} {}", timestamp, color, BOLD, prefix, RESET, RESET, msg);

        // Print to console
        eprintln!("{}", colored_prefix);

        // Write to file
        if let Ok(mut internal) = GLOBAL_LOGGER.file.lock() {
            if let Some(file) = internal.as_mut() {
                let clean_msg = Self::remove_ansi(&console_msg);
                let _ = writeln!(file, "{}", clean_msg);
                let _ = file.flush();
            }
        }
    }

    pub fn info(msg: &str)  { Self::log(LogLevel::Info, msg); }
    pub fn warn(msg: &str)  { Self::log(LogLevel::Warn, msg); }
    #[allow(dead_code)]
    pub fn error(msg: &str) { Self::log(LogLevel::Error, msg); }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => ($crate::shared::logging::logger::Logger::info(&format!($($arg)*)));
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => ($crate::shared::logging::logger::Logger::warn(&format!($($arg)*)));
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => ($crate::shared::logging::logger::Logger::error(&format!($($arg)*)));
}
