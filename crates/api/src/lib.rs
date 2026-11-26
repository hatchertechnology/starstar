//! HTTP API for starstar.

use starstar_shared::APP_NAME;

/// Returns a human-readable status line for the API.
pub fn status() -> String {
    format!("{APP_NAME} api: ok")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mentions_app_name() {
        assert!(status().starts_with("starstar"));
    }
}
