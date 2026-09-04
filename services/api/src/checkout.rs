//! Guest-facing checkout: replaces the physical paper checklist at the
//! camp. Completing checkout is the trigger that unlocks the journal
//! prompt for that stay — there is no scheduled job, the chaining is
//! inline via `journal_eligible` in [`submit`]'s response, read by the
//! frontend to transition straight into `/journal/new` on the same page.

use crate::{
    ApiResult, AppError, Shared,
    auth::{AdminUser, AuthUser},
    email, email_templates, notifications, users,
};
use axum::{Json, extract::State};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, types::Json as SqlxJson};
use std::collections::HashSet;
use uuid::Uuid;

fn app_url(state: &Shared) -> &str {
    state.cfg.frontend_url.trim_end_matches('/')
}

/// Whether a booking qualifies for the checkout-eligible list: approved,
/// departure day has arrived, and not already checked out. Pure mirror of
/// the `WHERE` clause in [`eligible`]'s query — kept here as the tested,
/// documented spec of the rule.
pub fn is_checkout_eligible(
    status: &str,
    check_out: NaiveDate,
    today: NaiveDate,
    already_checked_out: bool,
) -> bool {
    status == "approved" && check_out <= today && !already_checked_out
}

/// Guards against checking out the same stay twice. `submit()` calls this
/// with the existence check it already had to make.
fn require_not_already_checked_out(already: bool) -> ApiResult<()> {
    if already {
        return Err(AppError::Conflict(
            "This stay has already been checked out.".into(),
        ));
    }
    Ok(())
}

// ─────────────────────────── guest ───────────────────────────

#[derive(Debug, Serialize, FromRow)]
pub struct EligibleBooking {
    pub id: Uuid,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub guest_count_adults: i32,
    pub guest_count_kids: i32,
}

/// `GET /api/checkout/eligible` — the caller's own bookings that qualify
/// for checkout: approved, departure day has arrived, not already
/// checked out. Usually zero or one row, but back-to-back stays are
/// possible, so it's a list.
pub async fn eligible(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<EligibleBooking>>> {
    let rows = sqlx::query_as::<_, EligibleBooking>(
        "SELECT b.id, b.check_in, b.check_out, b.guest_count_adults, b.guest_count_kids
         FROM bookings b
         WHERE b.user_id = $1 AND b.status = 'approved' AND b.check_out <= $2
           AND NOT EXISTS (SELECT 1 FROM booking_checkouts bc WHERE bc.booking_id = b.id)
         ORDER BY b.check_out DESC",
    )
    .bind(user.id)
    .bind(Utc::now().date_naive())
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct SubmitCheckout {
    pub booking_id: Uuid,
    #[serde(default)]
    pub checked_item_ids: Vec<Uuid>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub success: bool,
    pub booking_id: Uuid,
    pub journal_eligible: bool,
}

/// `POST /api/checkout`. Honor-system checklist: submission is never
/// blocked on which (or how many) boxes are checked, only on whether this
/// stay is actually checkout-eligible in the first place.
pub async fn submit(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<SubmitCheckout>,
) -> ApiResult<Json<CheckoutResponse>> {
    let booking: Option<(String, NaiveDate, NaiveDate)> = sqlx::query_as(
        "SELECT status, check_in, check_out FROM bookings WHERE id = $1 AND user_id = $2",
    )
    .bind(body.booking_id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?;
    let Some((status, check_in, check_out)) = booking else {
        return Err(AppError::NotFound("Booking not found.".into()));
    };
    if status != "approved" {
        return Err(AppError::BadRequest(
            "Only approved stays can be checked out.".into(),
        ));
    }

    let (already,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM booking_checkouts WHERE booking_id = $1)")
            .bind(body.booking_id)
            .fetch_one(&state.db)
            .await?;
    require_not_already_checked_out(already)?;

    // Silently drop any id that isn't (or is no longer) a real checklist
    // item, rather than rejecting the whole submission — this is an
    // honor-system checklist, not a compliance gate.
    let valid_ids: Vec<Uuid> = if body.checked_item_ids.is_empty() {
        Vec::new()
    } else {
        let rows: Vec<(Uuid,)> =
            sqlx::query_as("SELECT id FROM checklist_items WHERE id = ANY($1)")
                .bind(&body.checked_item_ids)
                .fetch_all(&state.db)
                .await?;
        rows.into_iter().map(|(id,)| id).collect()
    };

    let notes = body
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    sqlx::query(
        "INSERT INTO booking_checkouts (booking_id, user_id, checked_item_ids, notes)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(body.booking_id)
    .bind(user.id)
    .bind(SqlxJson(&valid_ids))
    .bind(notes)
    .execute(&state.db)
    .await?;

    // Only send anything if there's actually something to flag — routine
    // checkouts generate no email.
    if let Some(note_text) = notes {
        let guest = user.display_name();
        email::spawn_opt(
            state.clone(),
            state.cfg.admin_email.clone(),
            email_templates::checkout_notes_to_admin(
                &guest,
                check_in,
                check_out,
                note_text,
                app_url(&state),
            ),
        );
        if let Some(admin) = users::first_admin(&state.db).await? {
            notifications::system_message(
                &state.db,
                admin.id,
                "Checkout note flagged",
                &format!(
                    "{guest} flagged something at checkout ({} – {}): {note_text}",
                    check_in.format("%b %-d"),
                    check_out.format("%b %-d, %Y")
                ),
                Some(body.booking_id),
            )
            .await?;
        }
    }

    // Always true: this checkout row didn't exist a moment ago, and
    // journal_entries.booking_id is unique, so this stay has no entry yet.
    Ok(Json(CheckoutResponse {
        success: true,
        booking_id: body.booking_id,
        journal_eligible: true,
    }))
}

// ─────────────────────────── admin ───────────────────────────

#[derive(Debug, Serialize)]
pub struct AdminChecklistState {
    pub id: Uuid,
    pub label: String,
    pub checked: bool,
}

#[derive(Debug, Serialize)]
pub struct AdminCheckout {
    pub id: Uuid,
    pub booking_id: Uuid,
    pub guest_name: String,
    pub guest_email: String,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub completed_at: DateTime<Utc>,
    pub notes: Option<String>,
    pub items: Vec<AdminChecklistState>,
}

#[derive(FromRow)]
struct Row {
    id: Uuid,
    booking_id: Uuid,
    guest_name: Option<String>,
    guest_email: String,
    check_in: NaiveDate,
    check_out: NaiveDate,
    completed_at: DateTime<Utc>,
    notes: Option<String>,
    checked_item_ids: SqlxJson<Vec<Uuid>>,
}

/// `GET /api/admin/checkouts` — completed checkouts with per-item
/// checked/unchecked state, resolved against every checklist item ever
/// created (active or not — a hard delete is blocked while an item is
/// referenced by any checkout, so the label always resolves).
pub async fn admin_list(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<AdminCheckout>>> {
    let rows = sqlx::query_as::<_, Row>(
        "SELECT bc.id, bc.booking_id, u.full_name AS guest_name, u.email AS guest_email,
                b.check_in, b.check_out, bc.completed_at, bc.notes, bc.checked_item_ids
         FROM booking_checkouts bc
         JOIN bookings b ON b.id = bc.booking_id
         JOIN users u ON u.id = bc.user_id
         ORDER BY bc.completed_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let items: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, label FROM checklist_items ORDER BY sort_order, created_at")
            .fetch_all(&state.db)
            .await?;

    let out = rows
        .into_iter()
        .map(|r| {
            let checked: HashSet<Uuid> = r.checked_item_ids.0.into_iter().collect();
            AdminCheckout {
                id: r.id,
                booking_id: r.booking_id,
                guest_name: r.guest_name.unwrap_or_else(|| r.guest_email.clone()),
                guest_email: r.guest_email,
                check_in: r.check_in,
                check_out: r.check_out,
                completed_at: r.completed_at,
                notes: r.notes,
                items: items
                    .iter()
                    .map(|(id, label)| AdminChecklistState {
                        id: *id,
                        label: label.clone(),
                        checked: checked.contains(id),
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn excludes_bookings_not_yet_at_check_out_date() {
        let today = date(2026, 9, 4);
        let departs_tomorrow = date(2026, 9, 5);
        assert!(!is_checkout_eligible(
            "approved",
            departs_tomorrow,
            today,
            false
        ));
    }

    #[test]
    fn check_out_day_itself_is_eligible() {
        let today = date(2026, 9, 4);
        assert!(is_checkout_eligible("approved", today, today, false));
    }

    #[test]
    fn a_stay_that_departed_in_the_past_is_eligible() {
        let today = date(2026, 9, 4);
        let departed_last_week = date(2026, 8, 28);
        assert!(is_checkout_eligible(
            "approved",
            departed_last_week,
            today,
            false
        ));
    }

    #[test]
    fn excludes_bookings_with_an_existing_checkout() {
        let today = date(2026, 9, 4);
        let already_checked_out = true;
        assert!(!is_checkout_eligible(
            "approved",
            today,
            today,
            already_checked_out
        ));
    }

    #[test]
    fn excludes_bookings_that_are_not_approved() {
        let today = date(2026, 9, 4);
        for status in ["pending", "denied", "cancelled"] {
            assert!(
                !is_checkout_eligible(status, today, today, false),
                "{status} should not be eligible"
            );
        }
    }

    #[test]
    fn duplicate_checkout_submission_is_blocked() {
        assert!(require_not_already_checked_out(false).is_ok());
        assert!(matches!(
            require_not_already_checked_out(true),
            Err(AppError::Conflict(_))
        ));
    }
}
