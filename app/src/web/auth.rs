//! Token gate for the dashboard.
//!
//! The rule is positional, not a preference: when the server is bound to
//! loopback, only processes on this machine can reach it and a token would add
//! nothing. The moment it is bound anywhere else, every route that exposes
//! telemetry requires the token — hardware serials, SPD contents and PCI config
//! space are not things to hand to whoever else is on the coffee-shop Wi-Fi.
//!
//! The token is generated per run and never written to disk, so stopping the
//! server invalidates it.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::AppState;

/// Token length in hex characters (128 bits of entropy).
const TOKEN_HEX_LEN: usize = 32;

/// Generate a fresh access token.
pub fn generate_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    // 128 bits, hex-encoded: short enough to retype, long enough that guessing
    // is not a strategy.
    let bytes: [u8; TOKEN_HEX_LEN / 2] = rng.random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Middleware: allow the request through when no token is required, or when the
/// presented token matches.
pub async fn require_token(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(expected) = state.token.as_deref() else {
        return next.run(request).await; // loopback: nothing to check
    };

    match presented_token(&request) {
        Some(got) if constant_time_eq(&got, expected) => next.run(request).await,
        Some(_) => deny("invalid token").into_response(),
        None => deny("missing token").into_response(),
    }
}

fn deny(reason: &str) -> (StatusCode, [(&'static str, &'static str); 1], String) {
    (
        StatusCode::UNAUTHORIZED,
        [("WWW-Authenticate", "Bearer realm=\"SensorView\"")],
        format!("{reason}: this dashboard is exposed on the network and requires the access token shown in the app"),
    )
}

/// Pull the token from `Authorization: Bearer <t>`, falling back to `?token=`.
///
/// The query fallback exists because a browser cannot set headers when opening
/// a page or a WebSocket — it is how the URL shown in the UI works.
fn presented_token(request: &Request) -> Option<String> {
    if let Some(value) = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(value.trim().to_string());
    }
    request.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == "token").then(|| v.to_string())
        })
    })
}

/// Compare without leaking the position of the first mismatch through timing.
///
/// Lengths are compared first, which is fine: the length of a fixed-format
/// token is not a secret.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn request_with(header: Option<&str>, uri: &str) -> Request {
        let mut b = Request::builder().uri(uri);
        if let Some(h) = header {
            b = b.header(axum::http::header::AUTHORIZATION, h);
        }
        b.body(Body::empty()).unwrap()
    }

    #[test]
    fn tokens_are_long_random_hex() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), TOKEN_HEX_LEN);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "tokens must not repeat across runs");
    }

    #[test]
    fn reads_the_token_from_the_authorization_header() {
        let r = request_with(Some("Bearer abc123"), "/api/telemetry");
        assert_eq!(presented_token(&r).as_deref(), Some("abc123"));
    }

    #[test]
    fn falls_back_to_the_query_string_for_browsers() {
        // Browsers can't set headers when opening a page or a WebSocket.
        let r = request_with(None, "/ws/telemetry?token=abc123");
        assert_eq!(presented_token(&r).as_deref(), Some("abc123"));

        let r = request_with(None, "/?foo=1&token=xyz&bar=2");
        assert_eq!(presented_token(&r).as_deref(), Some("xyz"));
    }

    #[test]
    fn header_wins_over_query_and_absence_is_none() {
        let r = request_with(Some("Bearer fromheader"), "/?token=fromquery");
        assert_eq!(presented_token(&r).as_deref(), Some("fromheader"));
        assert_eq!(presented_token(&request_with(None, "/api/telemetry")), None);
        // A non-Bearer scheme is not a token.
        assert_eq!(presented_token(&request_with(Some("Basic abc"), "/")), None);
    }

    #[test]
    fn constant_time_eq_matches_string_equality() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
    }
}
