use crate::{
    error::{AppError, AppResult},
    middleware::auth_middleware,
    models::{AddMemberRequest, Claims, CreateTeamRequest, Team, TeamMember, TeamResponse},
    state::AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use sqlx::Row;
use uuid::Uuid;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list_teams))
        .route("/", post(create_team))
        .route("/{id}", get(get_team))
        .route("/{id}", delete(delete_team))
        .route("/{id}/members", get(list_members))
        .route("/{id}/members", post(add_member))
        .route("/{id}/members/{user_id}", delete(remove_member))
        .layer(axum::middleware::from_fn_with_state(state, auth_middleware))
}

async fn list_teams(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> AppResult<impl IntoResponse> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.name, t.owner_id, t.created_at,
               COUNT(DISTINCT tm.user_id) as member_count,
               COUNT(DISTINCT u.id) as url_count
        FROM teams t
        LEFT JOIN team_members tm ON tm.team_id = t.id
        LEFT JOIN urls u ON u.team_id = t.id
        WHERE t.owner_id = $1 OR tm.user_id = $1
        GROUP BY t.id
        ORDER BY t.created_at DESC
        "#,
    )
    .bind(claims.user_id)
    .fetch_all(&state.db)
    .await?;

    let teams: Vec<TeamResponse> = rows
        .iter()
        .map(|row| TeamResponse {
            id: row.get("id"),
            name: row.get("name"),
            owner_id: row.get("owner_id"),
            member_count: row.get("member_count"),
            url_count: row.get("url_count"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(Json(teams))
}

async fn create_team(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<CreateTeamRequest>,
) -> AppResult<impl IntoResponse> {
    if payload.name.trim().is_empty() {
        return Err(AppError::Validation("Team name cannot be empty".into()));
    }

    let row = sqlx::query(
        "INSERT INTO teams (name, owner_id) VALUES ($1, $2) RETURNING id, name, owner_id, created_at",
    )
    .bind(&payload.name)
    .bind(claims.user_id)
    .fetch_one(&state.db)
    .await?;

    let team = Team {
        id: row.get("id"),
        name: row.get("name"),
        owner_id: row.get("owner_id"),
        created_at: row.get("created_at"),
    };

    sqlx::query(
        "INSERT INTO team_members (user_id, team_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(claims.user_id)
    .bind(team.id)
    .execute(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(TeamResponse {
        id: team.id,
        name: team.name,
        owner_id: team.owner_id,
        member_count: 1,
        url_count: 0,
        created_at: team.created_at,
    })))
}

async fn get_team(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<impl IntoResponse> {
    let row = sqlx::query(
        r#"
        SELECT t.id, t.name, t.owner_id, t.created_at,
               COUNT(DISTINCT tm.user_id) as member_count,
               COUNT(DISTINCT u.id) as url_count
        FROM teams t
        LEFT JOIN team_members tm ON tm.team_id = t.id
        LEFT JOIN urls u ON u.team_id = t.id
        WHERE t.id = $1
          AND (t.owner_id = $2 OR EXISTS (
              SELECT 1 FROM team_members WHERE team_id = t.id AND user_id = $2
          ))
        GROUP BY t.id
        "#,
    )
    .bind(id)
    .bind(claims.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Team not found".into()))?;

    Ok(Json(TeamResponse {
        id: row.get("id"),
        name: row.get("name"),
        owner_id: row.get("owner_id"),
        member_count: row.get("member_count"),
        url_count: row.get("url_count"),
        created_at: row.get("created_at"),
    }))
}

async fn delete_team(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<impl IntoResponse> {
    let result = sqlx::query("DELETE FROM teams WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(claims.user_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Team not found or you don't have permission".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn list_members(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<impl IntoResponse> {
    let _ = sqlx::query(
        "SELECT 1 FROM teams WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(claims.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Team not found".into()))?;

    let rows = sqlx::query(
        r#"
        SELECT tm.user_id, tm.team_id, tm.role, tm.joined_at, u.email
        FROM team_members tm
        JOIN users u ON u.id = tm.user_id
        WHERE tm.team_id = $1
        ORDER BY tm.joined_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let members: Vec<TeamMember> = rows
        .iter()
        .map(|row| TeamMember {
            user_id: row.get("user_id"),
            team_id: row.get("team_id"),
            role: row.get("role"),
            email: row.get("email"),
            joined_at: row.get("joined_at"),
        })
        .collect();

    Ok(Json(members))
}

async fn add_member(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<uuid::Uuid>,
    Json(payload): Json<AddMemberRequest>,
) -> AppResult<impl IntoResponse> {
    let _ = sqlx::query(
        "SELECT 1 FROM teams WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(claims.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Team not found or you don't have permission".into()))?;

    let user_row = sqlx::query("SELECT id FROM users WHERE email = $1")
        .bind(&payload.email)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    let user_id: Uuid = user_row.get("id");
    let role = payload.role.unwrap_or_else(|| "member".to_string());

    let result = sqlx::query(
        "INSERT INTO team_members (user_id, team_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(id)
    .bind(&role)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Validation("User is already a member".into()));
    }

    Ok(StatusCode::CREATED)
}

async fn remove_member(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((team_id, user_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> AppResult<impl IntoResponse> {
    if user_id == claims.user_id {
        return Err(AppError::Validation("Cannot remove yourself from team".into()));
    }

    let team_row = sqlx::query("SELECT owner_id FROM teams WHERE id = $1")
        .bind(team_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Team not found".into()))?;

    let owner_id: Uuid = team_row.get("owner_id");

    if owner_id != claims.user_id {
        return Err(AppError::NotFound("Only team owner can remove members".into()));
    }

    let result = sqlx::query(
        "DELETE FROM team_members WHERE team_id = $1 AND user_id = $2",
    )
    .bind(team_id)
    .bind(user_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Member not found".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}
