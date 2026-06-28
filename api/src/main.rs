use axum::{routing::{get, post}, Router};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

mod auth;
mod db;
mod error;
mod routes;
mod state;

use auth::JwksVerifier;
use state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    dotenvy::dotenv().ok();

    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = db::create_pool(&database_url)
        .await
        .expect("failed to create database pool");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let jwks_url = std::env::var("JWKS_URL").expect("JWKS_URL must be set");
    let jwt_iss = std::env::var("JWT_ISS").expect("JWT_ISS must be set");
    let jwt_aud = std::env::var("JWT_AUD").expect("JWT_AUD must be set");

    let verifier = JwksVerifier::new(jwks_url, jwt_iss, jwt_aud);

    let state = Arc::new(AppState {
        db: pool,
        verifier: Arc::new(verifier),
    });

    let frontend_origin = std::env::var("FRONTEND_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:5173".into());

    let cors = CorsLayer::new()
        .allow_origin(frontend_origin.parse::<axum::http::HeaderValue>().unwrap())
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]);

    let app = Router::new()
        .route("/api/session", post(routes::session::handler))
        .route("/api/me", get(routes::me::handler))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind");

    tracing::info!("mathcoin-api listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}
