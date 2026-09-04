//! Passwordless auth: email OTP in, JWT out.
//!
//! There is no invite list. The first time someone requests a code for an
//! address we create a `guest` row for them — the camp is small enough that
//! the owner's approve/deny step is the real gate, not registration.

use crate::{
    ApiResult, AppError, Shared,
    email::{self},
    email_templates,
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
