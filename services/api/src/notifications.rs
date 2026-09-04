//! In-app inbox.
//!
//! Guests can only write to the admin; the admin can write to anyone. System
//! messages (booking status changes) arrive with a `NULL` sender.

use crate::{
    ApiResult, AppError, Shared,
    auth::AuthUser,
    users::{self, User},
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Serialize, FromRow)]
pub struct MessageView {
    pub id: Uuid,
    pub subject: Option<String>,
    pub body: String,
    pub is_read: bool,
    pub booking_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub sender_id: Option<Uuid>,
    pub sender_name: Option<String>,
    pub sender_email: Option<String>,
    pub recipient_id: Uuid,
    pub recipient_name: Option<String>,
    pub recipient_email: String,
}

const MESSAGE_SELECT: &str = "SELECT m.id, m.subject, m.body, m.is_read, m.booking_id,
        m.created_at, m.sender_id,
        s.full_name AS sender_name, s.email AS sender_email,
        m.recipient_id, r.full_name AS recipient_name, r.email AS recipient_email
   FROM messages m
   LEFT JOIN users s ON s.id = m.sender_id
   JOIN users r ON r.id = m.recipient_id";

/// Writes a message with no sender — used for booking status notifications.
pub async fn system_message(
    db: &PgPool,
    recipient_id: Uuid,
    subject: &str,
    body: &str,
    booking_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO messages (recipient_id, subject, body, booking_id)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(recipient_id)
    .bind(subject)
    .bind(body)
    .bind(booking_id)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Admin-only: include every message in the system, not just the inbox.
    pub all: Option<bool>,
}

pub async fn list(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<MessageView>>> {
    let all = q.all.unwrap_or(false) && user.is_admin();

    // Admins reviewing "all" see both sides of every thread; everyone else
    // sees their own inbox plus the messages they sent.
    let sql = if all {
        format!("{MESSAGE_SELECT} ORDER BY m.created_at DESC")
    } else {
        format!(
            "{MESSAGE_SELECT} WHERE m.recipient_id = $1 OR m.sender_id = $1 ORDER BY m.created_at DESC"
        )
    };

    let rows = sqlx::query_as::<_, MessageView>(&sql);
    let rows = if all {
        rows.fetch_all(&state.db).await?
    } else {
        rows.bind(user.id).fetch_all(&state.db).await?
    };

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct SendMessage {
    /// Ignored for guests — their mail always goes to the admin.
    pub recipient_id: Option<Uuid>,
    pub subject: Option<String>,
    pub body: String,
}

pub async fn send(
    State(state): State<Shared>,
    AuthUser(sender): AuthUser,
    Json(body): Json<SendMessage>,
) -> ApiResult<Json<MessageView>> {
    let text = body.body.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("Message can't be empty.".into()));
    }

    let recipient_id = if sender.is_admin() {
        body.recipient_id
            .ok_or_else(|| AppError::BadRequest("Pick someone to send this to.".into()))?
    } else {
        // Guests may only reach the admin, whoever that currently is.
        users::first_admin(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("No admin is configured to receive mail.".into()))?
            .id
    };

    if users::find_by_id(&state.db, recipient_id).await?.is_none() {
        return Err(AppError::NotFound("Recipient not found.".into()));
    }

    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO messages (sender_id, recipient_id, subject, body)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(sender.id)
    .bind(recipient_id)
    .bind(
        body.subject
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .bind(text)
    .fetch_one(&state.db)
    .await?;

    let view = sqlx::query_as::<_, MessageView>(&format!("{MESSAGE_SELECT} WHERE m.id = $1"))
        .bind(id)
        .fetch_one(&state.db)
        .await?;

    Ok(Json(view))
}

/// Only the recipient can mark a message read — an admin browsing everything
/// shouldn't silently clear a guest's unread badge.
pub async fn mark_read(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MessageView>> {
    let updated =
        sqlx::query("UPDATE messages SET is_read = true WHERE id = $1 AND recipient_id = $2")
            .bind(id)
            .bind(user.id)
            .execute(&state.db)
            .await?;

    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound("Message not found.".into()));
    }

    let view = sqlx::query_as::<_, MessageView>(&format!("{MESSAGE_SELECT} WHERE m.id = $1"))
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(view))
}

pub async fn remove(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let sql = if user.is_admin() {
        "DELETE FROM messages WHERE id = $1"
    } else {
        "DELETE FROM messages WHERE id = $1 AND (recipient_id = $2 OR sender_id = $2)"
    };

    let q = sqlx::query(sql).bind(id);
    let done = if user.is_admin() {
        q.execute(&state.db).await?
    } else {
        q.bind(user.id).execute(&state.db).await?
    };

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Message not found.".into()));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Unread count, used for the bell badge. Kept next to the inbox queries so
/// the `is_read` semantics live in one place.
pub async fn unread_count(db: &PgPool, user: &User) -> Result<i64, sqlx::Error> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM messages WHERE recipient_id = $1 AND is_read = false")
            .bind(user.id)
            .fetch_one(db)
            .await?;
    Ok(n)
}
