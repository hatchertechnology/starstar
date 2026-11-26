//! Validation of file paths, storage names, camera ids and update environments.

use crate::error::ApiError;
use crate::prusalink::PrusaLinkClient;

/// Splits a starstar file path (as captured by axum, already percent-decoded)
/// into validated segments. The client pushes them with `path_segments_mut`,
/// which does the percent-encoding.
pub fn file_segments(path: &str) -> Result<Vec<String>, ApiError> {
    if path.contains('\0') {
        return Err(ApiError::BadRequest(
            "file paths cannot contain NUL bytes".to_owned(),
        ));
    }
    if path.contains('\\') {
        return Err(ApiError::BadRequest(
            "file paths cannot contain backslashes".to_owned(),
        ));
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.iter().any(|s| s.is_empty()) {
        return Err(ApiError::BadRequest(
            "file paths cannot contain empty segments".to_owned(),
        ));
    }
    if segments.iter().any(|s| matches!(*s, "." | "..")) {
        return Err(ApiError::BadRequest(
            "file paths cannot contain `.` or `..` segments".to_owned(),
        ));
    }
    Ok(segments.into_iter().map(str::to_owned).collect())
}

/// Ensures `storage` is one of the printer's storages, matched against its
/// `path` with `/` trimmed (`/usb/` is `usb`), and returns that name.
pub(crate) async fn resolve_storage(
    client: &PrusaLinkClient,
    storage: &str,
) -> Result<String, ApiError> {
    let list = client.storage().await?.storage_list;
    let names: Vec<String> = list
        .iter()
        .map(|s| s.path.trim_matches('/').to_owned())
        .collect();
    if !storage.is_empty() && names.iter().any(|n| n == storage) {
        return Ok(storage.to_owned());
    }
    Err(ApiError::NotFound(format!(
        "printer has no storage `{storage}`; available: {}",
        names.join(", ")
    )))
}

/// Camera ids: 1-64 ASCII alphanumerics, `-` or `_`.
pub fn validate_camera_id(id: &str) -> Result<(), ApiError> {
    let ok = (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(ApiError::BadRequest("invalid camera id".to_owned()))
    }
}

/// Only `prusalink` exists in the OpenAPI enum.
pub fn validate_update_env(env: &str) -> Result<(), ApiError> {
    if env == "prusalink" {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!(
            "unknown update environment `{env}`, expected `prusalink`"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_segments() {
        assert_eq!(
            file_segments("cube-12mm.bgcode").unwrap(),
            vec!["cube-12mm.bgcode"]
        );
        assert_eq!(
            file_segments("My Parts/café #1?%.bgcode").unwrap(),
            vec!["My Parts", "café #1?%.bgcode"]
        );
    }

    #[test]
    fn rejects_unsafe_paths() {
        for bad in [
            "", "/a", "a/", "folder/", "a//b", "..", "a/../b", "./a", "a\0b", "a\\b", "..\\x",
        ] {
            assert!(
                matches!(file_segments(bad), Err(ApiError::BadRequest(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn camera_ids() {
        assert!(validate_camera_id("Z42D4U2NqEX").is_ok());
        for bad in ["", "a/b", "a b", "..", &"x".repeat(65)] {
            assert!(validate_camera_id(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn update_envs() {
        assert!(validate_update_env("prusalink").is_ok());
        // The OpenAPI enum only has `prusalink` (review amendment 14).
        for bad in ["system", "../x", ""] {
            assert!(validate_update_env(bad).is_err(), "{bad:?}");
        }
    }
}
