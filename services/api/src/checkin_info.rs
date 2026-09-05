//! Admin-editable arrival details — key location, wifi code, water heater
//! instructions, and the like. Content only, same list-item CRUD shape as
//! [`crate::checklist`]/`site_content`'s rules and amenities. No used-in-
//! history delete guard here: unlike a checklist item, nothing else
//! references a checkin-info row, so a plain delete is always safe.
//!
//! Surfaced two places: baked live into the booking-confirmed email at send
//! time ([`all_for_email`]), and on `/my-stay` for as long as a guest holds
//! an approved, not-yet-checked-out booking ([`list_for_guest`]) — access is
//! derived from booking state, never a separate admin grant/revoke step.

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
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Serialize, FromRow)]
pub struct CheckinInfoItem {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const ITEM_COLUMNS: &str = "id, title, body, sort_order, created_at, updated_at";

/// Every check-in info title/body, in display order, for baking into the
/// booking-confirmed email at send time — never a snapshot from whenever
/// the template was written.
pub async fn all_for_email(db: &PgPool) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT title, body FROM checkin_info_items ORDER BY sort_order, created_at",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

// ─────────────────────────── admin ───────────────────────────

pub async fn list_admin(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<CheckinInfoItem>>> {
    let rows = sqlx::query_as::<_, CheckinInfoItem>(&format!(
        "SELECT {ITEM_COLUMNS} FROM checkin_info_items ORDER BY sort_order, created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct ItemBody {
    pub title: String,
    pub body: String,
    pub sort_order: i32,
}

impl ItemBody {
    fn validated(&self) -> ApiResult<(&str, &str)> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err(AppError::BadRequest("Title is required.".into()));
        }
        let body = self.body.trim();
        if body.is_empty() {
            return Err(AppError::BadRequest("Body can't be empty.".into()));
        }
        Ok((title, body))
    }
}

pub async fn create(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Json(body): Json<ItemBody>,
) -> ApiResult<Json<CheckinInfoItem>> {
    let (title, item_body) = body.validated()?;
    let row = sqlx::query_as::<_, CheckinInfoItem>(&format!(
        "INSERT INTO checkin_info_items (title, body, sort_order)
         VALUES ($1, $2, $3) RETURNING {ITEM_COLUMNS}"
    ))
    .bind(title)
    .bind(item_body)
    .bind(body.sort_order)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

pub async fn update(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ItemBody>,
) -> ApiResult<Json<CheckinInfoItem>> {
    let (title, item_body) = body.validated()?;
    let row = sqlx::query_as::<_, CheckinInfoItem>(&format!(
        "UPDATE checkin_info_items SET title = $2, body = $3, sort_order = $4, updated_at = now()
         WHERE id = $1 RETURNING {ITEM_COLUMNS}"
    ))
    .bind(id)
    .bind(title)
    .bind(item_body)
    .bind(body.sort_order)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Check-in info item not found.".into()))?;
    Ok(Json(row))
}

pub async fn remove(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let done = sqlx::query("DELETE FROM checkin_info_items WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Check-in info item not found.".into()));
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}

// ─────────────────────────── guest ───────────────────────────

/// Whether a single booking, given its status and whether it's been
/// checked out, currently grants check-in-info access. Deliberately not the
/// same rule as checkout eligibility (that also requires the departure day
/// to have arrived, and excludes anything not yet approved) — a guest needs
/// the wifi code *before* they arrive, and for as long as the stay hasn't
/// wrapped up, regardless of whether it's upcoming or already underway or
/// even already past (a booking is never automatically checked out just
/// because time has passed — only an actual checkout submission does that).
fn grants_access(status: &str, checked_out: bool) -> bool {
    status == "approved" && !checked_out
}

/// Whether *any* of a user's bookings currently grants access. Pure mirror
/// of the `EXISTS` query in [`has_checkin_access`] — kept here as the
/// tested, documented spec of the rule.
pub fn has_access(bookings: &[(&str, bool)]) -> bool {
    bookings
        .iter()
        .any(|(status, checked_out)| grants_access(status, *checked_out))
}

/// Whether `user_id` currently holds check-in-info access. See [`has_access`]
/// for the rule itself; this just evaluates it as one SQL `EXISTS` instead
/// of fetching every booking into Rust to reduce over.
async fn has_checkin_access(db: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let (has_access,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(
            SELECT 1 FROM bookings b
            WHERE b.user_id = $1 AND b.status = 'approved'
              AND NOT EXISTS (SELECT 1 FROM booking_checkouts bc WHERE bc.booking_id = b.id)
         )",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(has_access)
}

#[derive(Debug, Serialize, FromRow)]
pub struct GuestItem {
    pub id: Uuid,
    pub title: String,
    pub body: String,
}

/// `GET /api/checkin-info` — 403 for a guest with no approved,
/// not-yet-checked-out booking, rather than an empty list: unlike
/// checkout/journal eligibility (a personalized subset of the guest's own
/// bookings, where "none right now" is a normal, silent state), this
/// endpoint hands back shared content gated on access — an empty list here
/// would be ambiguous between "no access" and "admin hasn't added any yet".
pub async fn list_for_guest(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<GuestItem>>> {
    if !has_checkin_access(&state.db, user.id).await? {
        return Err(AppError::Forbidden(
            "Check-in info is only available while you have an active approved stay.".into(),
        ));
    }

    let rows = sqlx::query_as::<_, GuestItem>(
        "SELECT id, title, body FROM checkin_info_items ORDER BY sort_order, created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_approved_bookings_means_no_access() {
        assert!(!has_access(&[]));
        assert!(!has_access(&[
            ("denied", false),
            ("cancelled", false),
            ("pending", false)
        ]));
    }

    #[test]
    fn a_past_approved_booking_still_grants_access() {
        // Access never expires on its own with the passage of time — only
        // an actual completed checkout revokes it.
        assert!(has_access(&[("approved", false)]));
    }

    #[test]
    fn a_checked_out_approved_booking_no_longer_grants_access() {
        assert!(!has_access(&[("approved", true)]));
    }

    #[test]
    fn access_returns_the_moment_a_new_booking_is_approved() {
        // One denied/checked-out booking plus one live approved one: access
        // comes from *any* qualifying booking, not the most recent one.
        assert!(has_access(&[
            ("approved", true),
            ("denied", false),
            ("approved", false)
        ]));
    }

    #[test]
    fn item_body_rejects_blank_title_and_body() {
        let blank_title = ItemBody {
            title: "   ".into(),
            body: "Something".into(),
            sort_order: 0,
        };
        assert!(matches!(
            blank_title.validated(),
            Err(AppError::BadRequest(_))
        ));

        let blank_body = ItemBody {
            title: "Key Location".into(),
            body: "   ".into(),
            sort_order: 0,
        };
        assert!(matches!(
            blank_body.validated(),
            Err(AppError::BadRequest(_))
        ));

        let ok = ItemBody {
            title: " Key Location ".into(),
            body: " Under the third flowerpot. ".into(),
            sort_order: 0,
        };
        assert_eq!(
            ok.validated().unwrap(),
            ("Key Location", "Under the third flowerpot.")
        );
    }
}
