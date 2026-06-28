/// Tests for config validation — startup fails clearly on missing env vars.
use std::env;

#[test]
#[should_panic(expected = "Missing required environment variable: DATABASE_URL")]
fn missing_database_url_panics() {
    env::remove_var("DATABASE_URL");
    env::set_var("JWT_ISS", "https://test.supabase.co/auth/v1");
    env::set_var("JWT_AUD", "authenticated");
    env::set_var("JWKS_URL", "https://test.supabase.co/auth/v1/.well-known/jwks.json");

    let _ = mathcoin_api::config::AppConfig::from_env();
}

#[test]
#[should_panic(expected = "Missing required environment variable: JWT_ISS")]
fn missing_jwt_iss_panics() {
    env::remove_var("JWT_ISS");
    env::set_var("DATABASE_URL", "postgres://localhost/test");
    env::set_var("JWT_AUD", "authenticated");
    env::set_var("JWKS_URL", "https://test.supabase.co/auth/v1/.well-known/jwks.json");

    let _ = mathcoin_api::config::AppConfig::from_env();
}

#[test]
fn all_required_vars_present_succeeds() {
    env::set_var("DATABASE_URL", "postgres://localhost/test");
    env::set_var("JWT_ISS", "https://test.supabase.co/auth/v1");
    env::set_var("JWT_AUD", "authenticated");
    env::set_var("JWKS_URL", "https://test.supabase.co/auth/v1/.well-known/jwks.json");

    let config = mathcoin_api::config::AppConfig::from_env();
    assert!(config.jwks_url.is_some());
    assert_eq!(config.jwt_aud, "authenticated");
}
