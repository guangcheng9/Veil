use axum::http::{header, HeaderMap, StatusCode};
use sha2::{Digest, Sha256};

pub fn token_authorized(
    headers: &HeaderMap,
    expected_hash: &str,
    require: bool,
) -> Result<(), StatusCode> {
    if expected_hash.is_empty() && !require {
        return Ok(());
    }
    let Some(token) = extract_token(headers) else {
        return if require {
            Err(StatusCode::UNAUTHORIZED)
        } else {
            Ok(())
        };
    };
    let hash = hex::encode(Sha256::digest(token.as_bytes()));
    if hash == expected_hash {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub fn extract_token(headers: &HeaderMap) -> Option<String> {
    for name in ["x-veil-admin-token", "x-dgw-admin-token"] {
        if let Some(token) = headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            return Some(token.to_string());
        }
    }

    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
    {
        return Some(token.to_string());
    }

    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(value) = part
            .strip_prefix("veil_admin=")
            .or_else(|| part.strip_prefix("dgw_admin="))
        {
            let token = value.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

fn bearer_token(value: &str) -> Option<&str> {
    let mut parts = value.split_whitespace();
    let scheme = parts.next()?;
    let token = parts.next()?;
    if !scheme.eq_ignore_ascii_case("bearer") || parts.next().is_some() || token.is_empty() {
        return None;
    }
    Some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    fn hash(token: &str) -> String {
        hex::encode(Sha256::digest(token.as_bytes()))
    }

    #[test]
    fn accepts_dgw_admin_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-dgw-admin-token", HeaderValue::from_static("dgw-token"));
        assert_eq!(extract_token(&headers).as_deref(), Some("dgw-token"));
        assert!(token_authorized(&headers, &hash("dgw-token"), true).is_ok());
    }

    #[test]
    fn accepts_bearer_authorization_case_insensitively() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("bearer   secret-token"),
        );
        assert_eq!(extract_token(&headers).as_deref(), Some("secret-token"));
        assert!(token_authorized(&headers, &hash("secret-token"), true).is_ok());
        assert!(token_authorized(&headers, &hash("other-token"), true).is_err());
    }

    #[test]
    fn prefers_explicit_admin_header_over_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-veil-admin-token",
            HeaderValue::from_static("from-header"),
        );
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer from-authorization"),
        );
        assert_eq!(extract_token(&headers).as_deref(), Some("from-header"));
    }

    #[test]
    fn empty_primary_header_falls_through_to_dgw_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-veil-admin-token", HeaderValue::from_static(""));
        headers.insert("x-dgw-admin-token", HeaderValue::from_static("from-dgw"));
        assert_eq!(extract_token(&headers).as_deref(), Some("from-dgw"));
    }

    #[test]
    fn accepts_existing_admin_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=dark; dgw_admin=cookie-token"),
        );
        assert_eq!(extract_token(&headers).as_deref(), Some("cookie-token"));
    }

    #[test]
    fn rejects_malformed_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic not-bearer"),
        );
        assert_eq!(extract_token(&headers), None);

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token with-extra-part"),
        );
        assert_eq!(extract_token(&headers), None);
    }
}
