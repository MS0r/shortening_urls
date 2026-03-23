use crate::{
    error::{AppError, AppResult},
    models::{
        AnalyticsSummary, Claims, CountryStats, DailyClicks, DeviceStats,
    },
    state::AppState,
};
use axum::{
    extract::{Extension, Path, Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}/analytics", get(get_url_analytics))
        .route("/{id}/analytics/clicks", get(get_url_clicks))
        .route("/aggregate", get(get_aggregate_analytics))
}

#[derive(Deserialize)]
pub struct AnalyticsParams {
    from: Option<chrono::DateTime<chrono::Utc>>,
    to: Option<chrono::DateTime<chrono::Utc>>,
}

async fn get_url_analytics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<impl IntoResponse> {
    let _ = sqlx::query(
        "SELECT id FROM urls WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(claims.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("URL not found".into()))?;

    let now = chrono::Utc::now();
    let from = params.from.unwrap_or(now - chrono::Duration::days(30));
    let to = params.to.unwrap_or(now);

    let total_clicks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clicks WHERE url_id = $1 AND clicked_at BETWEEN $2 AND $3",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await?;

    let clicks_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clicks WHERE url_id = $1 AND clicked_at >= $2",
    )
    .bind(id)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    let clicks_this_week: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clicks WHERE url_id = $1 AND clicked_at >= $2",
    )
    .bind(id)
    .bind(now - chrono::Duration::days(7))
    .fetch_one(&state.db)
    .await?;

    let clicks_this_month: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clicks WHERE url_id = $1 AND clicked_at >= $2",
    )
    .bind(id)
    .bind(now - chrono::Duration::days(30))
    .fetch_one(&state.db)
    .await?;

    let top_countries: Vec<CountryStats> = sqlx::query(
        "SELECT COALESCE(country, 'XX') as country, COUNT(*) as count 
         FROM clicks WHERE url_id = $1 AND clicked_at BETWEEN $2 AND $3
         GROUP BY country ORDER BY count DESC LIMIT 10",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?
    .iter()
    .map(|row| CountryStats {
            country: row.get("country"),
            count: row.get("count"),
        })
        .collect();

    let top_devices: Vec<DeviceStats> = sqlx::query(
        "SELECT COALESCE(device_type, 'unknown') as device_type, COUNT(*) as count 
         FROM clicks WHERE url_id = $1 AND clicked_at BETWEEN $2 AND $3
         GROUP BY device_type ORDER BY count DESC",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?
    .iter()
    .map(|row| DeviceStats {
            device_type: row.get("device_type"),
            count: row.get("count"),
        })
        .collect();

    let clicks_by_day: Vec<DailyClicks> = sqlx::query(
        "SELECT DATE(clicked_at) as date, COUNT(*) as count 
         FROM clicks WHERE url_id = $1 AND clicked_at BETWEEN $2 AND $3
         GROUP BY DATE(clicked_at) ORDER BY date",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?
    .iter()
    .map(|row| DailyClicks {
            date: row.get::<chrono::NaiveDate, _>("date").to_string(),
            count: row.get("count"),
        })
        .collect();

    Ok(Json(AnalyticsSummary {
        total_clicks,
        unique_visitors: total_clicks,
        clicks_today,
        clicks_this_week,
        clicks_this_month,
        top_countries,
        top_devices,
        clicks_by_day,
    }))
}

async fn get_url_clicks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<impl IntoResponse> {
    let _ = sqlx::query(
        "SELECT id FROM urls WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(claims.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("URL not found".into()))?;

    let now = chrono::Utc::now();
    let from = params.from.unwrap_or(now - chrono::Duration::days(30));
    let to = params.to.unwrap_or(now);

    let rows = sqlx::query(
        "SELECT id, url_id, referer, user_agent, country, city, device_type, browser, os, clicked_at 
         FROM clicks WHERE url_id = $1 AND clicked_at BETWEEN $2 AND $3
         ORDER BY clicked_at DESC LIMIT 1000",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;

    let clicks: Vec<crate::models::Click> = rows
        .iter()
        .map(|row| crate::models::Click {
            id: row.get("id"),
            url_id: row.get("url_id"),
            referer: row.get("referer"),
            user_agent: row.get("user_agent"),
            country: row.get("country"),
            city: row.get("city"),
            device_type: row.get("device_type"),
            browser: row.get("browser"),
            os: row.get("os"),
            clicked_at: row.get("clicked_at"),
        })
        .collect();

    Ok(Json(clicks))
}

async fn get_aggregate_analytics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<AnalyticsParams>,
) -> AppResult<impl IntoResponse> {
    let now = chrono::Utc::now();
    let from = params.from.unwrap_or(now - chrono::Duration::days(30));
    let to = params.to.unwrap_or(now);

    let total_clicks: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM clicks c
        JOIN urls u ON u.id = c.url_id
        WHERE u.user_id = $1 AND c.clicked_at BETWEEN $2 AND $3
        "#,
    )
    .bind(claims.user_id)
    .bind(from)
    .bind(to)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "total_clicks": total_clicks,
        "date_range": {
            "from": from.to_rfc3339(),
            "to": to.to_rfc3339(),
        }
    })))
}
