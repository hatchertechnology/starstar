//! Human and JSON output.

use std::fmt::Display;

/// Where results go. With `json`, every success prints one JSON document.
#[derive(Debug, Clone, Copy)]
pub struct Output {
    pub json: bool,
}

impl Output {
    /// Prints `value` as pretty JSON, or `human()` as text.
    pub fn emit(&self, value: &serde_json::Value, human: impl FnOnce() -> String) {
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_owned())
            );
        } else {
            println!("{}", human());
        }
    }

    /// Like [`Output::emit`] for any serializable value.
    pub fn emit_value<T: serde::Serialize>(&self, value: &T, human: impl FnOnce() -> String) {
        let v = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
        self.emit(&v, human);
    }
}

pub fn or_dash<T: Display>(v: Option<T>) -> String {
    v.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

pub fn temperature(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_owned(), |t| format!("{t:.1}"))
}

/// `59 -> "59s"`, `1800 -> "30m"`, `3700 -> "1h 1m"`.
pub fn duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h {}m", secs / 3600, secs % 3600 / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations() {
        assert_eq!(duration(59), "59s");
        assert_eq!(duration(1800), "30m");
        assert_eq!(duration(3700), "1h 1m");
    }
}
