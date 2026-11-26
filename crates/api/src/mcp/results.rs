//! Building tool results, and reading the caller of a request.

use axum::http::request::Parts;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer};
use serde::Serialize;
use serde_json::json;

use crate::auth::Caller;
use crate::error::ApiError;

/// The caller `require_auth` put into this request. Missing only if `/mcp`
/// were ever mounted outside the auth layer.
pub(crate) fn caller_of(parts: &Parts) -> Result<Caller, ErrorData> {
    parts
        .extensions
        .get::<Caller>()
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("request was not authenticated", None))
}

/// Like [`caller_of`], for handlers that get the whole request context.
pub(crate) fn caller_from_ctx(ctx: &RequestContext<RoleServer>) -> Result<Caller, ErrorData> {
    let parts = ctx
        .extensions
        .get::<Parts>()
        .ok_or_else(|| ErrorData::internal_error("request was not authenticated", None))?;
    caller_of(parts)
}

/// A tool-level error: `isError: true`, the HTTP-style status and a message.
/// Internal errors are not shown to the client.
pub(crate) fn tool_error(err: ApiError) -> CallToolResult {
    let status = err.status();
    let message = match err {
        ApiError::Internal(_) => "internal error".to_owned(),
        other => other.message(),
    };
    let reason = status.canonical_reason().unwrap_or("error").to_lowercase();
    let mut result = CallToolResult::error(vec![ContentBlock::text(format!(
        "{reason} ({}): {message}",
        status.as_u16()
    ))]);
    result.structured_content = Some(json!({"status": status.as_u16(), "error": message}));
    result
}

/// A successful result whose `structuredContent` is `{ key: value }`.
pub(crate) fn structured_ok(key: &str, value: impl Serialize) -> CallToolResult {
    match serde_json::to_value(value) {
        Ok(value) => CallToolResult::structured(json!({ key: value })),
        Err(_) => tool_error(ApiError::Internal("could not encode the result".to_owned())),
    }
}

/// Maps an operation's result to a tool result.
pub(crate) fn outcome<T>(
    result: Result<T, ApiError>,
    ok: impl FnOnce(T) -> CallToolResult,
) -> CallToolResult {
    match result {
        Ok(value) => ok(value),
        Err(err) => tool_error(err),
    }
}
