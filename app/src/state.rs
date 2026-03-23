use crate::services::redis::RedisService;

use sqlx::PgPool;
use std::sync::Arc;


#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt_secret: Arc<String>,
    pub redis_service: RedisService,
}

impl AppState {
    pub fn new(db: PgPool, jwt_secret: String, redis_service: RedisService) -> Self {
        Self {
            db,
            jwt_secret: Arc::new(jwt_secret),
            redis_service,
        }
    }
}
