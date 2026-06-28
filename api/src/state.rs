use sqlx::PgPool;
use std::sync::Arc;
use crate::auth::AuthVerifier;

pub struct AppState {
    pub db: PgPool,
    pub verifier: Arc<dyn AuthVerifier>,
}
