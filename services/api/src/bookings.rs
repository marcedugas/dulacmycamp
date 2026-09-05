//! Stay requests: submission, the owner's one-click approve/deny, and the
//! read model the calendar and admin table are built from.
//!
//! Overlapping requests are deliberately *not* rejected. The camp is shared
//! among family and friends; two cousins asking for the same weekend is a
//! conversation, not an error. We flag the overlap and let the owner arbitrate.

use crate::{
    ApiResult, AppError, Shared,
    auth::{AdminUser, AuthUser, MaybeUser},
    email, email_templates, notifications,
    users::{self, User},
};
use axum::{
    Form, Json,
    extract::{Path, Query, State},
    response::Html,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// How long the buttons in the owner's email stay live.
const APPROVE_TOKEN_TTL_HOURS: i64 = 48;
/// Inside this window an approved stay can only be cancelled by an admin.
const LATE_CANCEL_HOURS: i64 = 48;

/// Booking columns, minus `approve_token*` — those never leave the server.
const BOOKING_COLUMNS: &str = "id, user_id, check_in, check_out, guest_count_adults, \
                               guest_count_kids, has_pets, other_requests, status, \
                               denied_reason, approved_at, approved_by, created_at, updated_at";

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Booking {
    pub id: Uuid,
    pub user_id: Uuid,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub guest_count_adults: i32,
    pub guest_count_kids: i32,
    pub has_pets: bool,
    pub other_requests: Option<String>,
    pub status: String,
    pub denied_reason: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A booking joined to its guest, for views that may reveal who booked.
#[derive(Debug, Clone, FromRow)]
struct BookingRow {
    #[sqlx(flatten)]
    booking: Booking,
    guest_name: Option<String>,
    guest_email: String,
    /// Whether `crate::checkout` has a completed record for this booking.
    checked_out: bool,
    checkout_notes: Option<String>,
    /// Present once a `crate::journal` entry exists for this booking.
    journal_id: Option<Uuid>,
    journal_status: Option<String>,
}

/// What a given caller is allowed to see about a booking.
///
/// Public callers get dates and head-count only — the calendar shows that the
/// camp is taken, never by whom.
#[derive(Debug, Serialize)]
pub struct BookingView {
    pub id: Uuid,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub status: String,
    pub guest_count_adults: i32,
    pub guest_count_kids: i32,
    pub is_mine: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_pets: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_requests: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denied_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// Whether checkout has been completed for this booking. Same
    /// visibility as `guest_name` etc. — the booking's owner and admins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_out: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_status: Option<String>,
}

impl BookingRow {
    /// Projects the row down to what `viewer` may see.
    fn to_view(&self, viewer: Option<&User>) -> BookingView {
        let b = &self.booking;
        let is_mine = viewer.is_some_and(|v| v.id == b.user_id);
        let full = is_mine || viewer.is_some_and(User::is_admin);

        BookingView {
            id: b.id,
            check_in: b.check_in,
            check_out: b.check_out,
            status: b.status.clone(),
            guest_count_adults: b.guest_count_adults,
            guest_count_kids: b.guest_count_kids,
            is_mine,
            user_id: full.then_some(b.user_id),
            guest_name: full.then(|| {
                self.guest_name
                    .clone()
                    .unwrap_or_else(|| self.guest_email.clone())
            }),
            guest_email: full.then(|| self.guest_email.clone()),
            has_pets: full.then_some(b.has_pets),
            other_requests: full.then(|| b.other_requests.clone()).flatten(),
            denied_reason: full.then(|| b.denied_reason.clone()).flatten(),
            approved_at: full.then_some(b.approved_at).flatten(),
            approved_by: full.then(|| b.approved_by.clone()).flatten(),
            created_at: full.then_some(b.created_at),
            checked_out: full.then_some(self.checked_out),
            checkout_notes: full.then(|| self.checkout_notes.clone()).flatten(),
            journal_id: full.then_some(self.journal_id).flatten(),
            journal_status: full.then(|| self.journal_status.clone()).flatten(),
        }
    }
}

fn select_rows() -> String {
    format!(
        "SELECT {}, u.full_name AS guest_name, u.email AS guest_email, \
                (bc.id IS NOT NULL) AS checked_out, bc.notes AS checkout_notes, \
                je.id AS journal_id, je.status AS journal_status
         FROM bookings b
         JOIN users u ON u.id = b.user_id
         LEFT JOIN booking_checkouts bc ON bc.booking_id = b.id
         LEFT JOIN journal_entries je ON je.booking_id = b.id",
        BOOKING_COLUMNS
            .split(", ")
            .map(|c| format!("b.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

async fn load_row(db: &PgPool, id: Uuid) -> Result<Option<BookingRow>, sqlx::Error> {
    sqlx::query_as::<_, BookingRow>(&format!("{} WHERE b.id = $1", select_rows()))
        .bind(id)
        .fetch_optional(db)
        .await
}

fn app_url(state: &Shared) -> &str {
    state.cfg.frontend_url.trim_end_matches('/')
}

fn api_url(state: &Shared) -> &str {
    state.cfg.api_base_url.trim_end_matches('/')
}

/// 256 bits of randomness, hex encoded. The token *is* the credential, so it
/// is stored single-use and cleared the moment it is redeemed.
fn new_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

// ─────────────────────────── list / read ───────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Restrict to the caller's own bookings.
    pub mine: Option<bool>,
    pub status: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

pub async fn list(
    State(state): State<Shared>,
    MaybeUser(viewer): MaybeUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<BookingView>>> {
    let is_admin = viewer.as_ref().is_some_and(User::is_admin);
    let viewer_id = viewer.as_ref().map(|v| v.id);

    // Visibility, expressed once in SQL rather than filtered in Rust:
    //  - admins see everything
    //  - a guest sees every approved stay plus all of their own rows
    //  - anonymous callers see approved stays only
    let visibility = if is_admin {
        "true"
    } else if viewer_id.is_some() {
        "(b.status = 'approved' OR b.user_id = $1)"
    } else {
        "b.status = 'approved'"
    };

    let mut sql = format!("{} WHERE {visibility}", select_rows());
    if q.mine.unwrap_or(false) {
        sql.push_str(" AND b.user_id = $1");
    }
    if q.status.is_some() {
        sql.push_str(" AND b.status = $2");
    }
    if q.from.is_some() {
        sql.push_str(" AND b.check_out > $3");
    }
    if q.to.is_some() {
        sql.push_str(" AND b.check_in < $4");
    }
    sql.push_str(" ORDER BY b.check_in ASC");

    // Bind every placeholder unconditionally; unreferenced binds are harmless
    // and keep the parameter numbering stable across filter combinations.
    let rows = sqlx::query_as::<_, BookingRow>(&sql)
        .bind(viewer_id)
        .bind(q.status.as_deref())
        .bind(q.from)
        .bind(q.to)
        .fetch_all(&state.db)
        .await?;

    Ok(Json(
        rows.iter().map(|r| r.to_view(viewer.as_ref())).collect(),
    ))
}

pub async fn get_one(
    State(state): State<Shared>,
    AuthUser(viewer): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BookingView>> {
    let row = load_row(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found.".into()))?;

    if row.booking.user_id != viewer.id && !viewer.is_admin() {
        return Err(AppError::Forbidden("That isn't your booking.".into()));
    }
    Ok(Json(row.to_view(Some(&viewer))))
}

// ─────────────────────────── capacity ───────────────────────────

#[derive(Debug, Serialize)]
pub struct Capacity {
    /// Adults already approved for any night in the requested range.
    pub approved_adults: i64,
    /// Adults once this request is counted.
    pub total_adults: i64,
    pub limit: i64,
    pub over_capacity: bool,
}

/// Adults on approved bookings overlapping `[check_in, check_out)`.
/// `exclude` skips the booking being evaluated so an edit doesn't count twice.
async fn approved_adults(
    db: &PgPool,
    check_in: NaiveDate,
    check_out: NaiveDate,
    exclude: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    let (total,): (Option<i64>,) = sqlx::query_as(
        "SELECT sum(guest_count_adults) FROM bookings
         WHERE status = 'approved' AND check_in < $2 AND check_out > $1
           AND ($3::uuid IS NULL OR id <> $3)",
    )
    .bind(check_in)
    .bind(check_out)
    .bind(exclude)
    .fetch_one(db)
    .await?;
    Ok(total.unwrap_or(0))
}

// ─────────────────────────── create ───────────────────────────

#[derive(Debug, PartialEq, Deserialize)]
pub struct CreateBooking {
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub guest_count_adults: i32,
    #[serde(default)]
    pub guest_count_kids: i32,
    #[serde(default)]
    pub has_pets: bool,
    pub other_requests: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateResponse {
    pub booking: BookingView,
    /// Non-blocking advisory shown to the guest on submit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    pub capacity: Capacity,
}

pub async fn create(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateBooking>,
) -> ApiResult<Json<CreateResponse>> {
    Ok(Json(create_booking_for(&state, &user, body).await?))
}

/// The actual booking-creation logic: validation, blackout check,
/// overlap/capacity flagging, the insert, and the full email chain
/// (guest confirmation, owner approve/deny, admin notification).
///
/// Shared verbatim by [`create`] (a guest submitting their own request) and
/// `admin::create_booking` (an admin entering one on a guest's behalf) —
/// the only thing that differs between the two call sites is which `User`
/// is passed in as the acting guest. Nothing here treats an admin-entered
/// booking any differently: it still starts `pending` and still requires
/// real owner approval.
pub async fn create_booking_for(
    state: &Shared,
    user: &User,
    body: CreateBooking,
) -> ApiResult<CreateResponse> {
    if body.check_out <= body.check_in {
        return Err(AppError::BadRequest(
            "Check-out must be after check-in.".into(),
        ));
    }
    if body.check_in < Utc::now().date_naive() {
        return Err(AppError::BadRequest(
            "Check-in can't be in the past.".into(),
        ));
    }
    if body.guest_count_adults < 1 {
        return Err(AppError::BadRequest(
            "At least one adult is required.".into(),
        ));
    }
    if body.guest_count_kids < 0 {
        return Err(AppError::BadRequest("Kid count can't be negative.".into()));
    }

    // Blackouts are inclusive on both ends and are a hard stop.
    let blackout: Option<(NaiveDate, NaiveDate, Option<String>)> = sqlx::query_as(
        "SELECT start_date, end_date, reason FROM blackout_dates
         WHERE start_date < $2 AND end_date >= $1
         ORDER BY start_date LIMIT 1",
    )
    .bind(body.check_in)
    .bind(body.check_out)
    .fetch_optional(&state.db)
    .await?;

    if let Some((start, end, reason)) = blackout {
        let why = reason
            .filter(|r| !r.trim().is_empty())
            .map(|r| format!(" ({r})"))
            .unwrap_or_default();
        return Err(AppError::Conflict(format!(
            "The camp is unavailable {} – {}{why}. Please choose different dates.",
            start.format("%b %-d"),
            end.format("%b %-d, %Y"),
        )));
    }

    // Overlap with an existing request is allowed but surfaced.
    let (overlaps,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM bookings
         WHERE status IN ('approved', 'pending') AND check_in < $2 AND check_out > $1",
    )
    .bind(body.check_in)
    .bind(body.check_out)
    .fetch_one(&state.db)
    .await?;

    let already = approved_adults(&state.db, body.check_in, body.check_out, None).await?;
    let capacity = Capacity {
        approved_adults: already,
        total_adults: already + i64::from(body.guest_count_adults),
        limit: state.cfg.capacity_adults,
        over_capacity: already + i64::from(body.guest_count_adults) > state.cfg.capacity_adults,
    };

    let token = new_token();
    let booking = sqlx::query_as::<_, Booking>(&format!(
        "INSERT INTO bookings
           (user_id, check_in, check_out, guest_count_adults, guest_count_kids,
            has_pets, other_requests, approve_token, approve_token_expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now() + ($9 || ' hours')::interval)
         RETURNING {BOOKING_COLUMNS}"
    ))
    .bind(user.id)
    .bind(body.check_in)
    .bind(body.check_out)
    .bind(body.guest_count_adults)
    .bind(body.guest_count_kids)
    .bind(body.has_pets)
    .bind(
        body.other_requests
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .bind(&token)
    .bind(APPROVE_TOKEN_TTL_HOURS.to_string())
    .fetch_one(&state.db)
    .await?;

    let mut warnings = Vec::new();
    if overlaps > 0 {
        warnings.push(
            "These dates overlap with an existing booking. The owner will review both requests."
                .to_string(),
        );
    }
    if capacity.over_capacity {
        warnings.push(format!(
            "That would put {} adults at the camp, which sleeps {}. The owner will take a look.",
            capacity.total_adults, capacity.limit
        ));
    }

    // ── notifications ──
    let guest_name = user.display_name();
    let approve_url = format!("{}/api/bookings/approve/{token}", api_url(state));
    let deny_url = format!("{}/api/bookings/deny/{token}", api_url(state));

    email::spawn_all(
        state.clone(),
        email::owner_recipients(state).await,
        email_templates::booking_request_to_owner(&booking, &guest_name, &approve_url, &deny_url),
    );
    email::spawn_opt(
        state.clone(),
        state.cfg.admin_email.clone(),
        email_templates::booking_request_to_admin(&booking, &guest_name, app_url(state)),
    );
    email::spawn(
        state.clone(),
        user.email.clone(),
        email_templates::booking_pending_to_guest(&booking, app_url(state)),
    );

    // Mirror the owner notification into the admin's in-app inbox.
    if let Some(admin) = users::first_admin(&state.db).await? {
        notifications::system_message(
            &state.db,
            admin.id,
            "New booking request",
            &format!(
                "{guest_name} requested {} – {}.",
                booking.check_in.format("%b %-d"),
                booking.check_out.format("%b %-d, %Y")
            ),
            Some(booking.id),
        )
        .await?;
    }

    let row = BookingRow {
        booking,
        guest_name: user.full_name.clone(),
        guest_email: user.email.clone(),
        checked_out: false,
        checkout_notes: None,
        journal_id: None,
        journal_status: None,
    };

    Ok(CreateResponse {
        booking: row.to_view(Some(user)),
        warning: (!warnings.is_empty()).then(|| warnings.join(" ")),
        capacity,
    })
}

#[derive(Debug, Deserialize)]
pub struct AdminCreateBooking {
    pub email: String,
    /// Only used if this creates a new user — never overwrites an existing
    /// account's name.
    pub full_name: Option<String>,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub guest_count_adults: i32,
    #[serde(default)]
    pub guest_count_kids: i32,
    #[serde(default)]
    pub has_pets: bool,
    pub other_requests: Option<String>,
}

/// The only place `AdminCreateBooking` and `CreateBooking` are made to line
/// up — proof the admin path feeds [`create_booking_for`] the exact same
/// shape a guest's own `/book` submission would, field for field.
impl From<AdminCreateBooking> for CreateBooking {
    fn from(admin: AdminCreateBooking) -> Self {
        CreateBooking {
            check_in: admin.check_in,
            check_out: admin.check_out,
            guest_count_adults: admin.guest_count_adults,
            guest_count_kids: admin.guest_count_kids,
            has_pets: admin.has_pets,
            other_requests: admin.other_requests,
        }
    }
}

/// `POST /api/admin/bookings` — admin only. For a stay arranged outside the
/// app (phone call, in person) that still needs to go through the normal
/// approval flow. This is *not* a simplified or parallel path: it looks up
/// or creates the guest's account exactly as OTP self-registration would,
/// then calls [`create_booking_for`] — the identical validation, blackout
/// check, overlap/capacity flagging, and email chain a guest submitting
/// `/book` themselves gets, just with the admin acting on their behalf. The
/// booking still lands `pending` and still needs real owner approval.
pub async fn admin_create(
    State(state): State<Shared>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<AdminCreateBooking>,
) -> ApiResult<Json<CreateResponse>> {
    let email = body.email.trim();
    if !email.contains('@') || email.len() < 5 {
        return Err(AppError::BadRequest(
            "Enter a valid guest email address.".into(),
        ));
    }

    let guest = users::find_or_create_guest(&state.db, email, body.full_name.as_deref()).await?;

    Ok(Json(create_booking_for(&state, &guest, body.into()).await?))
}

// ─────────────────────────── state transitions ───────────────────────────

/// Applies an approval and fires the guest notifications.
async fn do_approve(state: &Shared, row: &BookingRow, actor: &str) -> ApiResult<Booking> {
    let booking = sqlx::query_as::<_, Booking>(&format!(
        "UPDATE bookings
         SET status = 'approved', approved_at = now(), approved_by = $2,
             denied_reason = NULL, approve_token = NULL, approve_token_expires_at = NULL
         WHERE id = $1 RETURNING {BOOKING_COLUMNS}"
    ))
    .bind(row.booking.id)
    .bind(actor)
    .fetch_one(&state.db)
    .await?;

    let checkin_items = crate::checkin_info::all_for_email(&state.db).await?;
    email::spawn(
        state.clone(),
        row.guest_email.clone(),
        email_templates::booking_confirmed_to_guest(&booking, &checkin_items, app_url(state)),
    );
    notifications::system_message(
        &state.db,
        booking.user_id,
        "Your booking is confirmed",
        &format!(
            "Your stay {} – {} has been approved. See you at the camp!",
            booking.check_in.format("%b %-d"),
            booking.check_out.format("%b %-d, %Y")
        ),
        Some(booking.id),
    )
    .await?;

    Ok(booking)
}

/// Applies a denial and fires the guest notifications.
async fn do_deny(
    state: &Shared,
    row: &BookingRow,
    actor: &str,
    reason: Option<&str>,
) -> ApiResult<Booking> {
    let reason = reason.map(str::trim).filter(|r| !r.is_empty());

    let booking = sqlx::query_as::<_, Booking>(&format!(
        "UPDATE bookings
         SET status = 'denied', denied_reason = $2, approved_by = $3,
             approved_at = NULL, approve_token = NULL, approve_token_expires_at = NULL
         WHERE id = $1 RETURNING {BOOKING_COLUMNS}"
    ))
    .bind(row.booking.id)
    .bind(reason)
    .bind(actor)
    .fetch_one(&state.db)
    .await?;

    email::spawn(
        state.clone(),
        row.guest_email.clone(),
        email_templates::booking_denied_to_guest(&booking, reason, app_url(state)),
    );
    notifications::system_message(
        &state.db,
        booking.user_id,
        "Booking update",
        &match reason {
            Some(r) => format!(
                "Your request for {} – {} wasn't approved. {r}",
                booking.check_in.format("%b %-d"),
                booking.check_out.format("%b %-d, %Y")
            ),
            None => format!(
                "Your request for {} – {} wasn't approved. Feel free to pick other dates.",
                booking.check_in.format("%b %-d"),
                booking.check_out.format("%b %-d, %Y")
            ),
        },
        Some(booking.id),
    )
    .await?;

    Ok(booking)
}

// ─────────────────────────── one-click token actions ───────────────────────────

/// Resolves a token to its booking, or renders the reason it can't be used.
/// Returns `Err(Html)` rather than an error response: the owner is clicking a
/// link in their mail client and deserves a page, not a JSON envelope.
async fn resolve_token(state: &Shared, token: &str) -> Result<BookingRow, Html<String>> {
    let found: Option<(Uuid, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT id, approve_token_expires_at FROM bookings WHERE approve_token = $1",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "token lookup failed");
        Html(email_templates::action_result_page(
            "Something went wrong",
            "Please try again, or approve this booking from the app.",
            false,
        ))
    })?;

    let Some((id, expires_at)) = found else {
        return Err(Html(email_templates::action_result_page(
            "This link is no longer valid",
            "It may have already been used, or the booking was handled in the app.",
            false,
        )));
    };

    if expires_at.is_some_and(|e| e < Utc::now()) {
        return Err(Html(email_templates::action_result_page(
            "This link has expired",
            "Approval links are good for 48 hours. Please handle this booking in the app.",
            false,
        )));
    }

    match load_row(&state.db, id).await {
        Ok(Some(row)) if row.booking.status == "pending" => Ok(row),
        Ok(Some(row)) => Err(Html(email_templates::action_result_page(
            &format!("Already {}", row.booking.status),
            "This booking has already been handled — no further action needed.",
            false,
        ))),
        _ => Err(Html(email_templates::action_result_page(
            "This link is no longer valid",
            "The booking could not be found.",
            false,
        ))),
    }
}

pub async fn approve_by_token(
    State(state): State<Shared>,
    Path(token): Path<String>,
) -> Html<String> {
    let row = match resolve_token(&state, &token).await {
        Ok(r) => r,
        Err(page) => return page,
    };
    let guest = row
        .guest_name
        .clone()
        .unwrap_or_else(|| row.guest_email.clone());

    match do_approve(&state, &row, "owner").await {
        Ok(_) => Html(email_templates::action_result_page(
            "Booking approved!",
            &format!("{guest} has been notified by email."),
            true,
        )),
        Err(e) => {
            tracing::error!(error = ?e, "approve by token failed");
            Html(email_templates::action_result_page(
                "Something went wrong",
                "The booking wasn't updated. Please try again.",
                false,
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DenyQuery {
    /// Supplying a reason on the query string makes denial truly one-click.
    pub reason: Option<String>,
}

pub async fn deny_by_token(
    State(state): State<Shared>,
    Path(token): Path<String>,
    Query(q): Query<DenyQuery>,
) -> Html<String> {
    let row = match resolve_token(&state, &token).await {
        Ok(r) => r,
        Err(page) => return page,
    };

    // No reason on the URL: show the small form instead of acting immediately.
    let Some(reason) = q.reason else {
        let guest = row
            .guest_name
            .clone()
            .unwrap_or_else(|| row.guest_email.clone());
        let dates = format!(
            "{} – {}",
            row.booking.check_in.format("%b %-d"),
            row.booking.check_out.format("%b %-d, %Y")
        );
        return Html(email_templates::deny_form_page(
            &format!("{}/api/bookings/deny/{token}", api_url(&state)),
            &guest,
            &dates,
        ));
    };

    finish_deny(&state, row, Some(reason)).await
}

#[derive(Debug, Deserialize)]
pub struct DenyForm {
    pub reason: Option<String>,
}

pub async fn deny_by_token_submit(
    State(state): State<Shared>,
    Path(token): Path<String>,
    Form(form): Form<DenyForm>,
) -> Html<String> {
    let row = match resolve_token(&state, &token).await {
        Ok(r) => r,
        Err(page) => return page,
    };
    finish_deny(&state, row, form.reason).await
}

async fn finish_deny(state: &Shared, row: BookingRow, reason: Option<String>) -> Html<String> {
    let guest = row
        .guest_name
        .clone()
        .unwrap_or_else(|| row.guest_email.clone());

    match do_deny(state, &row, "owner", reason.as_deref()).await {
        Ok(_) => Html(email_templates::action_result_page(
            "Booking denied",
            &format!("{guest} has been notified by email."),
            true,
        )),
        Err(e) => {
            tracing::error!(error = ?e, "deny by token failed");
            Html(email_templates::action_result_page(
                "Something went wrong",
                "The booking wasn't updated. Please try again.",
                false,
            ))
        }
    }
}

// ─────────────────────────── admin + guest actions ───────────────────────────

fn require_pending(row: &BookingRow) -> ApiResult<()> {
    if row.booking.status != "pending" {
        return Err(AppError::Conflict(format!(
            "This booking is already {}.",
            row.booking.status
        )));
    }
    Ok(())
}

pub async fn admin_approve(
    State(state): State<Shared>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BookingView>> {
    let row = load_row(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found.".into()))?;
    require_pending(&row)?;

    let booking = do_approve(&state, &row, &admin.email).await?;
    Ok(Json(BookingRow { booking, ..row }.to_view(Some(&admin))))
}

#[derive(Debug, Deserialize)]
pub struct DenyBody {
    pub reason: Option<String>,
}

pub async fn admin_deny(
    State(state): State<Shared>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DenyBody>,
) -> ApiResult<Json<BookingView>> {
    let row = load_row(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found.".into()))?;
    require_pending(&row)?;

    let booking = do_deny(&state, &row, &admin.email, body.reason.as_deref()).await?;
    Ok(Json(BookingRow { booking, ..row }.to_view(Some(&admin))))
}

pub async fn cancel(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BookingView>> {
    let row = load_row(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found.".into()))?;

    if row.booking.user_id != user.id && !user.is_admin() {
        return Err(AppError::Forbidden("That isn't your booking.".into()));
    }
    if matches!(row.booking.status.as_str(), "cancelled" | "denied") {
        return Err(AppError::Conflict(format!(
            "This booking is already {}.",
            row.booking.status
        )));
    }

    // Late cancellations on a confirmed stay need an admin, so the owner isn't
    // left guessing whether the camp is occupied.
    if row.booking.status == "approved" && !user.is_admin() {
        let check_in = row
            .booking
            .check_in
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc())
            .unwrap_or_else(Utc::now);
        if check_in - Utc::now() < Duration::hours(LATE_CANCEL_HOURS) {
            return Err(AppError::Forbidden(format!(
                "Confirmed stays can't be cancelled within {LATE_CANCEL_HOURS} hours of check-in. Message the owner and they'll sort it out."
            )));
        }
    }

    let booking = sqlx::query_as::<_, Booking>(&format!(
        "UPDATE bookings
         SET status = 'cancelled', approve_token = NULL, approve_token_expires_at = NULL
         WHERE id = $1 RETURNING {BOOKING_COLUMNS}"
    ))
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let guest = row
        .guest_name
        .clone()
        .unwrap_or_else(|| row.guest_email.clone());

    // Only a *confirmed* stay getting pulled needs the owner's attention —
    // he never said yes to a merely-pending request, so there's nothing to
    // walk back and no dates were actually held.
    if should_notify_owner_of_cancellation(&row.booking.status) {
        let cancelled_by = if row.booking.user_id == user.id {
            "guest"
        } else {
            "admin"
        };
        email::spawn_all(
            state.clone(),
            email::owner_recipients(&state).await,
            email_templates::booking_confirmed_cancelled_to_owner(&booking, &guest, cancelled_by),
        );
    }
    email::spawn_opt(
        state.clone(),
        state.cfg.admin_email.clone(),
        email_templates::booking_cancelled_notice(&booking, &guest),
    );

    Ok(Json(BookingRow { booking, ..row }.to_view(Some(&user))))
}

/// Whether cancelling a booking that was in `previous_status` should notify
/// the owner — only when it had actually been approved. Pure so it's
/// unit-testable without a database.
fn should_notify_owner_of_cancellation(previous_status: &str) -> bool {
    previous_status == "approved"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_an_approved_booking_notifies_the_owner() {
        assert!(should_notify_owner_of_cancellation("approved"));
    }

    #[test]
    fn cancelling_a_pending_booking_does_not_notify_the_owner() {
        assert!(!should_notify_owner_of_cancellation("pending"));
    }

    // `should_notify_owner_of_cancellation` only decides *whether* to
    // notify; who actually receives it is `email::owner_recipients`, reused
    // unchanged here rather than duplicated — its own fan-out to every
    // is_owner=true account (same mechanism booking-submission notifications
    // use) is already covered by `email::tests::every_flagged_user_is_a_recipient`.
    #[test]
    fn a_cancelled_or_denied_or_confirmed_booking_would_never_reach_this_check_again() {
        // Sanity check on the literal the real code compares against —
        // guards against a typo silently breaking the gate.
        assert!(!should_notify_owner_of_cancellation("cancelled"));
        assert!(!should_notify_owner_of_cancellation("denied"));
    }

    // Both `create()` (a guest's own /book submission) and `admin_create()`
    // (an admin entering one on a guest's behalf) end by calling the exact
    // same `create_booking_for()` — the one function that contains every
    // email::spawn* call in booking creation. There is no second,
    // admin-specific code path for that email chain to drift from; the only
    // thing that could make the two diverge is this From impl silently
    // dropping or mis-mapping a field on the way in, which is what this
    // test guards against.
    #[test]
    fn admin_create_feeds_create_booking_for_the_identical_request_a_guest_submission_would() {
        let admin_input = AdminCreateBooking {
            email: "guest@example.com".into(),
            full_name: Some("Jean Guest".into()),
            check_in: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            check_out: NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
            guest_count_adults: 3,
            guest_count_kids: 2,
            has_pets: true,
            other_requests: Some("Bringing a boat trailer".into()),
        };
        let expected = CreateBooking {
            check_in: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            check_out: NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
            guest_count_adults: 3,
            guest_count_kids: 2,
            has_pets: true,
            other_requests: Some("Bringing a boat trailer".into()),
        };
        assert_eq!(CreateBooking::from(admin_input), expected);
    }
}
