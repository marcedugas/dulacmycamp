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
                               denied_reason, approved_at, approved_by, is_private, \
                               created_at, updated_at";

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
    /// Whether the booker has kept their identity off other people's
    /// calendars. Defaults to true — see `0014_booking_privacy.sql`.
    pub is_private: bool,
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
/// Two independent visibility rules live here, and they answer different
/// questions:
///
///   * `full` — the administrative view: name, email, pets, requests,
///     checkout notes. The booking's own guest, admins, and the camp owner
///     (`User::sees_guest_details`). Unchanged by the privacy toggle, because
///     it is not the calendar: an admin arbitrating overlaps has always had
///     to know who is asking.
///   * [`shows_booker`] — the *calendar's* view of who is coming, which the
///     booker themselves controls. Nobody sees a booker's name through this
///     field unless that booker chose to be seen.
///
/// The two are deliberately not the same switch. Making `full` respect the
/// toggle would break the owner's approval workflow; making `shows_booker`
/// follow `full` would mean a name the booker kept private still reached
/// every admin's calendar.
#[derive(Debug, Serialize)]
pub struct BookingView {
    pub id: Uuid,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub status: String,
    /// The party, withheld from anyone [`shows_booker`] would not name.
    ///
    /// "2 adults and a kid" is not meaningfully less identifying than the
    /// name it sits next to — it says how many people, and which of them are
    /// children, for a specific household on specific dates. Withholding the
    /// name while shipping the breakdown to every caller would have made the
    /// privacy setting a UI preference rather than a rule, since the numbers
    /// are a devtools Network tab away.
    ///
    /// The calendar's capacity warning does not read these. It reads
    /// [`occupancy`], a per-night total that is nobody's household in
    /// particular.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_count_adults: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_count_kids: Option<i32>,
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
    /// visibility as `guest_name` etc. — the guest who booked, admins, and the
    /// camp owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_out: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_status: Option<String>,
    /// The booker's own privacy setting, for the surfaces that let them
    /// change it (`/my-bookings`). Same `full` visibility as the rest — this
    /// is a preference, and only the people who can already see the booking's
    /// details have any use for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_private: Option<bool>,
    /// Who is at the camp these nights, first name only — present *only*
    /// when [`shows_booker`] says so. Its presence is the entire signal the
    /// calendar keys off: absent means render the stay exactly as an
    /// anonymous "Booked" cell, which is what every stay looked like before
    /// this field existed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_first_name: Option<String>,
}

/// Whether the calendar may name the person on this stay.
///
/// Two audiences, and the difference between them is the whole rule.
///
/// **The people who run the camp** (`sees_guest_details` — admins and anyone
/// flagged `is_owner`) always see who booked, on every stay, whatever its
/// status and whatever the booker chose. The privacy toggle was never aimed
/// at them: it exists so family browsing the calendar cannot see each other,
/// while the owner deciding whether to approve a request obviously has to
/// know whose request it is — they are already reading that name in the
/// approval email, in the admin table, and in `guest_name` on this very
/// response. Withholding it on the calendar alone protected nobody and just
/// made the one view they actually work from the least informative.
///
/// **Everyone else** is unchanged, and all three conditions still have to
/// hold, each ruling out a different way the feature could leak someone:
///
///   * `signed_in` — the public calendar never names anyone. Opting in makes
///     a booker visible to *registered family*, not to the internet, so an
///     anonymous visitor sees the same anonymous "Booked" cell they always
///     have regardless of what the booker chose.
///   * `!is_private` — the booker's own choice, and the one that defaults to
///     withholding.
///   * approved — a request nobody has said yes to yet is not news about who
///     is coming to the camp. Pending stays still block their nights on the
///     calendar; they just do it anonymously until they are real.
///
/// `sees_guest_details` implies `signed_in` (it is read off a `User`), so the
/// staff arm does not repeat that check.
///
/// Pure so the rule is testable as a rule, rather than only reachable through
/// a database and an HTTP request.
fn shows_booker(signed_in: bool, sees_guest_details: bool, is_private: bool, status: &str) -> bool {
    sees_guest_details || (signed_in && !is_private && status == "approved")
}

impl BookingRow {
    /// Projects the row down to what `viewer` may see.
    fn to_view(&self, viewer: Option<&User>) -> BookingView {
        let b = &self.booking;
        let is_mine = viewer.is_some_and(|v| v.id == b.user_id);
        let staff = viewer.is_some_and(User::sees_guest_details);
        let full = is_mine || staff;
        // The party travels with the name: whoever may know who is coming may
        // know how many, and nobody else. `full` covers the booking's own
        // guest and the admins who arbitrate it.
        let named = shows_booker(viewer.is_some(), staff, b.is_private, &b.status);
        let party = full || named;

        BookingView {
            id: b.id,
            check_in: b.check_in,
            check_out: b.check_out,
            status: b.status.clone(),
            guest_count_adults: party.then_some(b.guest_count_adults),
            guest_count_kids: party.then_some(b.guest_count_kids),
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
            is_private: full.then_some(b.is_private),
            // `users::first_name` and not `guest_name`: the journal feed
            // already settled how a guest is named to other people, and a
            // cousin should read as the same "Jean" on the calendar as on
            // their story. It also never falls back to the email the way the
            // admin-facing `guest_name` above does.
            guest_first_name: named.then(|| users::first_name(self.guest_name.as_deref())),
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
    // `sees_guest_details`, not `is_admin`: the camp owner is the person who
    // approves stays, so a calendar that hides pending requests from them
    // hides exactly the rows they are meant to act on. An owner already
    // receives every one of those requests by email, in full — this only
    // catches the app up to what their inbox has always shown them. The flag
    // is the one that matters and an owner need not hold the admin role, so
    // the same helper that decides whether they may see a booker decides
    // whether they are sent the booking at all.
    let staff = viewer.as_ref().is_some_and(User::sees_guest_details);
    let viewer_id = viewer.as_ref().map(|v| v.id);

    // Visibility, expressed once in SQL rather than filtered in Rust:
    //  - admins and the owner see everything
    //  - a guest sees every approved stay plus all of their own rows
    //  - anonymous callers see approved stays only
    let visibility = if staff {
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

// ─────────────────────────── occupancy ───────────────────────────

/// One night, and how many adults are approved to be at the camp for it.
#[derive(Debug, Serialize, FromRow)]
pub struct DayOccupancy {
    pub date: NaiveDate,
    pub adults: i64,
}

/// `GET /api/bookings/occupancy` — public, and deliberately so.
///
/// This is what the calendar's over-capacity warning and the booking form's
/// "camp sleeps N" figure are built from, and it exists because those two
/// features are the reason per-booking head counts used to be readable by
/// everyone. A night's total is a fact about the *camp* — it answers "is
/// there room on the 5th", which is the question a public availability
/// calendar is for. `BookingView`'s counts are a fact about a household, and
/// are gated accordingly.
///
/// The aggregate is not a perfect anonymiser and is not meant to be: on a
/// night with exactly one approved stay, the total is that stay's adult
/// count. That much has been public since the capacity warning shipped, it
/// is inherent to answering the availability question at all, and it still
/// never attaches a number to a name, never reveals how many of a party are
/// children, and never says anything about a stay nobody has approved.
pub async fn occupancy(State(state): State<Shared>) -> ApiResult<Json<Vec<DayOccupancy>>> {
    // One row per occupied night: a stay holds check-in through the night
    // before check-out, which is the same span `nightsOf` walks on the client
    // and the reason the departure day reads as free for the next guest.
    let rows = sqlx::query_as::<_, DayOccupancy>(
        "SELECT night::date AS date, sum(b.guest_count_adults)::bigint AS adults
         FROM bookings b
         CROSS JOIN LATERAL
             generate_series(b.check_in, b.check_out - 1, interval '1 day') AS night
         WHERE b.status = 'approved'
         GROUP BY night
         ORDER BY night",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
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

/// Live bookings whose nights collide with `[check_in, check_out)`.
///
/// Pending counts as live: two cousins asking for the same weekend is exactly
/// the situation worth flagging. `exclude` skips the booking being edited, so
/// moving a stay by a day doesn't report it as overlapping itself — the same
/// reason [`approved_adults`] takes one.
async fn overlapping_bookings(
    db: &PgPool,
    check_in: NaiveDate,
    check_out: NaiveDate,
    exclude: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM bookings
         WHERE status IN ('approved', 'pending') AND check_in < $2 AND check_out > $1
           AND ($3::uuid IS NULL OR id <> $3)",
    )
    .bind(check_in)
    .bind(check_out)
    .bind(exclude)
    .fetch_one(db)
    .await?;
    Ok(count)
}

/// What a proposed set of dates and head count would mean for the camp:
/// who else is already on those nights, and whether the beds add up.
///
/// Advisory, never a veto. Overlaps are deliberately permitted — the camp is
/// shared and the owner arbitrates — and over-capacity is a judgement call
/// that belongs to a person. Both come back as sentences for the submitter to
/// read, not as errors.
///
/// The one implementation of that assessment, shared by
/// [`create_booking_for`] and [`admin_update_booking`] so a guest requesting
/// dates and an admin moving a booking onto those same dates are told the same
/// thing in the same words. `exclude` is what makes it work for an edit: the
/// booking being moved must not be counted among the stays it collides with,
/// or every edit would warn about itself.
async fn assess_stay(
    state: &Shared,
    check_in: NaiveDate,
    check_out: NaiveDate,
    adults: i32,
    exclude: Option<Uuid>,
) -> ApiResult<(Capacity, Vec<String>)> {
    let overlaps = overlapping_bookings(&state.db, check_in, check_out, exclude).await?;
    let already = approved_adults(&state.db, check_in, check_out, exclude).await?;

    let capacity = Capacity {
        approved_adults: already,
        total_adults: already + i64::from(adults),
        limit: state.cfg.capacity_adults,
        over_capacity: already + i64::from(adults) > state.cfg.capacity_adults,
    };

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

    Ok((capacity, warnings))
}

// ─────────────────────────── create ───────────────────────────

/// The default every path that omits the field falls back to. `#[serde(default)]`
/// would give `false` — the exposing answer — so the default is spelled out
/// here, matching `0014_booking_privacy.sql`'s `DEFAULT true`.
fn private_by_default() -> bool {
    true
}

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
    /// Whether to keep the booker's name off other family members' calendars.
    /// Absent means private — see [`private_by_default`].
    #[serde(default = "private_by_default")]
    pub is_private: bool,
}

/// What every write that lands a booking returns — a guest's own request, an
/// admin entering one, and an admin editing one. The same shape on purpose:
/// all three can leave the camp overlapped or over capacity, and all three owe
/// the person who did it the same sentence about it.
#[derive(Debug, Serialize)]
pub struct BookingWriteResponse {
    pub booking: BookingView,
    /// Non-blocking advisory shown on submit. Never an error — see
    /// [`assess_stay`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    pub capacity: Capacity,
}

pub async fn create(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateBooking>,
) -> ApiResult<Json<BookingWriteResponse>> {
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
) -> ApiResult<BookingWriteResponse> {
    // Defence in depth, and the one check that isn't redundant on both paths.
    // A blocked guest submitting their own request never gets this far — the
    // auth extractor turns them away first. But `admin_create` resolves the
    // guest by email through `find_or_create_guest`, so an admin typing a
    // blocked address into the manual-entry form arrives here with a perfectly
    // valid admin session and a blocked `user`. This is the only place a
    // booking row is written, so it is the right place to say no.
    if user.is_blocked() {
        return Err(AppError::Forbidden(
            "That account is blocked and can't have a stay booked. Unblock it first if this \
             booking should go ahead."
                .into(),
        ));
    }
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

    // Advisory only — see `assess_stay`. Nothing below this point can turn an
    // overlap or an over-capacity night into a refusal.
    let (capacity, warnings) = assess_stay(
        state,
        body.check_in,
        body.check_out,
        body.guest_count_adults,
        None,
    )
    .await?;

    let token = new_token();
    let booking = sqlx::query_as::<_, Booking>(&format!(
        "INSERT INTO bookings
           (user_id, check_in, check_out, guest_count_adults, guest_count_kids,
            has_pets, other_requests, is_private, approve_token, approve_token_expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now() + ($10 || ' hours')::interval)
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
    .bind(body.is_private)
    .bind(&token)
    .bind(APPROVE_TOKEN_TTL_HOURS.to_string())
    .fetch_one(&state.db)
    .await?;

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

    Ok(BookingWriteResponse {
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
    /// Same choice, same default as the guest's own form — an admin entering
    /// a booking on someone's behalf is making the decision *for* them, so
    /// the quiet answer has to be the private one here too.
    #[serde(default = "private_by_default")]
    pub is_private: bool,
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
            is_private: admin.is_private,
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
) -> ApiResult<Json<BookingWriteResponse>> {
    let email = body.email.trim();
    if !email.contains('@') || email.len() < 5 {
        return Err(AppError::BadRequest(
            "Enter a valid guest email address.".into(),
        ));
    }

    let guest = users::find_or_create_guest(&state.db, email, body.full_name.as_deref()).await?;

    Ok(Json(create_booking_for(&state, &guest, body.into()).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpdateBooking {
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
    pub guest_count_adults: i32,
    pub guest_count_kids: i32,
}

/// Whether a proposed guest count is one the `bookings` CHECK constraints
/// will accept — `valid_adults` (> 0) and `valid_kids` (>= 0) in
/// `0001_core.sql`. Checked here so a bad number comes back as a plain
/// message instead of a constraint violation.
fn validate_guest_counts(adults: i32, kids: i32) -> ApiResult<()> {
    if adults < 1 {
        return Err(AppError::BadRequest(
            "A booking needs at least one adult.".into(),
        ));
    }
    if kids < 0 {
        return Err(AppError::BadRequest("Kids can't be negative.".into()));
    }
    Ok(())
}

/// The one date rule, mirroring `valid_dates` in `0001_core.sql`
/// (`check_out > check_in`) and worded exactly as `create_booking_for` words
/// it, so a stay cannot be edited into a shape a new booking could not have
/// been submitted in.
///
/// Note what is deliberately *not* here: `create_booking_for` additionally
/// refuses a check-in in the past, and an edit must not. Correcting the dates
/// on a stay that has already happened is a large part of why this tool
/// exists, and a rule meant to stop someone booking backwards would stop the
/// record being put right.
fn validate_stay_dates(check_in: NaiveDate, check_out: NaiveDate) -> ApiResult<()> {
    if check_out <= check_in {
        return Err(AppError::BadRequest(
            "Check-out must be after check-in.".into(),
        ));
    }
    Ok(())
}

/// "2 adults", "1 adult and 1 kid", "3 adults and 2 kids" — kids omitted
/// when there are none, so a notification never reads "and 0 kids".
pub(crate) fn guest_count_phrase(adults: i32, kids: i32) -> String {
    let adults = format!("{adults} {}", if adults == 1 { "adult" } else { "adults" });
    match kids {
        0 => adults,
        1 => format!("{adults} and 1 kid"),
        n => format!("{adults} and {n} kids"),
    }
}

/// Whether the guest is worth emailing about this edit — i.e. whether the
/// numbers actually moved. Re-saving the same figures is a no-op and should
/// stay silent rather than telling someone their booking changed when it did
/// not.
fn guest_counts_changed(before: (i32, i32), after: (i32, i32)) -> bool {
    before != after
}

/// The same question for the dates.
fn dates_changed(before: (NaiveDate, NaiveDate), after: (NaiveDate, NaiveDate)) -> bool {
    before != after
}

/// What an admin's edit actually moved, so the guest is told about that and
/// nothing else.
///
/// The point of carrying both halves in one value is that an edit is *one*
/// action even when it touches two things: a booking whose dates and party
/// both changed earns one email describing both, not one email per field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BookingEdit {
    /// The dates as they were, if they moved.
    pub dates_from: Option<(NaiveDate, NaiveDate)>,
    /// The counts as they were, if they moved.
    pub counts_from: Option<(i32, i32)>,
}

impl BookingEdit {
    /// Compares before and after, keeping only what actually differs.
    fn between(
        before: (NaiveDate, NaiveDate, i32, i32),
        after: (NaiveDate, NaiveDate, i32, i32),
    ) -> Self {
        let (bi, bo, ba, bk) = before;
        let (ai, ao, aa, ak) = after;
        Self {
            dates_from: dates_changed((bi, bo), (ai, ao)).then_some((bi, bo)),
            counts_from: guest_counts_changed((ba, bk), (aa, ak)).then_some((ba, bk)),
        }
    }

    /// Whether anything moved at all. False means say nothing to anybody.
    pub fn is_empty(&self) -> bool {
        self.dates_from.is_none() && self.counts_from.is_none()
    }
}

/// `PUT /api/bookings/{id}/edit` — admin only.
///
/// A data-correction tool, deliberately not gated on status the way the
/// delete is: the reason it exists is that a guest submitted something wrong
/// and cannot fix it themselves, and that is just as true of an approved stay
/// as a pending request. Dates and party size move; status, ownership and
/// everything else are left exactly as they were.
///
/// **Advisory, not blocking.** Unlike `create_booking_for`, which refuses a
/// blackout outright with a 409, this accepts dates that collide with a
/// blackout, overlap another stay, or put the camp over capacity, and reports
/// them. The asymmetry is deliberate and is about who is asking: a guest
/// submitting a request is being told the camp is closed, while an admin
/// moving an existing booking is the person who decides what the camp does.
/// A tool for fixing reality cannot refuse to describe it.
///
/// The guest hears about it once, covering whatever actually changed — see
/// [`BookingEdit`]. A save that moves nothing says nothing.
pub async fn admin_update_booking(
    State(state): State<Shared>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBooking>,
) -> ApiResult<Json<BookingWriteResponse>> {
    validate_stay_dates(body.check_in, body.check_out)?;
    validate_guest_counts(body.guest_count_adults, body.guest_count_kids)?;

    let row = load_row(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found.".into()))?;

    let edit = BookingEdit::between(
        (
            row.booking.check_in,
            row.booking.check_out,
            row.booking.guest_count_adults,
            row.booking.guest_count_kids,
        ),
        (
            body.check_in,
            body.check_out,
            body.guest_count_adults,
            body.guest_count_kids,
        ),
    );

    let booking = sqlx::query_as::<_, Booking>(&format!(
        "UPDATE bookings
         SET check_in = $2, check_out = $3, guest_count_adults = $4, guest_count_kids = $5,
             updated_at = now()
         WHERE id = $1 RETURNING {BOOKING_COLUMNS}"
    ))
    .bind(id)
    .bind(body.check_in)
    .bind(body.check_out)
    .bind(body.guest_count_adults)
    .bind(body.guest_count_kids)
    .fetch_one(&state.db)
    .await?;

    // Excluding this booking, which has already been written: without that it
    // would find itself on its own new dates and warn about overlapping
    // itself.
    let (capacity, warnings) = assess_stay(
        &state,
        booking.check_in,
        booking.check_out,
        booking.guest_count_adults,
        Some(id),
    )
    .await?;

    if !edit.is_empty() {
        email::spawn(
            state.clone(),
            row.guest_email.clone(),
            email_templates::booking_updated_to_guest(&booking, &edit, app_url(&state)),
        );
        notifications::system_message(
            &state.db,
            booking.user_id,
            "Booking updated",
            &email_templates::booking_edit_summary(&booking, &edit),
            Some(booking.id),
        )
        .await?;

        tracing::info!(
            booking_id = %id,
            by = %admin.email,
            dates_from = ?edit.dates_from,
            counts_from = ?edit.counts_from,
            "booking corrected",
        );
    }

    let updated = load_row(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found.".into()))?;

    Ok(Json(BookingWriteResponse {
        booking: updated.to_view(Some(&admin)),
        warning: (!warnings.is_empty()).then(|| warnings.join(" ")),
        capacity,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdatePrivacy {
    pub is_private: bool,
}

/// `PUT /api/bookings/{id}/privacy` — the booker's own switch.
///
/// Deliberately not gated on status, unlike every other write in this module.
/// The rest of them are steps in the approval workflow and only make sense at
/// a particular point in it; this is a standing preference about the person,
/// and someone who decides they would rather not be listed should not have to
/// find out their booking is in the wrong state to say so. Changing it on a
/// pending or cancelled stay simply has no visible effect yet — `shows_booker`
/// already withholds the name until a stay is approved.
///
/// It also, uniquely, notifies nobody: no email, no inbox message. Those exist
/// so a *guest* learns what the camp did to their booking. Here the guest is
/// the one acting, on their own row, and telling them what they just did — or
/// telling the owner, who does not need a feed of who is feeling private this
/// week — would be noise.
///
/// Admins may set it too, for the booking they entered on someone's behalf;
/// the ownership check below is the same one `cancel` uses.
pub async fn update_privacy(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePrivacy>,
) -> ApiResult<Json<BookingView>> {
    let row = load_row(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found.".into()))?;

    if row.booking.user_id != user.id && !user.is_admin() {
        return Err(AppError::Forbidden("That isn't your booking.".into()));
    }

    let booking = sqlx::query_as::<_, Booking>(&format!(
        "UPDATE bookings SET is_private = $2, updated_at = now()
         WHERE id = $1 RETURNING {BOOKING_COLUMNS}"
    ))
    .bind(id)
    .bind(body.is_private)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(
        booking_id = %id,
        is_private = body.is_private,
        "booking privacy changed",
    );

    Ok(Json(BookingRow { booking, ..row }.to_view(Some(&user))))
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

/// Whether a booking in this status may be hard-deleted.
///
/// Approved is the one status held back, and it is the whole point of the
/// gate: an approved booking is a real, confirmed stay, and every other part
/// of this app preserves confirmed history rather than erasing it — journal
/// entries archive instead of deleting, accounts with any history block
/// instead of deleting, and cancelling a booking keeps the row and changes its
/// status. A hard delete here would be the one place that breaks the pattern.
///
/// The listed statuses are the rest of `bookings_status_valid` in
/// `0001_core.sql`; that CHECK constraint is what keeps this exhaustive.
fn is_deletable_status(status: &str) -> bool {
    matches!(status, "pending" | "denied" | "cancelled")
}

/// What a delete took with it, so the panel can confirm what actually went
/// rather than just saying "done".
#[derive(Debug, Serialize)]
pub struct DeleteSummary {
    pub deleted: bool,
    /// Whether a `crate::journal` entry was removed alongside the booking.
    pub journal_entry: bool,
    /// Whether a `crate::checkout` record was removed alongside it.
    pub checkout: bool,
}

/// `DELETE /bookings/{id}` — removes a booking outright. Admins only.
///
/// Cancel is the everyday tool and the reversible one: it keeps the row, so a
/// stay that was really booked stays on the record even once it is called off.
/// This is for the narrower case where the row should never have existed —
/// test data, a duplicate — and filing it as "cancelled" would just leave
/// clutter that reads like history.
///
/// The booking's checkout record and journal entry go with it, in one
/// transaction. Both foreign keys are plain `REFERENCES` with no `ON DELETE`,
/// so the delete fails on them otherwise, and neither outlives its booking in
/// any meaningful way: a checkout is the record of that stay ending, and
/// `journal_entries.booking_id` is `NOT NULL UNIQUE`, so an entry has nowhere
/// left to belong once the stay is gone. Messages are the exception and are
/// left alone — `messages.booking_id` is `ON DELETE SET NULL`, so a
/// conversation survives with the link cleared, which is right: what was said
/// stands on its own.
///
/// Approved bookings are refused outright — see `is_deletable_status`. The way
/// to remove a confirmed stay is to cancel it first, which is unchanged and
/// still emails the owner; the cancelled row is then eligible here like any
/// other.
pub async fn admin_delete(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DeleteSummary>> {
    let row = load_row(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Booking not found.".into()))?;

    if !is_deletable_status(&row.booking.status) {
        return Err(AppError::Conflict(
            "Approved bookings can't be deleted directly — cancel it first if \
             it needs to be removed."
                .into(),
        ));
    }

    let mut tx = state.db.begin().await?;
    // Children first, for the foreign keys named above.
    sqlx::query("DELETE FROM journal_entries WHERE booking_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM booking_checkouts WHERE booking_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM bookings WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    tracing::info!(
        booking_id = %id,
        guest = %row.guest_email,
        check_in = %row.booking.check_in,
        status = %row.booking.status,
        "booking deleted",
    );

    Ok(Json(DeleteSummary {
        deleted: true,
        journal_entry: row.journal_id.is_some(),
        checkout: row.checked_out,
    }))
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
    // The gate that keeps confirmed history from being erased. `approved` is
    // the only status this must ever refuse, and the only one it does.
    // The silence rule: re-saving the same numbers must not tell a guest
    // their booking changed.
    #[test]
    fn the_guest_count_phrase_pluralises_and_drops_zero_kids() {
        assert_eq!(guest_count_phrase(2, 0), "2 adults");
        assert_eq!(guest_count_phrase(1, 0), "1 adult");
        assert_eq!(guest_count_phrase(3, 1), "3 adults and 1 kid");
        assert_eq!(guest_count_phrase(1, 4), "1 adult and 4 kids");
    }

    #[test]
    fn re_saving_the_same_counts_notifies_nobody() {
        assert!(!guest_counts_changed((2, 1), (2, 1)));
        assert!(!guest_counts_changed((4, 0), (4, 0)));
    }

    #[test]
    fn moving_either_count_notifies_the_guest() {
        assert!(guest_counts_changed((2, 1), (3, 1)));
        assert!(guest_counts_changed((2, 1), (2, 0)));
        assert!(guest_counts_changed((2, 1), (5, 4)));
    }

    // Mirrors `valid_adults` / `valid_kids` in 0001_core.sql, so a bad number
    // is a message rather than a constraint violation.
    #[test]
    fn guest_counts_must_satisfy_the_database_constraints() {
        assert!(validate_guest_counts(1, 0).is_ok());
        assert!(validate_guest_counts(12, 6).is_ok());
        assert!(matches!(
            validate_guest_counts(0, 2),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            validate_guest_counts(2, -1),
            Err(AppError::BadRequest(_))
        ));
    }

    // ── who the calendar may name ──
    //
    // `shows_booker` is the whole rule. The family arm is spelled out first
    // — each test pinning one of the three ways it can say no — and the
    // staff arm after it, since the two audiences are the point.
    //
    // Every family-arm case below passes `sees_guest_details: false`, which
    // is what makes them a regression net: if the staff bypass ever leaked
    // into the ordinary path, these would start returning true.

    #[test]
    fn an_approved_public_booking_is_named_to_a_signed_in_viewer() {
        assert!(shows_booker(true, false, false, "approved"));
    }

    #[test]
    fn a_private_booking_is_never_named_to_ordinary_family() {
        assert!(!shows_booker(true, false, true, "approved"));
        assert!(!shows_booker(false, false, true, "approved"));
    }

    // The public calendar is not what anyone opted in to. Choosing to be
    // visible to registered family must not make someone visible to a
    // stranger who never signed in.
    #[test]
    fn an_anonymous_visitor_is_never_shown_a_name_however_public_the_booking() {
        assert!(!shows_booker(false, false, false, "approved"));
    }

    // A request the owner has not said yes to yet is not news about who is
    // coming; it still blocks its nights, just anonymously.
    #[test]
    fn a_booking_that_is_not_yet_approved_is_not_named_to_ordinary_family() {
        for status in ["pending", "denied", "cancelled"] {
            assert!(!shows_booker(true, false, false, status));
        }
    }

    // ── the staff arm ──

    // Admins and the owner run the camp: no status gate, no privacy gate.
    // They are already reading this name in the approval email and the admin
    // table, so withholding it on the calendar protected nobody.
    #[test]
    fn staff_are_named_every_booking_whatever_its_status_or_privacy() {
        for status in ["approved", "pending", "denied", "cancelled"] {
            for is_private in [true, false] {
                assert!(
                    shows_booker(true, true, is_private, status),
                    "staff must see {status} / private={is_private}",
                );
            }
        }
    }

    // The bypass is a property of the viewer, not of the booking: the very
    // same rows that open up for staff stay shut for a family member.
    #[test]
    fn the_staff_bypass_does_not_widen_what_family_can_see() {
        for status in ["approved", "pending"] {
            assert!(shows_booker(true, true, true, status));
            assert!(!shows_booker(true, false, true, status));
        }
    }

    // The default is the one that matters most: a booking created without the
    // field, by any path, must land private. `private_by_default` is what the
    // serde defaults on both request bodies resolve to, and it mirrors the
    // column's own `DEFAULT true`.
    #[test]
    fn omitting_the_choice_entirely_yields_a_private_booking() {
        assert!(private_by_default());
        let submitted: CreateBooking = serde_json::from_str(
            r#"{"check_in":"2026-10-01","check_out":"2026-10-05","guest_count_adults":2}"#,
        )
        .expect("a form that predates the toggle still deserialises");
        assert!(submitted.is_private);
        assert!(!shows_booker(true, false, submitted.is_private, "approved"));
    }

    #[test]
    fn an_admin_entering_a_booking_also_defaults_it_to_private() {
        let entered: AdminCreateBooking = serde_json::from_str(
            r#"{"email":"guest@example.com","check_in":"2026-10-01","check_out":"2026-10-05","guest_count_adults":2}"#,
        )
        .expect("the admin form's older shape still deserialises");
        assert!(entered.is_private);
        assert!(CreateBooking::from(entered).is_private);
    }

    // The party breakdown travels with the name, and is withheld from
    // everyone else — the gap this closes was that it used to ship to every
    // caller regardless. `party` in `to_view` is `full || shows_booker`, so
    // the cases below are exactly the ones where `shows_booker` decides.
    #[test]
    fn the_party_breakdown_is_withheld_from_whoever_may_not_be_told_the_name() {
        for (signed_in, is_private, status) in [
            (true, true, "approved"),   // private: a signed-in cousin
            (false, false, "approved"), // public, but an anonymous visitor
            (true, false, "pending"),   // public, but not approved yet
        ] {
            assert!(
                !shows_booker(signed_in, false, is_private, status),
                "{signed_in}/{is_private}/{status} must not be named, and so must not \
                 carry a head count either",
            );
        }
    }

    // ── what an edit reports as having changed ──
    //
    // `BookingEdit::between` is the whole of the no-op rule, and the reason
    // one save can never produce two emails: it yields a single value
    // describing everything that moved.

    fn edit(before: (i32, i32, i32, i32), after: (i32, i32, i32, i32)) -> BookingEdit {
        let d = |y: i32, m: u32| NaiveDate::from_ymd_opt(2027, m, y as u32).unwrap();
        BookingEdit::between(
            (d(before.0, 1), d(before.1, 1), before.2, before.3),
            (d(after.0, 1), d(after.1, 1), after.2, after.3),
        )
    }

    #[test]
    fn moving_only_the_dates_reports_only_the_dates() {
        let e = edit((5, 8, 2, 1), (12, 15, 2, 1));
        assert!(e.dates_from.is_some());
        assert!(e.counts_from.is_none());
        assert!(!e.is_empty());
    }

    #[test]
    fn moving_only_the_counts_reports_only_the_counts() {
        let e = edit((5, 8, 2, 1), (5, 8, 4, 0));
        assert!(e.dates_from.is_none());
        assert!(e.counts_from.is_some());
        assert!(!e.is_empty());
    }

    // One action, one value describing it — so one email, never two.
    #[test]
    fn moving_both_reports_both_in_a_single_edit() {
        let e = edit((5, 8, 2, 1), (12, 15, 4, 0));
        assert_eq!(
            e.dates_from,
            Some((
                NaiveDate::from_ymd_opt(2027, 1, 5).unwrap(),
                NaiveDate::from_ymd_opt(2027, 1, 8).unwrap(),
            ))
        );
        assert_eq!(e.counts_from, Some((2, 1)));
        assert!(!e.is_empty());
    }

    // The silence rule, now covering both halves: re-saving a booking exactly
    // as it was must not tell anyone it changed.
    #[test]
    fn re_saving_an_unchanged_booking_notifies_nobody() {
        let e = edit((5, 8, 2, 1), (5, 8, 2, 1));
        assert!(e.is_empty());
        assert_eq!(
            e,
            BookingEdit {
                dates_from: None,
                counts_from: None
            }
        );
    }

    // Either end of the range moving on its own still counts.
    #[test]
    fn shifting_just_one_end_of_the_stay_counts_as_a_date_change() {
        assert!(edit((5, 8, 2, 1), (5, 9, 2, 1)).dates_from.is_some());
        assert!(edit((5, 8, 2, 1), (4, 8, 2, 1)).dates_from.is_some());
    }

    // ── date validation on an edit ──

    #[test]
    fn an_edit_cannot_end_a_stay_before_or_when_it_starts() {
        let jan = |d: u32| NaiveDate::from_ymd_opt(2027, 1, d).unwrap();
        assert!(validate_stay_dates(jan(5), jan(8)).is_ok());
        assert!(matches!(
            validate_stay_dates(jan(8), jan(5)),
            Err(AppError::BadRequest(_))
        ));
        // Equal is rejected too — `valid_dates` in 0001_core.sql is a strict
        // `>`, so there is no such thing as a zero-night stay here.
        assert!(matches!(
            validate_stay_dates(jan(5), jan(5)),
            Err(AppError::BadRequest(_))
        ));
    }

    // An edit must be able to fix a stay that already happened, which is why
    // it does not inherit `create_booking_for`'s no-check-in-in-the-past rule.
    #[test]
    fn an_edit_may_move_a_stay_that_is_already_in_the_past() {
        let past_in = NaiveDate::from_ymd_opt(2020, 6, 1).unwrap();
        let past_out = NaiveDate::from_ymd_opt(2020, 6, 4).unwrap();
        assert!(validate_stay_dates(past_in, past_out).is_ok());
    }

    #[test]
    fn an_approved_booking_cannot_be_deleted() {
        assert!(!is_deletable_status("approved"));
    }

    #[test]
    fn a_booking_that_is_not_a_confirmed_stay_can_be_deleted() {
        assert!(is_deletable_status("pending"));
        assert!(is_deletable_status("denied"));
        assert!(is_deletable_status("cancelled"));
    }

    // An admin removing a confirmed stay is meant to cancel it first; this is
    // the handoff between the two, and it only works because cancel leaves the
    // row in a status the delete will accept.
    #[test]
    fn cancelling_a_confirmed_stay_makes_it_deletable() {
        assert!(!is_deletable_status("approved"));
        assert!(is_deletable_status("cancelled"));
    }

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
            is_private: false,
        };
        let expected = CreateBooking {
            check_in: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            check_out: NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
            guest_count_adults: 3,
            guest_count_kids: 2,
            has_pets: true,
            other_requests: Some("Bringing a boat trailer".into()),
            is_private: false,
        };
        assert_eq!(CreateBooking::from(admin_input), expected);
    }
}
