use redis::{aio::MultiplexedConnection, AsyncCommands, Client};

#[derive(Clone)]
pub struct RedisService {
    client: Client,
}

impl RedisService {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = Client::open(redis_url)?;
        Ok(Self { client })
    }

    pub async fn get_connection(&self) -> Result<MultiplexedConnection, redis::RedisError> {
        self.client.get_multiplexed_async_connection().await
    }
}

pub async fn cache_short_url(
    redis_service: &RedisService,
    short_code: &str,
    original_url: &str,
) -> Result<(), redis::RedisError> {
    let mut conn: MultiplexedConnection = redis_service.get_connection().await?;
    
    let key = format!("url:{}", short_code);
    let _: () = conn.set_ex(&key, original_url, 3600).await?;
    
    Ok(())
}

pub async fn get_cached_url(
    redis_service: &RedisService,
    short_code: &str,
) -> Result<Option<String>, redis::RedisError> {
    let mut conn: MultiplexedConnection = redis_service.get_connection().await?;
    
    let key = format!("url:{}", short_code);
    let url: Option<String> = conn.get(&key).await?;
    
    Ok(url)
}

pub async fn invalidate_cache(
    redis_service: &RedisService,
    short_code: &str,
) -> Result<(), redis::RedisError> {
    let mut conn: MultiplexedConnection = redis_service.get_connection().await?;
    
    let key = format!("url:{}", short_code);
    let _: () = conn.del(&key).await?;
    
    Ok(())
}
