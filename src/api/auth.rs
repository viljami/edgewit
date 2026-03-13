use axum::{
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use std::sync::OnceLock;

static EXPECTED_AUTH_HEADER: OnceLock<Option<String>> = OnceLock::new();

/// Retrieves the cached API key formatted as a Bearer token.
/// It reads the `EDGEWIT_API_KEY` environment variable only once.
fn get_expected_auth_header() -> Option<&'static str> {
    EXPECTED_AUTH_HEADER
        .get_or_init(|| {
            std::env::var("EDGEWIT_API_KEY")
                .ok()
                .filter(|key| !key.is_empty())
                .map(|key| format!("Bearer {}", key))
        })
        .as_deref()
}

/// Axum middleware that enforces optional API key authentication.
/// If `EDGEWIT_API_KEY` is set, requires an `Authorization: Bearer <key>` header.
pub async fn auth_middleware(req: Request, next: Next) -> Result<Response, StatusCode> {
    if let Some(expected_auth) = get_expected_auth_header() {
        let auth_header = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        match auth_header {
            Some(header_val) if header_val == expected_auth => {
                // Authentication successful, proceed to the next handler
            }
            _ => {
                // Missing or invalid API key
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    // Either no API key is configured (Layer 1 trust), or authentication succeeded
    Ok(next.run(req).await)
}
