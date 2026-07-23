//! The dashboard SPA, compiled into the binary.
//!
//! `rust-embed` bakes `web-dashboard/` into the executable at build time, so a
//! portable build stays a single file with no runtime asset directory to lose.

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "web-dashboard/"]
struct Dashboard;

/// `GET /` — the dashboard shell.
pub async fn index() -> Response {
    serve("index.html")
}

/// `GET /{*path}` — any other bundled asset.
pub async fn static_file(uri: Uri) -> Response {
    serve(uri.path().trim_start_matches('/'))
}

fn serve(path: &str) -> Response {
    // Directory requests map to the shell, so a refresh on any client-side
    // route still loads the app.
    let path = if path.is_empty() { "index.html" } else { path };

    match Dashboard::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, mime.as_ref().to_string()),
                    // Assets are versioned by the binary itself; no caching, so
                    // an upgraded app never serves a stale dashboard.
                    (header::CACHE_CONTROL, "no-cache".to_string()),
                ],
                file.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dashboard_shell_is_embedded() {
        let index = Dashboard::get("index.html").expect("index.html must be bundled");
        let html = String::from_utf8_lossy(&index.data);
        assert!(html.contains("/ws/telemetry"), "the SPA must open the telemetry socket");
    }

    #[test]
    fn every_bundled_asset_resolves_to_a_known_mime_type() {
        let mut count = 0;
        for path in Dashboard::iter() {
            let mime = mime_guess::from_path(path.as_ref()).first_or_octet_stream();
            assert_ne!(
                mime.as_ref(),
                "application/octet-stream",
                "{path} has no recognised MIME type"
            );
            count += 1;
        }
        assert!(count >= 3, "expected the shell, script and stylesheet");
    }
}
