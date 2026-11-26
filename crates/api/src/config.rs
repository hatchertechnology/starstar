//! Runtime configuration, read from `STARSTAR_*` environment variables.
//!
//! **Interface contract:** every part reads settings from `AppState::config`.
//! New settings are added here (one field, one env var, one default) rather
//! than read from the environment elsewhere.

use std::env;

/// Settings for one API process. Not `Debug`: it holds the setup token.
#[derive(Clone)]
pub struct Config {
    /// `STARSTAR_DATABASE`, default `starstar.db`.
    pub database: String,
    /// `STARSTAR_BIND`, default `127.0.0.1:8080`.
    pub bind: String,
    /// `STARSTAR_PUBLIC_URL`, default `http://127.0.0.1:8080`, without a
    /// trailing slash. Used as the OAuth issuer and in redirects.
    pub public_url: String,
    /// `STARSTAR_ALLOW_INSECURE_HTTP=1` allows a non-loopback bind with an
    /// http public URL (spec resolution 4). Default false.
    pub allow_insecure_http: bool,
    /// `STARSTAR_SETUP_TOKEN`: fixed first-admin setup code instead of a
    /// random one printed at startup (spec resolution 1).
    pub setup_token: Option<String>,
    /// `STARSTAR_MAX_UPLOAD_MB`, default 512, as bytes.
    pub max_upload_bytes: u64,
    /// `STARSTAR_MCP_MAX_UPLOAD_MB`, default 25, as bytes.
    pub mcp_max_upload_bytes: u64,
    /// `STARSTAR_WEB_DIR`, default `web/.output/public`.
    pub web_dir: String,
}

const MIB: u64 = 1024 * 1024;

impl Config {
    /// Reads the environment, falling back to defaults. Invalid numbers fall
    /// back to the default too.
    pub fn from_env() -> Self {
        let var = |k: &str| env::var(k).ok().filter(|v| !v.is_empty());
        let mb = |k: &str, default: u64| {
            var(k)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(default)
                * MIB
        };
        let defaults = Self::default();
        Self {
            database: var("STARSTAR_DATABASE").unwrap_or(defaults.database),
            bind: var("STARSTAR_BIND").unwrap_or(defaults.bind),
            public_url: var("STARSTAR_PUBLIC_URL")
                .map(|u| u.trim_end_matches('/').to_owned())
                .unwrap_or(defaults.public_url),
            allow_insecure_http: var("STARSTAR_ALLOW_INSECURE_HTTP").is_some_and(|v| v == "1"),
            setup_token: var("STARSTAR_SETUP_TOKEN"),
            max_upload_bytes: mb("STARSTAR_MAX_UPLOAD_MB", 512),
            mcp_max_upload_bytes: mb("STARSTAR_MCP_MAX_UPLOAD_MB", 25),
            web_dir: var("STARSTAR_WEB_DIR").unwrap_or(defaults.web_dir),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: "starstar.db".to_owned(),
            bind: "127.0.0.1:8080".to_owned(),
            public_url: "http://127.0.0.1:8080".to_owned(),
            allow_insecure_http: false,
            setup_token: None,
            max_upload_bytes: 512 * MIB,
            mcp_max_upload_bytes: 25 * MIB,
            web_dir: "web/.output/public".to_owned(),
        }
    }
}
