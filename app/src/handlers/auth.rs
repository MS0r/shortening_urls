use crate::{
    error::{AppError, AppResult},
    middleware::auth_middleware,
    models::{AuthResponse, Claims, CreateUserRequest, LoginRequest, User},
    state::AppState,
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    Json, Router, extract::{Extension, State}, http::StatusCode, response::IntoResponse, routing::{get, post}
};
use jsonwebtoken::{encode, EncodingKey, Header};
use sqlx::Row;
use uuid::Uuid;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/me", get(me).route_layer(axum::middleware::from_fn_with_state(state.clone(), auth_middleware)))
}

async fn register(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> AppResult<impl IntoResponse> {
    let email = payload.email.trim().to_lowercase();
    let password = payload.password;

    if email.is_empty() || password.len() < 6 {
        return Err(AppError::Validation(
            "Email and password (min 6 chars) are required".into(),
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Validation(e.to_string()))?
        .to_string();

    let result = sqlx::query(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id, email, created_at",
    )
    .bind(&email)
    .bind(&password_hash)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(row) => {
            let user = User {
                id: row.get("id"),
                email: row.get("email"),
                created_at: row.get("created_at"),
            };

            let token = generate_token(&state, &user.id)?;
            Ok((StatusCode::CREATED, Json(AuthResponse { token, user })))
        }
        Err(sqlx::Error::Database(e)) => {
            if e.constraint().map(|c| c == "users_email_key").unwrap_or(false) {
                Err(AppError::Validation("Email already exists".into()))
            } else {
                Err(AppError::Database(sqlx::Error::Database(e)))
            }
        }
        Err(e) => Err(AppError::Database(e)),
    }
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> AppResult<impl IntoResponse> {
    let email = payload.email.trim().to_lowercase();

    let row = sqlx::query("SELECT id, email, password_hash, created_at FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::Auth("Invalid credentials".into()))?;

    let password_hash: String = row.get("password_hash");
    let parsed_hash = PasswordHash::new(&password_hash)
        .map_err(|_| AppError::Auth("Invalid password hash".into()))?;

    Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::Auth("Invalid credentials".into()))?;

    let user = User {
        id: row.get("id"),
        email: row.get("email"),
        created_at: row.get("created_at"),
    };

    let token = generate_token(&state, &user.id)?;
    Ok(Json(AuthResponse { token, user }))
}

async fn me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> AppResult<impl IntoResponse> {
    let row = sqlx::query("SELECT id, email, created_at FROM users WHERE id = $1")
        .bind(claims.user_id)
        .fetch_one(&state.db)
        .await?;

    let user = User {
        id: row.get("id"),
        email: row.get("email"),
        created_at: row.get("created_at"),
    };

    Ok(Json(user))
}

fn generate_token(state: &AppState, user_id: &Uuid) -> AppResult<String> {
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(7))
        .unwrap()
        .timestamp();

    let claims = Claims {
        sub: user_id.to_string(),
        user_id: *user_id,
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Auth(e.to_string()))
}
