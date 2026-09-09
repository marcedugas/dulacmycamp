//! Passwordless auth: email OTP in, JWT out.
//!
//! There is no invite list. The first time someone requests a code for an
//! address we create a `guest` row for them — the camp is small enough that
//! the owner's approve/deny step is the real gate, not registration.

use crate::{
    ApiResult, AppError, Shared,
    email::{self},
    email_templates, password,
    rate_limit::client_ip,
    users::{self, USER_COLUMNS, User},
};
use axum::{
    Json,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{HeaderMap, header::AUTHORIZATION, request::Parts},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::Rng;
use serde::{Deserialize, Serialize};

/// How long a login code stays valid.
const OTP_TTL_MINUTES: i64 = 10;
/// How long a session lasts before the guest has to request a new code.
const JWT_TTL_HOURS: i64 = 24;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// User id.
    pub sub: String,
    pub email: String,
    pub role: String,
    pub exp: i64,
    pub iat: i64,
}

pub fn issue_token(cfg: &crate::Config, user: &User) -> anyhow::Result<String> {
    let now = Utc::now();
    let claims = Claims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        role: user.role.clone(),
        iat: now.timestamp(),
        exp: (now + Duration::hours(JWT_TTL_HOURS)).timestamp(),
    };
    Ok(encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.jwt_secret.as_bytes()),
    )?)
}

fn decode_token(cfg: &crate::Config, token: &str) -> Result<Claims, AppError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(cfg.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|_| AppError::Unauthorized("Your session has expired. Please log in again.".into()))
}

// ─────────────────────────── extractors ───────────────────────────

/// A request carrying a valid Bearer token. Rejects with 401 otherwise.
pub struct AuthUser(pub User);

/// A request from an admin. Rejects with 401 or 403.
pub struct AdminUser(pub User);

/// Resolves the caller when a token is present, without requiring one.
/// Used by endpoints whose response detail widens for logged-in users.
pub struct MaybeUser(pub Option<User>);

async fn user_from_parts(parts: &Parts, state: &Shared) -> Result<Option<User>, AppError> {
    let Some(header) = parts.headers.get(AUTHORIZATION) else {
        return Ok(None);
    };
    let Some(token) = header
        .to_str()
        .ok()
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return Ok(None);
    };

    let claims = decode_token(&state.cfg, token)?;
    let id = claims
        .sub
        .parse()
        .map_err(|_| AppError::Unauthorized("Malformed token.".into()))?;

    // Load from the database rather than trusting the claims: a role change or
    // a deleted account must take effect before the 24h token expires.
    users::find_by_id(&state.db, id).await.map_err(Into::into)
}

impl FromRequestParts<Shared> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Shared,
    ) -> Result<Self, Self::Rejection> {
        user_from_parts(parts, state)
            .await?
            .map(AuthUser)
            .ok_or_else(|| AppError::Unauthorized("Sign in to continue.".into()))
    }
}

impl FromRequestParts<Shared> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Shared,
    ) -> Result<Self, Self::Rejection> {
        let user = user_from_parts(parts, state)
            .await?
            .ok_or_else(|| AppError::Unauthorized("Sign in to continue.".into()))?;
        if !user.is_admin() {
            return Err(AppError::Forbidden("Admins only.".into()));
        }
        Ok(AdminUser(user))
    }
}

impl FromRequestParts<Shared> for MaybeUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Shared,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeUser(user_from_parts(parts, state).await?))
    }
}

// ─────────────────────────── handlers ───────────────────────────

#[derive(Debug, Deserialize)]
pub struct RequestOtp {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

pub async fn request_otp(
    State(state): State<Shared>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<RequestOtp>,
) -> ApiResult<Json<MessageResponse>> {
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 5 {
        return Err(AppError::BadRequest("Enter a valid email address.".into()));
    }

    // This endpoint is public, unauthenticated, and spends money on every
    // accepted call, so it is throttled before it touches the database or
    // the mail provider. The IP limit is what caps spend — an attacker
    // cycling through addresses walks straight past a per-email limit.
    let ip = client_ip(&headers, Some(peer));
    if let Err(retry) = state.limits.otp_per_ip.check(&ip) {
        tracing::warn!(%retry, "otp request rate limited by ip");
        return Err(AppError::TooManyRequests(format!(
            "Too many login attempts from this network. Try again in {} minute{}.",
            retry.div_ceil(60),
            if retry.div_ceil(60) == 1 { "" } else { "s" },
        )));
    }
    if let Err(retry) = state.limits.otp_per_email.check(&email) {
        tracing::debug!(%retry, "otp request rate limited by email");
        return Err(AppError::TooManyRequests(format!(
            "We just sent a code to that address. Check your inbox, or try again in {retry} seconds."
        )));
    }

    // Self-registration: first code request creates the guest account.
    let user = match users::find_by_email(&state.db, &email).await? {
        Some(u) => u,
        None => {
            sqlx::query_as::<_, User>(&format!(
                "INSERT INTO users (email) VALUES ($1) RETURNING {USER_COLUMNS}"
            ))
            .bind(&email)
            .fetch_one(&state.db)
            .await?
        }
    };

    let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
    let expires_at = Utc::now() + Duration::minutes(OTP_TTL_MINUTES);

    let mut tx = state.db.begin().await?;
    // Retire any outstanding codes so only the newest one works.
    sqlx::query("UPDATE otp_codes SET used = true WHERE email = $1 AND used = false")
        .bind(&email)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO otp_codes (email, code, expires_at) VALUES ($1, $2, $3)")
        .bind(&email)
        .bind(&code)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    tracing::debug!(user_id = %user.id, "issued login code");
    email::spawn(
        state.clone(),
        email.clone(),
        email_templates::otp_email(&code, OTP_TTL_MINUTES),
    );

    Ok(Json(MessageResponse {
        message: "Code sent".into(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct VerifyOtp {
    pub email: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

pub async fn verify_otp(
    State(state): State<Shared>,
    Json(body): Json<VerifyOtp>,
) -> ApiResult<Json<AuthResponse>> {
    let email = body.email.trim().to_lowercase();
    let code = body.code.trim();

    let row: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT id FROM otp_codes
         WHERE email = $1 AND code = $2 AND used = false AND expires_at > now()
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&email)
    .bind(code)
    .fetch_optional(&state.db)
    .await?;

    let Some((otp_id,)) = row else {
        return Err(AppError::Unauthorized(
            "That code is invalid or has expired.".into(),
        ));
    };

    sqlx::query("UPDATE otp_codes SET used = true WHERE id = $1")
        .bind(otp_id)
        .execute(&state.db)
        .await?;

    let user = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET last_login_at = now() WHERE lower(email) = lower($1)
         RETURNING {USER_COLUMNS}"
    ))
    .bind(&email)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Account not found.".into()))?;

    let token = issue_token(&state.cfg, &user).map_err(AppError::Internal)?;
    Ok(Json(AuthResponse { token, user }))
}

pub async fn me(AuthUser(user): AuthUser) -> Json<User> {
    Json(user)
}

// ─────────────────────────── password login (admins) ───────────────────────────
//
// Additive to the OTP flow above, never a replacement. An admin may hold a
// password *and* keep requesting codes; a guest can do neither. The session
// this issues is the same `issue_token` session OTP issues — same claims, same
// expiry — so nothing downstream can tell how someone signed in, and nothing
// downstream has to care.

#[derive(Debug, Deserialize)]
pub struct SetPassword {
    pub password: String,
}

/// `POST /auth/set-password` — sets the caller's own password.
///
/// The bootstrap is deliberate: you need a live session to get here, and the
/// only way to a first session is an emailed code. So a password can only ever
/// be added by someone who has already proved they hold the mailbox.
pub async fn set_password(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
    Json(body): Json<SetPassword>,
) -> ApiResult<Json<MessageResponse>> {
    // Guests are refused explicitly rather than vaguely: this is a rule about
    // who the feature is for, not a secret, and the caller is authenticated
    // already, so there is nothing to give away.
    require_password_eligible(&user)?;

    let hash = password::hash(&body.password).map_err(AppError::BadRequest)?;
    let replacing = user.has_password;
    users::set_password_hash(&state.db, user.id, &hash).await?;

    // Not a confirmation step — the change has already happened — but the one
    // signal that would tell an admin their session had been taken over.
    tracing::info!(user_id = %user.id, replacing, "admin password set");
    email::spawn(
        state.clone(),
        user.email.clone(),
        email_templates::password_changed(&state.cfg.frontend_url),
    );

    Ok(Json(MessageResponse {
        message: if replacing {
            "Password updated.".into()
        } else {
            "Password set.".into()
        },
    }))
}

/// Guests are refused explicitly rather than vaguely: this is a rule about who
/// the feature is for, not a secret, and the caller is authenticated already,
/// so there is nothing to give away.
fn require_password_eligible(user: &User) -> Result<(), AppError> {
    if user.is_admin() {
        return Ok(());
    }
    Err(AppError::Forbidden(
        "Password sign-in is for admin accounts. Guests sign in with an emailed code.".into(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct LoginPassword {
    pub email: String,
    pub password: String,
}

/// The single answer to every way a password login can fail.
///
/// Unknown address, known address that is a guest, admin who never set a
/// password, admin with a password who typed it wrong — all identical, so the
/// endpoint cannot be used to discover which addresses exist, which of them
/// are admins, or which have a password. The only thing that varies is the
/// rate-limit refusal, which is about the caller, not the account.
fn invalid_credentials() -> AppError {
    AppError::Unauthorized("That email and password don't match an account.".into())
}

/// `POST /auth/login-password` — public. Trades an admin's email and password
/// for the same JWT `verify_otp` issues.
pub async fn login_password(
    State(state): State<Shared>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<LoginPassword>,
) -> ApiResult<Json<AuthResponse>> {
    let email = body.email.trim().to_lowercase();

    // Throttled before the database is touched, like `request_otp`. Here the
    // budget is protecting a secret rather than a mail bill, so it is tighter.
    let ip = client_ip(&headers, Some(peer));
    if let Err(retry) = state.limits.password_per_ip.check(&ip) {
        tracing::warn!(%retry, "password login rate limited by ip");
        return Err(too_many_attempts(retry));
    }
    if let Err(retry) = state.limits.password_per_email.check(&email) {
        tracing::warn!(%retry, "password login rate limited by email");
        return Err(too_many_attempts(retry));
    }

    let found = users::find_with_password(&state.db, &email).await?;

    // One argon2 verification on every path, including the ones that were
    // never going to succeed. Returning early for an unknown address would
    // answer in a millisecond where a real check takes tens of them, and the
    // identical error bodies above would leak through the timing instead.
    let password_matches = match found.as_ref().and_then(|f| f.password_hash.as_deref()) {
        Some(hash) => password::verify(&body.password, hash),
        None => password::verify_dummy(&body.password),
    };

    let id = authenticate(found.as_ref(), password_matches)?.id;

    // Same session the OTP path hands out, recorded the same way.
    let user = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET last_login_at = now() WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    let token = issue_token(&state.cfg, &user).map_err(AppError::Internal)?;
    Ok(Json(AuthResponse { token, user }))
}

/// Whether a looked-up row plus the result of the password check add up to a
/// session, and nothing else about why not.
///
/// Pure, and lifted out of the handler on purpose: "every way of failing looks
/// the same from outside" is the security property of this endpoint, and it is
/// only worth claiming if it can be tested directly. See the tests below.
fn authenticate(
    found: Option<&users::UserWithHash>,
    password_matches: bool,
) -> Result<&User, AppError> {
    match found {
        Some(f) if f.user.is_admin() && f.password_hash.is_some() && password_matches => {
            Ok(&f.user)
        }
        _ => Err(invalid_credentials()),
    }
}

fn too_many_attempts(retry: u64) -> AppError {
    let minutes = retry.div_ceil(60);
    AppError::TooManyRequests(format!(
        "Too many sign-in attempts. Try again in {minutes} minute{}, or sign in with an emailed code.",
        if minutes == 1 { "" } else { "s" },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::RateLimits;
    use crate::users::UserWithHash;
    use axum::response::IntoResponse;
    use uuid::Uuid;

    fn user(role: &str, has_password: bool) -> User {
        let now = Utc::now();
        User {
            id: Uuid::nil(),
            email: "someone@example.com".into(),
            full_name: None,
            phone: None,
            relationship: None,
            boat_info: None,
            notes: None,
            role: role.into(),
            is_owner: false,
            avatar_url: None,
            has_password,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn row(role: &str, hash: Option<&str>) -> UserWithHash {
        UserWithHash {
            user: user(role, hash.is_some()),
            password_hash: hash.map(str::to_string),
        }
    }

    /// The exact bytes a caller sees: status, then body.
    async fn rendered(e: AppError) -> (axum::http::StatusCode, Vec<u8>) {
        let resp = e.into_response();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("error bodies are small");
        (status, body.to_vec())
    }

    // ─────────────── the generic-failure property ───────────────

    /// The point of the endpoint's error handling: an attacker must not be
    /// able to tell an unknown address from a guest, from an admin who never
    /// set a password, from an admin who set one and had it typed wrong.
    ///
    /// Compared as rendered responses, not as enum variants — the shape the
    /// caller actually receives is the thing that must not vary.
    #[tokio::test]
    async fn every_failure_mode_renders_the_identical_response() {
        let hash = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let cases: Vec<(&str, AppError)> = vec![
            // No account at that address at all.
            ("unknown email", authenticate(None, false).unwrap_err()),
            // The address exists, and even matched — but it is a guest.
            (
                "guest with a password somehow set",
                authenticate(Some(&row("guest", Some(hash))), true).unwrap_err(),
            ),
            // A guest, the ordinary case: no password to check.
            (
                "guest with no password",
                authenticate(Some(&row("guest", None)), false).unwrap_err(),
            ),
            // An admin who has never set one.
            (
                "admin with no password set",
                authenticate(Some(&row("admin", None)), false).unwrap_err(),
            ),
            // An admin who has one, and got it wrong.
            (
                "admin with the wrong password",
                authenticate(Some(&row("admin", Some(hash))), false).unwrap_err(),
            ),
        ];

        let mut seen: Option<(axum::http::StatusCode, Vec<u8>)> = None;
        for (label, err) in cases {
            let got = rendered(err).await;
            match &seen {
                None => seen = Some(got),
                Some(first) => assert_eq!(
                    &got, first,
                    "`{label}` is distinguishable from the first case — the endpoint leaks which part failed",
                ),
            }
        }

        let (status, body) = seen.expect("cases is not empty");
        assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
        let text = String::from_utf8(body).unwrap();
        // Nothing in the message may hint at which check failed.
        for leak in [
            "admin",
            "guest",
            "role",
            "exists",
            "not found",
            "no password",
        ] {
            assert!(
                !text.to_lowercase().contains(leak),
                "error body mentions `{leak}`: {text}",
            );
        }
    }

    /// The one case that does succeed, so the test above isn't passing because
    /// nothing ever authenticates.
    #[test]
    fn an_admin_with_a_matching_password_authenticates() {
        let hash = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let entry = row("admin", Some(hash));
        assert!(authenticate(Some(&entry), true).is_ok());
    }

    /// Role is re-read on every attempt, so a hash left on a demoted account
    /// is inert even before `update_role` clears it.
    #[test]
    fn a_demoted_admin_cannot_use_a_leftover_hash() {
        let hash = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(authenticate(Some(&row("guest", Some(hash))), true).is_err());
    }

    // ─────────────── who may set a password ───────────────

    /// A guest is refused with a 403 — a clear "this isn't for you", not the
    /// deliberately vague login error. They are already authenticated here, so
    /// there is nothing to withhold.
    #[tokio::test]
    async fn a_guest_is_forbidden_from_setting_a_password() {
        let err = require_password_eligible(&user("guest", false))
            .expect_err("a guest must not be able to set a password");
        let (status, body) = rendered(err).await;
        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
        assert!(String::from_utf8(body).unwrap().contains("admin"));
    }

    #[test]
    fn an_admin_may_set_a_password() {
        assert!(require_password_eligible(&user("admin", false)).is_ok());
        assert!(require_password_eligible(&user("admin", true)).is_ok());
    }

    // ─────────────── throttling ───────────────

    /// Repeated attempts from one address are refused once the budget is
    /// spent, and the refusal points at the way in that still works.
    #[tokio::test]
    async fn repeated_password_attempts_are_rate_limited() {
        let limits = RateLimits::default();
        for i in 0..5 {
            assert!(
                limits.password_per_ip.check("198.51.100.9").is_ok(),
                "attempt {i} should be allowed",
            );
        }
        let retry = limits
            .password_per_ip
            .check("198.51.100.9")
            .expect_err("the sixth attempt should be refused");

        let (status, body) = rendered(too_many_attempts(retry)).await;
        assert_eq!(status, axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert!(String::from_utf8(body).unwrap().contains("emailed code"));
    }

    /// Per-address as well as per-IP, so a rotating pool of addresses cannot
    /// grind away at one account.
    #[test]
    fn password_attempts_are_limited_per_email_too() {
        let limits = RateLimits::default();
        for _ in 0..5 {
            assert!(limits.password_per_email.check("admin@example.com").is_ok());
        }
        assert!(
            limits
                .password_per_email
                .check("admin@example.com")
                .is_err()
        );
        // A different account is unaffected.
        assert!(limits.password_per_email.check("other@example.com").is_ok());
    }

    /// The password budget is meaner than the OTP one, which guards a mail
    /// bill rather than a secret.
    #[test]
    fn the_password_budget_is_tighter_than_the_otp_budget() {
        let limits = RateLimits::default();
        let spend = |rl: &crate::rate_limit::RateLimiter, key: &str| {
            let mut n = 0;
            while rl.check(key).is_ok() {
                n += 1;
                assert!(n < 100, "limiter never refused");
            }
            n
        };
        let otp = spend(&limits.otp_per_ip, "203.0.113.1");
        let pw = spend(&limits.password_per_ip, "203.0.113.1");
        assert!(
            pw <= otp,
            "password allows {pw} attempts, OTP allows {otp} — the password path must not be looser",
        );
    }
}
