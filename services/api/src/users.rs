//! The `users` table and the profile endpoints on top of it.

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
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub full_name: Option<String>,
    pub phone: Option<String>,
    pub relationship: Option<String>,
    pub boat_info: Option<String>,
    pub notes: Option<String>,
    pub role: String,
    /// Receives the one-click approve/deny email. Any number of users may be
    /// flagged; all of them get it.
    pub is_owner: bool,
    /// When this account was blocked, or `None` if it is in good standing.
    /// A blocked account keeps every row it ever wrote and simply stops being
    /// able to act — see [`User::is_blocked`].
    pub blocked_at: Option<DateTime<Utc>>,
    pub avatar_url: Option<String>,
    /// Whether an admin password is set — the boolean only, never the hash.
    /// Computed in SQL by `USER_COLUMNS` so `password_hash` itself is not in
    /// any query this struct is read from, and so cannot be serialised out.
    pub has_password: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// Whether this account has been blocked by an admin.
    ///
    /// Checked on every authenticated request (`crate::auth`) rather than only
    /// at login, so a block lands on a session that is already open — the same
    /// reason the user row is re-read from the database each time instead of
    /// trusting the token's claims.
    pub fn is_blocked(&self) -> bool {
        self.blocked_at.is_some()
    }

    /// Whether this user may see who is on a booking — name, email, pets,
    /// requests, checkout notes. Admins and the camp owner do; ordinary guests
    /// see an anonymous "Booked" cell. The owner is normally an admin too, but
    /// the flag is the one that matters: whoever approves stays is reading the
    /// same details the approval email already puts in their inbox.
    /// Applied in `crate::bookings::BookingRow::to_view`.
    pub fn sees_guest_details(&self) -> bool {
        self.is_admin() || self.is_owner
    }

    /// Best-effort display name for emails and admin tables.
    pub fn display_name(&self) -> String {
        self.full_name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&self.email)
            .to_string()
    }
}

/// First token of a full name — "Jean Dugas" reads as "Jean", and a missing
/// or blank name as "A guest". Never the email address, which is the point:
/// these are the surfaces where someone's identity is shown to *other*
/// people, so they get a first name or nothing that identifies anyone.
///
/// The single convention behind every such surface. `crate::journal` set it
/// for the public story feed; `crate::bookings` reuses it verbatim for the
/// name on a calendar stay the booker chose to make visible, so the same
/// person reads the same way in both places.
pub fn first_name(full_name: Option<&str>) -> String {
    full_name
        .and_then(|n| n.split_whitespace().next())
        .unwrap_or("A guest")
        .to_string()
}

/// Columns shared by every `users` read, so row shapes never drift.
///
/// `password_hash` is deliberately absent: only its presence is exposed, as
/// `has_password`. The one place the hash itself is read is
/// [`find_with_password`], which the password login calls and nothing else.
pub const USER_COLUMNS: &str = "id, email, full_name, phone, relationship, boat_info, notes, \
                                role, is_owner, blocked_at, avatar_url, \
                                (password_hash IS NOT NULL) AS has_password, \
                                last_login_at, created_at, updated_at";

pub async fn find_by_id(db: &sqlx::PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await
}

pub async fn find_by_email(db: &sqlx::PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE lower(email) = lower($1)"
    ))
    .bind(email)
    .fetch_optional(db)
    .await
}

/// A user together with their stored password hash — the only query that
/// reads `password_hash` at all.
///
/// Kept separate from [`find_by_email`] so the hash never rides along on the
/// row shape the rest of the app passes around and serialises.
#[derive(FromRow)]
pub struct UserWithHash {
    #[sqlx(flatten)]
    pub user: User,
    pub password_hash: Option<String>,
}

pub async fn find_with_password(
    db: &sqlx::PgPool,
    email: &str,
) -> Result<Option<UserWithHash>, sqlx::Error> {
    sqlx::query_as::<_, UserWithHash>(&format!(
        "SELECT {USER_COLUMNS}, password_hash FROM users WHERE lower(email) = lower($1)"
    ))
    .bind(email)
    .fetch_optional(db)
    .await
}

/// Stores (or replaces) an admin's password hash.
pub async fn set_password_hash(db: &sqlx::PgPool, id: Uuid, hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(hash)
        .execute(db)
        .await
        .map(|_| ())
}

/// Whether this account has ever had a booking approved — the "has actually
/// stayed here" test, with no date or checkout condition on it.
///
/// Lives here rather than in one caller because it is a fact about a person,
/// not about a section: [`crate::content_access`] asks it for any section
/// whose `approved_booking_grants` is set.
pub async fn has_approved_booking(db: &sqlx::PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let (ever,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM bookings WHERE user_id = $1 AND status = 'approved')",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(ever)
}

/// The single admin used as the fallback recipient for guest messages and
/// booking notifications.
pub async fn first_admin(db: &sqlx::PgPool) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "SELECT {USER_COLUMNS} FROM users WHERE role = 'admin' ORDER BY created_at LIMIT 1"
    ))
    .fetch_optional(db)
    .await
}

/// Every address flagged as a camp owner, oldest account first.
///
/// This is the source of truth for who receives the approve/deny email;
/// `OWNER_EMAIL` is only consulted when this comes back empty. See
/// [`crate::email::owner_recipients`].
pub async fn owner_emails(db: &sqlx::PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT email FROM users WHERE is_owner = true ORDER BY created_at")
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().map(|(email,)| email).collect())
}

/// Looks up a guest by email, creating one if none exists — the same
/// self-registration an OTP request performs, just triggered by an admin
/// entering a booking on someone's behalf instead of the guest logging in
/// themselves. Never overwrites `full_name` on an existing account.
pub async fn find_or_create_guest(
    db: &sqlx::PgPool,
    email: &str,
    full_name: Option<&str>,
) -> Result<User, sqlx::Error> {
    let email = email.trim().to_lowercase();
    if let Some(existing) = find_by_email(db, &email).await? {
        return Ok(existing);
    }
    sqlx::query_as::<_, User>(&format!(
        "INSERT INTO users (email, full_name) VALUES ($1, $2) RETURNING {USER_COLUMNS}"
    ))
    .bind(&email)
    .bind(full_name.map(str::trim).filter(|n| !n.is_empty()))
    .fetch_one(db)
    .await
}

// ─────────────────────────── handlers ───────────────────────────

pub async fn get_me(AuthUser(user): AuthUser) -> Json<User> {
    Json(user)
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfile {
    pub full_name: Option<String>,
    pub phone: Option<String>,
    pub relationship: Option<String>,
    pub boat_info: Option<String>,
    pub notes: Option<String>,
}

/// Trims to `None` so blank form fields clear the column rather than storing "".
fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub async fn update_me(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<UpdateProfile>,
) -> ApiResult<Json<User>> {
    let full_name = clean(body.full_name)
        .ok_or_else(|| AppError::BadRequest("Full name is required.".into()))?;

    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET full_name = $2, phone = $3, relationship = $4, boat_info = $5, notes = $6
         WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(user.id)
    .bind(full_name)
    .bind(clean(body.phone))
    .bind(clean(body.relationship))
    .bind(clean(body.boat_info))
    .bind(clean(body.notes))
    .fetch_one(&state.db)
    .await?;

    Ok(Json(updated))
}

/// Admin roster: every user with the history hanging off them, for the Users
/// tab.
///
/// The three counts are the same three [`history_counts`] gates a hard delete
/// on, sent up front so the Users tab can disable Delete on an account that
/// has any, instead of offering a button whose only outcome is an error.
#[derive(Debug, Serialize, FromRow)]
pub struct UserWithStats {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub user: User,
    pub booking_count: i64,
    pub journal_count: i64,
    /// Messages sent *or* received — either direction is history worth keeping.
    pub message_count: i64,
}

/// The roster's counts, as correlated subqueries rather than joins: three
/// `LEFT JOIN`s onto one `GROUP BY` would multiply rows against each other and
/// count every booking once per message.
const ROSTER_COUNTS: &str = "(SELECT count(*) FROM bookings b WHERE b.user_id = u.id) \
                             AS booking_count, \
                             (SELECT count(*) FROM journal_entries j WHERE j.user_id = u.id) \
                             AS journal_count, \
                             (SELECT count(*) FROM messages m \
                              WHERE m.sender_id = u.id OR m.recipient_id = u.id) \
                             AS message_count";

pub async fn list_all(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<UserWithStats>>> {
    let rows = sqlx::query_as::<_, UserWithStats>(&format!(
        "SELECT u.id, u.email, u.full_name, u.phone, u.relationship, u.boat_info, u.notes,
                u.role, u.is_owner, u.blocked_at, u.avatar_url,
                (u.password_hash IS NOT NULL) AS has_password,
                u.last_login_at, u.created_at, u.updated_at,
                {ROSTER_COUNTS}
         FROM users u
         ORDER BY u.created_at DESC"
    ))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRole {
    pub role: String,
}

pub async fn update_role(
    State(state): State<Shared>,
    AdminUser(actor): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRole>,
) -> ApiResult<Json<User>> {
    if !crate::content_access::ASSIGNABLE_ROLES.contains(&body.role.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Role must be one of: {}.",
            crate::content_access::ASSIGNABLE_ROLES.join(", ")
        )));
    }
    // Guard against an admin locking themselves out of the admin panel.
    if actor.id == id && body.role != "admin" {
        return Err(AppError::BadRequest(
            "You cannot remove your own admin role.".into(),
        ));
    }

    // Dropping out of admin drops any password with it. Password login re-checks
    // the role on every attempt, so a leftover hash would already be inert —
    // but a credential nobody can use is a credential worth not keeping.
    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users
         SET role = $2,
             password_hash = CASE WHEN $2 = 'admin' THEN password_hash ELSE NULL END
         WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .bind(&body.role)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found.".into()))?;

    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct UpdateOwner {
    pub is_owner: bool,
}

/// Flags or unflags a user as a camp owner.
///
/// Deliberately unrestricted in both directions: the camp can have several
/// owners, and unflagging the last one is allowed — the send path falls back
/// to `OWNER_EMAIL` and logs a warning rather than silently notifying nobody.
pub async fn update_owner(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateOwner>,
) -> ApiResult<Json<User>> {
    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET is_owner = $2 WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .bind(body.is_owner)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found.".into()))?;

    Ok(Json(updated))
}

// ─────────────────────── blocking and deletion ───────────────────────
//
// Two tools for the same problem, deliberately unequal. Blocking is the one
// to reach for: it is reversible, and the account's bookings, journal entries
// and messages stay exactly where they are. Deleting is only offered when
// there is provably nothing to lose.

/// Whether this account may be blocked at all.
///
/// Admins are refused rather than blocked, because blocking one is almost
/// always a mistake with an expensive shape: an admin who blocks themselves,
/// or the last other admin, locks the panel that would undo it. Demoting to
/// guest first is one extra click and makes the intent explicit.
fn require_blockable(user: &User) -> Result<(), AppError> {
    if user.is_admin() {
        return Err(AppError::BadRequest(
            "Admin accounts can't be blocked. Remove their admin role first, then block them."
                .into(),
        ));
    }
    Ok(())
}

/// Everything hanging off an account that a hard delete would destroy.
///
/// `booking_checkouts` is absent on purpose and not an oversight: a checkout
/// row requires a booking, so `bookings == 0` already implies none exists.
/// The same holds for journal entries, which are counted anyway because the
/// admin panel shows the number.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, FromRow)]
pub struct HistoryCounts {
    pub bookings: i64,
    pub journal: i64,
    pub messages: i64,
}

impl HistoryCounts {
    fn is_empty(&self) -> bool {
        self.bookings == 0 && self.journal == 0 && self.messages == 0
    }
}

async fn history_counts(db: &sqlx::PgPool, id: Uuid) -> Result<HistoryCounts, sqlx::Error> {
    sqlx::query_as::<_, HistoryCounts>(
        "SELECT (SELECT count(*) FROM bookings WHERE user_id = $1) AS bookings,
                (SELECT count(*) FROM journal_entries WHERE user_id = $1) AS journal,
                (SELECT count(*) FROM messages
                  WHERE sender_id = $1 OR recipient_id = $1) AS messages",
    )
    .bind(id)
    .fetch_one(db)
    .await
}

/// Whether this account may be removed from the database outright.
///
/// Two gates. An admin is never deleted, for the same reason one is never
/// blocked. And an account with any history at all is refused: the rows would
/// go with it (bookings cascade, messages sent to them cascade) or the delete
/// would fail on a foreign key, and either way a stay that actually happened
/// stops being on the record. Blocking is what that case wants, so the error
/// says so rather than just refusing.
fn require_deletable(user: &User, history: &HistoryCounts) -> Result<(), AppError> {
    if user.is_admin() {
        return Err(AppError::BadRequest(
            "Admin accounts can't be deleted. Remove their admin role first.".into(),
        ));
    }
    if !history.is_empty() {
        return Err(AppError::Conflict(
            "This account has booking, journal or message history — use Block to prevent \
             future access while keeping their record intact."
                .into(),
        ));
    }
    Ok(())
}

/// `PUT /users/{id}/block` — stops an account acting, from the next request on.
///
/// What this does *not* do is touch their bookings. A blocked guest's pending
/// and approved stays stay exactly as they were; cancelling one is a separate
/// decision about a specific weekend, made with the cancel action that already
/// exists. Auto-cascading would quietly deny a stay the admin may still want
/// to honour.
pub async fn block(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<User>> {
    let target = find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found.".into()))?;
    require_blockable(&target)?;

    // COALESCE keeps the original timestamp if they were already blocked, so a
    // second click doesn't rewrite when it happened.
    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET blocked_at = COALESCE(blocked_at, now())
         WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(user_id = %id, email = %updated.email, "user blocked");
    Ok(Json(updated))
}

/// `PUT /users/{id}/unblock` — restores an account. Nothing else to undo,
/// which is the point of blocking rather than deleting.
pub async fn unblock(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<User>> {
    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET blocked_at = NULL WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found.".into()))?;

    tracing::info!(user_id = %id, email = %updated.email, "user unblocked");
    Ok(Json(updated))
}

/// `DELETE /users/{id}` — removes an account that never did anything.
///
/// The narrow case this exists for: a typo'd address, or someone who asked for
/// a login code once and never came back. Anything with a trace is refused and
/// pointed at Block.
pub async fn remove(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let target = find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found.".into()))?;
    let history = history_counts(&state.db, id).await?;
    require_deletable(&target, &history)?;

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // otp_codes are keyed by address, not by a foreign key, so an outstanding
    // code would survive the row and still be redeemable by whoever
    // self-registers that address next. Same reasoning as migration 0004.
    sqlx::query("DELETE FROM otp_codes WHERE lower(email) = lower($1)")
        .bind(&target.email)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    tracing::info!(user_id = %id, email = %target.email, "user deleted");
    Ok(Json(serde_json::json!({ "deleted": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    // The one first-name convention shared by the journal feed and the
    // calendar's visible stays — see `first_name`.
    #[test]
    fn first_name_takes_only_the_first_token() {
        assert_eq!(first_name(Some("Jean Dugas")), "Jean");
        assert_eq!(first_name(Some("Cher")), "Cher");
        assert_eq!(first_name(None), "A guest");
        assert_eq!(first_name(Some("  ")), "A guest");
    }

    fn user(role: &str, blocked: bool) -> User {
        let now = Utc::now();
        User {
            id: Uuid::nil(),
            email: "guest@example.com".into(),
            full_name: None,
            phone: None,
            relationship: None,
            boat_info: None,
            notes: None,
            role: role.into(),
            is_owner: false,
            blocked_at: blocked.then_some(now),
            avatar_url: None,
            has_password: false,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// The status and body a caller actually receives.
    async fn rendered(e: AppError) -> (axum::http::StatusCode, String) {
        let resp = e.into_response();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("error bodies are small");
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    #[test]
    fn blocked_at_is_what_makes_an_account_blocked() {
        assert!(!user("guest", false).is_blocked());
        assert!(user("guest", true).is_blocked());
    }

    // "user" is a family tier, not a step toward admin. It must not pick up
    // admin powers, and it must not see other guests' identities — that stays
    // with admins and the camp owner.
    #[test]
    fn the_family_role_is_not_an_admin_and_sees_no_guest_details() {
        let family = user("user", false);
        assert!(!family.is_admin());
        assert!(!family.sees_guest_details());
    }

    #[test]
    fn the_family_role_can_be_blocked_and_deleted_like_any_non_admin() {
        // Only admin accounts are shielded from blocking/deletion; promoting
        // someone to family must not accidentally shield them too.
        assert!(require_blockable(&user("user", false)).is_ok());
        assert!(require_deletable(&user("user", false), &HistoryCounts::default()).is_ok());
    }

    // ─────────────── who may be blocked ───────────────

    #[test]
    fn a_guest_may_be_blocked() {
        assert!(require_blockable(&user("guest", false)).is_ok());
        // Already blocked is fine — the handler makes the second click a no-op
        // rather than an error.
        assert!(require_blockable(&user("guest", true)).is_ok());
    }

    /// The lockout guard: blocking an admin is refused, and the refusal says
    /// what to do instead rather than just "no".
    #[tokio::test]
    async fn an_admin_cannot_be_blocked() {
        let err = require_blockable(&user("admin", false))
            .expect_err("blocking an admin must be refused");
        let (status, body) = rendered(err).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        assert!(body.contains("admin role"), "unhelpful refusal: {body}");
    }

    // ─────────────── who may be deleted ───────────────

    #[test]
    fn a_guest_with_nothing_attached_can_be_deleted() {
        assert!(require_deletable(&user("guest", false), &HistoryCounts::default()).is_ok());
        // Blocked already, and still empty — deleting is allowed, just rarely
        // what anyone wants.
        assert!(require_deletable(&user("guest", true), &HistoryCounts::default()).is_ok());
    }

    /// Any one of the three counts is enough to refuse, so a guest with only
    /// messages is as protected as one with bookings.
    #[tokio::test]
    async fn any_history_at_all_refuses_the_delete() {
        let counts = |bookings, journal, messages| HistoryCounts {
            bookings,
            journal,
            messages,
        };
        let cases = [
            ("a booking", counts(1, 0, 0)),
            ("a journal entry", counts(0, 1, 0)),
            ("a message", counts(0, 0, 1)),
        ];
        for (label, history) in cases {
            let Err(err) = require_deletable(&user("guest", false), &history) else {
                panic!("{label} must protect the account from deletion");
            };
            let (status, body) = rendered(err).await;
            assert_eq!(status, axum::http::StatusCode::CONFLICT, "for {label}");
            // The refusal has to point at the tool that does work here,
            // otherwise the admin is left with a dead end.
            assert!(
                body.contains("Block"),
                "for {label}, no pointer to Block: {body}"
            );
        }
    }

    /// Same reasoning as blocking: an admin is never deleted, however empty
    /// their history is.
    #[tokio::test]
    async fn an_admin_cannot_be_deleted() {
        let err = require_deletable(&user("admin", false), &HistoryCounts::default())
            .expect_err("deleting an admin must be refused");
        let (status, body) = rendered(err).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        assert!(body.contains("admin role"), "unhelpful refusal: {body}");
    }
}
