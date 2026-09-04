//! The camp journal: free-form guest stories tied 1:1 to a completed stay.
//! Deliberately not a review system — no ratings, no stars, anywhere in
//! this module or its responses.

use crate::{
    ApiResult, AppError, Shared,
    auth::{AdminUser, AuthUser},
    email, email_templates, notifications, users,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

fn app_url(state: &Shared) -> &str {
    state.cfg.frontend_url.trim_end_matches('/')
}

/// First token of a full name, for the public feed — never the email.
fn first_name(full_name: Option<&str>) -> String {
    full_name
        .and_then(|n| n.split_whitespace().next())
        .unwrap_or("A guest")
        .to_string()
}

/// Whether a booking qualifies to receive a journal entry: it has a
/// *completed checkout* — not merely a past `check_out` date — and no
/// entry exists for it yet. Pure mirror of the rule `create()` and
/// `eligible_bookings()` both enforce, kept here as the tested spec of it.
pub fn is_journal_eligible(has_completed_checkout: bool, has_existing_entry: bool) -> bool {
    has_completed_checkout && !has_existing_entry
}

/// `create()` calls this with the checkout-existence check it already had
/// to make — a past check-out date alone is never enough.
fn require_checked_out(checked_out: bool) -> ApiResult<()> {
    if !checked_out {
        return Err(AppError::BadRequest(
            "You can share a story once you've completed checkout for this stay.".into(),
        ));
    }
    Ok(())
}

/// Guards against journaling the same stay twice.
fn require_no_existing_entry(already: bool) -> ApiResult<()> {
    if already {
        return Err(AppError::Conflict(
            "You've already shared a story for this stay.".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct JournalEntry {
    pub id: Uuid,
    pub user_id: Uuid,
    pub booking_id: Uuid,
    pub title: String,
    pub body: String,
    pub status: String,
    pub rejected_reason: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const ENTRY_COLUMNS: &str = "id, user_id, booking_id, title, body, status, rejected_reason, \
                             approved_at, approved_by, created_at, updated_at";

// ─────────────────────────── public feed ───────────────────────────

#[derive(Debug, Serialize)]
pub struct PublicEntry {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub guest_first_name: String,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
}

#[derive(FromRow)]
struct PublicRow {
    id: Uuid,
    title: String,
    body: String,
    created_at: DateTime<Utc>,
    approved_at: Option<DateTime<Utc>>,
    full_name: Option<String>,
    check_in: NaiveDate,
    check_out: NaiveDate,
}

const PAGE_SIZE: i64 = 10;

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    pub page: Option<i64>,
}

/// `GET /api/journal` — public, no auth. Approved entries only.
pub async fn list_public(
    State(state): State<Shared>,
    Query(q): Query<PageQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PAGE_SIZE;

    let rows = sqlx::query_as::<_, PublicRow>(
        "SELECT je.id, je.title, je.body, je.created_at, je.approved_at,
                u.full_name, b.check_in, b.check_out
         FROM journal_entries je
         JOIN users u ON u.id = je.user_id
         JOIN bookings b ON b.id = je.booking_id
         WHERE je.status = 'approved'
         ORDER BY je.approved_at DESC NULLS LAST, je.created_at DESC
         LIMIT $1 OFFSET $2",
    )
    .bind(PAGE_SIZE)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let (total,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM journal_entries WHERE status = 'approved'")
            .fetch_one(&state.db)
            .await?;

    let entries: Vec<PublicEntry> = rows
        .into_iter()
        .map(|r| PublicEntry {
            id: r.id,
            title: r.title,
            body: r.body,
            created_at: r.created_at,
            approved_at: r.approved_at,
            guest_first_name: first_name(r.full_name.as_deref()),
            check_in: r.check_in,
            check_out: r.check_out,
        })
        .collect();

    Ok(Json(json!({
        "entries": entries,
        "page": page,
        "page_size": PAGE_SIZE,
        "total": total,
        "total_pages": (total as f64 / PAGE_SIZE as f64).ceil().max(1.0) as i64,
    })))
}

// ─────────────────────────── guest ───────────────────────────

/// `GET /api/journal/mine` — every entry the caller has ever submitted,
/// any status.
pub async fn list_mine(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<JournalEntry>>> {
    let rows = sqlx::query_as::<_, JournalEntry>(&format!(
        "SELECT {ENTRY_COLUMNS} FROM journal_entries WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Serialize, FromRow)]
pub struct EligibleBooking {
    pub booking_id: Uuid,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
}

/// `GET /api/journal/eligible-bookings` — checked-out stays with no
/// journal entry yet. Drives `/journal/new`.
pub async fn eligible_bookings(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<EligibleBooking>>> {
    let rows = sqlx::query_as::<_, EligibleBooking>(
        "SELECT b.id AS booking_id, b.check_in, b.check_out
         FROM bookings b
         JOIN booking_checkouts bc ON bc.booking_id = b.id
         WHERE b.user_id = $1
           AND NOT EXISTS (SELECT 1 FROM journal_entries je WHERE je.booking_id = b.id)
         ORDER BY b.check_out DESC",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct CreateEntry {
    pub booking_id: Uuid,
    pub title: String,
    pub body: String,
}

/// `POST /api/journal`. Eligibility deliberately checks for a *completed
/// checkout*, not merely a past check-out date — a stay that hasn't been
/// checked out yet can't be journaled about even once departure has passed.
pub async fn create(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateEntry>,
) -> ApiResult<Json<JournalEntry>> {
    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("Title is required.".into()));
    }
    let story = body.body.trim();
    if story.is_empty() {
        return Err(AppError::BadRequest("Story can't be empty.".into()));
    }

    let booking: Option<(NaiveDate, NaiveDate)> =
        sqlx::query_as("SELECT check_in, check_out FROM bookings WHERE id = $1 AND user_id = $2")
            .bind(body.booking_id)
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;
    let Some((check_in, check_out)) = booking else {
        return Err(AppError::NotFound("Booking not found.".into()));
    };

    let (checked_out,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM booking_checkouts WHERE booking_id = $1)")
            .bind(body.booking_id)
            .fetch_one(&state.db)
            .await?;
    require_checked_out(checked_out)?;

    let (already,): (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE booking_id = $1)")
            .bind(body.booking_id)
            .fetch_one(&state.db)
            .await?;
    require_no_existing_entry(already)?;

    let entry = sqlx::query_as::<_, JournalEntry>(&format!(
        "INSERT INTO journal_entries (user_id, booking_id, title, body)
         VALUES ($1, $2, $3, $4) RETURNING {ENTRY_COLUMNS}"
    ))
    .bind(user.id)
    .bind(body.booking_id)
    .bind(title)
    .bind(story)
    .fetch_one(&state.db)
    .await?;

    let guest = user.display_name();
    email::spawn_opt(
        state.clone(),
        state.cfg.admin_email.clone(),
        email_templates::journal_submitted_to_admin(
            &guest,
            check_in,
            check_out,
            title,
            app_url(&state),
        ),
    );
    if let Some(admin) = users::first_admin(&state.db).await? {
        notifications::system_message(
            &state.db,
            admin.id,
            "New journal entry",
            &format!("{guest} submitted a journal entry: \"{title}\"."),
            Some(body.booking_id),
        )
        .await?;
    }

    Ok(Json(entry))
}

#[derive(Debug, Deserialize)]
pub struct UpdateEntry {
    pub title: String,
    pub body: String,
}

/// `PUT /api/journal/{id}` — owner only, and only while still pending;
/// once reviewed, the story is locked.
pub async fn update(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateEntry>,
) -> ApiResult<Json<JournalEntry>> {
    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("Title is required.".into()));
    }
    let story = body.body.trim();
    if story.is_empty() {
        return Err(AppError::BadRequest("Story can't be empty.".into()));
    }

    let existing: Option<(Uuid, String)> =
        sqlx::query_as("SELECT user_id, status FROM journal_entries WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some((owner_id, status)) = existing else {
        return Err(AppError::NotFound("Journal entry not found.".into()));
    };
    if owner_id != user.id {
        return Err(AppError::Forbidden("That isn't your journal entry.".into()));
    }
    if status != "pending" {
        return Err(AppError::Conflict(
            "This entry has already been reviewed and can no longer be edited.".into(),
        ));
    }

    let entry = sqlx::query_as::<_, JournalEntry>(&format!(
        "UPDATE journal_entries SET title = $2, body = $3, updated_at = now()
         WHERE id = $1 RETURNING {ENTRY_COLUMNS}"
    ))
    .bind(id)
    .bind(title)
    .bind(story)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(entry))
}

/// `DELETE /api/journal/{id}` — the entry's owner (any status) or an admin.
pub async fn remove(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM journal_entries WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let Some((owner_id,)) = existing else {
        return Err(AppError::NotFound("Journal entry not found.".into()));
    };
    if owner_id != user.id && !user.is_admin() {
        return Err(AppError::Forbidden("That isn't your journal entry.".into()));
    }

    sqlx::query("DELETE FROM journal_entries WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "deleted": true })))
}

// ─────────────────────────── admin ───────────────────────────

#[derive(Debug, Serialize, FromRow)]
pub struct AdminEntry {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub status: String,
    pub rejected_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub guest_name: Option<String>,
    pub guest_email: String,
    pub check_in: NaiveDate,
    pub check_out: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct AdminListQuery {
    pub status: Option<String>,
}

/// `GET /api/journal/admin` — every entry, filterable by status, pending
/// first (same "surface what needs attention" ordering as the bookings tab).
pub async fn admin_list(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Query(q): Query<AdminListQuery>,
) -> ApiResult<Json<Vec<AdminEntry>>> {
    let mut sql = "SELECT je.id, je.title, je.body, je.status, je.rejected_reason,
                          je.created_at, je.approved_at, je.approved_by,
                          u.full_name AS guest_name, u.email AS guest_email,
                          b.check_in, b.check_out
                   FROM journal_entries je
                   JOIN users u ON u.id = je.user_id
                   JOIN bookings b ON b.id = je.booking_id"
        .to_string();
    if q.status.is_some() {
        sql.push_str(" WHERE je.status = $1");
    }
    sql.push_str(" ORDER BY (je.status = 'pending') DESC, je.created_at DESC");

    let rows = sqlx::query_as::<_, AdminEntry>(&sql)
        .bind(q.status.as_deref())
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

#[derive(FromRow)]
struct EntryWithGuestEmail {
    #[sqlx(flatten)]
    entry: JournalEntry,
    guest_email: String,
}

async fn load_entry(db: &PgPool, id: Uuid) -> Result<Option<EntryWithGuestEmail>, sqlx::Error> {
    sqlx::query_as::<_, EntryWithGuestEmail>(&format!(
        "SELECT {cols}, u.email AS guest_email
         FROM journal_entries je JOIN users u ON u.id = je.user_id WHERE je.id = $1",
        cols = ENTRY_COLUMNS
            .split(", ")
            .map(|c| format!("je.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
    .bind(id)
    .fetch_optional(db)
    .await
}

fn require_pending(entry: &JournalEntry) -> ApiResult<()> {
    if entry.status != "pending" {
        return Err(AppError::Conflict(format!(
            "This entry is already {}.",
            entry.status
        )));
    }
    Ok(())
}

/// `PUT /api/journal/{id}/approve`.
pub async fn approve(
    State(state): State<Shared>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<JournalEntry>> {
    let row = load_entry(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Journal entry not found.".into()))?;
    require_pending(&row.entry)?;

    let entry = sqlx::query_as::<_, JournalEntry>(&format!(
        "UPDATE journal_entries
         SET status = 'approved', approved_at = now(), approved_by = $2,
             rejected_reason = NULL, updated_at = now()
         WHERE id = $1 RETURNING {ENTRY_COLUMNS}"
    ))
    .bind(id)
    .bind(&admin.email)
    .fetch_one(&state.db)
    .await?;

    email::spawn(
        state.clone(),
        row.guest_email,
        email_templates::journal_approved_to_guest(app_url(&state)),
    );
    notifications::system_message(
        &state.db,
        entry.user_id,
        "Your story is live!",
        "Your camp journal entry has been approved and is now posted.",
        Some(entry.booking_id),
    )
    .await?;

    Ok(Json(entry))
}

#[derive(Debug, Deserialize)]
pub struct RejectBody {
    pub reason: Option<String>,
}

/// `PUT /api/journal/{id}/reject`.
pub async fn reject(
    State(state): State<Shared>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RejectBody>,
) -> ApiResult<Json<JournalEntry>> {
    let row = load_entry(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("Journal entry not found.".into()))?;
    require_pending(&row.entry)?;
    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty());

    let entry = sqlx::query_as::<_, JournalEntry>(&format!(
        "UPDATE journal_entries
         SET status = 'rejected', rejected_reason = $2, approved_by = $3,
             approved_at = NULL, updated_at = now()
         WHERE id = $1 RETURNING {ENTRY_COLUMNS}"
    ))
    .bind(id)
    .bind(reason)
    .bind(&admin.email)
    .fetch_one(&state.db)
    .await?;

    email::spawn(
        state.clone(),
        row.guest_email,
        email_templates::journal_rejected_to_guest(reason, app_url(&state)),
    );
    notifications::system_message(
        &state.db,
        entry.user_id,
        "About your journal entry",
        &match reason {
            Some(r) => format!("Your camp journal entry wasn't posted. {r}"),
            None => "Your camp journal entry wasn't posted. Feel free to submit again!".to_string(),
        },
        Some(entry.booking_id),
    )
    .await?;

    Ok(Json(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_eligibility_requires_a_completed_checkout() {
        // A past check_out date isn't part of this function's inputs at
        // all — checked_out here means "has a booking_checkouts row",
        // not "has departed". That's the point of the rule.
        assert!(!is_journal_eligible(false, false));
    }

    #[test]
    fn journal_eligibility_with_a_completed_checkout_and_no_entry() {
        assert!(is_journal_eligible(true, false));
    }

    #[test]
    fn journal_eligibility_excludes_a_stay_that_already_has_an_entry() {
        assert!(!is_journal_eligible(true, true));
    }

    #[test]
    fn require_checked_out_blocks_journaling_before_checkout() {
        assert!(require_checked_out(true).is_ok());
        assert!(matches!(
            require_checked_out(false),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn duplicate_journal_submission_is_blocked() {
        assert!(require_no_existing_entry(false).is_ok());
        assert!(matches!(
            require_no_existing_entry(true),
            Err(AppError::Conflict(_))
        ));
    }

    #[test]
    fn first_name_takes_only_the_first_token() {
        assert_eq!(first_name(Some("Jean Dugas")), "Jean");
        assert_eq!(first_name(Some("Cher")), "Cher");
        assert_eq!(first_name(None), "A guest");
        assert_eq!(first_name(Some("  ")), "A guest");
    }
}
