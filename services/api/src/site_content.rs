//! Admin-editable site content: hero/about text, house rules, amenities, and
//! a photo gallery. Replaces the hardcoded RULES/AMENITIES arrays and
//! placeholder images in `Landing.tsx` — content changes need no code edit
//! or redeploy, just an admin with the Site Content tab open.
//!
//! `site_settings` is a fixed-id singleton (see migration 0005): every read
//! and write here targets [`SETTINGS_ID`], and nothing ever inserts a second
//! row.

use crate::{
    ApiResult, AppError, Shared,
    auth::{AdminUser, AuthUser},
};
use axum::{
    Json,
    extract::{Multipart, Path, State, multipart::Field},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};
use std::path::Path as FsPath;
use uuid::Uuid;

const SETTINGS_ID: Uuid = Uuid::from_u128(1);

/// The hard cap on a single uploaded image.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// The `DefaultBodyLimit` applied to upload routes — comfortably above
/// [`MAX_IMAGE_BYTES`] so a realistically-oversized photo (a raw export, a
/// phone shot at full resolution) still reaches our own "too big" message
/// instead of a bare multipart-parse failure from the request-size
/// middleware. Only a genuinely abusive request is cut off before that.
pub const UPLOAD_REQUEST_LIMIT: usize = MAX_IMAGE_BYTES * 3;

fn ext_for_content_type(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

/// Checks an upload against the type/size rules, returning the extension to
/// save it under. Split out from the multipart-reading code so it's testable
/// without a request.
fn validate_upload(content_type: &str, size: usize) -> ApiResult<&'static str> {
    let ext = ext_for_content_type(content_type).ok_or_else(|| {
        AppError::BadRequest("Only JPG, PNG, and WEBP images are allowed.".into())
    })?;
    if size > MAX_IMAGE_BYTES {
        return Err(AppError::BadRequest(format!(
            "Images must be {}MB or smaller.",
            MAX_IMAGE_BYTES / (1024 * 1024)
        )));
    }
    Ok(ext)
}

fn bad_multipart(e: axum::extract::multipart::MultipartError) -> AppError {
    AppError::BadRequest(format!("Malformed upload: {e}"))
}

/// Validates and writes one multipart field to `UPLOAD_DIR` under a
/// generated name — the client's filename is never trusted or used.
async fn save_upload(state: &Shared, field: Field<'_>) -> ApiResult<String> {
    let content_type = field.content_type().unwrap_or_default().to_string();
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("Could not read the upload: {e}")))?;
    let ext = validate_upload(&content_type, bytes.len())?;

    let filename = format!("{}.{ext}", Uuid::new_v4());
    let path = FsPath::new(&state.cfg.upload_dir).join(&filename);
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(anyhow::Error::from)?;

    Ok(format!("/uploads/{filename}"))
}

/// Best-effort delete of a previously-saved upload. Never fails the caller's
/// request — a stale file left on disk is a cleanup task, not an outage; a
/// missing file (already gone) isn't even worth a log line.
async fn delete_upload_file(state: &Shared, url: &str) {
    let Some(filename) = url.strip_prefix("/uploads/") else {
        tracing::warn!(%url, "upload URL has an unexpected shape — not deleting");
        return;
    };
    let path = FsPath::new(&state.cfg.upload_dir).join(filename);
    if let Err(e) = tokio::fs::remove_file(&path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(error = ?e, %url, "failed to remove old upload file");
    }
}

// ─────────────────────────── settings ───────────────────────────

#[derive(Debug, Serialize, FromRow)]
struct SettingsRow {
    hero_title: String,
    hero_subtitle: String,
    about_text: String,
    hero_image_url: Option<String>,
    guest_photos_url: Option<String>,
}

async fn fetch_settings(db: &PgPool) -> Result<SettingsRow, sqlx::Error> {
    sqlx::query_as::<_, SettingsRow>(
        "SELECT hero_title, hero_subtitle, about_text, hero_image_url, guest_photos_url
         FROM site_settings WHERE id = $1",
    )
    .bind(SETTINGS_ID)
    .fetch_one(db)
    .await
}

#[derive(Debug, Serialize, FromRow)]
pub struct RulePublic {
    pub id: Uuid,
    pub text: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AmenityPublic {
    pub id: Uuid,
    pub label: String,
    pub icon: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct GalleryPublic {
    pub id: Uuid,
    pub url: String,
    pub caption: Option<String>,
}

/// Everything `GET /api/site-content` hands to an anonymous visitor.
///
/// A struct rather than an ad-hoc `json!` so the public shape is stated in
/// one place and checked by the compiler. `guest_photos_url` is deliberately
/// not a field: the album is for people who have actually stayed at the camp
/// (see [`guest_photos_link`]), and leaving it out here means a future edit
/// to `fetch_settings` cannot quietly put it back on the public endpoint.
#[derive(Debug, Serialize)]
pub struct PublicSiteContent {
    pub hero_title: String,
    pub hero_subtitle: String,
    pub about_text: String,
    pub hero_image_url: Option<String>,
    pub rules: Vec<RulePublic>,
    pub amenities: Vec<AmenityPublic>,
    pub gallery: Vec<GalleryPublic>,
}

/// `GET /api/site-content` — public, no auth. Everything the landing page
/// needs in one payload, so it's one fetch instead of five.
pub async fn get_site_content(State(state): State<Shared>) -> ApiResult<Json<PublicSiteContent>> {
    let settings = fetch_settings(&state.db).await?;
    let rules = sqlx::query_as::<_, RulePublic>(
        "SELECT id, text FROM rules_items ORDER BY sort_order, created_at",
    )
    .fetch_all(&state.db)
    .await?;
    let amenities = sqlx::query_as::<_, AmenityPublic>(
        "SELECT id, label, icon FROM amenities_items ORDER BY sort_order, created_at",
    )
    .fetch_all(&state.db)
    .await?;
    let gallery = sqlx::query_as::<_, GalleryPublic>(
        "SELECT id, url, caption FROM gallery_photos ORDER BY sort_order, created_at",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(PublicSiteContent {
        hero_title: settings.hero_title,
        hero_subtitle: settings.hero_subtitle,
        about_text: settings.about_text,
        hero_image_url: settings.hero_image_url,
        rules,
        amenities,
        gallery,
    }))
}

/// `GET /api/admin/site-content/settings` — admin only.
///
/// The Site Content tab seeds its form from this rather than from the public
/// payload above, which no longer carries `guest_photos_url`. Without it the
/// admin's link field would load blank and the next save would write that
/// blank straight over the stored link.
pub async fn get_settings_admin(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Value>> {
    let settings = fetch_settings(&state.db).await?;
    Ok(Json(json!(settings)))
}

// ─────────────────────── guest photos link ───────────────────────

/// Whether one booking, by status alone, grants access to the photo album.
///
/// Deliberately looser than [`crate::checkin_info::has_access`], and the two
/// must not be conflated: check-in info is operational (the key location, the
/// wifi code) and stops the moment a stay is checked out, whereas the album is
/// a communal keepsake. Someone who stayed at the camp helped make it, so
/// their access does not expire — not when they check out, and not when the
/// stay recedes into the past. Dates are irrelevant here for the same reason.
fn grants_photos_access(status: &str) -> bool {
    status == "approved"
}

/// Whether *any* of a user's bookings grants album access. Pure mirror of the
/// `EXISTS` query in [`has_photos_access`] — kept here as the tested,
/// documented spec of the rule.
pub fn has_access(statuses: &[&str]) -> bool {
    statuses.iter().any(|status| grants_photos_access(status))
}

/// Whether `user_id` may see the album. See [`has_access`] for the rule; this
/// evaluates it as one SQL `EXISTS` rather than pulling every booking into
/// Rust to fold over.
async fn has_photos_access(db: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let (allowed,): (bool,) = sqlx::query_as(
        "SELECT EXISTS(
            SELECT 1 FROM bookings WHERE user_id = $1 AND status = 'approved'
         )",
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(allowed)
}

#[derive(Debug, Serialize)]
pub struct GuestPhotosLink {
    /// `None` when no admin has set a link yet — a normal state for an
    /// otherwise-eligible guest, and distinct from being turned away.
    pub url: Option<String>,
}

/// `GET /api/guest-photos-link` — the album link, for anyone who has stayed.
///
/// 403 for a guest with no approved booking rather than `{ "url": null }`,
/// matching [`crate::checkin_info::list_for_guest`]: null already means
/// "nobody has set one yet", and one shape cannot carry both answers without
/// the frontend guessing which it got.
pub async fn guest_photos_link(
    State(state): State<Shared>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<GuestPhotosLink>> {
    if !has_photos_access(&state.db, user.id).await? {
        return Err(AppError::Forbidden(
            "The camp photo album is for guests who have stayed with us.".into(),
        ));
    }

    let settings = fetch_settings(&state.db).await?;
    Ok(Json(GuestPhotosLink {
        url: settings.guest_photos_url,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettings {
    pub hero_title: String,
    pub hero_subtitle: String,
    pub about_text: String,
    pub guest_photos_url: Option<String>,
}

/// `PUT /api/admin/site-content/settings` — admin only.
pub async fn update_settings(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Json(body): Json<UpdateSettings>,
) -> ApiResult<Json<Value>> {
    let hero_title = body.hero_title.trim();
    if hero_title.is_empty() {
        return Err(AppError::BadRequest("Hero title is required.".into()));
    }
    let hero_subtitle = body.hero_subtitle.trim();
    if hero_subtitle.is_empty() {
        return Err(AppError::BadRequest("Hero subtitle is required.".into()));
    }
    let guest_photos_url = body
        .guest_photos_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(url) = guest_photos_url
        && !(url.starts_with("http://") || url.starts_with("https://"))
    {
        return Err(AppError::BadRequest(
            "Guest photos link must start with http:// or https://.".into(),
        ));
    }

    sqlx::query(
        "UPDATE site_settings
         SET hero_title = $2, hero_subtitle = $3, about_text = $4, guest_photos_url = $5,
             updated_at = now()
         WHERE id = $1",
    )
    .bind(SETTINGS_ID)
    .bind(hero_title)
    .bind(hero_subtitle)
    .bind(body.about_text.trim())
    .bind(guest_photos_url)
    .execute(&state.db)
    .await?;

    let settings = fetch_settings(&state.db).await?;
    Ok(Json(json!(settings)))
}

/// `POST /api/admin/site-content/hero-image` — admin only, multipart. Field
/// name `file`. Replaces `hero_image_url`; the old file is removed only
/// after the new one is written and the row is updated.
pub async fn upload_hero_image(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    mut multipart: Multipart,
) -> ApiResult<Json<Value>> {
    let mut saved_url = None;
    while let Some(field) = multipart.next_field().await.map_err(bad_multipart)? {
        if field.name() == Some("file") {
            saved_url = Some(save_upload(&state, field).await?);
            break;
        }
    }
    let url = saved_url.ok_or_else(|| AppError::BadRequest("No file provided.".into()))?;

    let old_url = fetch_settings(&state.db).await?.hero_image_url;

    sqlx::query("UPDATE site_settings SET hero_image_url = $2, updated_at = now() WHERE id = $1")
        .bind(SETTINGS_ID)
        .bind(&url)
        .execute(&state.db)
        .await?;

    if let Some(old_url) = old_url {
        delete_upload_file(&state, &old_url).await;
    }

    Ok(Json(json!({ "hero_image_url": url })))
}

// ─────────────────────────── rules ───────────────────────────

#[derive(Debug, Serialize, FromRow)]
pub struct RuleItem {
    pub id: Uuid,
    pub text: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

const RULE_COLUMNS: &str = "id, text, sort_order, created_at";

pub async fn list_rules_admin(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<RuleItem>>> {
    let rows = sqlx::query_as::<_, RuleItem>(&format!(
        "SELECT {RULE_COLUMNS} FROM rules_items ORDER BY sort_order, created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct RuleBody {
    pub text: String,
    pub sort_order: i32,
}

pub async fn create_rule(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Json(body): Json<RuleBody>,
) -> ApiResult<Json<RuleItem>> {
    let text = body.text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("Rule text is required.".into()));
    }
    let row = sqlx::query_as::<_, RuleItem>(&format!(
        "INSERT INTO rules_items (text, sort_order) VALUES ($1, $2) RETURNING {RULE_COLUMNS}"
    ))
    .bind(text)
    .bind(body.sort_order)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

pub async fn update_rule(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RuleBody>,
) -> ApiResult<Json<RuleItem>> {
    let text = body.text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("Rule text is required.".into()));
    }
    let row = sqlx::query_as::<_, RuleItem>(&format!(
        "UPDATE rules_items SET text = $2, sort_order = $3 WHERE id = $1 RETURNING {RULE_COLUMNS}"
    ))
    .bind(id)
    .bind(text)
    .bind(body.sort_order)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Rule not found.".into()))?;
    Ok(Json(row))
}

pub async fn delete_rule(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let done = sqlx::query("DELETE FROM rules_items WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Rule not found.".into()));
    }
    Ok(Json(json!({ "deleted": true })))
}

// ─────────────────────────── amenities ───────────────────────────

#[derive(Debug, Serialize, FromRow)]
pub struct AmenityItem {
    pub id: Uuid,
    pub label: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

const AMENITY_COLUMNS: &str = "id, label, icon, sort_order, created_at";

pub async fn list_amenities_admin(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<AmenityItem>>> {
    let rows = sqlx::query_as::<_, AmenityItem>(&format!(
        "SELECT {AMENITY_COLUMNS} FROM amenities_items ORDER BY sort_order, created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct AmenityBody {
    pub label: String,
    pub icon: Option<String>,
    pub sort_order: i32,
}

impl AmenityBody {
    fn validated(&self) -> ApiResult<(&str, Option<&str>)> {
        let label = self.label.trim();
        if label.is_empty() {
            return Err(AppError::BadRequest("Amenity label is required.".into()));
        }
        let icon = self
            .icon
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        Ok((label, icon))
    }
}

pub async fn create_amenity(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Json(body): Json<AmenityBody>,
) -> ApiResult<Json<AmenityItem>> {
    let (label, icon) = body.validated()?;
    let row = sqlx::query_as::<_, AmenityItem>(&format!(
        "INSERT INTO amenities_items (label, icon, sort_order)
         VALUES ($1, $2, $3) RETURNING {AMENITY_COLUMNS}"
    ))
    .bind(label)
    .bind(icon)
    .bind(body.sort_order)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

pub async fn update_amenity(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AmenityBody>,
) -> ApiResult<Json<AmenityItem>> {
    let (label, icon) = body.validated()?;
    let row = sqlx::query_as::<_, AmenityItem>(&format!(
        "UPDATE amenities_items SET label = $2, icon = $3, sort_order = $4
         WHERE id = $1 RETURNING {AMENITY_COLUMNS}"
    ))
    .bind(id)
    .bind(label)
    .bind(icon)
    .bind(body.sort_order)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Amenity not found.".into()))?;
    Ok(Json(row))
}

pub async fn delete_amenity(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let done = sqlx::query("DELETE FROM amenities_items WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    if done.rows_affected() == 0 {
        return Err(AppError::NotFound("Amenity not found.".into()));
    }
    Ok(Json(json!({ "deleted": true })))
}

// ─────────────────────────── gallery ───────────────────────────

#[derive(Debug, Serialize, FromRow)]
pub struct GalleryPhotoItem {
    pub id: Uuid,
    pub url: String,
    pub caption: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

const GALLERY_COLUMNS: &str = "id, url, caption, sort_order, created_at";

pub async fn list_gallery_admin(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<GalleryPhotoItem>>> {
    let rows = sqlx::query_as::<_, GalleryPhotoItem>(&format!(
        "SELECT {GALLERY_COLUMNS} FROM gallery_photos ORDER BY sort_order, created_at"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// `POST /api/admin/gallery` — admin only, multipart. Field name `file`,
/// optional field name `caption`. New photos are appended to the end of the
/// order — the admin reorders afterward if it needs to move.
pub async fn create_gallery_photo(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    mut multipart: Multipart,
) -> ApiResult<Json<GalleryPhotoItem>> {
    let mut url = None;
    let mut caption = None;
    while let Some(field) = multipart.next_field().await.map_err(bad_multipart)? {
        match field.name() {
            Some("file") => url = Some(save_upload(&state, field).await?),
            Some("caption") => {
                let text = field.text().await.map_err(bad_multipart)?;
                caption = Some(text).filter(|s: &String| !s.trim().is_empty());
            }
            _ => {}
        }
    }
    let url = url.ok_or_else(|| AppError::BadRequest("No file provided.".into()))?;

    let row = sqlx::query_as::<_, GalleryPhotoItem>(&format!(
        "INSERT INTO gallery_photos (url, caption, sort_order)
         VALUES ($1, $2, COALESCE((SELECT max(sort_order) + 1 FROM gallery_photos), 0))
         RETURNING {GALLERY_COLUMNS}"
    ))
    .bind(&url)
    .bind(caption)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(row))
}

#[derive(Debug, Deserialize)]
pub struct GalleryUpdateBody {
    pub caption: Option<String>,
    pub sort_order: i32,
}

pub async fn update_gallery_photo(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<GalleryUpdateBody>,
) -> ApiResult<Json<GalleryPhotoItem>> {
    let caption = body
        .caption
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let row = sqlx::query_as::<_, GalleryPhotoItem>(&format!(
        "UPDATE gallery_photos SET caption = $2, sort_order = $3
         WHERE id = $1 RETURNING {GALLERY_COLUMNS}"
    ))
    .bind(id)
    .bind(caption)
    .bind(body.sort_order)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Photo not found.".into()))?;
    Ok(Json(row))
}

/// `DELETE /api/admin/gallery/{id}` — removes the row and the file on disk.
pub async fn delete_gallery_photo(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row: Option<(String,)> =
        sqlx::query_as("DELETE FROM gallery_photos WHERE id = $1 RETURNING url")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (url,) = row.ok_or_else(|| AppError::NotFound("Photo not found.".into()))?;
    delete_upload_file(&state, &url).await;
    Ok(Json(json!({ "deleted": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_jpeg_png_webp() {
        assert!(validate_upload("image/jpeg", 1024).is_ok());
        assert!(validate_upload("image/png", 1024).is_ok());
        assert!(validate_upload("image/webp", 1024).is_ok());
    }

    #[test]
    fn rejects_disallowed_content_types() {
        assert!(validate_upload("image/gif", 1024).is_err());
        assert!(validate_upload("application/pdf", 1024).is_err());
        assert!(validate_upload("text/html", 1024).is_err());
        assert!(validate_upload("", 1024).is_err());
    }

    #[test]
    fn rejects_files_over_the_cap() {
        assert!(validate_upload("image/jpeg", MAX_IMAGE_BYTES).is_ok());
        assert!(validate_upload("image/jpeg", MAX_IMAGE_BYTES + 1).is_err());
    }

    // The public payload must not carry the album link. Asserting on the
    // serialized shape, not the struct, since that is what actually ships.
    #[test]
    fn the_public_payload_has_no_guest_photos_url_at_all() {
        let payload = PublicSiteContent {
            hero_title: "Dulac My Camp".into(),
            hero_subtitle: "On the bayou".into(),
            about_text: "A camp.".into(),
            hero_image_url: Some("/uploads/hero.jpg".into()),
            rules: vec![],
            amenities: vec![],
            gallery: vec![],
        };
        let json = serde_json::to_value(&payload).unwrap();
        let object = json.as_object().unwrap();

        // Absent, not present-and-null: `get` returns None either way, so
        // check the key set itself.
        assert!(!object.contains_key("guest_photos_url"));
        // The rest of the landing page still arrives.
        assert!(object.contains_key("hero_title"));
        assert!(object.contains_key("gallery"));
    }

    #[test]
    fn a_guest_with_no_approved_booking_is_turned_away() {
        assert!(!has_access(&[]));
        assert!(!has_access(&["pending"]));
        assert!(!has_access(&["denied"]));
        assert!(!has_access(&["cancelled"]));
        assert!(!has_access(&["pending", "denied", "cancelled"]));
    }

    #[test]
    fn one_approved_booking_is_enough() {
        assert!(has_access(&["approved"]));
        assert!(has_access(&["denied", "approved", "pending"]));
    }

    // The point of difference from check-in info, spelled out so the two
    // rules cannot be quietly merged later. Album access is decided by
    // status alone: no date, no checkout state, nothing that expires.
    #[test]
    fn a_completed_past_stay_still_grants_album_access() {
        // `crate::checkin_info::has_access` says false for this same guest,
        // whose only approved booking has been checked out.
        assert!(!crate::checkin_info::has_access(&[("approved", true)]));
        assert!(has_access(&["approved"]));
    }

    #[test]
    fn oversized_and_wrong_type_both_report_bad_request() {
        assert!(matches!(
            validate_upload("image/gif", 10),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            validate_upload("image/jpeg", MAX_IMAGE_BYTES + 1),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn picks_the_extension_matching_the_content_type() {
        assert_eq!(validate_upload("image/jpeg", 10).unwrap(), "jpg");
        assert_eq!(validate_upload("image/png", 10).unwrap(), "png");
        assert_eq!(validate_upload("image/webp", 10).unwrap(), "webp");
    }
}
