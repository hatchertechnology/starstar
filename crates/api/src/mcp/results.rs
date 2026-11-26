//! Building tool results, and reading the caller of a request.

use axum::http::request::Parts;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer};
use serde::Serialize;
use serde_json::json;

use crate::auth::Caller;
use crate::error::ApiError;
use crate::printers::cameras::Snapshot;

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

/// A successful result that is one sentence.
pub(crate) fn text_ok(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text)])
}

/// Unwraps a `Result<_, ApiError>` inside a tool, returning the tool error
/// as the tool's result.
macro_rules! tool_try {
    ($e:expr) => {
        match $e {
            Ok(value) => value,
            Err(err) => return Ok($crate::mcp::results::tool_error(err)),
        }
    };
}
pub(crate) use tool_try;

/// A picture as image content (standard base64).
pub(crate) fn image_ok(snapshot: Snapshot) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::image(
        STANDARD.encode(&snapshot.bytes),
        snapshot.content_type,
    )])
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

const MIB: u64 = 1024 * 1024;

/// The size limit as people write it: `25 MB`, or bytes below one MiB.
pub(crate) fn format_limit(max: u64) -> String {
    if max >= MIB {
        format!("{} MB", max / MIB)
    } else {
        format!("{max} bytes")
    }
}

pub(crate) fn upload_too_large(max: u64) -> ApiError {
    ApiError::PayloadTooLarge(format!(
        "file is larger than the MCP upload limit of {}; upload it with `starstar files upload` instead",
        format_limit(max)
    ))
}

/// Decodes the base64 of an upload. ASCII whitespace is ignored (agents wrap
/// lines). The length is checked before decoding, so an oversized payload
/// never allocates the file.
pub(crate) fn decode_upload(content_base64: &str, max: u64) -> Result<Vec<u8>, ApiError> {
    let cleaned: String = content_base64
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if cleaned.len() as u64 > max.div_ceil(3) * 4 {
        return Err(upload_too_large(max));
    }
    let bytes = STANDARD
        .decode(cleaned)
        .map_err(|_| ApiError::BadRequest("content_base64 is not valid base64".to_owned()))?;
    if bytes.len() as u64 > max {
        return Err(upload_too_large(max));
    }
    Ok(bytes)
}
