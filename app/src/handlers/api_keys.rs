use crate::{
    error::{AppError, AppResult},
    middleware::auth_middleware,
    models::{ApiKeyResponse, Claims, CreateApiKeyRequest},
    state::AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use sqlx::Row;
use uuid::Uuid;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list_api_keys))
        .route("/", post(create_api_key))
        .route("/{id}", delete(delete_api_key))
        .layer(axum::middleware::from_fn_with_state(state, auth_middleware))
}

async fn list_api_keys(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> AppResult<impl IntoResponse> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, key_hash, team_id, last_used_at, created_at
        FROM api_keys
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(claims.user_id)
    .fetch_all(&state.db)
    .await?;

    let keys: Vec<ApiKeyResponse> = rows
        .iter()
        .map(|row| {
            let key_hash: String = row.get("key_hash");
            ApiKeyResponse {
                id: row.get("id"),
                name: row.get("name"),
                key_preview: format!("sk_...{}", &key_hash[key_hash.len().saturating_sub(8)..]),
                team_id: row.get("team_id"),
                last_used_at: row.get("last_used_at"),
                created_at: row.get("created_at"),
            }
        })
        .collect();

    Ok(Json(keys))
}

async fn create_api_key(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> AppResult<impl IntoResponse> {
    if payload.name.trim().is_empty() {
        return Err(AppError::Validation("API key name cannot be empty".into()));
    }

    if let Some(team_id) = payload.team_id {
        let team = sqlx::query(
            "SELECT owner_id FROM teams WHERE id = $1",
        )
        .bind(team_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Team not found".into()))?;

        let owner_id: Uuid = team.get("owner_id");
        if owner_id != claims.user_id {
            return Err(AppError::Auth("Only team owner can create API keys for team".into()));
        }
    }

    let api_key = generate_api_key();
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let key_hash = argon2
        .hash_password(api_key.as_bytes(), &salt)
        .map_err(|e| AppError::Validation(e.to_string()))?
        .to_string();

    let row = sqlx::query(
        r#"
        INSERT INTO api_keys (key_hash, user_id, team_id, name)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, team_id, last_used_at, created_at
        "#,
    )
    .bind(&key_hash)
    .bind(claims.user_id)
    .bind(payload.team_id)
    .bind(&payload.name)
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "id": row.get::<Uuid, _>("id"),
        "name": row.get::<String, _>("name"),
        "key": api_key,
        "team_id": row.get::<Option<Uuid>, _>("team_id"),
        "last_used_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at"),
        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    }))))
}

async fn delete_api_key(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<impl IntoResponse> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(claims.user_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("API key not found".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}

fn generate_api_key() -> String {
    let bytes: [u8; 32] = rand::random();
    format!(
        "sk_{}{}",
        chrono::Utc::now().timestamp(),
        hex::encode(bytes)
    )
}
