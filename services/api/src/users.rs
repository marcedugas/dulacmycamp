//! The `users` table and the profile endpoints on top of it.

use crate::{
    ApiResult, AppError, Shared,
    auth::{AdminUser, AuthUser},
    contact::ContactFields,
    pii::Sealed,
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
    /// Encrypted at rest; decrypted only when this struct is serialised, which
    /// only happens in the account's own responses and the admin ones.
    pub phone: Option<Sealed>,
    pub home_city: Option<String>,
    pub home_state: Option<String>,
    /// Encrypted at rest, like `phone`.
    pub mailing_address: Option<Sealed>,
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
    /// Whether a password is set — the boolean only, never the hash.
    /// Computed in SQL by `USER_COLUMNS` so `password_hash` itself is not in
    /// any query this struct is read from, and so cannot be serialised out.
    pub has_password: bool,
    /// Set while the password is a temporary one an admin issued: the account
    /// may do nothing but choose a new password until it has. Enforced in
    /// `crate::auth`'s extractors, not only in the UI.
    pub must_change_password: bool,
    /// When an admin-issued temporary password stops working for sign-in.
    /// `None` for an ordinary password.
    pub temp_password_expires_at: Option<DateTime<Utc>>,
    /// Bumped whenever the password changes; a token minted under an older
    /// value is refused. Never serialised — it is server bookkeeping.
    #[serde(skip)]
    pub session_version: i32,
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
pub const USER_COLUMNS: &str = "id, email, full_name, phone, home_city, home_state, \
                                mailing_address, relationship, boat_info, notes, \
                                role, is_owner, blocked_at, avatar_url, \
                                (password_hash IS NOT NULL) AS has_password, \
                                must_change_password, temp_password_expires_at, \
                                session_version, \
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

/// What a password change was, for the audit log. Strings match the CHECK
/// constraint in migration 0019.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditAction {
    PasswordTempSet,
    PasswordRemoved,
    PasswordResetByUser,
    UserDeleted,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PasswordTempSet => "password_temp_set",
            Self::PasswordRemoved => "password_removed",
            Self::PasswordResetByUser => "password_reset_by_user",
            Self::UserDeleted => "user_deleted",
        }
    }
}

/// The one write path for every password change: user-chosen (profile or
/// reset), admin temporary, admin removal.
///
/// Always ends every existing session (`session_version + 1`), in the same
/// statement as the change itself so there is no window where the new
/// password exists and the old sessions still work. Returns the updated row,
/// which carries the new `session_version` for minting a fresh token.
///
/// `temp_expires_at` of `Some` marks the password temporary
/// (`must_change_password = true`); `None` clears both fields. `hash` of
/// `None` removes the password.
pub async fn apply_password_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    hash: Option<&str>,
    temp_expires_at: Option<DateTime<Utc>>,
) -> Result<User, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "UPDATE users
            SET password_hash = $2,
                must_change_password = ($3::timestamptz IS NOT NULL),
                temp_password_expires_at = $3,
                session_version = session_version + 1
          WHERE id = $1
      RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .bind(hash)
    .bind(temp_expires_at)
    .fetch_one(&mut **tx)
    .await
}

/// `target` is `None` when there is no row left to point at (a deletion).
/// `detail` is free text and must never name a person.
pub async fn write_audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: Uuid,
    target: Option<Uuid>,
    action: AuditAction,
    detail: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO admin_audit_log (actor_id, target_user_id, action, detail)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(actor)
    .bind(target)
    .bind(action.as_str())
    .bind(detail)
    .execute(&mut **tx)
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
        "UPDATE users SET full_name = $2, relationship = $4, boat_info = $5, notes = $6
         WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(user.id)
    .bind(full_name)
    .bind(None::<String>) // $3 unused: phone is saved via /users/me/contact
    .bind(clean(body.relationship))
    .bind(clean(body.boat_info))
    .bind(clean(body.notes))
    .fetch_one(&state.db)
    .await?;

    Ok(Json(updated))
}

// No `Debug`: these are private contact details and must not reach a log.
#[derive(Deserialize)]
pub struct UpdateContact {
    pub phone: Option<String>,
    pub home_city: Option<String>,
    pub home_state: Option<String>,
    pub mailing_address: Option<String>,
}

/// `PUT /users/me/contact` — replaces the caller's contact info. Blank clears
/// a field. Nothing is required here; the booking form is where guests are
/// asked for it.
pub async fn update_my_contact(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<UpdateContact>,
) -> ApiResult<Json<User>> {
    let c = ContactFields::parse(
        &body.phone,
        &body.home_city,
        &body.home_state,
        &body.mailing_address,
    )?;
    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET phone = $2, home_city = $3, home_state = $4, mailing_address = $5
         WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(user.id)
    .bind(c.sealed_phone().map_err(AppError::Internal)?)
    .bind(&c.home_city)
    .bind(&c.home_state)
    .bind(c.sealed_address().map_err(AppError::Internal)?)
    .fetch_one(&state.db)
    .await?;
    tracing::info!(user_id = %user.id, "contact info updated");
    Ok(Json(updated))
}

/// Clears phone, address, city and state for one account.
async fn clear_contact(db: &sqlx::PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET phone = NULL, home_city = NULL, home_state = NULL,
                          mailing_address = NULL
         WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(db)
    .await
}

/// `DELETE /users/me/contact` — the applicant deleting their own contact info.
pub async fn clear_my_contact(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<User>> {
    let updated = clear_contact(&state.db, user.id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found.".into()))?;
    tracing::info!(user_id = %user.id, "contact info deleted by owner of the account");
    Ok(Json(updated))
}

/// `DELETE /users/{id}/contact` — an admin clearing someone's contact info on
/// request. Account-level data only; stay-level answers stay on their booking
/// and go when that booking is deleted.
pub async fn clear_contact_admin(
    State(state): State<Shared>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<User>> {
    let updated = clear_contact(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found.".into()))?;
    tracing::info!(user_id = %id, by = %admin.email, "contact info cleared by admin");
    Ok(Json(updated))
}

/// Admin roster: every user with the history hanging off them, for the Users
/// tab.
///
/// The three counts are a quick read on how much history an account has.
/// What a permanent delete would remove is spelled out per account by
/// `crate::account_delete`'s preview.
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
        "SELECT u.id, u.email, u.full_name, u.phone, u.home_city, u.home_state,
                u.mailing_address, u.relationship, u.boat_info, u.notes,
                u.role, u.is_owner, u.blocked_at, u.avatar_url,
                (u.password_hash IS NOT NULL) AS has_password,
                u.must_change_password, u.temp_password_expires_at, u.session_version,
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

    // A password is no longer tied to any particular role, so changing role
    // leaves it exactly where it was — same as every other profile field.
    let updated = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET role = $2 WHERE id = $1 RETURNING {USER_COLUMNS}"
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

// ─────────────────────── admin password tools ───────────────────────
//
// Any admin may do this to any other account, the owner's included. That is a
// deliberate decision (the person who built the site has to be able to help
// the owner), and it is made safe by two things rather than by a restriction:
// the affected person is always emailed, and every use is written to
// `admin_audit_log`. An admin can never see or choose anyone's real password:
// a temporary one is generated here and has to be replaced at first use.

/// How long a temporary password can be used to sign in.
const TEMP_PASSWORD_TTL_DAYS: i64 = 7;
const TEMP_PASSWORD_LEN: usize = 14;
/// No 0/O/1/l/I — a password read out over the phone must survive being
/// transcribed.
const TEMP_PASSWORD_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";

fn generate_temp_password() -> String {
    use rand::{Rng, rngs::OsRng};
    let mut rng = OsRng;
    (0..TEMP_PASSWORD_LEN)
        .map(|_| TEMP_PASSWORD_ALPHABET[rng.gen_range(0..TEMP_PASSWORD_ALPHABET.len())] as char)
        .collect()
}

/// Admins use the profile page for their own password, so these tools never
/// need to reason about acting on yourself.
fn require_not_self(actor: &User, target: Uuid) -> Result<(), AppError> {
    if actor.id == target {
        return Err(AppError::BadRequest(
            "Change your own password from your profile page.".into(),
        ));
    }
    Ok(())
}

/// The only response in the API that carries a plaintext password.
#[derive(Serialize)]
pub struct TempPasswordResponse {
    pub password: String,
    pub expires_at: DateTime<Utc>,
}

/// `POST /users/{id}/temp-password`
pub async fn set_temp_password(
    State(state): State<Shared>,
    AdminUser(actor): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl axum::response::IntoResponse> {
    require_not_self(&actor, id)?;
    let target = find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found.".into()))?;

    let password = generate_temp_password();
    let hash =
        crate::password::hash(&password).map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let expires_at = Utc::now() + chrono::Duration::days(TEMP_PASSWORD_TTL_DAYS);

    let mut tx = state.db.begin().await?;
    apply_password_change(&mut tx, id, Some(&hash), Some(expires_at)).await?;
    write_audit(
        &mut tx,
        actor.id,
        Some(id),
        AuditAction::PasswordTempSet,
        None,
    )
    .await?;
    tx.commit().await?;

    // Ids only: no password, no hash, no address.
    tracing::info!(actor_id = %actor.id, target_id = %id, "temporary password set by admin");
    crate::email::spawn(
        state.clone(),
        target.email,
        crate::email_templates::admin_temp_password_notice(&state.cfg.frontend_url),
    );

    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(TempPasswordResponse {
            password,
            expires_at,
        }),
    ))
}

/// `POST /users/{id}/remove-password`
pub async fn remove_password(
    State(state): State<Shared>,
    AdminUser(actor): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl axum::response::IntoResponse> {
    require_not_self(&actor, id)?;
    let target = find_by_id(&state.db, id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found.".into()))?;

    let mut tx = state.db.begin().await?;
    let updated = apply_password_change(&mut tx, id, None, None).await?;
    write_audit(
        &mut tx,
        actor.id,
        Some(id),
        AuditAction::PasswordRemoved,
        None,
    )
    .await?;
    tx.commit().await?;

    tracing::info!(actor_id = %actor.id, target_id = %id, "password removed by admin");
    crate::email::spawn(
        state.clone(),
        target.email,
        crate::email_templates::admin_password_removed_notice(&state.cfg.frontend_url),
    );

    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(updated),
    ))
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
            home_city: None,
            home_state: None,
            mailing_address: None,
            relationship: None,
            boat_info: None,
            notes: None,
            role: role.into(),
            is_owner: false,
            blocked_at: blocked.then_some(now),
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

    // ─────────────── temporary passwords ───────────────

    #[test]
    fn a_generated_temp_password_is_long_unambiguous_and_valid() {
        for _ in 0..200 {
            let p = generate_temp_password();
            assert!(p.len() >= 12, "too short: {}", p.len());
            assert!(
                crate::password::validate(&p).is_ok(),
                "must pass the normal rules"
            );
            assert!(
                p.chars()
                    .all(|c| !"0O1lI".contains(c) && c.is_ascii_alphanumeric()),
                "ambiguous or odd character in a generated password"
            );
        }
    }

    #[test]
    fn generated_temp_passwords_differ() {
        assert_ne!(generate_temp_password(), generate_temp_password());
    }

    #[tokio::test]
    async fn an_admin_cannot_use_the_tools_on_themselves() {
        let mut me = user("admin", false);
        me.id = Uuid::new_v4();
        assert!(require_not_self(&me, me.id).is_err());
        assert!(require_not_self(&me, Uuid::new_v4()).is_ok());
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
    fn the_family_role_can_be_blocked_like_any_non_admin() {
        // Only admin accounts are shielded from blocking/deletion; promoting
        // someone to family must not accidentally shield them too.
        assert!(require_blockable(&user("user", false)).is_ok());
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
}
