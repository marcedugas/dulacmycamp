//! Image uploads to `UPLOAD_DIR`, shared by every feature that stores a
//! photo: the site content gallery and hero image (see
//! [`crate::site_content`]) and journal entry photos (see
//! [`crate::journal`]).
//!
//! Extracted so those features cannot drift on the parts that matter —
//! which content types are allowed, how big a file may be, and the rule that
//! the client's filename is never trusted or reused.

use crate::{ApiResult, AppError, Shared};
use axum::extract::multipart::Field;
use std::path::Path as FsPath;
use uuid::Uuid;

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
pub fn validate_upload(content_type: &str, size: usize) -> ApiResult<&'static str> {
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

pub fn bad_multipart(e: axum::extract::multipart::MultipartError) -> AppError {
    AppError::BadRequest(format!("Malformed upload: {e}"))
}

/// Validates and writes one multipart field to `UPLOAD_DIR` under a
/// generated name — the client's filename is never trusted or used.
pub async fn save_upload(state: &Shared, field: Field<'_>) -> ApiResult<String> {
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
pub async fn delete_upload_file(state: &Shared, url: &str) {
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
