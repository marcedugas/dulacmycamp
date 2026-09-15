//! The admin-managed species list behind a journal entry's catch log.
//!
//! Content only — the catch rows that reference these live in
//! [`crate::journal`]. Deliberately the same shape as
//! [`crate::checklist`]: an ordered, renameable list whose entries are
//! deactivated rather than deleted once history points at them.

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
pub struct FishSpecies {
    pub id: Uuid,
    pub name: String,
    pub sort_order: i32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SPECIES_COLUMNS: &str = "id, name, sort_order, active, created_at, updated_at";

/// `GET /api/fish-species` — active species only, for the catch-log dropdown.
///
/// Auth'd rather than public: the only thing that renders it is the journal
/// entry form, which is behind a login anyway.
pub async fn list_active(
    State(state): State<Shared>,
    crate::auth::AuthUser(_): crate::auth::AuthUser,
) -> ApiResult<Json<Vec<FishSpecies>>> {
    let rows = sqlx::query_as::<_, FishSpecies>(&format!(
        "SELECT {SPECIES_COLUMNS} FROM fish_species
         WHERE active = true ORDER BY sort_order, created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// `GET /api/admin/fish-species` — every species, including inactive ones.
pub async fn list_all(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<FishSpecies>>> {
    let rows = sqlx::query_as::<_, FishSpecies>(&format!(
        "SELECT {SPECIES_COLUMNS} FROM fish_species ORDER BY sort_order, created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct CreateSpecies {
    pub name: String,
    pub sort_order: i32,
}

pub async fn create(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Json(body): Json<CreateSpecies>,
) -> ApiResult<Json<FishSpecies>> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Species name is required.".into()));
    }
    let row = sqlx::query_as::<_, FishSpecies>(&format!(
        "INSERT INTO fish_species (name, sort_order) VALUES ($1, $2) RETURNING {SPECIES_COLUMNS}"
    ))
    .bind(name)
    .bind(body.sort_order)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpecies {
    pub name: String,
    pub sort_order: i32,
    pub active: bool,
}

pub async fn update(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSpecies>,
) -> ApiResult<Json<FishSpecies>> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Species name is required.".into()));
    }
    let row = sqlx::query_as::<_, FishSpecies>(&format!(
        "UPDATE fish_species SET name = $2, sort_order = $3, active = $4, updated_at = now()
         WHERE id = $1 RETURNING {SPECIES_COLUMNS}"
    ))
    .bind(id)
    .bind(name)
    .bind(body.sort_order)
    .bind(body.active)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Species not found.".into()))?;
    Ok(Json(row))
}

/// Hard-deletes a species nobody has logged a catch against; otherwise
/// deactivates it and says why, so an existing catch keeps naming what was
/// actually caught. Same bargain [`crate::checklist::remove`] strikes with
/// completed checkouts.
pub async fn remove(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let (used,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM journal_catches WHERE species_id = $1)")
            .bind(id)
            .fetch_one(&state.db)
            .await?;

    if used {
        let done =
            sqlx::query("UPDATE fish_species SET active = false, updated_at = now() WHERE id = $1")
                .bind(id)
                .execute(&state.db)
                .await?;
        if done.rows_affected() == 0 {
            return Err(AppError::NotFound("Species not found.".into()));
        }
        return Ok(Json(serde_json::json!({
            "deleted": false,
            "deactivated": true,
            "message": "Someone has logged a catch of this species, so it was deactivated \
                        instead of deleted — that keeps their catch log intact.",
        })));
    }

    let done = sqlx::query("DELETE FROM fish_species WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Species not found.".into()));
    }
    Ok(Json(
        serde_json::json!({ "deleted": true, "deactivated": false }),
    ))
}
