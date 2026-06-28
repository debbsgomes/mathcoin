use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Simple in-memory rate limiter. Per-key sliding-window counters.
/// Defaults: 60 requests per 60 seconds per key.
pub struct RateLimiter {
    window_secs: u64,
    max_requests: u64,
    buckets: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(window_secs: u64, max_requests: u64) -> Self {
        Self {
            window_secs,
            max_requests,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    async fn check(&self, key: String) -> bool {
        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);
        let cutoff = now.checked_sub(window).unwrap_or(now);

        let mut buckets = self.buckets.lock().await;
        let timestamps = buckets.entry(key).or_default();

        // Evict old entries
        timestamps.retain(|t| *t >= cutoff);

        if timestamps.len() >= self.max_requests as usize {
            return false; // rate limited
        }

        timestamps.push(now);
        true
    }
}

/// Middleware: extracts rate-limit key from the Authorization header
/// (different JWT tokens → different subs → different keys).
/// Falls back to "anonymous" for unauthenticated requests.
pub async fn rate_limit_middleware(
    State(state): State<Arc<crate::state::AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let key = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|auth| {
            // Use portion of the token as key (different sessions = different tokens)
            let start = auth.len().saturating_sub(80);
            auth[start..].to_string()
        })
        .unwrap_or_else(|| "anonymous".to_string());

    if !state.rate_limiter.check(key).await {
        let body = axum::Json(serde_json::json!({
            "error": "rate_limited",
            "message": "Too many requests. Please slow down."
        }));
        return (axum::http::StatusCode::TOO_MANY_REQUESTS, body).into_response();
    }

    next.run(req).await
}
