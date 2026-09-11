//! Dulac My Camp API — Axum service backing the camp booking app.
//!
//! Single property, single household: there is no tenant scoping. Access
//! control is two-tier (`guest` / `admin`) plus opaque single-use tokens that
//! let the camp owner approve or deny a stay straight from an email.

pub mod admin;
pub mod auth;
pub mod bookings;
pub mod checkin_info;
pub mod checklist;
pub mod checkout;
pub mod content_access;
pub mod email;
pub mod email_templates;
pub mod events;
pub mod holidays;
pub mod journal;
pub mod my_stay;
pub mod notifications;
pub mod password;
pub mod rate_limit;
pub mod site_content;
pub mod solunar;
pub mod users;
pub mod weather;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde_json::json;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

/// Dulac, Louisiana — the camp's own physical location. Used only where that
/// is what matters (the solar calculation for the moon/sun widget, "at the
/// camp"). NOT for the on-the-water feeds — see `FISHING_LAT`/`FISHING_LON`.
pub const CAMP_LAT: f64 = 29.3802;
pub const CAMP_LON: f64 = -90.7148;

/// The Cocodrie estuary — the open water people actually fish, ~15 miles south
/// of the camp and matching NOAA tide station `8762928`. Wind and barometric
/// pressure differ meaningfully between inland Dulac and the estuary; sun and
/// moon position do not (15 miles is nothing there). Used for the weather feed
/// and the solunar fishing forecast.
pub const FISHING_LAT: f64 = 29.245;
pub const FISHING_LON: f64 = -90.662;

/// Process configuration, read once at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub resend_api_key: Option<String>,
    pub email_from: String,
    pub frontend_url: String,
    /// A previous public origin of the frontend, still allowed through CORS
    /// while links built from it are in circulation. Unset once the old
    /// address is genuinely retired — no code change needed to drop it.
    pub legacy_frontend_url: Option<String>,
    /// Public origin of this API. Used to build the approve/deny links that
    /// go into the owner's email, so it must be reachable from their inbox.
    pub api_base_url: String,
    /// Bootstrap fallback for the approve/deny email. The recipient list is
    /// the `users.is_owner` flag; this is only used when no user carries it.
    pub owner_email: Option<String>,
    /// Admin notification address. Gets an informational copy, no buttons.
    pub admin_email: Option<String>,
    pub noaa_station_id: String,
    pub capacity_adults: i64,
    pub bind_addr: String,
    /// Where uploaded hero/gallery images are written and served from. Must
    /// point at a persistent volume in production — the container
    /// filesystem is wiped on every redeploy, and anything saved outside
    /// this path (or on a Railway service with no volume attached) does not
    /// survive one.
    pub upload_dir: String,
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
            legacy_frontend_url: opt("LEGACY_FRONTEND_URL"),
            api_base_url: opt("API_BASE_URL").unwrap_or_else(|| "http://localhost:8080".into()),
            owner_email: opt("OWNER_EMAIL"),
            admin_email: opt("ADMIN_EMAIL"),
            // 8762928 is NOAA's Cocodrie gauge (29.245, -90.662) — the water
            // Dulac actually drains into. Override per .env if needed.
            noaa_station_id: opt("NOAA_STATION_ID").unwrap_or_else(|| "8762928".into()),
            capacity_adults: opt("CAPACITY_ADULTS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            bind_addr: opt("BIND_ADDR").unwrap_or_else(|| {
                let port = opt("PORT").unwrap_or_else(|| "8080".into());
                format!("0.0.0.0:{port}")
            }),
            upload_dir: opt("UPLOAD_DIR").unwrap_or_else(|| "/data/uploads".into()),
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
    /// A short rolling window of recent station observations (newest first),
    /// shared by the weather card and the fishing forecast — the latter reads
    /// a barometric trend off it, which a single latest reading can't give.
    pub observations: RwLock<Option<CacheEntry>>,
    /// The full solunar fishing forecast payload (astronomical windows plus the
    /// weather-nudged star rating), sliced to the requested day count on return.
    pub fishing: RwLock<Option<CacheEntry>>,
    /// The tide station's decadal-average range (one number), for the fishing
    /// forecast's tide-strength modifier. Refreshed daily.
    pub tide_datums: RwLock<Option<CacheEntry>>,
    /// Max daytime wind per date from the NWS forecast for the fishing grounds,
    /// as `{"YYYY-MM-DD": mph}`. Two upstream hops that every forecast request
    /// — including arbitrary-range ones, which skip the payload cache above —
    /// otherwise repeats for the same answer.
    pub fishing_wind: RwLock<Option<CacheEntry>>,
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

    // A no-op if the path already exists (e.g. a mounted volume's root) —
    // only matters for a fresh local checkout with no UPLOAD_DIR override.
    tokio::fs::create_dir_all(&cfg.upload_dir).await?;

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
    let upload_dir = state.cfg.upload_dir.clone();

    let api = Router::new()
        .route("/health", get(health))
        .route("/config", get(public_config))
        // ── auth ──
        .route("/auth/request-otp", post(auth::request_otp))
        .route("/auth/verify-otp", post(auth::verify_otp))
        // Optional password sign-in, admins only — additive to the OTP flow
        // above, which stays the way in for everyone. See `auth`.
        .route("/auth/login-password", post(auth::login_password))
        .route("/auth/set-password", post(auth::set_password))
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
        .route("/bookings/{id}/cancel", put(bookings::cancel))
        // Cancel is the reversible tool and the one to reach for; the delete
        // is for rows that should never have been there at all.
        .route(
            "/bookings/{id}",
            get(bookings::get_one).delete(bookings::admin_delete),
        )
        // Correcting what a guest submitted, not a state change — see
        // `bookings::admin_update_guests`.
        .route("/bookings/{id}/guests", put(bookings::admin_update_guests))
        // The booker's own visibility preference. Not admin-only and not
        // gated on status — see `bookings::update_privacy`.
        .route("/bookings/{id}/privacy", put(bookings::update_privacy))
        .route("/bookings/{id}/approve", put(bookings::admin_approve))
        .route("/bookings/{id}/deny", put(bookings::admin_deny))
        // Admin entering a booking on a guest's behalf (phone call, in
        // person) — reuses bookings::create_booking_for verbatim.
        .route("/admin/bookings", post(bookings::admin_create))
        // ── users ──
        .route("/users/me", get(users::get_me).put(users::update_me))
        .route("/users", get(users::list_all))
        .route("/users/{id}/role", put(users::update_role))
        .route("/users/{id}/owner", put(users::update_owner))
        // Blocking is the reversible tool and the one to reach for; the
        // delete is only permitted on an account with no history at all.
        .route("/users/{id}/block", put(users::block))
        .route("/users/{id}/unblock", put(users::unblock))
        .route("/users/{id}", delete(users::remove))
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
        // ── site content ──
        .route("/site-content", get(site_content::get_site_content))
        // The album link is not part of the public payload above — it is for
        // people who have actually stayed. See `site_content::has_access`.
        .route("/guest-photos-link", get(site_content::guest_photos_link))
        // ── who can see what ──
        .route("/admin/content-access", get(content_access::list_admin))
        .route(
            "/admin/content-access/{section_key}",
            put(content_access::update_admin),
        )
        .route(
            "/admin/site-content/settings",
            get(site_content::get_settings_admin).put(site_content::update_settings),
        )
        .route(
            "/admin/site-content/hero-image",
            post(site_content::upload_hero_image)
                .layer(DefaultBodyLimit::max(site_content::UPLOAD_REQUEST_LIMIT)),
        )
        .route(
            "/admin/rules",
            get(site_content::list_rules_admin).post(site_content::create_rule),
        )
        .route(
            "/admin/rules/{id}",
            put(site_content::update_rule).delete(site_content::delete_rule),
        )
        .route(
            "/admin/amenities",
            get(site_content::list_amenities_admin).post(site_content::create_amenity),
        )
        .route(
            "/admin/amenities/{id}",
            put(site_content::update_amenity).delete(site_content::delete_amenity),
        )
        .route(
            "/admin/gallery",
            get(site_content::list_gallery_admin)
                .post(site_content::create_gallery_photo)
                .layer(DefaultBodyLimit::max(site_content::UPLOAD_REQUEST_LIMIT)),
        )
        .route(
            "/admin/gallery/{id}",
            put(site_content::update_gallery_photo).delete(site_content::delete_gallery_photo),
        )
        // ── checklist ──
        .route("/checklist", get(checklist::list_active))
        .route(
            "/admin/checklist",
            get(checklist::list_all).post(checklist::create),
        )
        .route(
            "/admin/checklist/{id}",
            put(checklist::update).delete(checklist::remove),
        )
        // ── checkout ──
        // Completing checkout is the trigger that unlocks the journal
        // prompt — chained inline via `journal_eligible` in the response,
        // not a separate background job.
        .route("/checkout/eligible", get(checkout::eligible))
        .route("/checkout", post(checkout::submit))
        .route("/admin/checkouts", get(checkout::admin_list))
        // ── journal ──
        .route("/journal", get(journal::list_public).post(journal::create))
        .route("/journal/mine", get(journal::list_mine))
        .route(
            "/journal/eligible-bookings",
            get(journal::eligible_bookings),
        )
        .route("/journal/admin", get(journal::admin_list))
        .route(
            "/journal/{id}",
            put(journal::update).delete(journal::remove),
        )
        .route("/journal/{id}/approve", put(journal::approve))
        .route("/journal/{id}/reject", put(journal::reject))
        .route("/journal/{id}/archive", put(journal::archive))
        .route("/journal/{id}/unarchive", put(journal::unarchive))
        // ── check-in info ──
        // Access is derived from booking state (approved, not yet checked
        // out) — never a separate admin grant/revoke step.
        .route("/checkin-info", get(checkin_info::list_for_guest))
        .route(
            "/admin/checkin-info",
            get(checkin_info::list_admin).post(checkin_info::create),
        )
        .route(
            "/admin/checkin-info/{id}",
            put(checkin_info::update).delete(checkin_info::remove),
        )
        .route("/my-stay", get(my_stay::my_stay))
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
        .route("/lunar", get(weather::lunar))
        // Reference-only, in the spirit of the lunar widget: solunar bite
        // windows (pure astronomy) plus a 1–5 star rating nudged by weather.
        .route("/fishing-forecast", get(solunar::fishing_forecast));

    Router::new()
        .nest("/api", api)
        // Not under /api — matches the plain `/uploads/{filename}` URLs
        // saved onto site_settings/gallery_photos rows.
        .nest_service("/uploads", ServeDir::new(upload_dir))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// The browser origins allowed to call this API: the two local dev servers,
/// the live frontend, and — while a retired address still has links pointing
/// at it — whatever `LEGACY_FRONTEND_URL` names.
fn allowed_origins(frontend_url: &str, legacy_frontend_url: Option<&str>) -> Vec<HeaderValue> {
    let mut origins: Vec<HeaderValue> = ["http://localhost:5173", "http://127.0.0.1:5173"]
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    // A blank or unparseable value is skipped rather than pushed: an empty
    // entry matches no origin and would quietly look like it was configured.
    for url in [Some(frontend_url), legacy_frontend_url]
        .into_iter()
        .flatten()
    {
        let url = url.trim().trim_end_matches('/');
        if url.is_empty() {
            continue;
        }
        if let Ok(v) = url.parse() {
            origins.push(v);
        }
    }
    origins
}

fn cors_layer(cfg: &Config) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(allowed_origins(
            &cfg.frontend_url,
            cfg.legacy_frontend_url.as_deref(),
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn origins(frontend: &str, legacy: Option<&str>) -> Vec<String> {
        allowed_origins(frontend, legacy)
            .iter()
            .map(|o| o.to_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn the_frontend_joins_the_dev_servers_in_the_allow_list() {
        let list = origins("https://www.example.com", None);
        assert_eq!(
            list,
            [
                "http://localhost:5173",
                "http://127.0.0.1:5173",
                "https://www.example.com",
            ]
        );
    }

    /// The whole point of the legacy slot: the old address keeps working
    /// without displacing the current one.
    #[test]
    fn a_legacy_origin_is_added_alongside_the_frontend() {
        let list = origins("https://www.example.com", Some("https://old.example.net"));
        assert!(list.contains(&"https://www.example.com".to_owned()));
        assert!(list.contains(&"https://old.example.net".to_owned()));
    }

    /// An unset legacy URL must leave the list exactly as it was — an extra
    /// empty entry would match nothing and read like it was configured.
    #[test]
    fn an_unset_legacy_origin_adds_nothing() {
        assert_eq!(
            origins("https://www.example.com", None),
            origins("https://www.example.com", Some("")),
        );
        assert_eq!(
            origins("https://www.example.com", None),
            origins("https://www.example.com", Some("   ")),
        );
    }

    /// Origins are compared verbatim, so a configured trailing slash would
    /// otherwise never match the `Origin` header a browser sends.
    #[test]
    fn trailing_slashes_are_trimmed_from_both_urls() {
        let list = origins("https://www.example.com/", Some("https://old.example.net/"));
        assert!(list.contains(&"https://www.example.com".to_owned()));
        assert!(list.contains(&"https://old.example.net".to_owned()));
    }
}
