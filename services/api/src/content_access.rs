//! Which roles may see which gated sections — admin-editable, so opening the
//! photo album to family is a toggle rather than a deploy.
//!
//! Two independent ways past a section's gate, either sufficient on its own:
//! holding one of its `allowed_roles`, or having an ever-approved booking
//! when the section says bookings count. That second path is what keeps the
//! camp working for a `guest` who has stayed but was never promoted.
//!
//! Not every gate belongs here. A section marked `configurable = false` is
//! listed for the admin's benefit and refused for editing: its real rule
//! lives in code and depends on more than a role — see [`JOURNAL_ENTRY`] and
//! [`CHECKIN_INFO`], both of which turn on a specific booking rather than on
//! who the person is.

use crate::{
    ApiResult, AppError, Shared,
    auth::AdminUser,
    users::{self, User},
};
use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// The shared photo album link (`site_content::guest_photos_link`).
pub const GUEST_PHOTOS: &str = "guest_photos";

/// Writing a journal entry. Listed but not configurable: eligibility is a
/// property of a *stay*, not of a person — see [`crate::journal`].
pub const JOURNAL_ENTRY: &str = "journal_entry";

/// Arrival details. Listed but not configurable — see [`crate::checkin_info`].
pub const CHECKIN_INFO: &str = "checkin_info";

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SectionAccess {
    pub section_key: String,
    pub label: String,
    pub description: String,
    pub allowed_roles: Vec<String>,
    pub approved_booking_grants: bool,
    pub configurable: bool,
    pub sort_order: i32,
    pub updated_at: DateTime<Utc>,
}

/// The rule itself, as a pure function of the row and the two facts about the
/// viewer that can satisfy it. Kept separate from the queries below so it is
/// testable without a database, and so there is exactly one statement of it.
///
/// Role and booking history are OR'd, not AND'd: a promoted family member
/// with no bookings and a guest who stayed last summer are both legitimate
/// viewers of the album, by different routes.
pub fn grants(section: &SectionAccess, role: &str, has_approved_booking: bool) -> bool {
    section.allowed_roles.iter().any(|r| r == role)
        || (section.approved_booking_grants && has_approved_booking)
}

pub async fn load(db: &PgPool, section_key: &str) -> Result<Option<SectionAccess>, sqlx::Error> {
    sqlx::query_as::<_, SectionAccess>(
        "SELECT section_key, label, description, allowed_roles, approved_booking_grants,
                configurable, sort_order, updated_at
         FROM content_access WHERE section_key = $1",
    )
    .bind(section_key)
    .fetch_optional(db)
    .await
}

/// Whether `user` may view `section_key`, per the current configuration.
///
/// A missing row denies rather than allows: a section whose configuration was
/// never seeded is one nobody has decided about yet, and guessing "open" on a
/// gate is the wrong way to be wrong.
///
/// The booking lookup is skipped entirely when the role alone already answers
/// it, so the common case for a promoted family member is one query, not two.
pub async fn user_may_view(db: &PgPool, section_key: &str, user: &User) -> ApiResult<bool> {
    let Some(section) = load(db, section_key).await? else {
        tracing::warn!(%section_key, "no content_access row; denying");
        return Ok(false);
    };

    if section.allowed_roles.contains(&user.role) {
        return Ok(true);
    }
    if !section.approved_booking_grants {
        return Ok(false);
    }

    let has_booking = users::has_approved_booking(db, user.id).await?;
    Ok(grants(&section, &user.role, has_booking))
}

// ─────────────────────────── admin ───────────────────────────

pub async fn list_admin(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
) -> ApiResult<Json<Vec<SectionAccess>>> {
    let rows = sqlx::query_as::<_, SectionAccess>(
        "SELECT section_key, label, description, allowed_roles, approved_booking_grants,
                configurable, sort_order, updated_at
         FROM content_access ORDER BY sort_order, section_key",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccess {
    pub allowed_roles: Vec<String>,
    pub approved_booking_grants: bool,
}

/// Every role a section may be opened to. Mirrors `users_role_valid` in
/// migration 0011 — an unknown role here would be a toggle that silently
/// never matches anyone.
pub const ASSIGNABLE_ROLES: [&str; 3] = ["guest", "user", "admin"];

fn validate_roles(roles: &[String]) -> ApiResult<()> {
    if let Some(bad) = roles
        .iter()
        .find(|r| !ASSIGNABLE_ROLES.contains(&r.as_str()))
    {
        return Err(AppError::BadRequest(format!(
            "'{bad}' is not a role. Valid roles: {}.",
            ASSIGNABLE_ROLES.join(", ")
        )));
    }
    Ok(())
}

/// `PUT /api/admin/content-access/{section_key}` — admin only.
///
/// Refuses sections marked `configurable = false`. Their rule turns on a
/// booking rather than a role, so accepting role toggles for them would be
/// writing a setting that does nothing — or worse, one an admin believes is
/// in force.
pub async fn update_admin(
    State(state): State<Shared>,
    AdminUser(_): AdminUser,
    Path(section_key): Path<String>,
    Json(body): Json<UpdateAccess>,
) -> ApiResult<Json<SectionAccess>> {
    let existing = load(&state.db, &section_key)
        .await?
        .ok_or_else(|| AppError::NotFound("No such content section.".into()))?;

    if !existing.configurable {
        return Err(AppError::Conflict(format!(
            "\"{}\" isn't role-configurable — access to it depends on a booking, not on who \
             someone is.",
            existing.label
        )));
    }
    validate_roles(&body.allowed_roles)?;

    let row = sqlx::query_as::<_, SectionAccess>(
        "UPDATE content_access
         SET allowed_roles = $2, approved_booking_grants = $3
         WHERE section_key = $1
         RETURNING section_key, label, description, allowed_roles, approved_booking_grants,
                   configurable, sort_order, updated_at",
    )
    .bind(&section_key)
    .bind(&body.allowed_roles)
    .bind(body.approved_booking_grants)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(
        %section_key,
        roles = ?row.allowed_roles,
        booking_grants = row.approved_booking_grants,
        "content access updated",
    );
    Ok(Json(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(roles: &[&str], booking_grants: bool) -> SectionAccess {
        SectionAccess {
            section_key: GUEST_PHOTOS.into(),
            label: "Camp photo album".into(),
            description: String::new(),
            allowed_roles: roles.iter().map(|r| (*r).to_string()).collect(),
            approved_booking_grants: booking_grants,
            configurable: true,
            sort_order: 1,
            updated_at: Utc::now(),
        }
    }

    // The shipped default for the album: family by role, plus anyone who has
    // actually stayed regardless of role.
    #[test]
    fn a_family_member_needs_no_booking() {
        let s = section(&["user", "admin"], true);
        assert!(grants(&s, "user", false));
        assert!(grants(&s, "admin", false));
    }

    #[test]
    fn a_guest_who_has_stayed_still_gets_in_without_being_promoted() {
        let s = section(&["user", "admin"], true);
        assert!(grants(&s, "guest", true));
    }

    #[test]
    fn a_guest_who_has_never_stayed_is_turned_away() {
        let s = section(&["user", "admin"], true);
        assert!(!grants(&s, "guest", false));
    }

    // The toggle has to actually do something in both directions, since that
    // is the whole point of storing this rather than hardcoding it.
    #[test]
    fn closing_a_role_off_takes_effect() {
        let closed = section(&["admin"], true);
        assert!(!grants(&closed, "user", false));
        // ...and the booking route is unaffected by the role toggle.
        assert!(grants(&closed, "user", true));
    }

    #[test]
    fn dropping_the_booking_route_leaves_only_roles() {
        let roles_only = section(&["user"], false);
        assert!(!grants(&roles_only, "guest", true));
        assert!(grants(&roles_only, "user", false));
    }

    #[test]
    fn a_section_open_to_nobody_admits_nobody() {
        let shut = section(&[], false);
        for role in ASSIGNABLE_ROLES {
            assert!(!grants(&shut, role, false));
            assert!(!grants(&shut, role, true));
        }
    }

    #[test]
    fn unknown_roles_are_rejected_rather_than_stored_as_dead_toggles() {
        assert!(validate_roles(&["guest".into(), "user".into()]).is_ok());
        assert!(matches!(
            validate_roles(&["owner".into()]),
            Err(AppError::BadRequest(_))
        ));
    }
}
