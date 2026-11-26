//! A manual digest `Authorization` header, for requests whose body is a stream.
//!
//! diqwest re-sends a request after the challenge by cloning it, and a stream
//! cannot be cloned. Uploads therefore probe for the challenge first and sign
//! the real request themselves.

use digest_auth::{AuthContext, HttpMethod};

use super::PrusaError;

/// Builds the `Authorization` header answering a `WWW-Authenticate` challenge
/// for `method` on `uri` (the encoded path plus query).
pub(crate) fn authorization(
    www_authenticate: &str,
    username: &str,
    password: &str,
    method: &str,
    uri: &str,
) -> Result<String, PrusaError> {
    // Error text must never include the password, so none of it is forwarded
    // beyond the parser's own message about the challenge.
    let mut prompt = digest_auth::parse(www_authenticate)
        .map_err(|e| PrusaError::Decode(format!("bad digest challenge from printer: {e}")))?;
    let context = AuthContext::new_with_method(
        username,
        password,
        uri,
        None::<&[u8]>,
        HttpMethod::from(method.to_owned()),
    );
    let answer = prompt
        .respond(&context)
        .map_err(|e| PrusaError::Decode(format!("could not answer digest challenge: {e}")))?;
    Ok(answer.to_header_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_a_challenge_without_leaking_the_password() {
        let header = authorization(
            r#"Digest realm="PrusaLink", nonce="abc123", qop="auth", algorithm=MD5"#,
            "maker",
            "hunter2",
            "PUT",
            "/api/v1/files/usb/a.bgcode",
        )
        .unwrap();
        assert!(header.starts_with("Digest "));
        assert!(header.contains(r#"username="maker""#));
        assert!(header.contains(r#"uri="/api/v1/files/usb/a.bgcode""#));
        assert!(header.contains(r#"nonce="abc123""#));
        assert!(!header.contains("hunter2"));
    }

    #[test]
    fn bad_challenge_is_a_decode_error() {
        let err = authorization("Basic realm=x", "u", "secret", "PUT", "/x").unwrap_err();
        assert!(matches!(err, PrusaError::Decode(_)));
        assert!(!err.to_string().contains("secret"));
    }
}
