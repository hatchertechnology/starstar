//! The MCP endpoint at `/mcp`: one tool per printer operation.
//!
//! The endpoint is mounted inside the `require_auth` layer, which
//! authenticates every request and puts the [`Caller`] into the request
//! extensions. rmcp copies each request's `http::request::Parts` into the
//! tool call context, so a tool reads the caller of its own request. Roles
//! are enforced by the `printers::` functions; the only role comparison here
//! is the `tools/list` filter.
//!
//! The transport is stateless with JSON responses: no session id and no
//! per-session state, so a disabled or demoted user takes effect on the very
//! next request.

use std::sync::Arc;

use axum::Router;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{CacheScope, ListToolsResult, PaginatedRequestParams, ResultType};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData, RoleServer, ServerHandler, tool_handler};

use crate::AppState;
use crate::config::Config;

mod catalog;
mod params;
mod results;
mod tools;

/// The MCP server: the shared state and the tool table.
#[derive(Clone)]
pub struct StarstarMcp {
    state: AppState,
    tools: ToolRouter<StarstarMcp>,
}

impl StarstarMcp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tools: tools::all_tools(),
        }
    }
}

#[tool_handler(
    router = self.tools,
    name = "starstar",
    instructions = "Controls the 3D printers registered with starstar. Call printer_status before start_print or upload_file with print=true, and never call stop_job, delete_file, remove_printer or apply_update without the user's explicit go-ahead. Files larger than the upload limit go through the starstar CLI."
)]
impl ServerHandler for StarstarMcp {
    /// Lists the tools the caller's role allows. Discovery only: `tools/call`
    /// is not filtered, and the `printers::` function rejects a call that is
    /// above the caller's role. Differs per caller, so it is cached privately.
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let caller = results::caller_from_ctx(&context)?;
        let supports_cache_hints = context
            .protocol_version()
            .is_some_and(|version| version >= rmcp::model::ProtocolVersion::V_2026_07_28);
        let tools = self
            .tools
            .list_all()
            .into_iter()
            .filter(|t| catalog::tool_role(&t.name).is_some_and(|r| r <= caller.role))
            .collect();
        Ok(ListToolsResult {
            result_type: Some(ResultType::COMPLETE),
            tools,
            meta: None,
            next_cursor: None,
            ttl_ms: supports_cache_hints.then_some(0),
            cache_scope: supports_cache_hints.then_some(CacheScope::Private),
        })
    }
}

/// Transport settings (see the module docs): stateless JSON, a body limit
/// that fits the largest allowed base64 upload, and DNS-rebinding checks on
/// `Host` and `Origin`.
pub(crate) fn http_config(config: &Config) -> StreamableHttpServerConfig {
    let b64_cap = config.mcp_max_upload_bytes.div_ceil(3) * 4;
    let body_limit = b64_cap + b64_cap / 16 + 64 * 1024;

    let mut hosts = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
    ];
    let mut origins = vec![
        "http://localhost:*".to_owned(),
        "http://127.0.0.1:*".to_owned(),
        "http://[::1]:*".to_owned(),
    ];
    if let Ok(url) = url::Url::parse(&config.public_url)
        && let Some(host) = url.host_str()
    {
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        // `Url::port` is `None` for the scheme's default port.
        hosts.push(match url.port() {
            Some(port) if host.starts_with('[') => format!("[{bare}]:{port}"),
            Some(port) => format!("{host}:{port}"),
            None => bare.to_owned(),
        });
        if let Some(port) = url.port_or_known_default() {
            origins.push(format!("{}://{host}:{port}", url.scheme()));
        }
    }

    StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(hosts)
        .with_allowed_origins(origins)
        .with_max_request_body_bytes(body_limit as usize)
}

/// The `/mcp` router. Merge it before the `require_auth` layer is added.
pub fn router(state: AppState) -> Router {
    // Logging: never enable rmcp's debug or trace logs. They print request
    // arguments, including the printer password in register_printer.
    let http = http_config(&state.config);
    let service = StreamableHttpService::new(
        move || Ok(StarstarMcp::new(state.clone())),
        Arc::new(LocalSessionManager::default()),
        http,
    );
    Router::new().route_service("/mcp", service)
}
