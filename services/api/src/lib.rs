//! Dulac My Camp API — Axum service backing the camp booking app.
//!
//! Single property, single household: there is no tenant scoping. Access
//! control is two-tier (`guest` / `admin`) plus opaque single-use tokens that
//! let the camp owner approve or deny a stay straight from an email.

pub mod admin;
pub mod auth;
pub mod bookings;
pub mod email;
pub mod email_templates;
pub mod events;
pub mod holidays;
pub mod notifications;
pub mod rate_limit;
pub mod users;
pub mod weather;

use axum::{
    Json, Router,
    http::{HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde_json::json;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

/// Dulac, Louisiana — used for the forecast grid and the solar calculation.
pub const CAMP_LAT: f64 = 29.3802;
pub const CAMP_LON: f64 = -90.7148;

/// Process configuration, read once at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub resend_api_key: Option<String>,
    pub email_from: String,
    pub frontend_url: String,
    /// Public origin of this API. Used to build the approve/deny links that
    /// go into the owner's email, so it must be reachable from their inbox.
    pub api_base_url: String,
    /// Camp owner (Jean). Receives the one-click approve/deny email.
    pub owner_email: Option<String>,
    /// Admin notification address. Gets an informational copy, no buttons.
    pub admin_email: Option<String>,
    pub noaa_station_id: String,
    pub capacity_adults: i64,
    pub bind_addr: String,
}

impl Config {
    /// Reads configuration from the environment, applying documented defaults.
    ///
    /// `DATABASE_URL` and `JWT_SECRET` are the only hard requirements; without
    /// `RESEND_API_KEY` the service still runs and logs outbound mail instead
    /// of sending it, which is what local development wants.
    pub fn from_env() -> anyhow::Result<Self> {
        fn opt(key: &str) -> Option<String> {
            std::env::var(key).ok().filter(|v| !v.trim().is_empty())
        }
        Ok(Self {
            database_url: opt("DATABASE_URL")
                .ok_or_else(|| anyhow::anyhow!("DATABASE_URL is required"))?,
            jwt_secret: opt("JWT_SECRET")
                .ok_or_else(|| anyhow::anyhow!("JWT_SECRET is required"))?,
            resend_api_key: opt("RESEND_API_KEY"),
            email_from: opt("EMAIL_FROM_ADDRESS")
                .unwrap_or_else(|| "Dulac My Camp <no-reply@dulacmycamp.com>".into()),
            frontend_url: opt("FRONTEND_URL").unwrap_or_else(|| "http://localhost:5173".into()),
            api_base_url: opt("API_BASE_URL").unwrap_or_else(|| "http://localhost:8080".into()),
            owner_email: opt("OWNER_EMAIL"),
            admin_email: opt("ADMIN_EMAIL"),
            // 8762928 is NOAA's Cocodrie gauge (29.245, -90.662) — the water
            // Dulac actually drains into. Override per .env if needed.
            noaa_station_id: opt("NOAA_STATION_ID").unwrap_or_else(|| "8762928".into()),
            capacity_adults: opt("CAPACITY_ADULTS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(6),
            bind_addr: opt("BIND_ADDR").unwrap_or_else(|| {
                let port = opt("PORT").unwrap_or_else(|| "8080".into());
                format!("0.0.0.0:{port}")
            }),
        })
    }
}

/// A cached upstream response plus the time it was fetched.
pub struct CacheEntry {
    pub fetched_at: std::time::Instant,
    pub value: serde_json::Value,
}

/// NOAA responses are cached in-process; both feeds change far more slowly
/// than the landing page is loaded.
#[derive(Default)]
pub struct Caches {
    pub weather: RwLock<Option<CacheEntry>>,
    pub tides: RwLock<Option<CacheEntry>>,
}

pub struct AppState {
    pub db: PgPool,
    pub cfg: Config,
    pub http: reqwest::Client,
    pub cache: Caches,
    pub limits: rate_limit::RateLimits,
}

pub type Shared = Arc<AppState>;

/// Builds the pool, runs migrations, and assembles shared state.
pub async fn build_state(cfg: Config) -> anyhow::Result<Shared> {
    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&cfg.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&db).await?;

    let http = reqwest::Client::builder()
        // api.weather.gov rejects requests without an identifying User-Agent.
        .user_agent("dulacmycamp/0.1 (marc@recoresystems.net)")
        .timeout(Duration::from_secs(15))
        .build()?;

    Ok(Arc::new(AppState {
        db,
        cfg,
        http,
        cache: Caches::default(),
        limits: rate_limit::RateLimits::default(),
    }))
}

/// Every HTTP route in the service.
pub fn router(state: Shared) -> Router {
    let cors = cors_layer(&state.cfg);

    let api = Router::new()
        .route("/health", get(health))
        .route("/config", get(public_config))
        // ── auth ──
        .route("/auth/request-otp", post(auth::request_otp))
        .route("/auth/verify-otp", post(auth::verify_otp))
        .route("/auth/me", get(auth::me))
        // ── bookings ──
        // The two token routes sit above `/bookings/{id}` on purpose: they are
        // unauthenticated and matched by a literal segment.
        .route("/bookings/approve/{token}", get(bookings::approve_by_token))
        .route(
            "/bookings/deny/{token}",
            get(bookings::deny_by_token).post(bookings::deny_by_token_submit),
        )
        .route("/bookings", get(bookings::list).post(bookings::create))
        .route("/bookings/{id}", get(bookings::get_one))
        .route("/bookings/{id}/cancel", put(bookings::cancel))
        .route("/bookings/{id}/approve", put(bookings::admin_approve))
        .route("/bookings/{id}/deny", put(bookings::admin_deny))
        // ── users ──
        .route("/users/me", get(users::get_me).put(users::update_me))
        .route("/users", get(users::list_all))
        .route("/users/{id}/role", put(users::update_role))
        // ── calendar annotations ──
        .route(
            "/blackout-dates",
            get(admin::list_blackouts).post(admin::create_blackout),
        )
        .route("/blackout-dates/{id}", delete(admin::delete_blackout))
        .route("/events", get(events::list).post(events::create))
        .route("/events/{id}", put(events::update).delete(events::remove))
        // Reference-only, computed — never touches availability or capacity.
        .route("/holidays", get(holidays::list))
        // ── inbox ──
        .route(
            "/messages",
            get(notifications::list).post(notifications::send),
        )
        .route("/messages/{id}/read", put(notifications::mark_read))
        .route("/messages/{id}", delete(notifications::remove))
        // ── environment feeds ──
        .route("/weather", get(weather::weather))
        .route("/tides", get(weather::tides))
        .route("/lunar", get(weather::lunar));

    Router::new()
        .nest("/api", api)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

fn cors_layer(cfg: &Config) -> CorsLayer {
    let mut origins: Vec<HeaderValue> = ["http://localhost:5173", "http://127.0.0.1:5173"]
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    if let Ok(v) = cfg.frontend_url.trim_end_matches('/').parse() {
        origins.push(v);
    }
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Non-secret settings the frontend needs so capacity rules don't have to be
/// duplicated (and drift) between the API and the UI.
async fn public_config(
    axum::extract::State(state): axum::extract::State<Shared>,
) -> Json<serde_json::Value> {
    Json(json!({
        "camp_name": "Dulac My Camp",
        "location": "Dulac, Louisiana",
        "capacity_adults": state.cfg.capacity_adults,
        "latitude": CAMP_LAT,
        "longitude": CAMP_LON,
    }))
}

/// Error envelope: `{"error": {"code": "...", "message": "..."}}`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            Self::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT"),
            Self::TooManyRequests(_) => (StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL"),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Self::NotFound("Not found".into()),
            other => Self::Internal(other.into()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        // Internal errors are logged in full but never echoed to the client.
        let message = match &self {
            Self::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                "Something went wrong on our end.".to_string()
            }
            other => other.to_string(),
        };
        (
            status,
            Json(json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

pub type ApiResult<T> = Result<T, AppError>;
