use sqlx::PgPool;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::auth::AuthVerifier;
use crate::config::OnchainConfig;
use crate::difficulty::{Clock, MintingStats, RetargetConfig};
use crate::rate_limit::RateLimiter;

pub struct AppState {
    pub db: PgPool,
    pub verifier: Arc<dyn AuthVerifier>,
    pub difficulty: Arc<AtomicU32>,
    pub mint_stats: Arc<Mutex<MintingStats>>,
    pub clock: Arc<dyn Clock>,
    pub retarget_config: RetargetConfig,
    pub rate_limiter: Arc<RateLimiter>,
    pub onchain_config: Option<OnchainConfig>,
}
