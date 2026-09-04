//! A small fixed-window rate limiter for the one public endpoint that spends
//! money: `POST /auth/request-otp` sends an email on every accepted call.
//!
//! Deliberately in-process rather than Redis-backed. The camp runs a single
//! API replica, and a family booking app does not justify another service to
//! operate. The trade-off is explicit: **limits reset on deploy and are not
//! shared between replicas**, so if this service is ever scaled past one
//! instance the ceiling multiplies by the replica count. That is acceptable
//! for a cost guard; it would not be for anything security-critical.

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

/// Limits for the OTP endpoint.
pub struct RateLimits {
    /// Per address. Stops one inbox being flooded with codes.
    pub otp_per_email: RateLimiter,
    /// Per client IP. This is the one that caps spend, since an attacker
    /// cycling through addresses defeats the per-email limit.
    pub otp_per_ip: RateLimiter,
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
