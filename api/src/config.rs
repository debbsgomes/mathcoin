use std::time::Duration;

/// Application configuration loaded from environment variables.
/// All fields are validated at startup — missing required vars cause a clear panic.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub jwks_url: Option<String>,
    pub jwt_secret: Option<String>,
    pub jwt_verification_mode: JwtMode,
    pub jwt_iss: String,
    pub jwt_aud: String,
    pub frontend_origin: String,
    pub bind_address: String,
    pub rate_limit_window: Duration,
    pub rate_limit_max: u64,
    pub retarget_window: Duration,
    pub retarget_target_rate: f64,
    pub retarget_hysteresis_low: f64,
    pub retarget_hysteresis_high: f64,
    pub retarget_diff_min: u32,
    pub retarget_diff_max: u32,
    pub retarget_max_step: u32,
    pub onchain: Option<OnchainConfig>,
}

/// On-chain configuration. Only present if CONTRACT_ADDRESS is set.
/// When absent, all on-chain routes return "on-chain disabled" responses.
#[derive(Debug, Clone)]
pub struct OnchainConfig {
    pub contract_address: String,
    pub chain_name: String,
    pub chain_id: u64,
    pub explorer_url: String,
    pub rpc_url: Option<String>,
    pub relayer_private_key: Option<String>,
}

#[derive(Debug, Clone)]
pub enum JwtMode {
    Jwks,
    SharedSecret,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let database_url = require("DATABASE_URL");
        let jwt_iss = require("JWT_ISS");
        let jwt_aud = require("JWT_AUD");

        let jwt_verification_mode =
            std::env::var("JWT_VERIFICATION_MODE").unwrap_or_else(|_| "jwks".into());

        let (jwks_url, jwt_secret, mode) = match jwt_verification_mode.as_str() {
            "shared_secret" | "hs256" => {
                let secret = require("JWT_SECRET");
                (None, Some(secret), JwtMode::SharedSecret)
            }
            _ => {
                let url = require("JWKS_URL");
                (Some(url), None, JwtMode::Jwks)
            }
        };

        let frontend_origin =
            std::env::var("FRONTEND_ORIGIN").unwrap_or_else(|_| "http://localhost:5173".into());

        let bind_address = std::env::var("BIND_ADDRESS")
            .unwrap_or_else(|_| {
                std::env::var("PORT")
                    .map(|p| format!("0.0.0.0:{p}"))
                    .unwrap_or_else(|_| "127.0.0.1:3000".into())
            });

        let onchain = std::env::var("CONTRACT_ADDRESS").ok().map(|contract_address| {
            OnchainConfig {
                contract_address,
                chain_name: std::env::var("CHAIN_NAME")
                    .unwrap_or_else(|_| "base_sepolia".into()),
                chain_id: std::env::var("CHAIN_ID")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(84532),
                explorer_url: std::env::var("EXPLORER_URL")
                    .unwrap_or_else(|_| "https://sepolia.basescan.org".into()),
                rpc_url: std::env::var("BASE_RPC_URL").ok(),
                relayer_private_key: std::env::var("RELAYER_PRIVATE_KEY").ok(),
            }
        });

        if onchain.is_some() {
            tracing::info!("on-chain layer enabled");
        } else {
            tracing::info!("CONTRACT_ADDRESS not set — on-chain layer disabled");
        }

        Self {
            database_url,
            jwks_url,
            jwt_secret,
            jwt_verification_mode: mode,
            jwt_iss,
            jwt_aud,
            frontend_origin,
            bind_address,
            rate_limit_window: Duration::from_secs(60),
            rate_limit_max: 60,
            retarget_window: Duration::from_secs(60),
            retarget_target_rate: 20.0,
            retarget_hysteresis_low: 15.0,
            retarget_hysteresis_high: 25.0,
            retarget_diff_min: 1,
            retarget_diff_max: 12,
            retarget_max_step: 1,
            onchain,
        }
    }
}

fn require(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        panic!(
            "Missing required environment variable: {key}. \
             See .env.example for the list of required variables."
        )
    })
}
