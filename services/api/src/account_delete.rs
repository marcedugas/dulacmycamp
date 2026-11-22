//! Permanent deletion of a guest or family account, by an admin.
//!
//! Block is the reversible "deactivate"; this is the other tool, for
//! test accounts and for honouring "delete my information" requests. It removes
//! the account and everything that is *about* them, in one transaction.
//!
//! # What "about them" means
//!
//! Beyond the rows that point at the user, their bookings leave a trail in other
//! people's tables: the booking-request, journal-post and checkout-flag
//! notices written to an *admin's* inbox are sent by nobody (`sender_id` NULL)
//! and carry the guest's name in the body, linked only through `booking_id`.
//! Deleting just "messages sent or received by the user" would leave those
//! behind, so messages are matched on the user's bookings as well.
//!
//! # Order
//!
//! Dictated by the foreign keys (see migration notes in `0006`): `booking_checkouts`
//! and `journal_entries` reference bookings and users with plain `REFERENCES`
//! (no cascade), so they must go first; messages must go *before* their
//! bookings, because `messages.booking_id` is `ON DELETE SET NULL` and the link
//! is how the admin-inbox notices are found; bookings then user last.
//!
//! # Privacy
//!
//! Nothing identifying the deleted person is logged or audited: not email, not
//! name, not phone, not address, not file names. Counts only.

use crate::{
    ApiResult, AppError, Shared,
    auth::AdminUser,
    uploads,
    users::{self, AuditAction, User},
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

/// The user's bookings, as a sub-select. `$1` is the user id throughout.
const BOOKINGS_OF: &str = "(SELECT id FROM bookings WHERE user_id = $1)";
/// Stories (journal entries) that go with the account: theirs, or attached to
/// one of their stays.
const STORY: &str =
    "(e.user_id = $1 OR e.booking_id IN (SELECT id FROM bookings WHERE user_id = $1))";
/// Every message the account sent, received, or that is about one of its stays.
const MESSAGE: &str = "(m.sender_id = $1 OR m.recipient_id = $1 \
                       OR m.booking_id IN (SELECT id FROM bookings WHERE user_id = $1))";

/// Counts only — no personal content.
#[derive(Debug, Serialize, FromRow)]
pub struct Counts {
    pub bookings_pending: i64,
    pub bookings_approved: i64,
    pub bookings_denied: i64,
    pub bookings_cancelled: i64,
    /// Pending or approved stays that have not finished yet. Approved ones
    /// disappear from the calendar.
    pub bookings_future: i64,
    pub messages: i64,
    pub stories: i64,
    pub catches: i64,
    pub photos: i64,
}

impl Counts {
    fn bookings_total(&self) -> i64 {
        self.bookings_pending
            + self.bookings_approved
            + self.bookings_denied
            + self.bookings_cancelled
    }

    /// The audit detail: what was removed, never who.
    fn summary(&self, role: &str) -> String {
        format!(
            "{} account, {} bookings ({} approved), {} messages, {} stories, {} photos",
            role_label(role),
            self.bookings_total(),
            self.bookings_approved,
            self.messages,
            self.stories,
            self.photos,
        )
    }
}

fn role_label(role: &str) -> &'static str {
    match role {
        "user" => "family",
        "admin" => "admin",
        _ => "guest",
    }
}

async fn counts(conn: &mut PgConnection, id: Uuid) -> Result<Counts, sqlx::Error> {
    sqlx::query_as::<_, Counts>(&format!(
        "SELECT
           (SELECT count(*) FROM bookings WHERE user_id = $1 AND status = 'pending')   AS bookings_pending,
           (SELECT count(*) FROM bookings WHERE user_id = $1 AND status = 'approved')  AS bookings_approved,
           (SELECT count(*) FROM bookings WHERE user_id = $1 AND status = 'denied')    AS bookings_denied,
           (SELECT count(*) FROM bookings WHERE user_id = $1 AND status = 'cancelled') AS bookings_cancelled,
           (SELECT count(*) FROM bookings WHERE user_id = $1
              AND status IN ('pending', 'approved') AND check_out >= current_date)     AS bookings_future,
           (SELECT count(*) FROM messages m WHERE {MESSAGE}) AS messages,
           (SELECT count(*) FROM journal_entries e WHERE {STORY}) AS stories,
           (SELECT count(*) FROM journal_catches c
              JOIN journal_entries e ON e.id = c.journal_entry_id WHERE {STORY}) AS catches,
           (SELECT count(*) FROM journal_photos p
              JOIN journal_entries e ON e.id = p.journal_entry_id WHERE {STORY}) AS photos"
    ))
    .bind(id)
    .fetch_one(conn)
    .await
}

/// Why this account may not be deleted, or `None` if it may.
///
/// Admins and owners are refused rather than deleted: demote first, so the
/// intent is explicit and the site can never be left without anyone able to run
/// it. Because those roles are refused outright, deleting a guest or family
/// account can never reduce the number of admins or owners — the "never leave
/// zero admins" rule is satisfied by construction, not by a separate count.
pub fn refusal(actor_id: Uuid, target: &User) -> Option<&'static str> {
    if actor_id == target.id {
        return Some("You can't delete your own account.");
    }
    if target.is_admin() || target.is_owner {
        return Some("This is an admin or owner account. Remove that role first, then delete it.");
    }
    None
}

#[derive(Debug, Serialize)]
pub struct DeletePreview {
    pub role: String,
    /// Set when the delete would be refused; the dialog shows it instead of
    /// offering the button.
    pub refusal: Option<&'static str>,
    #[serde(flatten)]
    pub counts: Counts,
}

/// `GET /users/{id}/delete-preview` — what a delete would remove, as counts.
pub async fn preview(
    State(state): State<Shared>,
    AdminUser(actor): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DeletePreview>> {
    let target = users::find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found.".into()))?;
    let mut conn = state.db.acquire().await?;
    let counts = counts(&mut conn, id).await?;
    Ok(Json(DeletePreview {
        role: target.role.clone(),
        refusal: refusal(actor.id, &target),
        counts,
    }))
}

// No `Debug`: carries an address.
#[derive(Deserialize)]
pub struct DeleteBody {
    /// The account's email, typed by the admin as confirmation.
    pub confirm_email: String,
}

/// `DELETE /users/{id}` — permanent and complete, in one transaction.
pub async fn delete_user(
    State(state): State<Shared>,
    AdminUser(actor): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DeleteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut tx = state.db.begin().await?;

    // Locked and re-read inside the transaction: a role change landing between
    // the check and the delete must not slip an admin through.
    let target = sqlx::query_as::<_, User>(&format!(
        "SELECT {} FROM users WHERE id = $1 FOR UPDATE",
        users::USER_COLUMNS
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found.".into()))?;

    if let Some(why) = refusal(actor.id, &target) {
        return Err(AppError::BadRequest(why.into()));
    }
    if body.confirm_email.trim().to_lowercase() != target.email.to_lowercase() {
        return Err(AppError::BadRequest(
            "The email you typed doesn't match this account. Nothing was deleted.".into(),
        ));
    }

    let counts = counts(&mut tx, id).await?;

    // Files are collected now: once the rows are gone nothing says whose they were.
    let mut urls: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT p.url FROM journal_photos p
           JOIN journal_entries e ON e.id = p.journal_entry_id WHERE {STORY}"
    ))
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    urls.extend(
        target
            .avatar_url
            .clone()
            .filter(|u| u.starts_with("/uploads/")),
    );

    // Children before parents — see the module docs for why this order.
    sqlx::query(&format!("DELETE FROM journal_entries e WHERE {STORY}"))
        .bind(id)
        .execute(&mut *tx)
        .await?; // catches and photo rows cascade
    sqlx::query(&format!(
        "DELETE FROM booking_checkouts WHERE user_id = $1 OR booking_id IN {BOOKINGS_OF}"
    ))
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(&format!("DELETE FROM messages m WHERE {MESSAGE}"))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // `approved_by` records the approving admin's email on *other* people's
    // bookings. A demoted ex-admin deleted later would otherwise leave their
    // address behind there.
    sqlx::query(
        "UPDATE bookings SET approved_by = 'admin'
          WHERE lower(approved_by) = lower($2) AND user_id <> $1",
    )
    .bind(id)
    .bind(&target.email)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM bookings WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM otp_codes WHERE lower(email) = lower($1)")
        .bind(&target.email)
        .execute(&mut *tx)
        .await?;
    users::write_audit(
        &mut tx,
        actor.id,
        None,
        AuditAction::UserDeleted,
        Some(&counts.summary(&target.role)),
    )
    .await?;
    let gone = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if gone != 1 {
        return Err(AppError::Internal(anyhow::anyhow!("user row not deleted")));
    }
    tx.commit().await?;

    // Committed. From here nothing may undo the deletion: a file that can't be
    // removed is counted and reported, not an error.
    let (removed, failed) = uploads::remove_upload_files(&state, &urls).await;
    tracing::info!(
        actor_id = %actor.id,
        bookings = counts.bookings_total(),
        messages = counts.messages,
        stories = counts.stories,
        files_removed = removed,
        files_failed = failed,
        "account permanently deleted",
    );
    if failed > 0 {
        tracing::warn!(
            files_failed = failed,
            "some files of a deleted account could not be removed; sweep them"
        );
    }

    Ok(Json(serde_json::json!({
        "deleted": true,
        "files_removed": removed,
        "files_failed": failed,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn user(role: &str, is_owner: bool) -> User {
        let now = Utc::now();
        User {
            id: Uuid::new_v4(),
            email: "x@example.com".into(),
            full_name: None,
            phone: None,
            home_city: None,
            home_state: None,
            mailing_address: None,
            relationship: None,
            boat_info: None,
            notes: None,
            role: role.into(),
            is_owner,
            blocked_at: None,
            avatar_url: None,
            has_password: false,
            must_change_password: false,
            temp_password_expires_at: None,
            session_version: 0,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn guests_and_family_may_be_deleted() {
        let actor = Uuid::new_v4();
        assert!(refusal(actor, &user("guest", false)).is_none());
        assert!(refusal(actor, &user("user", false)).is_none());
        let mut blocked = user("guest", false);
        blocked.blocked_at = Some(Utc::now());
        assert!(refusal(actor, &blocked).is_none());
    }

    #[test]
    fn you_cannot_delete_yourself() {
        let me = user("guest", false);
        assert!(refusal(me.id, &me).is_some());
    }

    #[test]
    fn admins_and_owners_are_refused() {
        let actor = Uuid::new_v4();
        assert!(refusal(actor, &user("admin", false)).is_some());
        assert!(refusal(actor, &user("guest", true)).is_some());
        assert!(refusal(actor, &user("admin", true)).is_some());
    }

    #[test]
    fn the_audit_summary_names_nothing() {
        let c = Counts {
            bookings_pending: 1,
            bookings_approved: 1,
            bookings_denied: 1,
            bookings_cancelled: 0,
            bookings_future: 1,
            messages: 2,
            stories: 1,
            catches: 3,
            photos: 5,
        };
        assert_eq!(
            c.summary("guest"),
            "guest account, 3 bookings (1 approved), 2 messages, 1 stories, 5 photos"
        );
        assert!(c.summary("user").starts_with("family account"));
    }
}
