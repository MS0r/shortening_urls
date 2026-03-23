use crate::{
    models::Claims,
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::Response,
};
use redis::AsyncCommands;
use sqlx::Row;
use uuid::Uuid;

pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {

    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if let Some(claims) = try_jwt_auth(&state, auth_header).await {
        request.extensions_mut().insert(claims);
        return Ok(next.run(request).await);
    }

    let api_key = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if let Some(claims) = try_api_key_auth(&state, api_key).await {
        request.extensions_mut().insert(claims);
        return Ok(next.run(request).await);
    }

    Err(StatusCode::UNAUTHORIZED)
}

async fn try_jwt_auth(state: &AppState, auth_header: &str) -> Option<Claims> {
    let token = auth_header.strip_prefix("Bearer ")?;

    let token_data = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .ok()?;

    Some(token_data.claims)
}

async fn try_api_key_auth(state: &AppState, api_key: &str) -> Option<Claims> {
    let key_row = sqlx::query(
        "SELECT id, user_id FROM api_keys WHERE key_hash = $1",
    )
    .bind(api_key)
    .fetch_optional(&state.db)
    .await
    .ok()?;

    let row = key_row?;
    let user_id: Uuid = row.get("user_id");

    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(365))
        .unwrap()
        .timestamp();

    Some(Claims {
        sub: user_id.to_string(),
        user_id,
        exp: expiration,
    })
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let identifier = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .or_else(|| {
            request.headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "anonymous".to_string());

    let key = format!("ratelimit:{}", identifier);

    let service = state.redis_service.clone();
    let mut conn = service.get_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let current: i64 = conn.incr(&key, 1).await.unwrap_or(1);
    
    if current == 1 {
        let _: () = conn.expire(&key, 60).await.unwrap_or(());
    }

    if current > 100 {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}
