//! Applicant contact details: validation, normalisation, and the one place the
//! required-vs-optional rule lives.
//!
//! Person-level facts (phone, city/state, mailing address) are stored on the
//! user so a returning guest is prefilled; stay-level facts (referral,
//! watercraft) are stored on the booking. See `0018_applicant_info.sql`.
//!
//! Nothing here logs a field value, and every error message is generic about
//! *what* was wrong without echoing what was typed.

use crate::{AppError, pii::Sealed, users::User};
use serde::{Deserialize, Serialize};

pub const WATERCRAFT_KINDS: [&str; 4] = ["none", "boat", "kayak_canoe", "other"];
pub const MAX_CITY: usize = 80;
pub const MAX_STATE: usize = 40;
pub const MAX_ADDRESS: usize = 300;
pub const MAX_REFERRAL: usize = 300;
pub const MAX_WATERCRAFT_DETAILS: usize = 300;

/// What the booking form sends. Everything optional at the wire level; the
/// rules below decide what a given caller must supply.
#[derive(Default, PartialEq, Deserialize)]
pub struct ApplicantInput {
    pub phone: Option<String>,
    pub home_city: Option<String>,
    pub home_state: Option<String>,
    pub mailing_address: Option<String>,
    pub referral: Option<String>,
    pub watercraft: Option<String>,
    pub watercraft_details: Option<String>,
    /// Honeypot. Hidden from people on the form; bots fill every input.
    /// Named to look uninteresting rather than like a trap.
    #[serde(default)]
    pub fax: Option<String>,
}

// Never print the values: this struct rides inside request types that derive
// Debug, and a stray `{:?}` must not put a phone number in a log.
impl std::fmt::Debug for ApplicantInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApplicantInput(..)")
    }
}

impl ApplicantInput {
    pub fn honeypot_tripped(&self) -> bool {
        self.fax.as_deref().is_some_and(|v| !v.trim().is_empty())
    }
}

/// `555-123-4567`, `(555) 123-4567`, `+1 555 123 4567`, `5551234567` all
/// normalise to digits, with a leading `+` kept only if the user typed one.
/// 10–15 digits. No lookup or verification.
pub fn normalize_phone(raw: &str) -> Result<String, AppError> {
    let t = raw.trim();
    let plus = t.starts_with('+');
    let mut digits = String::new();
    for c in t.chars() {
        match c {
            '0'..='9' => digits.push(c),
            ' ' | '-' | '(' | ')' | '.' => {}
            '+' if digits.is_empty() && plus => {}
            _ => {
                return Err(AppError::BadRequest(
                    "Phone number can only contain digits, spaces, dashes and parentheses.".into(),
                ));
            }
        }
    }
    if !(10..=15).contains(&digits.len()) {
        return Err(AppError::BadRequest(
            "Phone number should have 10 to 15 digits.".into(),
        ));
    }
    Ok(if plus { format!("+{digits}") } else { digits })
}

fn blank(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn bounded(v: &Option<String>, max: usize, label: &str) -> Result<Option<String>, AppError> {
    match blank(v) {
        None => Ok(None),
        Some(s) if s.chars().count() > max => Err(AppError::BadRequest(format!(
            "{label} is too long (max {max} characters)."
        ))),
        Some(s) => Ok(Some(s.to_string())),
    }
}

/// Person-level fields, validated. `None` means "not supplied".
#[derive(Default)]
pub struct ContactFields {
    pub phone: Option<String>,
    pub home_city: Option<String>,
    pub home_state: Option<String>,
    pub mailing_address: Option<String>,
}

impl ContactFields {
    pub fn parse(
        phone: &Option<String>,
        city: &Option<String>,
        state: &Option<String>,
        address: &Option<String>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            phone: blank(phone).map(normalize_phone).transpose()?,
            home_city: bounded(city, MAX_CITY, "City")?,
            home_state: bounded(state, MAX_STATE, "State")?,
            mailing_address: bounded(address, MAX_ADDRESS, "Mailing address")?,
        })
    }

    /// The encrypted forms to store.
    pub fn sealed_phone(&self) -> anyhow::Result<Option<Sealed>> {
        self.phone.as_deref().map(Sealed::seal).transpose()
    }
    pub fn sealed_address(&self) -> anyhow::Result<Option<Sealed>> {
        self.mailing_address
            .as_deref()
            .map(Sealed::seal)
            .transpose()
    }

    pub fn is_empty(&self) -> bool {
        self.phone.is_none()
            && self.home_city.is_none()
            && self.home_state.is_none()
            && self.mailing_address.is_none()
    }
}

/// A validated booking-request submission.
pub struct Applicant {
    pub contact: ContactFields,
    pub referral: Option<String>,
    pub watercraft: Option<String>,
    pub watercraft_details: Option<String>,
}

/// Validates `input` and, when `enforce` is set (a `guest` submitting their own
/// request), requires phone, city, state, referral and a watercraft choice —
/// counting what is already on the user's profile as supplied.
///
/// `enforce` is false for family (`user`) and admin accounts, who are known to
/// us, and for an admin entering a booking on someone's behalf. Values that
/// *are* supplied are validated in every case.
pub fn prepare(user: &User, input: &ApplicantInput, enforce: bool) -> Result<Applicant, AppError> {
    let contact = ContactFields::parse(
        &input.phone,
        &input.home_city,
        &input.home_state,
        &input.mailing_address,
    )?;
    let referral = bounded(&input.referral, MAX_REFERRAL, "Referral")?;

    let watercraft = match blank(&input.watercraft) {
        None => None,
        Some(w) if WATERCRAFT_KINDS.contains(&w) => Some(w.to_string()),
        Some(_) => {
            return Err(AppError::BadRequest(
                "Choose none, boat, kayak or canoe, or other for watercraft.".into(),
            ));
        }
    };
    // Details only mean something when something is being brought.
    let watercraft_details = if watercraft.as_deref().is_some_and(|w| w != "none") {
        bounded(
            &input.watercraft_details,
            MAX_WATERCRAFT_DETAILS,
            "Watercraft details",
        )?
    } else {
        None
    };

    if enforce {
        let on_file_phone = user.phone.as_ref().and_then(Sealed::open).is_some();
        let has = |new: &Option<String>, old: &Option<String>| {
            new.is_some() || old.as_deref().is_some_and(|s| !s.trim().is_empty())
        };
        let mut missing = Vec::new();
        if contact.phone.is_none() && !on_file_phone {
            missing.push("a phone number");
        }
        if !has(&contact.home_city, &user.home_city) {
            missing.push("your city");
        }
        if !has(&contact.home_state, &user.home_state) {
            missing.push("your state");
        }
        if referral.is_none() {
            missing.push("who referred you (type \"none\" if nobody did)");
        }
        if watercraft.is_none() {
            missing.push("whether you're bringing a boat, kayak/canoe or other watercraft (choose \"none\" if not)");
        }
        if !missing.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Please add {}.",
                missing.join(", ")
            )));
        }
    }

    Ok(Applicant {
        contact,
        referral,
        watercraft,
        watercraft_details,
    })
}

/// What an admin or the applicant sees about a stay's applicant. An explicit
/// allow-list, built only inside `bookings::BookingRow::to_view` behind the
/// same gate as the booker's name — deliberately its own struct rather than
/// extra fields on a shared one, so a new caller cannot inherit it by
/// accident.
#[derive(Serialize)]
pub struct ApplicantView {
    pub phone: Option<String>,
    pub home_city: Option<String>,
    pub home_state: Option<String>,
    pub mailing_address: Option<String>,
    pub referral: Option<String>,
    /// Blank-ish referral ("none", "n/a", empty) — drives the neutral
    /// "No referral" badge. Nothing is auto-flagged beyond that.
    pub no_referral: bool,
    pub watercraft: Option<String>,
    pub watercraft_details: Option<String>,
}

impl std::fmt::Debug for ApplicantView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApplicantView(..)")
    }
}

/// Whether a referral says nobody sent them.
pub fn no_referral(referral: Option<&str>) -> bool {
    let norm: String = referral
        .unwrap_or("")
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect();
    matches!(
        norm.trim(),
        "" | "none" | "na" | "no" | "nobody" | "no one" | "nothing" | "nope" | "n a"
    ) || referral.is_some_and(|r| r.trim().eq_ignore_ascii_case("n/a"))
}

/// Human label for the watercraft kind, for emails.
pub fn watercraft_label(kind: &str) -> &'static str {
    match kind {
        "none" => "None",
        "boat" => "Boat",
        "kayak_canoe" => "Kayak or canoe",
        _ => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_phone_formats() {
        assert_eq!(normalize_phone("555-123-4567").unwrap(), "5551234567");
        assert_eq!(normalize_phone("(555) 123-4567").unwrap(), "5551234567");
        assert_eq!(normalize_phone("+1 555 123 4567").unwrap(), "+15551234567");
        assert_eq!(normalize_phone("5551234567").unwrap(), "5551234567");
    }

    #[test]
    fn rejected_phones() {
        assert!(normalize_phone("555-1234").is_err());
        assert!(normalize_phone("555-CALL-NOW").is_err());
        assert!(normalize_phone("1234567890123456").is_err());
        assert!(normalize_phone("555+123+4567").is_err());
    }

    #[test]
    fn blankish_referrals() {
        for r in ["", "  ", "none", "None.", "N/A", "n/a", "na", "No one"] {
            assert!(no_referral(Some(r)), "{r:?}");
        }
        assert!(no_referral(None));
        for r in ["Google", "my cousin Jean", "Facebook group"] {
            assert!(!no_referral(Some(r)), "{r:?}");
        }
    }

    #[test]
    fn debug_hides_values() {
        let i = ApplicantInput {
            phone: Some("5551234567".into()),
            ..Default::default()
        };
        assert!(!format!("{i:?}").contains("555"));
    }
}
