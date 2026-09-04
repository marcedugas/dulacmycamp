//! Special events — rodeos, tournaments, holidays. Annotations on the
//! calendar; they never block a booking.

use crate::{ApiResult, AppError, Shared, auth::AdminUser};
use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, FromRow)]
pub struct SpecialEvent {
    pub id: Uuid,
    pub name: String,
    pub event_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub emoji: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

const EVENT_COLUMNS: &str =
    "id, name, event_date, end_date, description, emoji, created_by, created_at";

pub async fn list(State(state): State<Shared>) -> ApiResult<Json<Vec<SpecialEvent>>> {
    let rows = sqlx::query_as::<_, SpecialEvent>(&format!(
        "SELECT {EVENT_COLUMNS} FROM special_events ORDER BY event_date"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct EventBody {
    pub name: String,
    pub event_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub emoji: Option<String>,
}

impl EventBody {
    fn validate(&self) -> ApiResult<(String, Option<String>, String)> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest("Event name is required.".into()));
        }
        if self.end_date.is_some_and(|e| e < self.event_date) {
            return Err(AppError::BadRequest(
                "End date must be on or after the start date.".into(),
            ));
        }
        let emoji = self
            .emoji
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("🎉")
            .to_string();
        Ok((
            name.to_string(),
            self.description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            emoji,
        ))
    }
}

pub async fn create(
    State(state): State<Shared>,
    AdminUser(admin): AdminUser,
    Json(body): Json<EventBody>,
) -> ApiResult<Json<SpecialEvent>> {
    let (name, description, emoji) = body.validate()?;

    let row = sqlx::query_as::<_, SpecialEvent>(&format!(
        "INSERT INTO special_events (name, event_date, end_date, description, emoji, created_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {EVENT_COLUMNS}"
    ))
    .bind(name)
    .bind(body.event_date)
    .bind(body.end_date)
    .bind(description)
    .bind(emoji)
    .bind(admin.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(row))
}

pub async fn update(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<EventBody>,
) -> ApiResult<Json<SpecialEvent>> {
    let (name, description, emoji) = body.validate()?;

    let row = sqlx::query_as::<_, SpecialEvent>(&format!(
        "UPDATE special_events
         SET name = $2, event_date = $3, end_date = $4, description = $5, emoji = $6
         WHERE id = $1 RETURNING {EVENT_COLUMNS}"
    ))
    .bind(id)
    .bind(name)
    .bind(body.event_date)
    .bind(body.end_date)
    .bind(description)
    .bind(emoji)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Event not found.".into()))?;

    Ok(Json(row))
}

pub async fn remove(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let done = sqlx::query("DELETE FROM special_events WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Event not found.".into()));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}
