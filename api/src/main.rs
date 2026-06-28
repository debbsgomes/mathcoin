use axum::{routing::{get, post}, Router, Json};
use axum::middleware;
use axum::response::Response;
use serde::Serialize;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod auth;
mod chain;
mod challenge;
mod config;
mod db;
mod difficulty;
mod error;
mod rate_limit;
mod routes;
mod state;

use auth::JwksVerifier;
use config::AppConfig;
use difficulty::{RealClock, RetargetConfig, MintingStats};
use state::AppState;
use rate_limit::RateLimiter;
use std::sync::atomic::AtomicU32;
use std::time::Duration;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    dotenvy::dotenv().ok();

    let config = AppConfig::from_env();

    let pool = db::create_pool(&config.database_url)
        .await
        .expect("failed to create database pool");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let jwks_url = config.jwks_url.expect("JWKS_URL required in JWKS mode");
    let verifier = JwksVerifier::new(jwks_url, config.jwt_iss, config.jwt_aud);

    let retarget_config = RetargetConfig {
        window: Duration::from_secs(60),
        target_rate: 20.0,
        hysteresis_low: 15.0,
        hysteresis_high: 25.0,
        diff_min: 1,
        diff_max: 12,
        max_step: 1,
    };

    let difficulty = Arc::new(AtomicU32::new(3));
    let mint_stats = Arc::new(tokio::sync::Mutex::new(MintingStats::new()));
    let clock: Arc<dyn difficulty::Clock> = Arc::new(RealClock);

    let rate_limiter = Arc::new(RateLimiter::new(60, 60)); // 60 req per 60s window

    let state = Arc::new(AppState {
        db: pool,
        verifier: Arc::new(verifier),
        difficulty: difficulty.clone(),
        mint_stats: mint_stats.clone(),
        clock: clock.clone(),
        retarget_config: retarget_config.clone(),
        rate_limiter: rate_limiter.clone(),
        onchain_config: config.onchain.clone(),
    });

    // Periodic retarget: every 5s, recompute difficulty from the sliding window
    let retarget_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let current = retarget_state.difficulty.load(std::sync::atomic::Ordering::Relaxed);
            let mut stats = retarget_state.mint_stats.lock().await;
            let now = retarget_state.clock.now();
            let rate = stats.rate(retarget_state.retarget_config.window, now);
            let new_diff = difficulty::difficulty_retarget(current, rate, &retarget_state.retarget_config);
            if new_diff != current {
                retarget_state.difficulty.store(new_diff, std::sync::atomic::Ordering::Relaxed);
            }
        }
    });

    let frontend_origin = config.frontend_origin;

    let cors = CorsLayer::new()
        .allow_origin(frontend_origin.parse::<axum::http::HeaderValue>()
            .expect("FRONTEND_ORIGIN must be a valid HTTP Origin (e.g. http://localhost:5173)"))
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]);

    let app = Router::new()
        .route("/api/health", get(health_check))
        .route("/api/session", post(routes::session::handler))
        .route("/api/me", get(routes::me::handler))
        .route("/api/challenge", get(routes::challenge::handler))
        .route("/api/mint", post(routes::mint::handler))
        .route("/api/stats", get(routes::stats::handler))
        .route("/api/audit", get(routes::audit::handler))
        .route("/api/claim-address", post(routes::claim_address::handler))
        .route("/api/claim-data", get(routes::claim_data::handler))
        .route("/api/claim-relay", get(routes::claim_relay::handler))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit::rate_limit_middleware))
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let bind_address = config.bind_address;
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .expect("failed to bind");

    tracing::info!("mathcoin-api listening on http://{bind_address}");
    axum::serve(listener, app).await.unwrap();
}

async fn security_headers(
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::STRICT_TRANSPORT_SECURITY,
        "max-age=63072000; includeSubDomains; preload"
            .parse()
            .expect("hardcoded HSTS header"),
    );
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("hardcoded X-Content-Type-Options header"),
    );
    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        "DENY".parse().expect("hardcoded X-Frame-Options header"),
    );
    headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        "default-src 'none'".parse().expect("hardcoded CSP header"),
    );
    response
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}
