use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Url {
    pub id: Uuid,
    pub short_code: String,
    pub original_url: String,
    pub user_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub click_count: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

#[derive(Debug, Deserialize)]
pub struct CreateUrlRequest {
    pub original_url: String,
    pub short_code: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub team_id: Option<Uuid>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UrlResponse {
    pub id: Uuid,
    pub short_code: String,
    pub original_url: String,
    pub click_count: i32,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<Url> for UrlResponse {
    fn from(url: Url) -> Self {
        Self {
            id: url.id,
            short_code: url.short_code,
            original_url: url.original_url,
            click_count: url.click_count,
            created_at: url.created_at,
            expires_at: url.expires_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub user_id: Uuid,
    pub exp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Click {
    pub id: Uuid,
    pub url_id: Uuid,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub country: Option<String>,
    pub city: Option<String>,
    pub device_type: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub clicked_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UrlAnalytics {
    pub url_id: Uuid,
    pub total_clicks: i64,
    pub clicks: Vec<Click>,
}

#[derive(Debug, Serialize)]
pub struct AnalyticsSummary {
    pub total_clicks: i64,
    pub unique_visitors: i64,
    pub clicks_today: i64,
    pub clicks_this_week: i64,
    pub clicks_this_month: i64,
    pub top_countries: Vec<CountryStats>,
    pub top_devices: Vec<DeviceStats>,
    pub clicks_by_day: Vec<DailyClicks>,
}

#[derive(Debug, Serialize)]
pub struct CountryStats {
    pub country: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct DeviceStats {
    pub device_type: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct DailyClicks {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub user_id: Uuid,
    pub team_id: Uuid,
    pub role: String,
    pub email: Option<String>,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub email: String,
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TeamResponse {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub member_count: i64,
    pub url_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub key_preview: String,
    pub team_id: Option<Uuid>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub team_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyClaims {
    pub key_id: Uuid,
    pub user_id: Uuid,
    pub team_id: Option<Uuid>,
    pub exp: i64,
}
