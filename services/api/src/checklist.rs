//! The admin-managed checkout checklist. Content only — the guest-facing
//! submission flow lives in [`crate::checkout`].

use crate::{ApiResult, AppError, Shared, auth::AdminUser};
use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, FromRow)]
pub struct ChecklistItem {
    pub id: Uuid,
    pub label: String,
    pub sort_order: i32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const ITEM_COLUMNS: &str = "id, label, sort_order, active, created_at, updated_at";

/// `GET /api/checklist` — active items only, for rendering the checkout form.
pub async fn list_active(State(state): State<Shared>) -> ApiResult<Json<Vec<ChecklistItem>>> {
    let rows = sqlx::query_as::<_, ChecklistItem>(&format!(
        "SELECT {ITEM_COLUMNS} FROM checklist_items
         WHERE active = true ORDER BY sort_order, created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// `GET /api/admin/checklist` — every item, including inactive ones.
pub async fn list_all(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<ChecklistItem>>> {
    let rows = sqlx::query_as::<_, ChecklistItem>(&format!(
        "SELECT {ITEM_COLUMNS} FROM checklist_items ORDER BY sort_order, created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct CreateItem {
    pub label: String,
    pub sort_order: i32,
}

pub async fn create(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Json(body): Json<CreateItem>,
) -> ApiResult<Json<ChecklistItem>> {
    let label = body.label.trim();
    if label.is_empty() {
        return Err(AppError::BadRequest("Item label is required.".into()));
    }
    let row = sqlx::query_as::<_, ChecklistItem>(&format!(
        "INSERT INTO checklist_items (label, sort_order) VALUES ($1, $2) RETURNING {ITEM_COLUMNS}"
    ))
    .bind(label)
    .bind(body.sort_order)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

#[derive(Debug, Deserialize)]
pub struct UpdateItem {
    pub label: String,
    pub sort_order: i32,
    pub active: bool,
}

pub async fn update(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateItem>,
) -> ApiResult<Json<ChecklistItem>> {
    let label = body.label.trim();
    if label.is_empty() {
        return Err(AppError::BadRequest("Item label is required.".into()));
    }
    let row = sqlx::query_as::<_, ChecklistItem>(&format!(
        "UPDATE checklist_items SET label = $2, sort_order = $3, active = $4, updated_at = now()
         WHERE id = $1 RETURNING {ITEM_COLUMNS}"
    ))
    .bind(id)
    .bind(label)
    .bind(body.sort_order)
    .bind(body.active)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Checklist item not found.".into()))?;
    Ok(Json(row))
}

/// Hard-deletes an item that has never been used in a completed checkout;
/// otherwise deactivates it instead and says why, so historical checkout
/// records keep resolving the label correctly (see `checkout::admin_list`).
pub async fn remove(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let (used,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(
            SELECT 1 FROM booking_checkouts
            WHERE checked_item_ids @> jsonb_build_array($1)
         )",
    )
    .bind(id.to_string())
    .fetch_one(&state.db)
    .await?;

    if used {
        let done = sqlx::query(
            "UPDATE checklist_items SET active = false, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&state.db)
        .await?;
        if done.rows_affected() == 0 {
            return Err(AppError::NotFound("Checklist item not found.".into()));
        }
        return Ok(Json(serde_json::json!({
            "deleted": false,
            "deactivated": true,
            "message": "This item has been used in a completed checkout, so it was deactivated \
                        instead of deleted — that keeps past checkout records intact.",
        })));
    }

    let done = sqlx::query("DELETE FROM checklist_items WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Checklist item not found.".into()));
    }
    Ok(Json(
        serde_json::json!({ "deleted": true, "deactivated": false }),
    ))
}
