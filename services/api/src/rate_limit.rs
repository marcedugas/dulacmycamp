//! A small fixed-window rate limiter for the public endpoints worth throttling:
//!
//! * `POST /auth/request-otp` — sends an email on every accepted call, so this
//!   is a spend guard.
//! * `POST /auth/login-password` — the admin password login, where the point is
//!   to make guessing expensive.
//!
//! Deliberately in-process rather than Redis-backed. The camp runs a single
//! API replica, and a family booking app does not justify another service to
//! operate. The trade-off is explicit: **limits reset on deploy and are not
//! shared between replicas**, so if this service is ever scaled past one
//! instance the ceiling multiplies by the replica count, and a deploy hands
//! everyone a fresh budget.
//!
//! For the OTP spend guard that is plainly fine. For the password endpoint it
//! is a real, if modest, weakness — a determined attacker who can time deploys
//! gets extra attempts. It is accepted here because the exposure is small: the
//! endpoint only ever answers for admin accounts, of which there are a handful,
//! all of which also hold a `MIN_PASSWORD_LEN`-plus secret. If this service is
//! ever replicated, the password limiter is the piece that has to move to
//! shared storage first.

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

/// Once the map exceeds this many keys, expired entries are swept. Keeps a
/// long-running process from accumulating one entry per address seen forever.
const SWEEP_THRESHOLD: usize = 1_024;

pub struct RateLimiter {
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
    max: usize,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            hits: Mutex::new(HashMap::new()),
            max,
            window,
        }
    }

    /// Records an attempt for `key`.
    ///
    /// Returns `Err(seconds)` when the caller is over the limit, where
    /// `seconds` is how long until the oldest hit falls out of the window.
    /// A rejected attempt is *not* recorded, so hammering the endpoint cannot
    /// extend anyone's own lockout indefinitely.
    pub fn check(&self, key: &str) -> Result<(), u64> {
        let now = Instant::now();
        let mut map = self.hits.lock().unwrap_or_else(|e| e.into_inner());

        if map.len() > SWEEP_THRESHOLD {
            map.retain(|_, times| times.iter().any(|t| now.duration_since(*t) < self.window));
        }

        let times = map.entry(key.to_string()).or_default();
        while times
            .front()
            .is_some_and(|t| now.duration_since(*t) >= self.window)
        {
            times.pop_front();
        }

        if times.len() >= self.max {
            let oldest = times.front().copied().unwrap_or(now);
            let elapsed = now.duration_since(oldest);
            let retry = self.window.saturating_sub(elapsed).as_secs().max(1);
            return Err(retry);
        }

        times.push_back(now);
        Ok(())
    }
}

/// Limits for the unauthenticated auth endpoints.
pub struct RateLimits {
    /// Per address. Stops one inbox being flooded with codes.
    pub otp_per_email: RateLimiter,
    /// Per client IP. This is the one that caps spend, since an attacker
    /// cycling through addresses defeats the per-email limit.
    pub otp_per_ip: RateLimiter,
    /// Password attempts per client IP.
    pub password_per_ip: RateLimiter,
    /// Password attempts per address, so one account cannot be ground down
    /// from a rotating pool of addresses.
    pub password_per_email: RateLimiter,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            // One code per address per minute — a real person who mistypes
            // and retries waits a moment; a script gets nothing.
            otp_per_email: RateLimiter::new(1, Duration::from_secs(60)),
            // A whole family can share one NAT address at the camp, so allow
            // a small burst rather than one per minute per household.
            otp_per_ip: RateLimiter::new(5, Duration::from_secs(600)),

            // Tighter than the OTP ceiling above — same attempt count over a
            // window half again as long. Guessing a password is a different
            // threat from flooding an inbox: there is no cost ceiling to
            // protect, only a secret, so the budget should be mean.
            //
            // Every attempt counts, not only the failures, which is the
            // stricter reading: an attacker gets five tries per window whether
            // or not any of them land. The cost is that an admin signing in
            // five times in fifteen minutes is asked to wait — and OTP is
            // still right there, so they are inconvenienced, never locked out.
            password_per_ip: RateLimiter::new(5, Duration::from_secs(900)),
            // Same budget keyed by address. This one is deliberately
            // exhaustible by a third party: someone else burning an admin's
            // password budget costs that admin nothing but the password path,
            // which OTP already backs up.
            password_per_email: RateLimiter::new(5, Duration::from_secs(900)),
        }
    }
}

/// Best-effort client address.
///
/// Railway terminates TLS at its edge and forwards the original address in
/// `X-Forwarded-For`, so the socket address is always the proxy. The leftmost
/// entry is the client — and is client-supplied, so it is spoofable. That is
/// tolerable here: the header only gates a cost guard, never authorisation,
/// and the per-email limit still applies to a spoofed request.
pub fn client_ip(
    headers: &axum::http::HeaderMap,
    fallback: Option<std::net::SocketAddr>,
) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.map_or_else(|| "unknown".to_string(), |a| a.ip().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_the_limit_then_rejects() {
        let rl = RateLimiter::new(2, Duration::from_secs(60));
        assert!(rl.check("a").is_ok());
        assert!(rl.check("a").is_ok());
        let retry = rl.check("a").expect_err("third attempt should be refused");
        assert!(retry > 0 && retry <= 60, "retry was {retry}");
    }

    #[test]
    fn keys_are_independent() {
        let rl = RateLimiter::new(1, Duration::from_secs(60));
        assert!(rl.check("a").is_ok());
        assert!(rl.check("b").is_ok(), "one key must not limit another");
        assert!(rl.check("a").is_err());
    }

    #[test]
    fn window_expiry_lets_the_caller_back_in() {
        let rl = RateLimiter::new(1, Duration::from_millis(40));
        assert!(rl.check("a").is_ok());
        assert!(rl.check("a").is_err());
        std::thread::sleep(Duration::from_millis(60));
        assert!(rl.check("a").is_ok(), "limit should lapse with the window");
    }

    /// A rejected attempt must not count, or a caller hammering the endpoint
    /// would keep pushing their own unlock further away.
    #[test]
    fn rejected_attempts_do_not_extend_the_lockout() {
        let rl = RateLimiter::new(1, Duration::from_millis(60));
        assert!(rl.check("a").is_ok());
        for _ in 0..20 {
            assert!(rl.check("a").is_err());
        }
        std::thread::sleep(Duration::from_millis(80));
        assert!(rl.check("a").is_ok());
    }

    #[test]
    fn forwarded_header_wins_over_the_socket_address() {
        let mut h = axum::http::HeaderMap::new();
        h.insert(
            "x-forwarded-for",
            "203.0.113.7, 70.41.3.18".parse().unwrap(),
        );
        let sock = Some("10.0.0.1:5000".parse().unwrap());
        assert_eq!(client_ip(&h, sock), "203.0.113.7");
    }

    #[test]
    fn falls_back_to_the_socket_address_then_to_unknown() {
        let h = axum::http::HeaderMap::new();
        assert_eq!(
            client_ip(&h, Some("10.0.0.1:5000".parse().unwrap())),
            "10.0.0.1"
        );
        assert_eq!(client_ip(&h, None), "unknown");
    }
}
