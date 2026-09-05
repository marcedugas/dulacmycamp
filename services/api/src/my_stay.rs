//! `GET /api/my-stay` — the guest's most relevant booking for the My Stay
//! hub: a combined view over bookings, checkout, and journal state, reusing
//! the checkout-eligibility rule from [`crate::checkout`] rather than
//! reimplementing it.

use crate::{ApiResult, Shared, auth::AuthUser, checkout, journal};
use axum::{Json, extract::State};
use chrono::{NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct MyStay {
    pub has_stay: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub booking_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_in: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_out: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_count_adults: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_count_kids: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_out: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout_eligible: Option<bool>,
    /// Whether `/journal/new` should be offered for this booking — see
    /// [`journal::is_journal_eligible`]. `false` whenever `journal_status`
    /// is already set, since an existing entry is shown as a badge instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_eligible: Option<bool>,
    /// Set once an entry exists for this booking, so the frontend can show
    /// a status badge in place of the "Share your story" button.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_status: Option<String>,
}

impl MyStay {
    fn none() -> Self {
        MyStay {
            has_stay: false,
            booking_id: None,
            check_in: None,
            check_out: None,
            guest_count_adults: None,
            guest_count_kids: None,
            checked_out: None,
            checkout_eligible: None,
            journal_eligible: None,
            journal_status: None,
        }
    }
}

type BookingRow = (Uuid, NaiveDate, NaiveDate, i32, i32);

/// Picks the more relevant of an "upcoming or currently-active" candidate
/// and a "most recent past" candidate — the former always wins when both
/// exist. Pure mirror of the two-query fallback below, kept here as the
/// tested, documented spec of the preference rule.
fn pick_relevant_booking<T>(upcoming: Option<T>, past: Option<T>) -> Option<T> {
    upcoming.or(past)
}

pub async fn my_stay(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<MyStay>> {
    let today = Utc::now().date_naive();

    // Prefer the nearest upcoming or currently-active approved booking
    // (check_out >= today already covers "in progress" — its check_in is
    // necessarily <= today, so ordering by check_in ASC surfaces it first
    // over anything further out).
    let upcoming: Option<BookingRow> = sqlx::query_as(
        "SELECT id, check_in, check_out, guest_count_adults, guest_count_kids
         FROM bookings
         WHERE user_id = $1 AND status = 'approved' AND check_out >= $2
         ORDER BY check_in ASC LIMIT 1",
    )
    .bind(user.id)
    .bind(today)
    .fetch_optional(&state.db)
    .await?;

    // Only queried when there's no upcoming/active booking — no need to
    // pay for a second round trip when the first already answered it.
    let past: Option<BookingRow> = if upcoming.is_some() {
        None
    } else {
        sqlx::query_as(
            "SELECT id, check_in, check_out, guest_count_adults, guest_count_kids
             FROM bookings
             WHERE user_id = $1 AND status = 'approved'
             ORDER BY check_out DESC LIMIT 1",
        )
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?
    };
    let chosen = pick_relevant_booking(upcoming, past);

    let Some((booking_id, check_in, check_out, adults, kids)) = chosen else {
        return Ok(Json(MyStay::none()));
    };

    let (checked_out,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM booking_checkouts WHERE booking_id = $1)")
            .bind(booking_id)
            .fetch_one(&state.db)
            .await?;
    let journal_status: Option<(String,)> =
        sqlx::query_as("SELECT status FROM journal_entries WHERE booking_id = $1")
            .bind(booking_id)
            .fetch_optional(&state.db)
            .await?;
    let journal_status = journal_status.map(|(s,)| s);

    let checkout_eligible =
        checkout::is_checkout_eligible("approved", check_out, today, checked_out);
    let journal_eligible =
        journal::is_journal_eligible("approved", check_in, today, journal_status.is_some());

    Ok(Json(MyStay {
        has_stay: true,
        booking_id: Some(booking_id),
        check_in: Some(check_in),
        check_out: Some(check_out),
        guest_count_adults: Some(adults),
        guest_count_kids: Some(kids),
        checked_out: Some(checked_out),
        checkout_eligible: Some(checkout_eligible),
        journal_eligible: Some(journal_eligible),
        journal_status,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_upcoming_over_past_when_both_exist() {
        assert_eq!(
            pick_relevant_booking(Some("upcoming"), Some("past")),
            Some("upcoming")
        );
    }

    #[test]
    fn falls_back_to_past_when_no_upcoming_booking() {
        assert_eq!(pick_relevant_booking(None, Some("past")), Some("past"));
    }

    #[test]
    fn no_stay_when_neither_upcoming_nor_past_exists() {
        assert_eq!(pick_relevant_booking::<&str>(None, None), None);
    }
}
