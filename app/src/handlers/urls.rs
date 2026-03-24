use crate::{
    error::{AppError, AppResult},
    middleware::auth_middleware,
    models::{Claims, CreateUrlRequest, UrlResponse},
    services::redis,
    state::AppState,
    handlers::analytics::router as analytics_router,
};
use axum::{
    extract::{Extension, Path, State},
    http::{header::HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use nanoid::nanoid;
use sqlx::Row;
use tokio::task;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list_urls))
        .route("/", post(create_url))
        .route("/{id}", get(get_url))
        .route("/{id}", delete(delete_url))
        .merge(analytics_router())
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth_middleware))
}

pub fn redirect_router() -> Router<AppState> {
    Router::new()
        .route("/s/{short_code}", get(redirect_url))
}

async fn list_urls(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> AppResult<impl IntoResponse> {
    let rows = sqlx::query(
        "SELECT id, short_code, original_url, click_count, created_at, expires_at 
         FROM urls WHERE user_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(claims.user_id)
    .fetch_all(&state.db)
    .await?;

    let urls: Vec<UrlResponse> = rows
        .into_iter()
        .map(|row| {
            UrlResponse {
                id: row.get("id"),
                short_code: row.get("short_code"),
                original_url: row.get("original_url"),
                click_count: row.get("click_count"),
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            }
        })
        .collect();

    Ok(Json(urls))
}

async fn create_url(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CreateUrlRequest>,
) -> AppResult<impl IntoResponse> {
    let original_url = payload.original_url;
    if original_url.is_empty() {
        return Err(AppError::Validation("URL cannot be empty".into()));
    }

    let short_code = if let Some(code) = payload.short_code {
        if !code.chars().all(|c| c.is_alphanumeric()) {
            return Err(AppError::Validation("Invalid short code format".into()));
        }
        code
    } else {
        nanoid!(8)
    };

    let exists: bool = sqlx::query("SELECT EXISTS(SELECT 1 FROM urls WHERE short_code = $1)")
        .bind(&short_code)
        .fetch_one(&state.db)
        .await?
        .get("exists");

    if exists {
        return Err(AppError::Validation("Short code already exists".into()));
    }

    let is_active = payload.is_active.unwrap_or(true);

    let row = sqlx::query(
        "INSERT INTO urls (short_code, original_url, user_id, expires_at, team_id, is_active) 
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id, short_code, original_url, click_count, created_at, expires_at",
    )
    .bind(&short_code)
    .bind(&original_url)
    .bind(claims.user_id)
    .bind(payload.expires_at)
    .bind(payload.team_id)
    .bind(is_active)
    .fetch_one(&state.db)
    .await?;

    let url = UrlResponse {
        id: row.get("id"),
        short_code: row.get("short_code"),
        original_url: row.get("original_url"),
        click_count: row.get("click_count"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
    };

    if let Err(e) = redis::cache_short_url(&state.redis_service, &short_code, &original_url).await {
        tracing::warn!("Failed to cache URL in Redis: {}", e);
    }

    Ok((StatusCode::CREATED, Json(url)))
}

async fn get_url(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<impl IntoResponse> {
    let row = sqlx::query(
        "SELECT id, short_code, original_url, click_count, created_at, expires_at 
         FROM urls WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(claims.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("URL not found".into()))?;

    Ok(Json(UrlResponse {
        id: row.get("id"),
        short_code: row.get("short_code"),
        original_url: row.get("original_url"),
        click_count: row.get("click_count"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
    }))
}

async fn delete_url(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<impl IntoResponse> {
    let row = sqlx::query("SELECT short_code FROM urls WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(claims.user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("URL not found".into()))?;

    let short_code: String = row.get("short_code");

    let result = sqlx::query("DELETE FROM urls WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(claims.user_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("URL not found".into()));
    }

    if let Err(e) = redis::invalidate_cache(&state.redis_service, &short_code).await {
        tracing::warn!("Failed to invalidate cache in Redis: {}", e);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn redirect_url(
    State(state): State<AppState>,
    Path(short_code): Path<String>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    let original_url = if let Ok(Some(cached_url)) = redis::get_cached_url(&state.redis_service, &short_code).await {
        tracing::debug!("Cache hit for short code: {}", short_code);
        cached_url
    } else {
        tracing::debug!("Cache miss for short code: {}", short_code);
        
        let row = sqlx::query(
            "SELECT original_url, is_active, expires_at FROM urls WHERE short_code = $1",
        )
        .bind(&short_code)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("URL not found".into()))?;

        let url: String = row.get("original_url");
        let is_active: bool = row.get("is_active");
        let expires_at: Option<chrono::DateTime<chrono::Utc>> = row.get("expires_at");

        if !is_active {
            return Err(AppError::NotFound("URL is inactive".into()));
        }

        if let Some(exp) = expires_at {
            if exp < chrono::Utc::now() {
                return Err(AppError::NotFound("URL has expired".into()));
            }
        }

        if let Err(e) = redis::cache_short_url(&state.redis_service, &short_code, &url).await {
            tracing::warn!("Failed to cache URL in Redis: {}", e);
        }

        url
    };

    let referer = headers
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let ip_address = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
        .or_else(|| headers.get("remote_addr").and_then(|v| v.to_str().ok()))
        .map(|s| s.to_string());

    let url_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM urls WHERE short_code = $1"
    )
    .bind(&short_code)
    .fetch_optional(&state.db)
    .await?;

    if let Some(id) = url_id {
        task::spawn(async move {
            let _ = record_click(state.db.clone(), &id, &referer, &user_agent, &ip_address).await;
        });
    }

    Ok((StatusCode::FOUND, axum::response::Redirect::temporary(&original_url)))
}

async fn record_click(
    pool: sqlx::Pool<sqlx::Postgres>,
    url_id: &uuid::Uuid,
    referer: &Option<String>,
    user_agent: &Option<String>,
    ip_address: &Option<String>,
) -> Result<(), sqlx::Error> {
    let (device_type, browser, os) = parse_user_agent(user_agent);
    let (country, city, ip_hash) = parse_ip_info(ip_address);

    sqlx::query(
        "INSERT INTO clicks (url_id, referer, user_agent, country, city, device_type, browser, os, ip_hash) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(url_id)
    .bind(referer)
    .bind(user_agent)
    .bind(country)
    .bind(city)
    .bind(device_type)
    .bind(browser)
    .bind(os)
    .bind(ip_hash)
    .execute(&pool)
    .await?;

    sqlx::query("UPDATE urls SET click_count = click_count + 1 WHERE id = $1")
        .bind(url_id)
        .execute(&pool)
        .await?;

    Ok(())
}

fn parse_user_agent(user_agent: &Option<String>) -> (Option<String>, Option<String>, Option<String>) {
    let ua = match user_agent {
        Some(s) => s.as_str(),
        None => return (None, None, None),
    };

    let ua_lower = ua.to_lowercase();
    
    let device_type = if ua_lower.contains("mobile") || ua_lower.contains("android") {
        Some("mobile".to_string())
    } else if ua_lower.contains("tablet") || ua_lower.contains("ipad") {
        Some("tablet".to_string())
    } else {
        Some("desktop".to_string())
    };

    let browser = if ua_lower.contains("chrome") && !ua_lower.contains("edg") {
        Some("Chrome".to_string())
    } else if ua_lower.contains("safari") && !ua_lower.contains("chrome") {
        Some("Safari".to_string())
    } else if ua_lower.contains("firefox") {
        Some("Firefox".to_string())
    } else if ua_lower.contains("edg") {
        Some("Edge".to_string())
    } else if ua_lower.contains("opera") || ua_lower.contains("opr") {
        Some("Opera".to_string())
    } else {
        Some("Other".to_string())
    };

    let os = if ua_lower.contains("windows") {
        Some("Windows".to_string())
    } else if ua_lower.contains("mac os") || ua_lower.contains("darwin") {
        Some("macOS".to_string())
    } else if ua_lower.contains("linux") {
        Some("Linux".to_string())
    } else if ua_lower.contains("android") {
        Some("Android".to_string())
    } else if ua_lower.contains("ios") || ua_lower.contains("iphone") || ua_lower.contains("ipad") {
        Some("iOS".to_string())
    } else {
        Some("Other".to_string())
    };

    (device_type, browser, os)
}

fn parse_ip_info(ip_address: &Option<String>) -> (Option<String>, Option<String>, Option<String>) {
    let ip = match ip_address {
        Some(s) => s.as_str(),
        None => return (None, None, None),
    };

    let ip_hash = Some(format!("{:x}", md5::compute(ip.as_bytes())));

    let (country, city) = match ip {
        _ if ip.starts_with("192.168") || ip.starts_with("10.") || ip.starts_with("172.") => {
            (Some("XX".to_string()), None)
        }
        _ => (Some("US".to_string()), Some("Unknown".to_string())),
    };

    (country, city, ip_hash)
}
