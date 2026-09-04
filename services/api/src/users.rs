//! The `users` table and the profile endpoints on top of it.

use crate::{
    ApiResult, AppError, Shared,
    auth::{AdminUser, AuthUser},
};
use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub full_name: Option<String>,
    pub phone: Option<String>,
    pub relationship: Option<String>,
    pub boat_info: Option<String>,
    pub notes: Option<String>,
    pub role: String,
    /// Receives the one-click approve/deny email. Any number of users may be
    /// flagged; all of them get it.
    pub is_owner: bool,
    pub avatar_url: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// Best-effort display name for emails and admin tables.
    pub fn display_name(&self) -> String {
        self.full_name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&self.email)
            .to_string()
    }
}

/// Columns shared by every `users` read, so row shapes never drift.
pub const USER_COLUMNS: &str = "id, email, full_name, phone, relationship, boat_info, notes, \
                                role, is_owner, avatar_url, last_login_at, created_at, \
                                updated_at";

pub async fn find_by_id(db: &sqlx::PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await
}

pub async fn find_by_email(db: &sqlx::PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE lower(email) = lower($1)"
    ))
    .bind(email)
    .fetch_optional(db)
    .await
}

/// The single admin used as the fallback recipient for guest messages and
/// booking notifications.
pub async fn first_admin(db: &sqlx::PgPool) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE role = 'admin' ORDER BY created_at LIMIT 1"
    ))
    .fetch_optional(db)
    .await
}

/// Every address flagged as a camp owner, oldest account first.
///
/// This is the source of truth for who receives the approve/deny email;
/// `OWNER_EMAIL` is only consulted when this comes back empty. See
/// [`crate::email::owner_recipients`].
pub async fn owner_emails(db: &sqlx::PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT email FROM users WHERE is_owner = true ORDER BY created_at")
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().map(|(email,)| email).collect())
}

// ─────────────────────────── handlers ───────────────────────────

pub async fn get_me(AuthUser(user): AuthUser) -> Json<User> {
    Json(user)
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfile {
    pub full_name: Option<String>,
    pub phone: Option<String>,
    pub relationship: Option<String>,
    pub boat_info: Option<String>,
    pub notes: Option<String>,
}

/// Trims to `None` so blank form fields clear the column rather than storing "".
fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub async fn update_me(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<UpdateProfile>,
) -> ApiResult<Json<User>> {
    let full_name = clean(body.full_name)
        .ok_or_else(|| AppError::BadRequest("Full name is required.".into()))?;

    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET full_name = $2, phone = $3, relationship = $4, boat_info = $5, notes = $6
         WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(user.id)
    .bind(full_name)
    .bind(clean(body.phone))
    .bind(clean(body.relationship))
    .bind(clean(body.boat_info))
    .bind(clean(body.notes))
    .fetch_one(&state.db)
    .await?;

    Ok(Json(updated))
}

/// Admin roster: every user with their booking count, for the Users tab.
#[derive(Debug, Serialize, FromRow)]
pub struct UserWithStats {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub user: User,
    pub booking_count: i64,
}

pub async fn list_all(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<UserWithStats>>> {
    let rows = sqlx::query_as::<_, UserWithStats>(
        "SELECT u.id, u.email, u.full_name, u.phone, u.relationship, u.boat_info, u.notes,
                u.role, u.is_owner, u.avatar_url, u.last_login_at, u.created_at, u.updated_at,
                count(b.id) AS booking_count
         FROM users u
         LEFT JOIN bookings b ON b.user_id = u.id
         GROUP BY u.id
         ORDER BY u.created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRole {
    pub role: String,
}

pub async fn update_role(
    State(state): State<Shared>,
    AdminUser(actor): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRole>,
) -> ApiResult<Json<User>> {
    if body.role != "guest" && body.role != "admin" {
        return Err(AppError::BadRequest(
            "Role must be 'guest' or 'admin'.".into(),
        ));
    }
    // Guard against an admin locking themselves out of the admin panel.
    if actor.id == id && body.role != "admin" {
        return Err(AppError::BadRequest(
            "You cannot remove your own admin role.".into(),
        ));
    }

    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET role = $2 WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .bind(&body.role)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found.".into()))?;

    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct UpdateOwner {
    pub is_owner: bool,
}

/// Flags or unflags a user as a camp owner.
///
/// Deliberately unrestricted in both directions: the camp can have several
/// owners, and unflagging the last one is allowed — the send path falls back
/// to `OWNER_EMAIL` and logs a warning rather than silently notifying nobody.
pub async fn update_owner(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateOwner>,
) -> ApiResult<Json<User>> {
    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET is_owner = $2 WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .bind(body.is_owner)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found.".into()))?;

    Ok(Json(updated))
}
