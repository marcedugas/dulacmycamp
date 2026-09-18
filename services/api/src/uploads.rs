//! Image uploads to `UPLOAD_DIR`, shared by every feature that stores a
//! photo: the site content gallery and hero image (see
//! [`crate::site_content`]) and journal entry photos (see
//! [`crate::journal`]).
//!
//! Extracted so those features cannot drift on the parts that matter —
//! which content types are allowed, how big a file may be, how it is
//! compressed, and the rule that the client's filename is never trusted or
//! reused.
//!
//! ## Compression
//!
//! Every upload passes through [`compress_image`] before it touches disk.
//! The volume is a few GB shared by every photo feature, and a phone camera
//! JPEG is routinely 3-8MB at 4000px+ — far more than a web page ever shows.
//! So:
//!
//! - **JPEG** is always decoded, turned upright per its EXIF orientation,
//!   shrunk to [`MAX_EDGE_PX`] on the long edge if larger, and re-encoded at
//!   [`JPEG_QUALITY`]. Always, not only when oversized: the re-encode is also
//!   what strips EXIF, including GPS coordinates, which a family camp site
//!   has no business storing.
//! - **PNG** stays PNG — it may be a logo whose transparency matters. It is
//!   only re-encoded when it needs shrinking; otherwise the original bytes are
//!   kept, since a re-encode would gain nothing and could come out larger.
//! - **WebP** is kept as-is unless oversized. The `image` crate only encodes
//!   WebP losslessly, which would *grow* a photo, so an oversized one is
//!   shrunk and saved as JPEG (or PNG, if it has transparency to keep).

use crate::{ApiResult, AppError, Shared};
use axum::extract::multipart::Field;
use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, codecs::jpeg::JpegEncoder,
    imageops::FilterType,
};
use std::io::Cursor;
use std::path::Path as FsPath;
use uuid::Uuid;

/// The hard cap on a single uploaded file, checked before any decoding. A
/// sanity backstop rather than the storage control — compression is that —
/// so it is set well above what any phone camera produces.
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// The `DefaultBodyLimit` applied to upload routes — above
/// [`MAX_IMAGE_BYTES`] so a realistically-oversized photo still reaches our
/// own "too big" message instead of a bare multipart-parse failure from the
/// request-size middleware. Only a genuinely abusive request is cut off
/// before that.
pub const UPLOAD_REQUEST_LIMIT: usize = MAX_IMAGE_BYTES * 2;

/// Longest edge a stored photo may have. Comfortably more than the widest
/// the site ever displays one, including the lightbox on a large monitor.
pub const MAX_EDGE_PX: u32 = 2000;

/// Re-encode quality for JPEGs: no visible loss at the sizes this site shows
/// photos, a fraction of a camera original's bytes.
pub const JPEG_QUALITY: u8 = 80;

fn ext_for_content_type(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

fn too_big() -> AppError {
    AppError::BadRequest(format!(
        "Photos must be {}MB or smaller.",
        MAX_IMAGE_BYTES / (1024 * 1024)
    ))
}

/// Checks an upload against the type/size rules, returning the extension to
/// save it under. Split out from the multipart-reading code so it's testable
/// without a request.
pub fn validate_upload(content_type: &str, size: usize) -> ApiResult<&'static str> {
    let ext = ext_for_content_type(content_type).ok_or_else(|| {
        AppError::BadRequest("Only JPG, PNG, and WEBP images are allowed.".into())
    })?;
    if size > MAX_IMAGE_BYTES {
        return Err(too_big());
    }
    Ok(ext)
}

/// An upload after [`compress_image`]: the bytes to write and the extension
/// they are actually encoded as, which may differ from what was sent.
#[derive(Debug)]
pub struct Processed {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
}

fn not_an_image() -> AppError {
    AppError::BadRequest("That file doesn't look like a JPG, PNG, or WEBP image.".into())
}

/// Decodes, orients, shrinks and re-encodes one upload — see the module docs
/// for the per-format rules. CPU-bound: call it off the async runtime.
///
/// The format is sniffed from the bytes, never taken from the declared
/// content type, so a renamed PDF is refused here even though it claimed to
/// be `image/jpeg`.
pub fn compress_image(original: Vec<u8>) -> ApiResult<Processed> {
    let reader = ImageReader::new(Cursor::new(&original))
        .with_guessed_format()
        .map_err(|_| not_an_image())?;
    let format = match reader.format() {
        Some(f @ (ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)) => f,
        _ => return Err(not_an_image()),
    };

    // The reader's default limits (512MB of decode buffer) are what stand
    // between a small-on-disk "decompression bomb" and the server's memory.
    let mut decoder = reader.into_decoder().map_err(decode_error)?;
    let orientation = decoder.orientation().map_err(decode_error)?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    // Before the resize, so the long edge is measured the way it displays.
    img.apply_orientation(orientation);

    let oversized = img.width().max(img.height()) > MAX_EDGE_PX;
    if oversized {
        // `resize` fits within the box and keeps the aspect ratio.
        img = img.resize(MAX_EDGE_PX, MAX_EDGE_PX, FilterType::CatmullRom);
    }

    match format {
        ImageFormat::Jpeg => encode_jpeg(&img),
        ImageFormat::Png if oversized => encode_png(&img),
        ImageFormat::WebP if oversized && img.color().has_alpha() => encode_png(&img),
        ImageFormat::WebP if oversized => encode_jpeg(&img),
        // Already a sensible size, and a re-encode would only cost bytes.
        ImageFormat::Png => Ok(Processed {
            bytes: original,
            ext: "png",
        }),
        _ => Ok(Processed {
            bytes: original,
            ext: "webp",
        }),
    }
}

fn decode_error(e: image::ImageError) -> AppError {
    match e {
        image::ImageError::Limits(_) => {
            AppError::BadRequest("That image is too large to process.".into())
        }
        _ => AppError::BadRequest("That image couldn't be read — it may be damaged.".into()),
    }
}

fn encode_jpeg(img: &DynamicImage) -> ApiResult<Processed> {
    let mut bytes = Vec::new();
    // JPEG has no alpha channel; flatten rather than let the encoder refuse.
    JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)
        .encode_image(&img.to_rgb8())
        .map_err(|e| anyhow::anyhow!("jpeg encode failed: {e}"))?;
    Ok(Processed { bytes, ext: "jpg" })
}

fn encode_png(img: &DynamicImage) -> ApiResult<Processed> {
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| anyhow::anyhow!("png encode failed: {e}"))?;
    Ok(Processed { bytes, ext: "png" })
}

pub fn bad_multipart(e: axum::extract::multipart::MultipartError) -> AppError {
    AppError::BadRequest(format!("Malformed upload: {e}"))
}

/// Validates, compresses and writes one multipart field to `UPLOAD_DIR`
/// under a generated name — the client's filename is never trusted or used.
///
/// The body is read chunk by chunk so an oversized file is refused as soon as
/// it crosses [`MAX_IMAGE_BYTES`], before anything tries to decode it.
pub async fn save_upload(state: &Shared, mut field: Field<'_>) -> ApiResult<String> {
    let content_type = field.content_type().unwrap_or_default().to_string();
    validate_upload(&content_type, 0)?;

    let mut bytes = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| AppError::BadRequest(format!("Could not read the upload: {e}")))?
    {
        if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
            return Err(too_big());
        }
        bytes.extend_from_slice(&chunk);
    }

    let original_len = bytes.len();
    let processed = tokio::task::spawn_blocking(move || compress_image(bytes))
        .await
        .map_err(anyhow::Error::from)??;
    tracing::info!(
        original_bytes = original_len,
        stored_bytes = processed.bytes.len(),
        ext = processed.ext,
        "stored upload"
    );

    let filename = format!("{}.{}", Uuid::new_v4(), processed.ext);
    let path = FsPath::new(&state.cfg.upload_dir).join(&filename);
    tokio::fs::write(&path, &processed.bytes)
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
    use image::{GenericImageView, ImageEncoder, Rgb, RgbImage, Rgba, RgbaImage};

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

    // ─────────────── compression ───────────────

    /// A noisy gradient: flat colour would compress to nothing and prove
    /// nothing about sizes.
    fn photo(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let n = (x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)) % 23;
            Rgb([(x % 256) as u8, (y % 256) as u8, (n * 11) as u8])
        })
    }

    fn jpeg(img: &RgbImage, quality: u8) -> Vec<u8> {
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, quality)
            .encode_image(img)
            .unwrap();
        out
    }

    fn png(img: &DynamicImage) -> Vec<u8> {
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn decoded(bytes: &[u8]) -> DynamicImage {
        image::load_from_memory(bytes).unwrap()
    }

    /// Splices a minimal EXIF APP1 segment carrying only an orientation tag
    /// in right after the JPEG's SOI marker — what a phone does when it
    /// stores a portrait shot as landscape pixels plus a "rotate me" flag.
    fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut tiff = vec![b'M', b'M', 0, 42, 0, 0, 0, 8]; // big-endian, IFD at 8
        tiff.extend_from_slice(&1u16.to_be_bytes()); // one entry
        tiff.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
        tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_be_bytes()); // count
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&[0, 0]); // value padding
        tiff.extend_from_slice(&0u32.to_be_bytes()); // no next IFD

        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&tiff);
        let mut out = jpeg[..2].to_vec(); // SOI
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    #[test]
    fn an_oversized_jpeg_is_shrunk_to_the_long_edge_and_stays_jpeg() {
        let original = jpeg(&photo(4000, 3000), 95);
        let out = compress_image(original.clone()).unwrap();
        assert_eq!(out.ext, "jpg");
        assert_eq!(decoded(&out.bytes).dimensions(), (2000, 1500));
        assert!(
            out.bytes.len() < original.len() / 2,
            "{} → {} bytes is not meaningful compression",
            original.len(),
            out.bytes.len()
        );
    }

    #[test]
    fn a_portrait_jpeg_is_measured_on_its_long_edge() {
        let out = compress_image(jpeg(&photo(1500, 3000), 90)).unwrap();
        assert_eq!(decoded(&out.bytes).dimensions(), (1000, 2000));
    }

    #[test]
    fn a_small_jpeg_keeps_its_size_but_is_still_re_encoded() {
        let original = with_exif_orientation(&jpeg(&photo(800, 600), 95), 1);
        let out = compress_image(original.clone()).unwrap();
        assert_eq!(decoded(&out.bytes).dimensions(), (800, 600));
        // The re-encode is what strips EXIF (GPS included) — so it must
        // happen even when no resize was needed.
        assert!(!out.bytes.windows(4).any(|w| w == b"Exif"));
    }

    #[test]
    fn exif_orientation_is_applied_so_phone_photos_stay_upright() {
        // Orientation 6: the pixels are stored landscape, display rotated 90°.
        let original = with_exif_orientation(&jpeg(&photo(3000, 1000), 90), 6);
        let out = compress_image(original).unwrap();
        assert_eq!(decoded(&out.bytes).dimensions(), (667, 2000));
    }

    #[test]
    fn a_small_png_is_stored_byte_for_byte() {
        let original = png(&DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            300,
            200,
            Rgba([10, 20, 30, 0]),
        )));
        let out = compress_image(original.clone()).unwrap();
        assert_eq!(out.ext, "png");
        assert_eq!(out.bytes, original);
    }

    #[test]
    fn an_oversized_png_is_shrunk_but_keeps_its_transparency() {
        let original = png(&DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            3000,
            1000,
            Rgba([200, 100, 50, 0]),
        )));
        let out = compress_image(original).unwrap();
        assert_eq!(out.ext, "png");
        let img = decoded(&out.bytes);
        assert_eq!(img.dimensions(), (2000, 667));
        assert!(img.color().has_alpha());
        assert_eq!(img.to_rgba8().get_pixel(10, 10)[3], 0);
    }

    #[test]
    fn an_oversized_opaque_webp_becomes_a_jpeg() {
        let mut original = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut original)
            .write_image(
                photo(2400, 1200).as_raw(),
                2400,
                1200,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        let out = compress_image(original).unwrap();
        assert_eq!(out.ext, "jpg");
        assert_eq!(decoded(&out.bytes).dimensions(), (2000, 1000));
    }

    #[test]
    fn something_that_only_claims_to_be_an_image_is_refused() {
        for junk in [
            b"%PDF-1.7 not a photo at all".to_vec(),
            b"<html><body>hi</body></html>".to_vec(),
            vec![],
        ] {
            assert!(matches!(compress_image(junk), Err(AppError::BadRequest(_))));
        }
    }

    /// A JPEG cut off partway through its pixel data still decodes — the
    /// decoder fills the rest, the way a browser would show it — so the case
    /// that must be refused is one cut off before there is a picture at all.
    #[test]
    fn a_jpeg_with_no_image_data_is_refused_rather_than_stored() {
        let whole = jpeg(&photo(400, 300), 90);
        assert!(matches!(
            compress_image(whole[..24].to_vec()),
            Err(AppError::BadRequest(_))
        ));
    }
}
