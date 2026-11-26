//! `POST /oauth/register`: RFC 7591 dynamic client registration (public
//! clients only).

use axum::extract::State;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use super::OAuthError;
use super::redirect::is_registrable;
use crate::AppState;
use crate::auth::client_ip::client_ip;

pub fn routes() -> Router<AppState> {
    Router::new().route("/oauth/register", post(register))
}

#[derive(Deserialize)]
struct Registration {
    #[serde(default)]
    redirect_uris: Vec<String>,
    client_name: Option<String>,
    token_endpoint_auth_method: Option<String>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
}

/// Trimmed, control characters removed, at most 100 characters.
/// Unicode general category Cf (format) characters: bidi controls, zero-width
/// characters, the BOM, soft hyphen and tag characters.
fn is_format_char(c: char) -> bool {
    matches!(c as u32,
        0x00AD | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x08E2 | 0x180E
        | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F
        | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F
        | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F)
}

fn sanitize_name(name: Option<&str>) -> String {
    let cleaned: String = name
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control() && !is_format_char(*c))
        .collect();
    let cleaned: String = cleaned.trim().chars().take(100).collect();
    if cleaned.is_empty() {
        "Unnamed app".to_owned()
    } else {
        cleaned
    }
}

async fn register(
    State(state): State<AppState>,
    parts: Parts,
    Json(req): Json<Registration>,
) -> Result<(StatusCode, Json<Value>), OAuthError> {
    let ip = client_ip(&parts, &state.config.trusted_proxies);
    if !state.auth.register_limiter().hit(&ip, state.auth.now()) {
        return Err(OAuthError::too_many_requests());
    }

    if !(1..=10).contains(&req.redirect_uris.len()) {
        return Err(OAuthError::invalid_redirect_uri(
            "provide between 1 and 10 redirect_uris",
        ));
    }
    if req
        .redirect_uris
        .iter()
        .any(|u| !is_registrable(u, &state.config.public_url, &state.config.bind))
    {
        return Err(OAuthError::invalid_redirect_uri(
            "redirect URIs must be https, or http on 127.0.0.1, localhost or [::1], and not on this server",
        ));
    }
    if req
        .token_endpoint_auth_method
        .as_deref()
        .is_some_and(|m| m != "none")
    {
        return Err(OAuthError::invalid_client_metadata(
            "only public clients are supported (token_endpoint_auth_method none)",
        ));
    }
    if req.grant_types.as_ref().is_some_and(|g| {
        g.iter()
            .any(|t| t != "authorization_code" && t != "refresh_token")
    }) {
        return Err(OAuthError::invalid_client_metadata(
            "grant_types may only contain authorization_code and refresh_token",
        ));
    }
    if req
        .response_types
        .as_ref()
        .is_some_and(|r| r.len() != 1 || r[0] != "code")
    {
        return Err(OAuthError::invalid_client_metadata(
            "response_types must be [\"code\"]",
        ));
    }

    let name = sanitize_name(req.client_name.as_deref());
    let client = state.auth.register_client(&name, &req.redirect_uris)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "client_id": client.id,
            "client_id_issued_at": client.created_at,
            "client_name": client.name,
            "redirect_uris": client.redirect_uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bidi_and_format_characters_are_stripped() {
        assert_eq!(
            sanitize_name(Some("Good\u{202E}ppA\u{200B}live\u{FEFF}")),
            "GoodppAlive"
        );
        assert_eq!(
            sanitize_name(Some("a\u{2066}b\u{2069}c\u{061C}d\u{00AD}e")),
            "abcde"
        );
        assert_eq!(sanitize_name(Some("\u{E0041}tag\u{E007F}")), "tag");
        assert_eq!(sanitize_name(Some("\u{200E}\u{202A}")), "Unnamed app");
        // Ordinary non-ASCII text survives.
        assert_eq!(sanitize_name(Some("Café 猫 🐱")), "Café 猫 🐱");
    }

    #[test]
    fn names_are_sanitized() {
        assert_eq!(sanitize_name(Some("  Evil\u{0007}App  ")), "EvilApp");
        assert_eq!(sanitize_name(None), "Unnamed app");
        assert_eq!(sanitize_name(Some("   ")), "Unnamed app");
        assert_eq!(sanitize_name(Some(&"x".repeat(150))).chars().count(), 100);
    }
}
