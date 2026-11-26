//! Random tokens, hashing and constant-time comparison.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// `N` bytes from the operating system's random source.
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("the operating system random source failed");
    buf
}

/// A 256-bit random token: `prefix` plus base64url (no padding).
pub fn random_token(prefix: &str) -> String {
    format!("{prefix}{}", URL_SAFE_NO_PAD.encode(random_bytes::<32>()))
}

/// SHA-256 of a token. Only hashes are stored.
pub fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// Constant-time equality. Different lengths are simply unequal.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A 128-bit setup code as grouped Crockford base32.
pub fn new_setup_code() -> String {
    let bytes = random_bytes::<16>();
    let mut out = String::with_capacity(32);
    let (mut acc, mut bits, mut n) = (0u32, 0u32, 0usize);
    let push = |idx: u32, out: &mut String, n: &mut usize| {
        if *n > 0 && (*n).is_multiple_of(5) {
            out.push('-');
        }
        out.push(CROCKFORD[idx as usize] as char);
        *n += 1;
    };
    for b in bytes {
        acc = (acc << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            push((acc >> bits) & 31, &mut out, &mut n);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        push((acc << (5 - bits)) & 31, &mut out, &mut n);
    }
    out
}

/// Uppercases and strips `-` and whitespace. Crockford look-alikes map to
/// their digits: `I` and `L` to `1`, `O` to `0`.
pub fn normalize_setup_code(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .map(|c| match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            other => other,
        })
        .collect()
}

/// PKCE S256: base64url (no padding) of SHA-256(verifier).
pub fn pkce_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// RFC 7636: 43 to 128 characters of `[A-Za-z0-9._~-]`.
pub fn is_valid_pkce_verifier(v: &str) -> bool {
    (43..=128).contains(&v.len())
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-'))
}

/// A PKCE challenge is a base64url SHA-256: exactly 43 characters.
pub fn is_valid_pkce_challenge(c: &str) -> bool {
    c.len() == 43
        && c.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_tokens_are_prefixed_unique_and_256_bit() {
        let a = random_token("ss_at_");
        let b = random_token("ss_at_");
        assert!(a.starts_with("ss_at_"));
        assert_ne!(a, b);
        assert_eq!(a.len(), "ss_at_".len() + 43);
    }

    #[test]
    fn pkce_matches_rfc7636_appendix_b() {
        assert_eq!(
            pkce_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert!(is_valid_pkce_challenge(
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        ));
        assert!(!is_valid_pkce_challenge("short"));
    }

    #[test]
    fn verifier_rules() {
        assert!(is_valid_pkce_verifier(&"a".repeat(43)));
        assert!(!is_valid_pkce_verifier(&"a".repeat(42)));
        assert!(!is_valid_pkce_verifier(&"a".repeat(129)));
        assert!(!is_valid_pkce_verifier(&format!("{}+", "a".repeat(43))));
    }

    #[test]
    fn setup_codes_are_128_bit_base32_and_normalize() {
        let code = new_setup_code();
        let bare = code.replace('-', "");
        assert_eq!(bare.len(), 26);
        assert_eq!(code.len(), 31);
        assert!(
            bare.chars()
                .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c))
        );
        assert_eq!(
            normalize_setup_code(&code.to_lowercase().replace('-', " ")),
            bare
        );
        assert_ne!(new_setup_code(), code);
    }

    #[test]
    fn normalize_maps_lookalikes() {
        assert_eq!(normalize_setup_code("o1-iL"), "0111");
    }

    #[test]
    fn ct_eq_handles_lengths() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }
}
