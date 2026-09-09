//! Password hashing for the optional admin password login.
//!
//! argon2id with the crate's default parameters, stored as a PHC string in
//! `users.password_hash` — never plaintext, and never logged. The same choice
//! the sibling ShiftScheduler app made, so the two apps don't drift.
//!
//! Policy is deliberately thin: a length floor and nothing else. This gates a
//! handful of trusted admin accounts who also still have OTP, so composition
//! rules would buy irritation rather than security.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use std::sync::LazyLock;

/// Minimum password length, in characters rather than bytes so the count means
/// what the person typing it thinks it means.
pub const MIN_PASSWORD_LEN: usize = 10;

/// Upper bound, well past any real passphrase. Argon2's cost barely moves with
/// input length, but there is no reason to hash a megabyte of request body.
const MAX_PASSWORD_LEN: usize = 256;

/// A real hash, verified against when there is no stored hash to check.
///
/// Argon2 verification takes tens of milliseconds, so returning early for an
/// unknown address would make "no such account" and "wrong password"
/// trivially distinguishable by timing — the identical error bodies in
/// [`crate::auth::login_password`] would then leak anyway. Every login attempt
/// runs exactly one verification, this one when there is nothing else.
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    // Hashed from fresh randomness, so no input a caller can send will ever
    // match it — the stand-in spends the time without becoming a password.
    let unguessable = SaltString::generate(&mut OsRng).to_string();
    hash(&unguessable).expect("a generated salt string clears the length floor")
});

/// Checks a candidate against the length policy, returning the reason it was
/// refused. `Ok` means [`hash`] will accept it.
pub fn validate(password: &str) -> Result<(), String> {
    let len = password.chars().count();
    if len < MIN_PASSWORD_LEN {
        return Err(format!(
            "Password must be at least {MIN_PASSWORD_LEN} characters."
        ));
    }
    if len > MAX_PASSWORD_LEN {
        return Err(format!(
            "Password must be at most {MAX_PASSWORD_LEN} characters."
        ));
    }
    Ok(())
}

/// Hashes a password for storage. Validates first, so no caller can persist
/// something the policy would have refused.
pub fn hash(password: &str) -> Result<String, String> {
    validate(password)?;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("hashing failed: {e}"))
}

/// Whether `password` matches a stored PHC hash. Comparison is constant-time,
/// inside `argon2`.
///
/// A stored hash we can't parse is a corrupt row, not a match: it returns
/// `false` and logs, rather than surfacing an error the login path would have
/// to render differently from a plain wrong password.
pub fn verify(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(e) => {
            tracing::error!(error = ?e, "stored password hash is malformed");
            false
        }
    }
}

/// Spends a verification's worth of work against [`DUMMY_HASH`]. Always false.
pub fn verify_dummy(password: &str) -> bool {
    verify(password, &DUMMY_HASH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_enforces_the_length_floor() {
        assert!(validate(&"a".repeat(MIN_PASSWORD_LEN - 1)).is_err());
        assert!(validate(&"a".repeat(MIN_PASSWORD_LEN)).is_ok());
        assert!(validate("a long enough passphrase").is_ok());
    }

    /// Counted in characters, not bytes: nine accented letters are nine
    /// characters however many bytes UTF-8 spends on them.
    #[test]
    fn length_is_counted_in_characters() {
        let nine = "é".repeat(9);
        assert_eq!(nine.len(), 18, "…and eighteen bytes");
        assert!(validate(&nine).is_err());
        assert!(validate(&"é".repeat(MIN_PASSWORD_LEN)).is_ok());
    }

    #[test]
    fn validate_rejects_absurdly_long_input() {
        assert!(validate(&"a".repeat(MAX_PASSWORD_LEN)).is_ok());
        assert!(validate(&"a".repeat(MAX_PASSWORD_LEN + 1)).is_err());
    }

    #[test]
    fn hash_refuses_what_validate_refuses() {
        assert!(hash("short").is_err());
    }

    #[test]
    fn hash_then_verify_round_trips() {
        let h = hash("correct horse battery staple").unwrap();
        assert!(verify("correct horse battery staple", &h));
        assert!(!verify("Tr0ub4dour&3 not the one", &h));
    }

    /// Salted, so the same password never produces the same stored string.
    #[test]
    fn two_hashes_of_one_password_differ() {
        let a = hash("a repeated passphrase").unwrap();
        let b = hash("a repeated passphrase").unwrap();
        assert_ne!(a, b);
        assert!(verify("a repeated passphrase", &a));
        assert!(verify("a repeated passphrase", &b));
    }

    #[test]
    fn the_stored_hash_is_not_the_password() {
        let h = hash("a memorable passphrase").unwrap();
        assert!(!h.contains("memorable"));
        assert!(h.starts_with("$argon2id$"), "{h}");
    }

    /// A corrupt row must read as "wrong password", not as an error the login
    /// path would have to distinguish.
    #[test]
    fn a_malformed_stored_hash_is_not_a_match() {
        assert!(!verify("a memorable passphrase", "not-a-phc-string"));
        assert!(!verify("a memorable passphrase", ""));
    }

    /// The stand-in is hashed from per-process randomness, so nothing a caller
    /// could send matches it.
    #[test]
    fn the_dummy_hash_never_verifies() {
        assert!(!verify_dummy("a memorable passphrase"));
        assert!(!verify_dummy(""));
        assert!(!verify_dummy(&"a".repeat(64)));
    }

    /// Both branches do a real argon2 verification, so an unknown account and
    /// a wrong password cost roughly the same. Generous bounds — this is a
    /// guard against an early return, not a timing benchmark.
    #[test]
    fn a_missing_hash_costs_about_what_a_real_check_costs() {
        use std::time::Instant;
        let stored = hash("a memorable passphrase").unwrap();

        let t0 = Instant::now();
        assert!(!verify("the wrong passphrase", &stored));
        let real = t0.elapsed();

        let t1 = Instant::now();
        assert!(!verify_dummy("the wrong passphrase"));
        let dummy = t1.elapsed();

        let ratio = dummy.as_secs_f64() / real.as_secs_f64().max(f64::EPSILON);
        assert!(
            (0.2..5.0).contains(&ratio),
            "dummy {dummy:?} vs real {real:?} — one path is skipping the work"
        );
    }
}
