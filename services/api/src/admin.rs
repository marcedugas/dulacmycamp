//! Blackout dates — ranges the camp is closed. Public to read (the calendar
//! needs them), admin to write.

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
pub struct BlackoutDate {
    pub id: Uuid,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub reason: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub async fn list_blackouts(State(state): State<Shared>) -> ApiResult<Json<Vec<BlackoutDate>>> {
    let rows = sqlx::query_as::<_, BlackoutDate>(
        "SELECT id, start_date, end_date, reason, created_by, created_at
         FROM blackout_dates ORDER BY start_date",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct CreateBlackout {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub reason: Option<String>,
}

pub async fn create_blackout(
    State(state): State<Shared>,
    AdminUser(admin): AdminUser,
    Json(body): Json<CreateBlackout>,
) -> ApiResult<Json<BlackoutDate>> {
    if body.end_date < body.start_date {
        return Err(AppError::BadRequest(
            "End date must be on or after the start date.".into(),
        ));
    }

    let row = sqlx::query_as::<_, BlackoutDate>(
        "INSERT INTO blackout_dates (start_date, end_date, reason, created_by)
         VALUES ($1, $2, $3, $4)
         RETURNING id, start_date, end_date, reason, created_by, created_at",
    )
    .bind(body.start_date)
    .bind(body.end_date)
    .bind(
        body.reason
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .bind(admin.id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(row))
}

pub async fn delete_blackout(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let done = sqlx::query("DELETE FROM blackout_dates WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Blackout not found.".into()));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}
